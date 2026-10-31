# Lobocode project memory

## Supported scope — 2026-10-29

User decision: support only `qwen38-q6`, only on RunPod or Vast GPUs. No model choice (no `q6`/`q8`) and no local runs. The authorized cleanup is complete: one catalog entry, no local inference/provider/commands, no full-image model packaging. Legacy saved configs migrate; explicit retired choices fail validation. Cloud ownership safety remains. Keep user model files, the installed app and optional CLI unchanged. Historical Go source is outside this cleanup.

## SSH only, SECURE default, startup retry — 2026-10-31

User decisions (2026-10-31): remove the Cloudflare connection (SSH only), default RunPod cloud `secure`, retry a failed start up to 10 attempts with a reason per attempt, show the agent's failure reason, and release image, app and CLI together. Plan: `plans/2026-10-31-startup-retry/plan-v1.0.html`. Cause: the user's legacy config ran in Cloudflare mode; the pod tunnel never connected (error 1033) and the app showed only "pod is gone". The 2026-10-31 leak audit found no user Cloudflare token, domain or account key in git history, GitHub text, release assets, the Homebrew tap or the public image layers.

## Image 0.2.5 speculative decoding — 2026-10-31

The pod agent starts `llama-server` with `--spec-type draft-mtp,ngram-mod --spec-draft-n-max 5` (`LOBO_SPEC`, default `mtp-ngram`). Public `latest-qwen38-q6` digest: `sha256:7b53f3158c91042cf8ce6375c9a4f24918f894db7b353d32025e574aa8a51604`, built from `548ac7aee405c9996c22b9f8ce82effd158b5406`. Through `lobo up` on an RTX 5090: prose 55.6 → 102.2 tok/s, code 54.5 → 147.9, 25k-token document 50.4 → 212.4, code edit 54.4 → 504.4. VRAM +1.5 GB. The app and optional CLI are unchanged. Rollback: re-tag `sha256:3d64617400c59420f0aea21a0bb6c0ac83376a8ee377dd2710ef63d5fc41fc0a` as `latest-qwen38-q6`. See [speculative decoding](docs/speculative-decoding.md) and [results](plans/done/2026-10-31-spec-decoding/results.html). PR #5 (merged, not yet in an image): the free-VRAM boot check adds 1,536 MiB for MTP modes (25,487 MiB). Rebuild the image to ship it. Under `mtp-ngram`, output is not reproducible at temperature 0; do not use byte-equal output as a regression test for W1/W2.

While a paid bench pod is up, check it every minute (`tools/pod_watch.py`) and terminate it when its work ends.

## App 0.2.4 cleanup delivery — 2026-10-31

Cleanup and review fixes are merged and pushed to `master`. App `0.2.4` and the public GPU image are published from `240989574fbc1c9029e229864e2182b74368dbb1`. All 575 build-source files matched the source archive. DMG SHA-256: `9b1907e66878625d140b8b6f857e5f42ee5518f206264fdb86a657822bec5bf8`. Public `latest-qwen38-q6` digest: `sha256:3d64617400c59420f0aea21a0bb6c0ac83376a8ee377dd2710ef63d5fc41fc0a`. Anonymous downloads, asset checksums, image source and mounted DMG checks pass.

Validation: 452 workspace tests, two added review regression tests, the separately run SSH integration, 99 app checks, 21 UI tests and six native setup/migration cases passed. Core/CLI and app suites passed again after review fixes. No live GPU inference or local installation ran. App signing remains ad-hoc, without notarization. GitHub Actions remains disabled; the optional CLI latest release remains `v0.1.0`. See [cleanup results](plans/done/2026-10-31-retired-models/results.md).

## App 0.2.3 delivery — 2026-10-31

PR #2's cleaned review branch is merged and pushed to `master` at `13aa8d64bd3874dcf61464050cdc6150b87d1304`. The original PR is closed because its commits were rewritten before integration. App 0.2.3 is published from that exact source. Feesh passed 548 workspace tests, 21 UI tests and 25 image-verifier checks; Air passed 97 app library tests, two binary tests and six native setup checks. Two workspace integration tests remain ignored. No new GPU inference ran. Public image and DMG checks passed. DMG SHA-256: `309f3581ca23b365e2954616ad3e12153e6766f6ca35c5003a8d3231401dcede`. The installed app and optional CLI remain unchanged. GitHub Actions remains disabled. See [review results](docs/pr2-review-results.md).

## Contributor PRs #2 and #3 — review resumed 2026-10-31

