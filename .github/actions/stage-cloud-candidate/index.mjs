import {createHash} from 'node:crypto';
import {fork} from 'node:child_process';
import {constants} from 'node:fs';
import {appendFile, lstat, open, readdir, realpath} from 'node:fs/promises';
import https from 'node:https';
import path from 'node:path';
import {fileURLToPath} from 'node:url';

const MAX_PART = 536870912;
const MAX_TOTAL = 64 * 1024 ** 3;
const MAX_MANIFEST = 4 * 1024 ** 2;
const ZIP_ALLOWANCE = 1024 ** 2;
const HEX = /^[0-9a-f]{64}$/;
const SOURCE = /^[0-9a-f]{40}$/;
const DIGEST = /^sha256:[0-9a-f]{64}$/;
const FILE_PATH = /^(?:q6-metadata\.json|q6-oci\/(?:index\.json|oci-layout|blobs\/sha256\/[0-9a-f]{64}))$/;

export class StageError extends Error {}
function requireValue(ok, code) {
  if (!ok) throw new StageError(code);
}
const integer = (value, low, high) => Number.isSafeInteger(value) && value >= low && value <= high;
const positiveId = value => integer(value, 1, Number.MAX_SAFE_INTEGER);
const exactKeys = (value, keys) => value && typeof value === 'object' && !Array.isArray(value)
  && Object.keys(value).sort().join(',') === [...keys].sort().join(',');
const identity = st => [st.dev, st.ino, st.size, st.mtimeMs, st.ctimeMs].join(':');

async function checkedFile(filename, expectedSize, collect = false) {
  const file = await open(filename, constants.O_RDONLY | constants.O_NOFOLLOW | constants.O_NONBLOCK);
  try {
    const before = await file.stat();
    requireValue(before.isFile() && integer(before.size, 1, expectedSize), 'file_invalid');
    const hash = createHash('sha256');
    const buffer = Buffer.alloc(1024 * 1024);
    const chunks = [];
    let size = 0;
    for (;;) {
      const {bytesRead} = await file.read(buffer, 0, buffer.length, null);
      if (!bytesRead) break;
      size += bytesRead;
      requireValue(size <= expectedSize, 'file_oversized');
      hash.update(buffer.subarray(0, bytesRead));
      if (collect) chunks.push(Buffer.from(buffer.subarray(0, bytesRead)));
    }
    requireValue(size === before.size && identity(before) === identity(await file.stat()), 'file_changed');
    return {file: filename, size, hash: hash.digest('hex'), identity: identity(before),
      bytes: collect ? Buffer.concat(chunks) : undefined};
  } finally {
    await file.close();
  }
}

export async function loadCandidate(manifestPath, source, digest) {
  requireValue(SOURCE.test(source) && DIGEST.test(digest), 'candidate_identity_invalid');
  requireValue(typeof manifestPath === 'string' && manifestPath.length <= 4096
    && path.isAbsolute(manifestPath) && path.basename(manifestPath) === 'transfer-manifest.json', 'manifest_path_invalid');
  const root = path.dirname(path.resolve(manifestPath));
  requireValue(await realpath(root) === root, 'symlink_directory_rejected');
  const manifest = await checkedFile(manifestPath, MAX_MANIFEST, true);
  let data;
  try { data = JSON.parse(manifest.bytes.toString('utf8')); }
  catch { throw new StageError('manifest_json_invalid'); }
  requireValue(exactKeys(data, ['schema', 'source_sha', 'image_digest', 'part_size', 'total_bytes', 'files', 'parts'])
    && data.schema === 1 && data.source_sha === source && data.image_digest === digest
    && integer(data.part_size, 1, MAX_PART) && integer(data.total_bytes, 1, MAX_TOTAL), 'manifest_header_invalid');
  requireValue(Array.isArray(data.files) && data.files.length >= 4 && data.files.length <= 32768
    && Array.isArray(data.parts) && integer(data.parts.length, 1, 128)
    && data.parts.length === Math.ceil(data.total_bytes / data.part_size), 'manifest_counts_invalid');
  let offset = 0;
  let previous = '';
  const requiredFiles = new Set(['q6-metadata.json', 'q6-oci/index.json', 'q6-oci/oci-layout']);
  for (const file of data.files) {
    requireValue(exactKeys(file, ['path', 'size', 'sha256', 'offset']) && typeof file.path === 'string' && FILE_PATH.test(file.path)
      && file.path > previous && integer(file.size, 0, MAX_TOTAL) && typeof file.sha256 === 'string' && HEX.test(file.sha256)
      && file.offset === offset, 'manifest_file_invalid');
    offset += file.size;
    requireValue(offset <= data.total_bytes, 'manifest_file_coverage_invalid');
    previous = file.path;
    requiredFiles.delete(file.path);
  }
  requireValue(offset === data.total_bytes && requiredFiles.size === 0, 'manifest_file_coverage_invalid');
  const payloads = [{...manifest, bytes: undefined, kind: 'manifest'}];
  for (const [index, part] of data.parts.entries()) {
    const name = `part-${String(index).padStart(6, '0')}.bin`;
    const size = Math.min(data.part_size, data.total_bytes - index * data.part_size);
    requireValue(exactKeys(part, ['name', 'size', 'sha256']) && part.name === name
      && part.size === size && typeof part.sha256 === 'string' && HEX.test(part.sha256), 'manifest_part_invalid');
    const checked = await checkedFile(path.join(root, name), size);
    requireValue(checked.size === size && checked.hash === part.sha256, 'part_content_mismatch');
    payloads.push({...checked, kind: `part-${String(index).padStart(6, '0')}`});
  }
  const expectedNames = payloads.map(p => path.basename(p.file)).sort();
  requireValue((await readdir(root)).sort().join('\n') === expectedNames.join('\n'), 'unexpected_transfer_files');
  return {root, hash: manifest.hash, payloads};
}

