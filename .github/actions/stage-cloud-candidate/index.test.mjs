import test from 'node:test';
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {mkdtemp, readFile, rm, symlink, writeFile} from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import {artifactPrefix, loadCandidate, runAction, runContext, runtimeTokenGate, stage, uploadWorker} from './index.mjs';

const source = 'a'.repeat(40);
const digest = `sha256:${'b'.repeat(64)}`;
const now = Date.parse('2026-10-01T00:00:00Z');
const context = {repository: '1905/lobocode', runId: '12345678', attempt: 3};
const hash = bytes => createHash('sha256').update(bytes).digest('hex');

test('the pinned official SDK imports without install scripts or provider calls', async () => {
  const {DefaultArtifactClient} = await import('@actions/artifact');
  assert.equal(typeof new DefaultArtifactClient().uploadArtifact, 'function');
});

async function fixture(t) {
  const root = await mkdtemp(path.join(os.tmpdir(), 'lobo-artifact-fake-'));
  t.after(() => rm(root, {recursive: true, force: true}));
  const files = ['q6-metadata.json', `q6-oci/blobs/sha256/${'c'.repeat(64)}`, 'q6-oci/index.json', 'q6-oci/oci-layout'];
  const bytes = Buffer.from('{}XX[]{}');
  const data = {schema: 1, source_sha: source, image_digest: digest, part_size: 4,
    total_bytes: bytes.length,
    files: files.map((file, index) => ({path: file, size: 2, sha256: hash(bytes.subarray(index * 2, index * 2 + 2)), offset: index * 2})),
    parts: [bytes.subarray(0, 4), bytes.subarray(4)].map((part, index) => ({
      name: `part-${String(index).padStart(6, '0')}.bin`, size: part.length, sha256: hash(part)}))};
  const manifest = path.join(root, 'transfer-manifest.json');
  const save = () => writeFile(manifest, JSON.stringify(data) + '\n');
  await save();
  for (const [index, part] of data.parts.entries()) {
    await writeFile(path.join(root, part.name), bytes.subarray(index * 4, index * 4 + part.size));
  }
  return {root, manifest, source, digest, context, data, save};
}

function fake(f, overrides = {}) {
  const outputs = [];
  const uploads = [];
  const requests = [];
  const logs = [];
  const options = {...f, now: () => now,
    request: async endpoint => { requests.push(endpoint); return {total_count: 0, artifacts: []}; },
    upload: async (name, files, root, settings) => {
      const bytes = await readFile(files[0]);
      uploads.push({name, files, root, settings});
      return {id: 1000 + uploads.length, size: bytes.length + 128, digest: hash('ZIP, not payload')};
    }, emit: async value => outputs.push(value), log: value => logs.push(value), ...overrides};
  return {options, outputs, uploads, requests, logs};
}

function record(candidate, payload, id, attempt = 1) {
  return {id, name: artifactPrefix(context, candidate.hash, payload) + attempt,
    workflow_run: {id: Number(context.runId)}, expired: false,
    expires_at: '2026-10-02T00:00:00Z', size_in_bytes: payload.size + 128,
    digest: `sha256:${hash('ZIP digest is not a raw part checksum')}`};
}

test('uploads every payload independently and emits only the finalized complete set', async t => {
  const f = await fixture(t);
  const io = fake(f);
  const output = await stage(io.options);
  assert.equal(io.uploads.length, 3);
  assert.deepEqual(io.uploads.map(u => u.settings), Array(3).fill({compressionLevel: 0, retentionDays: 1}));
  assert.ok(io.uploads.every(u => u.files.length === 1 && u.root === f.root && u.name.endsWith('-attempt-3')));
  const manifestHash = hash(await readFile(f.manifest));
  assert.equal(io.uploads[0].name, `lobo-q6-12345678-${manifestHash}-manifest-attempt-3`);
  assert.equal(io.uploads[1].name, `lobo-q6-12345678-${manifestHash}-part-000000-${f.data.parts[0].sha256}-attempt-3`);
  assert.equal(output['artifact-ids'], '1001,1002,1003');
  assert.equal(output['manifest-sha256'], hash(await readFile(f.manifest)));
  assert.deepEqual(io.outputs, [output]);
});