The user asked to review, fix, merge and release PRs #2 and #3 from SandQuattro (fullsend). Reviews: self, China and Rival on each PR. Session trailers were stripped from the contributor commit messages before merge.

PR #3 merged on October 26. PR #2 remained on `review/pr-2` after a failed native migration test; the earlier heading incorrectly claimed both were merged. The resumed review fixed the persisted model at the backend save boundary. Workspace, app and native setup checks pass on Feesh and Air. GitHub Actions is disabled and workflow files are removed. The public `latest-qwen38-q6` image is verified at `sha256:0491a075ad2c7f2c3f29cd2dffb0d58a8139ed1963905ccbd4378fa3ae9a5637`. See [review results](docs/pr2-review-results.md) for validation and remaining limits. App 0.2.3 publication is complete.

PR #3 opens the SSH control socket as `ssh.sock` relative to the cloud directory, so long config paths no longer fail preflight. Review fix: `dtolnay/rust-toolchain` is pinned to a commit.

PR #2 adds the catalog model selector and makes `qwen38-q6` (huihui-ai Qwen3.8-27B abliterated, Q6_K, cloud only) the app cloud default. It runs from a thin image and downloads 84 verified 256 MiB chunks. The default CDN `https://models.stealthcopilot.io` belongs to the contributor, not to this project. User decision 2026-10-26: keep that CDN and fall back to Hugging Face byte ranges of the original file when it fails. The pinned chunk table matched the complete Hugging Face file (84/84 chunks, whole-file SHA-256) in a streamed check on feesh9. This repository holds no R2 secrets for those chunks. Other review fixes: catalog models in the CLI wizard, accepted retried chunk PUTs, workflow actions pinned to SHAs. The contributor reported live Vast CLI and Mac app runs reaching Ready in 222–264 s at ~116 MB/s; no live GPU run was done during this review.

## Startup diagnostics release and local update — 2026-10-05

The user explicitly requested durable startup logs, re-release and local update. This authorized app 0.2.2 publication and installation; older app holds below are historical. CLI release, Q8/image rebuilds, updater work, local inference and OpenCode launch remain outside this task. Rust compilation/tests ran on Dell or hosted macOS under the 2026-10-03 host correction. Feesh ran UI checks; this Mac only handled source, orchestration and the authorized installation.

App `0.2.2` is published and installed at `/Applications/lobocode.app`, source `24b30284063a49a3492e029b38c8a34cf32ae6a5`. Exact master CI `37271794244` and release `37272648758` passed. App tests: 95 Rust library tests plus two binary tests, 20 UI tests and six native setup/migration cases. Anonymous asset, checksum, arm64 signature, mounted contents and no bundled CLI checks passed. DMG SHA256: `c080fa94f04dfb9e389836a067606b273a91ae48598a8ae4815d47a3ec7b3b32`.

Real logs now persist at `~/.config/lobo/config.app-logs/app.jsonl` (0600 file, 0700 directory, 2 MiB plus three archives). Launch/config/snapshot records and configured-secret absence were verified after installation. Config, preferences and CLI hashes are unchanged. The old RunPod step-2 stall has no durable trace, so its exact cause remains unknown. Current provider checks found zero instances. No new GPU rental occurred. Local Computer Use could not start; hosted native no-scroll checks passed. Ad-hoc signing remains; notarization and broader native acceptance remain unverified. See [diagnostics results](plans/done/2026-10-05-startup-diagnostics/results.md).

## Cloud-only app release — 2026-10-02

Latest request: completely remove local execution from the Mac app, full auto, then release it. This explicitly supersedes the previous app release hold. Keep CLI local support and existing model/config/runtime files. Use Feesh or hosted CI for builds and backend tests; actual native UI checks remain authorized. Do not publish the Rust CLI, rebuild Q8, resume updater work or import dirty Task11 files. Implementation and app-v0.2.1 release evidence: [cloud-only plan](plans/done/2026-10-02-cloud-only-app/plan-v1.0.md). Older checkpoints below are historical where they conflict.

App 0.2.1 is published from `358b9cbd331902722c61ac4ffde0acc0949c1497`. Exact master Rust CI `36991557535` passed, including 81 Rust app tests, 19 UI tests and six native setup/migration cases. Release run `36992628194` attempt 2 passed against the published assets. Independent anonymous downloads, source receipt, arm64 signature, mounted DMG layout and no bundled CLI checks passed. DMG SHA256: `50af54392577b6f51b5e78a7280b76ec69a7c2b5a92e7a12d4c17884dd52704b`. The first publication attempt hit a draft tag-lookup 404; verified publication by release ID recovered it. Future workflow lookups use authenticated release listing and numeric IDs. Installed app and CLI remain unchanged. App signing is ad-hoc; notarization, drag position, Dock/Command-Tab, live OpenCode and broader loading telemetry remain unverified.