export function runContext(env) {
  const repository = env.GITHUB_REPOSITORY;
  const runId = env.GITHUB_RUN_ID;
  const attempt = env.GITHUB_RUN_ATTEMPT;
  requireValue(repository === '1905/lobocode'
    && typeof runId === 'string' && /^[1-9][0-9]{0,15}$/.test(runId)
    && positiveId(Number(runId)) && typeof attempt === 'string' && /^[1-9][0-9]{0,8}$/.test(attempt), 'run_context_invalid');
  requireValue(!env.GITHUB_API_URL || env.GITHUB_API_URL === 'https://api.github.com', 'github_host_rejected');
  return {repository, runId, attempt: Number(attempt)};
}

// Claims come from the runner environment. This checks time coverage, not JWT
// authenticity or the lifetime of the separate signed upload URL.
export function runtimeTokenGate(token, now, log) {
  requireValue(typeof token === 'string' && token.length <= 65536
    && /^[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+$/.test(token), 'runtime_token_invalid');
  let claims;
  try { claims = JSON.parse(Buffer.from(token.split('.')[1], 'base64url').toString('utf8')); }
  catch { throw new StageError('runtime_token_invalid'); }
  const {iat, nbf, exp} = claims ?? {};
  requireValue([iat, nbf, exp].every(value => integer(value, 1, 253402300799))
    && nbf <= iat && iat < exp && Number.isFinite(now), 'runtime_token_invalid');
  const seconds = Math.floor(now / 1000);
  const remaining_seconds = exp - seconds;
  const required_seconds = Math.max(0, 720 * 60 - (seconds - iat)) + 300;
  const passed = iat <= seconds + 60 && nbf <= seconds + 60 && remaining_seconds >= required_seconds;
  log(JSON.stringify({event: 'runtime_token_gate', iat, nbf, exp, remaining_seconds, required_seconds, passed}));
  requireValue(passed, 'runtime_token_lifetime_insufficient');
}

// Never follow redirects or log response bodies, tokens or request errors.
export function requestArtifacts(endpoint, token) {
  return new Promise((resolve, reject) => {
    let timer;
    const fail = () => { clearTimeout(timer); reject(new StageError('artifact_list_failed')); };
    const req = https.request({hostname: 'api.github.com', path: endpoint, method: 'GET',
      headers: {Authorization: `Bearer ${token}`, Accept: 'application/vnd.github+json',
        'X-GitHub-Api-Version': '2022-11-28', 'User-Agent': 'lobocode-candidate-stage'}}, res => {
      res.on('error', fail);
      if (res.statusCode !== 200) { res.destroy(); fail(); return; }
      const chunks = [];
      let size = 0;
      res.on('data', chunk => {
        size += chunk.length;
        if (size > 2 * 1024 ** 2) { req.destroy(); fail(); return; }
        chunks.push(chunk);
      });
      res.on('end', () => {
        clearTimeout(timer);
        try { resolve(JSON.parse(Buffer.concat(chunks).toString('utf8'))); }
        catch { fail(); }
      });
    });
    req.on('error', () => { clearTimeout(timer); fail(); });
    timer = setTimeout(() => { req.destroy(); fail(); }, 30000);
    req.end();
  });
}

export async function listArtifacts(context, request) {
  const all = [];
  for (let page = 1; page <= 100; page++) {
    const response = await request(`/repos/${context.repository}/actions/runs/${context.runId}/artifacts?per_page=100&page=${page}`);
    requireValue(response && integer(response.total_count, 0, 10000) && Array.isArray(response.artifacts)
      && response.artifacts.length <= 100, 'artifact_list_invalid');
    all.push(...response.artifacts);
    if (response.artifacts.length < 100) return all;
  }
  throw new StageError('artifact_list_limit');
}

export function artifactPrefix(context, manifestHash, payload) {
  return `lobo-q6-${context.runId}-${manifestHash}-${payload.kind}`
    + (payload.kind === 'manifest' ? '' : `-${payload.hash}`) + '-attempt-';
}

function reusable(artifacts, prefix, context, payload, now) {
  return artifacts.filter(item => {
    if (!item || typeof item.name !== 'string' || !item.name.startsWith(prefix)) return false;
    const attempt = item.name.slice(prefix.length);
    return /^[1-9][0-9]{0,8}$/.test(attempt) && Number(attempt) < context.attempt
      && item.workflow_run?.id === Number(context.runId) && positiveId(item.id)
      && item.expired === false && typeof item.expires_at === 'string' && Date.parse(item.expires_at) > now
      && integer(item.size_in_bytes, payload.size, payload.size + ZIP_ALLOWANCE);
  }).sort((a, b) => b.id - a.id)[0];
}

async function unchanged(payload) {
  const st = await lstat(payload.file);
  requireValue(st.isFile() && identity(st) === payload.identity, 'file_changed');
}

// The SDK can reject while other requests still log. Isolate its entire process
// lifetime, including import and late writes; only numeric receipts cross IPC.
export function uploadWorker(name, files, rootDirectory, options, settings = {}) {
  return new Promise((resolve, reject) => {
    const worker = fork(settings.workerPath ?? fileURLToPath(import.meta.url), ['--sdk-worker'], {
      stdio: ['ignore', 'ignore', 'ignore', 'ipc'], env: settings.env ?? process.env,
      execArgv: []
    });
    let result;
    let settled = false;
    let failed = false;
    const cancel = () => fail();
    const timer = setTimeout(() => fail(), settings.timeoutMs ?? 30 * 60 * 1000);
    const cleanup = () => {
      clearTimeout(timer);
      process.off('SIGINT', cancel);
      process.off('SIGTERM', cancel);
      settings.signal?.removeEventListener('abort', cancel);
    };
    const fail = () => {
      if (settled || failed) return;
      failed = true;
      worker.kill('SIGKILL');
    };
    process.once('SIGINT', cancel);
    process.once('SIGTERM', cancel);
    settings.signal?.addEventListener('abort', cancel, {once: true});
    worker.on('error', fail);
    worker.on('message', message => {
      if (result || !exactKeys(message, ['id', 'size']) || !positiveId(message.id)
        || !integer(message.size, 1, MAX_PART + ZIP_ALLOWANCE)) { fail(); return; }
      result = message;
    });
    worker.on('disconnect', () => { if (!result) fail(); });
    worker.on('close', code => {
      if (settled) return;
      settled = true;
      cleanup();
      if (failed || code !== 0 || !result) reject(new StageError('artifact_upload_failed'));
      else resolve(result);
    });
    if (settings.signal?.aborted) { fail(); return; }
    worker.send({name, files, rootDirectory, options}, error => { if (error) fail(); });
  });
}

function validUpload(message) {
  return exactKeys(message, ['name', 'files', 'rootDirectory', 'options'])
    && typeof message.name === 'string'
    && /^lobo-q6-[1-9][0-9]{0,15}-[0-9a-f]{64}-(?:manifest|part-[0-9]{6}-[0-9a-f]{64})-attempt-[1-9][0-9]{0,8}$/.test(message.name)
    && typeof message.rootDirectory === 'string' && message.rootDirectory.length <= 4096
    && path.isAbsolute(message.rootDirectory) && Array.isArray(message.files) && message.files.length === 1
    && typeof message.files[0] === 'string' && message.files[0].length <= 4096 && path.isAbsolute(message.files[0])
    && path.dirname(message.files[0]) === message.rootDirectory
    && /^(?:transfer-manifest\.json|part-[0-9]{6}\.bin)$/.test(path.basename(message.files[0]))
    && exactKeys(message.options, ['compressionLevel', 'retentionDays'])
    && message.options.compressionLevel === 0 && message.options.retentionDays === 1;
}

function sdkWorker() {
  if (!process.connected) process.exit(1);
  process.once('message', async message => {
    try {
      requireValue(validUpload(message), 'upload_input_invalid');
      const {DefaultArtifactClient} = await import('@actions/artifact');
      const result = await new DefaultArtifactClient().uploadArtifact(message.name, message.files,
        message.rootDirectory, message.options);
      requireValue(result && positiveId(result.id)
        && integer(result.size, 1, MAX_PART + ZIP_ALLOWANCE), 'artifact_finalize_invalid');
      process.send({id: result.id, size: result.size}, error => process.exit(error ? 1 : 0));
    } catch { process.exit(1); }
  });
}

export async function stage({manifest, source, digest, context, request, upload, emit, log = () => {}, now = () => Date.now()}) {
  const candidate = await loadCandidate(manifest, source, digest);
  const artifacts = await listArtifacts(context, request);
  const ids = [];
  const reused = [];
  for (const payload of candidate.payloads) {
    await unchanged(payload);
    const prefix = artifactPrefix(context, candidate.hash, payload);
    const prior = reusable(artifacts, prefix, context, payload, now());
    let id;
    if (prior) {
      id = prior.id;
      reused.push(prior);
      log(`reused ${payload.kind} artifact_id=${id}`);
    } else {
      log(`uploading ${payload.kind} bytes=${payload.size}`);
      const result = await upload(prefix + context.attempt, [payload.file], candidate.root,
        {compressionLevel: 0, retentionDays: 1});
      requireValue(result && positiveId(result.id) && integer(result.size, payload.size, payload.size + ZIP_ALLOWANCE), 'artifact_finalize_invalid');
      id = result.id;
      log(`finalized ${payload.kind} artifact_id=${id}`);
    }
    await unchanged(payload);
    ids.push(id);
  }
  for (const payload of candidate.payloads) await unchanged(payload);
  requireValue(ids.length === candidate.payloads.length && new Set(ids).size === ids.length, 'artifact_set_incomplete');
  requireValue(reused.every(item => Date.parse(item.expires_at) > now()), 'reused_artifact_expired');
  const outputs = {'artifact-ids': ids.join(','), 'manifest-sha256': candidate.hash};
  await emit(outputs);
  return outputs;
}

export async function runAction(env = process.env, dependencies = {}) {
  const error = dependencies.error ?? (message => process.stderr.write(`::error::${message}\n`));
  try {
    const context = runContext(env);
    const token = env['INPUT_GITHUB-TOKEN'];
    requireValue(typeof token === 'string' && token.length > 0 && token.length <= 8192 && !/[\r\n]/.test(token), 'github_token_invalid');
    requireValue(typeof env.GITHUB_OUTPUT === 'string' && env.GITHUB_OUTPUT.length > 0, 'output_path_missing');
    const log = dependencies.log ?? (message => process.stdout.write(`${message}\n`));
    return await stage({manifest: env.INPUT_MANIFEST, source: env['INPUT_CANDIDATE-SOURCE'],
      digest: env['INPUT_CANDIDATE-DIGEST'], context,
      request: dependencies.request ?? (endpoint => requestArtifacts(endpoint, token)),
      upload: (...args) => {
        runtimeTokenGate(env.ACTIONS_RUNTIME_TOKEN, (dependencies.now ?? Date.now)(), log);
        return (dependencies.upload ?? uploadWorker)(...args);
      },
      emit: dependencies.emit ?? (outputs => appendFile(env.GITHUB_OUTPUT,
        `artifact-ids=${outputs['artifact-ids']}\nmanifest-sha256=${outputs['manifest-sha256']}\n`)),
      log,
      now: dependencies.now});
  } catch (failure) {
    error(failure instanceof StageError ? failure.message : 'candidate_staging_failed');
    return null;
  }
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  if (process.argv[2] === '--sdk-worker') sdkWorker();
  else if (!await runAction()) process.exitCode = 1;
}