test('paginates this exact run and reuses earlier finalized IDs across attempt suffixes', async t => {
  const f = await fixture(t);
  const candidate = await loadCandidate(f.manifest, source, digest);
  const valid = candidate.payloads.map((payload, i) => record(candidate, payload, 50 + i, i === 1 ? 2 : 1));
  const requested = [];
  const io = fake(f, {request: async endpoint => {
    requested.push(endpoint);
    return {total_count: 103, artifacts: endpoint.endsWith('page=1') ? Array(100).fill({name: 'unrelated'}) : valid};
  }});
  assert.equal((await stage(io.options))['artifact-ids'], '50,51,52');
  assert.equal(io.uploads.length, 0);
  assert.deepEqual(requested, [1, 2].map(page => `/repos/1905/lobocode/actions/runs/12345678/artifacts?per_page=100&page=${page}`));
  assert.equal(io.logs.filter(line => line.startsWith('reused')).length, 3);
});

test('refuses foreign, expired, mismatched, current and future-attempt reuse', async t => {
  const f = await fixture(t);
  const candidate = await loadCandidate(f.manifest, source, digest);
  const good = record(candidate, candidate.payloads[1], 55);
  const variants = [
    {...good, workflow_run: {id: 999}}, {...good, workflow_run: undefined},
    {...good, expired: true}, {...good, expires_at: '2026-09-30T00:00:00Z'},
    {...good, expires_at: 'invalid'}, {...good, size_in_bytes: 0}, {...good, id: 0},
    {...good, name: good.name.replace(candidate.hash, 'f'.repeat(64))},
    {...good, name: good.name.replace(candidate.payloads[1].hash, 'e'.repeat(64))},
    {...good, name: good.name.replace('-part-000000-', '-part-000001-')},
    {...good, name: good.name.replace('-attempt-1', '-attempt-3')},
    {...good, name: good.name.replace('-attempt-1', '-attempt-4')},
    {...good, name: good.name + '-suffix'}
  ];
  for (const invalid of variants) {
    const io = fake(f, {request: async () => ({total_count: 1, artifacts: [invalid]})});
    await stage(io.options);
    assert.equal(io.uploads.length, 3);
  }
});


test('ZIP overhead permits the full 1 MiB bound for upload and reuse, never more', async t => {
  const f = await fixture(t);
  const candidate = await loadCandidate(f.manifest, source, digest);
  for (const overhead of [65537, 1024 ** 2, 1024 ** 2 + 1]) {
    let calls = 0;
    const uploaded = fake(f, {upload: async (_name, files) => ({id: ++calls,
      size: (await readFile(files[0])).length + overhead})});
    if (overhead <= 1024 ** 2) await stage(uploaded.options);
    else {
      await assert.rejects(stage(uploaded.options), /artifact_finalize_invalid/);
      assert.equal(uploaded.outputs.length, 0);
    }
    const records = candidate.payloads.map((payload, index) => ({...record(candidate, payload, 30 + index),
      size_in_bytes: payload.size + overhead}));
    const reused = fake(f, {request: async () => ({total_count: records.length, artifacts: records})});
    await stage(reused.options);
    assert.equal(reused.uploads.length, overhead <= 1024 ** 2 ? 0 : 3);
  }
});

test('uploads only missing parts while preserving prior completed IDs', async t => {
  const f = await fixture(t);
  const candidate = await loadCandidate(f.manifest, source, digest);
  const prior = [record(candidate, candidate.payloads[0], 40), record(candidate, candidate.payloads[1], 41)];
  const io = fake(f, {request: async () => ({total_count: 2, artifacts: prior})});
  assert.equal((await stage(io.options))['artifact-ids'], '40,41,1001');
  assert.equal(io.uploads.length, 1);
  assert.match(io.uploads[0].name, /-part-000001-/);
});

test('partial upload failure, invalid finalization and duplicate IDs emit no outputs', async t => {
  for (const mode of ['throws', 'unfinalized', 'duplicate']) {
    const f = await fixture(t);
    let calls = 0;
    const io = fake(f, {upload: async (_name, files) => {
      calls++;
      if (calls === 3 && mode === 'throws') throw new Error('upstream private details');
      if (calls === 3 && mode === 'unfinalized') return {size: 50};
      return {id: mode === 'duplicate' ? 1 : calls, size: (await readFile(files[0])).length + 128};
    }});
    await assert.rejects(stage(io.options));
    assert.equal(io.outputs.length, 0);
  }
});

test('an expired reused part at completion prevents outputs', async t => {
  const f = await fixture(t);
  const candidate = await loadCandidate(f.manifest, source, digest);
  const item = record(candidate, candidate.payloads[0], 33);
  let elapsed = now;
  const io = fake(f, {request: async () => ({total_count: 1, artifacts: [item]}), now: () => elapsed});
  const upload = io.options.upload;
  io.options.upload = async (...args) => { elapsed = now + 2 * 86400000; return upload(...args); };
  await assert.rejects(stage(io.options), /reused_artifact_expired/);
  assert.equal(io.outputs.length, 0);
});