## Current delivery instruction — 2026-10-01

The user explicitly requested commit, merge and README updates after cloud E2E.
Merge the completed, committed app-runtime work into `master` and push it.
This supersedes the earlier master-merge hold for that completed work.
Preserve unfinished Task11 files in the app-runtime worktree; do not include
them as verified work. Keep app/CLI releases, local inference, Q8 and updater
work held. Use Feesh or hosted CI for backend/build checks, not Dell or this Mac.
Q6 image publication and bounded TUI/Mac cloud lifecycle checks passed.
Loading-memory progression and broad native acceptance remain unfinished.

User corrections saved on 2026-09-30.

- This is a public application. Users must not need the developer's bucket,
  private hosts, credentials or personal configuration.
- The GPU pulls a complete public Docker image containing the agent, inference
  runtime, SSH daemon and selected model weights. Q8 and Q6 use separate images.
- Always resolve the latest public image for the selected model at each new cloud
  start. Pass the newly resolved digest to the GPU provider. Do not silently use
  an older cached image or select an image based on the installed DMG version.
- The DMG contains the standalone Mac app. It neither installs nor requires the
  CLI. The optional CLI installs separately through Homebrew.
- App windows must feel native, move normally, have rounded corners and fit
  without scrolling. Verify actual native behavior, not only browser renders.
- Run only UI checks and builds on this Mac. Run backend, container and lifecycle
  checks on Dell. Do not run model inference on this Mac.
- Before local Start, assess current Mac memory for the selected model and
  context. If memory is insufficient or cannot be checked, show an error and
  block startup. Do not treat installed RAM or an on-disk model as proof that
  it fits. User correction: 2026-09-30. The new
  [memory-check spec](plans/2026-09-30-local-memory-check/spec.md) is approved.
  The guard is implemented at `b9a1966`, with documentation checkpoint `597fb1c`.
  It is integrated into the isolated app-runtime branch. Native E2E and master
  merge remain pending. The queued-Start selection race is fixed in the
  isolated app-runtime branch; hosted app regressions pass. The installed
  app has no guard yet.
- Release remains on hold. Normal branch pushes, CI and direct merge after
  required checks are authorized by full auto and the approved app spec.
  Do not publish images, tag a release or rent GPUs.
- Keep updater work and image builds paused. Finish the approved app fixes
  with unit/build checks first. E2E is deferred at the user's request.
- Keep mistakes and unresolved acceptance visible in
  [docs/implementation-mistakes.md](docs/implementation-mistakes.md).

Current state: SSH transport, complete public-image startup and fresh latest-image
resolution are implemented locally. Agent/core/CLI fixture checks and clippy pass
on Dell. Native setup/save and all browser layouts pass. Native drag automation
did not observe movement; manual acceptance remains open. Full image builds are
paused at the user's request, and images remain unpublished. The installed app and verified
`bin/lobocode.dmg` now contain `2bd9538`.
The CLI, config and model files remain unchanged. Live provider acceptance is held.
The user approved the [written spec](plans/2026-09-30-public-pod-images/spec.md)
on 2026-09-30. Continue the authorized implementation and preserve existing work.
The [earlier plan](plans/2026-09-30-public-pod-images.md) remains as a record.

## Session checkpoint — app setup and visibility

The user paused the earlier feature queue on 2026-09-30. Keep GPU image builds,
validators and updater work paused until asked to resume. Preserve the memory
worktree and its committed documentation checkpoint `597fb1c`.

New requests: configure or repair OpenCode from the app, show measured memory
through loading and normal use, and appear in the macOS app switcher. The user
also reported misleading `0 / 0` token rates while a request was still reading
its prompt. Track these additions in
[the app setup and visibility spec](plans/2026-09-30-app-setup-memory-visibility/spec.md).
The expanded spec and implementation plan are approved. Implementation is in progress;
acceptance remains pending.

The local OpenCode config now uses the running Q6 model at
`http://127.0.0.1:8931/v1`, a private credential-file reference, and the existing
`lobo` agent as its default. Authenticated model metadata and config loading pass.
No new inference request was sent. Existing user-started inference was left alone.
The observed long wait was prompt processing: an 18-character user message became
roughly 47,000 input tokens. The app's zero rates did not prove the server was idle.
Do not save credentials in project documents.