test('all local content is verified before the first network or upload operation', async t => {
  const f = await fixture(t);
  await writeFile(path.join(f.root, 'part-000001.bin'), 'XX');
  const io = fake(f);
  await assert.rejects(stage(io.options), /part_content_mismatch/);
  assert.equal(io.requests.length, 0);
  assert.equal(io.uploads.length, 0);
  assert.equal(io.outputs.length, 0);
});

test('invalid headers, paths, order, offsets, counts and limits are rejected', async t => {
  const changes = [
    d => d.schema = 2, d => d.source_sha = 'c'.repeat(40), d => d.image_digest = `sha256:${'c'.repeat(64)}`,
    d => d.part_size = 536870913, d => d.total_bytes = 64 * 1024 ** 3 + 1,
    d => d.files[0].path = '../secret', d => d.files.reverse(), d => d.files[1].offset++,
    d => d.files[0].sha256 = ['c'.repeat(64)], d => d.parts.reverse(), d => d.parts[1].size++,
    d => d.parts[0].name = '../part.bin', d => d.parts.push(d.parts[0]), d => d.unexpected = true,
    d => d.files.splice(1, 1)
  ];
  for (const change of changes) {
    const f = await fixture(t);
    change(f.data); await f.save();
    await assert.rejects(loadCandidate(f.manifest, source, digest));
  }
});

test('missing/extra payloads and symlinks are rejected', async t => {
  for (const mode of ['missing', 'extra', 'symlink']) {
    const f = await fixture(t);
    const part = path.join(f.root, 'part-000001.bin');
    if (mode === 'extra') await writeFile(path.join(f.root, 'extra'), 'x');
    else {
      await rm(part);
      if (mode === 'symlink') await symlink('part-000000.bin', part);
    }
    await assert.rejects(loadCandidate(f.manifest, source, digest));
  }
});

test('a local part changed during upload prevents outputs', async t => {
  const f = await fixture(t);
  const io = fake(f);
  const upload = io.options.upload;
  io.options.upload = async (...args) => {
    const result = await upload(...args);
    await writeFile(args[1][0], 'changed');
    return result;
  };
  await assert.rejects(stage(io.options), /file_changed/);
  assert.equal(io.outputs.length, 0);
});

const jwt = claims => `e30.${Buffer.from(JSON.stringify(claims)).toString('base64url')}.fake`;
const seconds = Math.floor(now / 1000);
const tokenClaims = {iat: seconds - 600, nbf: seconds - 600, exp: seconds + 43200};

test('runtime coverage emits only numeric claims and rejects insufficient or malformed tokens', () => {
  const lines = [];
  runtimeTokenGate(jwt(tokenClaims), now, line => lines.push(JSON.parse(line)));
  assert.deepEqual(lines, [{event: 'runtime_token_gate', ...tokenClaims,
    remaining_seconds: 43200, required_seconds: 42900, passed: true}]);
  for (const claims of [{...tokenClaims, exp: seconds + 42899}, {...tokenClaims, iat: seconds + 61, nbf: seconds + 61}]) {
    assert.throws(() => runtimeTokenGate(jwt(claims), now, line => lines.push(JSON.parse(line))), /lifetime_insufficient/);
    assert.equal(lines.at(-1).passed, false);
  }
  const count = lines.length;
  for (const value of ['secret', jwt({...tokenClaims, iat: 'private'}), jwt({...tokenClaims, nbf: seconds}),
    jwt({...tokenClaims, exp: tokenClaims.iat}), jwt({...tokenClaims, exp: 1.2})]) {
    assert.throws(() => runtimeTokenGate(value, now, line => lines.push(line)), /runtime_token_invalid/);
  }
  assert.equal(lines.length, count);
});