The user then reported a Vast error during Local Stop. The old app called
all-provider operations and could delete every listed runtime. The app-runtime
branch now isolates Start/status/Stop by recorded identity, including pending
operations and saved connections. Hosted unit/build checks pass; native E2E is
pending. Do not contact a real cloud provider to reproduce the old error.

The user explicitly authorized a limited real-model E2E after these fixes on
2026-09-30: use direct scripts, not OpenCode; keep token counts small, measure
speed and inspect native visuals. Planned limit: two sequential requests, each
at most 64 input and 32 output tokens. This is the only exception to the Mac
inference restriction for this feature. The memory guard must pass normally.
Backend/full/lifecycle suites still run off this Mac. No cloud rental or release.
The user has now quit OpenCode and stopped the local runtime; leave it stopped
until the authorized post-fix E2E.

The expanded app spec was approved on 2026-09-30. The user then said “e2e test
later”: implement first with unit/build checks, and defer native E2E plus the
bounded inference benchmark to the later validation stage. The memory branch
must be integrated into the new isolated feature branch without prematurely
merging unverified code to master. Image builds and updater remain paused.

Current host constraint (2026-09-30): the user reported no available Mac memory during the app-runtime fixes. Keep the Mac to code edits. Use Dell/hosted macOS for builds and unit checks. Local install and all E2E remain pending. Do not start a model or rent a GPU.

## Direct model diagnostic exception — 2026-09-30

Latest user instruction resumes one direct local comparison without OpenCode: exactly 47,000 synthetic input tokens, Q6, context 65,536, at most 32 output tokens. Use the hosted build's normal memory guard and isolated task-owned runtime. This supersedes the earlier 64-token input cap and E2E hold for this diagnostic only. Native UI E2E, broad local tests, installation and release remain deferred. Preserve personal config, installed app and CLI. See `plans/2026-09-30-app-setup-memory-visibility/plan-v1.1.md`.

Diagnostic result: hosted build `b5617ae` failed normal memory admission before model load (26.7 GiB required, 9.4 GiB available after reserves). Zero generations. Owned cleanup passed. The same-size comparison is pending; do not claim model success or start the conditional OpenCode investigation from this result. The ready script and fake checks are recorded in the v1.1 plan and results.

## Cloud E2E priority — 2026-09-30

Latest testing instruction: “local test later. do cloud e2e”, after asking about TUI and Mac. RunPod test rentals and cloud-client E2E are authorized; local inference stays deferred. Use Q6, one GPU at a time, isolated config/state, bounded direct prompts without OpenCode and exact owned cleanup. App/CLI release remains held. Image publication is authorized below. Track plan v1.11.

Historical Dell image checkpoint: a complete Q6 candidate was built and independently verified from source `528322e`, digest `sha256:83db6998106ca53b67b2bcec9cba445f91924f942b664958eebb742a8539a2d5`, 24.69 GB. Artifact root: `/storage/lobocode-cloud-e2e-q6-528322e-20260930/artifacts`. Verification-only CI run `36730862442` passed at verifier revision `505c255`; publication steps were skipped. That image remains unpublished. At this checkpoint both required public tags returned 404; the later Feesh publication and TUI result below supersede that status. The Dell builder is stopped; original Q8 jobs remain paused. Keep prepared native Task11 changes separate.

## RunPod execution approved — 2026-10-01

Latest host correction: the user said “dont use dell! use feesh for all of that!” on2026-10-01. Use the configured `feesh9` SSH host for Lobocode build/publication/backend and cloud TUI work. Dell transfer cleanup passed; its image and ten completed data artifacts (5GiB) plus manifest are preserved. Do not restart work on Dell. Native Mac cloud UI checks and RunPod GPU inference retain their authorized scope. This supersedes earlier Dell execution directions in this file. Current plan is [v1.11](plans/2026-09-30-app-setup-memory-visibility/plan-v1.11.md). Feesh Q6 build, public-image verification, the controlled TUI repeat and bounded native Mac cloud lifecycle passed. Broader native and feature acceptance remains pending.

The user explicitly said “do test e2e on runpod. do image etc. docker is available on dell”. This authorizes publishing the verified Q6 candidate and promoting `latest-q6`, then executing the planned TUI and Mac cloud tests. It supersedes the earlier image-publication and cloud-rental holds for this work. Use one GPU at a time and exact task-owned cleanup. Local inference, OpenCode launch, app/CLI releases, Q8 work and updater work remain held. Do not request the same approval again.

Historical Dell transport checkpoint (superseded by v1.10): direct monolithic upload was too slow; regctl chunks failed with HTTP416. A single CI artifact then failed authentication after 3,609 seconds despite a valid twelve-hour runtime token. Independent 512 MiB transfer parts are implemented in `14f9f8c`; Dell checks pass. BuildKit's empty `ingest/` directory is accepted by correction `aa32c246`, with 22 passing Dell fixtures. Run36812995172 finalized its manifest and two data parts before controlled cancellation. Attempt2 reused all three original artifact IDs and passed nine terminal-log token gates. The user's host correction cancelled attempt2. Exact runner/container/scratch/ACL cleanup passed; ten data parts and the manifest remain until normal expiry. Do not resume Dell transfer.

Feesh Q6 build passed in 12m14s at source `528322e468deeab23f8c0b74e804510d98cca071`. New digest: `sha256:aa590424f7862b5664c943cc5753bb72d447cbdaec5ed03a66642e138e3857f4`. Independent verification passed all 29 decoded layers, six shard hashes and 851 tensors. The original model and build cache are preserved; the bounded build worker is stopped. Feesh CLI version/help also pass. Publication run `36826266337` passed at workflow `aa32c246`; all 47 transfer artifacts were removed after publication. Independent anonymous versioned-tag and `latest-q6` checks passed on 2026-10-01 at 07:06 UTC. The TUI and bounded native Mac cloud results are recorded below. A complete image removes bootstrap downloads but does not guarantee cached layers on a new RunPod host. Measure actual startup; do not promise instant starts.

## Real RunPod TUI diagnostic — 2026-10-01

The controlled v1.11 repeat passed on the published Feesh Q6 image: context 65,536, exactly 47,000 input tokens, `cache_n=0`, two output tokens and expected answer `4`. First content took 24.040 seconds; prompt processing was 2,186.677 tok/s. Reported output speed was 45.271 tok/s over only two tokens. Do not present that as sustained generation speed or as a replay of the private OpenCode request.

Preserve the first request's validator rejection as failed acceptance. Corrected fixtures use the flat `generation_settings/n_predict` response key and do not expect unsupported `n_cmpl` metadata. Python 3.10 expiry parsing accepts Rust nanoseconds while ownership checks retain original bytes. The full fake suite passed on Feesh in 1.968 seconds before the controlled repeat.

Exact cleanup passed at 07:35:07 UTC: one DELETE, matching pod GET 404, saved desired state removed, helper lock released, both listeners absent and matching boot keys removed. No process signals were sent. Ready/status fit 80 columns; startup detail clipping remains unresolved. Evidence: Feesh `/srv/lobo-cloud-client-e2e-20261001/evidence/large-request-002/summary.json` and Mac `/tmp/lobo-cloud-mac-e2e-20261001.a542eX/tui-cleanup.json`.

The later native Mac cloud lifecycle passed as recorded below. Local Metal and OpenCode are untested by this cloud diagnostic. App/CLI releases, Q8 and broader feature acceptance remain held. Preserve dirty Task11 work.

## Actual native Mac cloud check — 2026-10-01

Hosted app `b5617ae` passed Cloud Start → Ready → one direct short request → native Stop → Off. It used 25 input tokens, `cache_n=0`, two output tokens and the expected answer `4`. First content took 1.107 seconds. Two output tokens do not establish sustained generation speed. No OpenCode or local inference ran.

Ready occurred between the last Boot observation at about 21m11s and the first Running capture at 22m30s after the Start capture. No Mac `boots.jsonl` or exact Ready timestamp was captured. The TUI startup took 230.178 seconds. The Mac provider explicitly reported `Downloading` at 15 minutes; download bytes and cache hits were not measured. Do not attribute the difference to the client or promise fast startup from this evidence.

Controls fit without scrolling, and rounded corners were visible. Native Ready showed 22.8 / 31.8 GB GPU memory. Post-request idle gauges showed `0 / 0`; live rate updates were not verified. Actual drag, global Command-Tab, Dock, local Metal, OpenCode, Q8 and broader telemetry/feature acceptance remain pending.

The exact Mac pod returned GET 404 at 08:01:30 UTC. Native Quit and guarded preference restoration passed at 08:01:51 UTC. Receipt: `/private/tmp/lobo-mac-cloud-launch-512mj3id/prefs-restored.json`. Read-only local cleanup verification passed at 08:03:40 UTC: no task helper/tunnel processes, listeners, desired state, app ownership, boot keys or SSH socket remained; both locks were released. Receipt: `/tmp/lobo-cloud-mac-e2e-20261001.a542eX/post-stop-local.json`. App/CLI release stays held; preserve the installed app, optional CLI and dirty Task11 work.