test('runtime gate runs before each fresh upload and failed gates emit no outputs', async t => {
  const f = await fixture(t);
  const env = {GITHUB_REPOSITORY: context.repository, GITHUB_RUN_ID: context.runId,
    GITHUB_RUN_ATTEMPT: String(context.attempt), GITHUB_OUTPUT: '/unused',
    INPUT_MANIFEST: f.manifest, 'INPUT_CANDIDATE-SOURCE': source, 'INPUT_CANDIDATE-DIGEST': digest,
    'INPUT_GITHUB-TOKEN': 'fake-token-private', ACTIONS_RUNTIME_TOKEN: jwt(tokenClaims)};
  const io = fake(f);
  assert.ok(await runAction(env, io.options));
  assert.equal(io.logs.filter(line => line.includes('runtime_token_gate')).length, 3);
  const denied = fake(f, {error: () => {}});
  assert.equal(await runAction({...env, ACTIONS_RUNTIME_TOKEN: jwt({...tokenClaims, exp: seconds + 60})}, denied.options), null);
  assert.equal(denied.uploads.length, 0);
  assert.equal(denied.outputs.length, 0);
  const errors = [];
  assert.equal(await runAction(env, {...io.options, request: async () => { throw new Error('fake-secret https://host/?sig=private'); },
    error: message => errors.push(message)}), null);
  assert.deepEqual(errors, ['candidate_staging_failed']);
});

test('isolated SDK worker suppresses late secret logs and waits for its exit', async t => {
  const f = await fixture(t);
  const workerPath = path.join(f.root, 'fake-worker.mjs');
  const marker = path.join(f.root, 'finished');
  await writeFile(workerPath, `import {writeFileSync} from 'node:fs';
    process.once('message', () => {
      process.stdout.write('fake-secret https://store.example/?sig=private');
      process.send({id: 1, size: 128});
      setTimeout(() => {
        process.stderr.write('late-fake-secret https://store.example/?sig=private');
        writeFileSync(process.env.FAKE_MARKER, 'exited');
        process.exit(Number(process.env.FAKE_EXIT));
      }, 40);
    });`);
  const captured = [];
  const originalOut = process.stdout.write;
  const originalErr = process.stderr.write;
  const capture = chunk => { captured.push(String(chunk)); return true; };
  process.stdout.write = capture;
  process.stderr.write = capture;
  try {
    for (const code of ['0', '1']) {
      await rm(marker, {force: true});
      const promise = uploadWorker('fake', [], '/unused', {}, {workerPath, env: {FAKE_MARKER: marker, FAKE_EXIT: code}});
      if (code === '0') assert.deepEqual(await promise, {id: 1, size: 128});
      else await assert.rejects(promise, /artifact_upload_failed/);
      assert.equal(await readFile(marker, 'utf8'), 'exited');
    }
    assert.equal(process.stdout.write, capture);
    assert.equal(process.stderr.write, capture);
    process.stdout.write('normal writes preserved');
    assert.deepEqual(captured, ['normal writes preserved']);
  } finally { process.stdout.write = originalOut; process.stderr.write = originalErr; }
});

test('failed, disconnected, invalid, duplicate, timed-out and cancelled workers are rejected', async t => {
  const f = await fixture(t);
  const workerPath = path.join(f.root, 'fake-worker.mjs');
  const modes = [
    `setTimeout(() => { process.stderr.write('late-fake-secret https://host/?sig=private'); process.exit(1); }, 30);`,
    `process.exit(0);`,
    `process.send({id: 1, size: 128, secret: 'private'});`,
    `process.send({id: 1, size: 128}); process.send({id: 2, size: 128});`,
    `process.disconnect(); setInterval(() => {}, 1000);`,
    `setInterval(() => {}, 1000);`
  ];
  for (const code of modes) {
    await writeFile(workerPath, `process.once('message', () => { ${code} });`);
    await assert.rejects(uploadWorker('fake', [], '/unused', {}, {workerPath, timeoutMs: 500}), /artifact_upload_failed/);
  }
  const controller = new AbortController();
  controller.abort();
  await assert.rejects(uploadWorker('fake', [], '/unused', {}, {workerPath, signal: controller.signal}), /artifact_upload_failed/);
  // Real worker rejects malformed IPC before importing the SDK or making calls.
  await assert.rejects(uploadWorker('invalid', [], '/unused', {}), /artifact_upload_failed/);
});

test('scope rejects alternate hosts and malformed repository/run identifiers', () => {
  const env = {GITHUB_REPOSITORY: context.repository, GITHUB_RUN_ID: context.runId, GITHUB_RUN_ATTEMPT: '3'};
  assert.deepEqual(runContext(env), context);
  for (const changed of [{GITHUB_API_URL: 'https://evil.example'}, {GITHUB_REPOSITORY: '../other'}, {GITHUB_REPOSITORY: '1905/other'},
    {GITHUB_RUN_ID: '123/../4'}, {GITHUB_RUN_ID: '0'}, {GITHUB_RUN_ATTEMPT: '0'}]) {
    assert.throws(() => runContext({...env, ...changed}));
  }
});
