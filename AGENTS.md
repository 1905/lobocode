# Lobocode project memory

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

Latest user instruction: “local test later. do cloud e2e”, after asking about TUI and Mac. RunPod test rentals and cloud-client E2E are now authorized; local inference stays deferred. Use Q6, one GPU at a time, isolated config/state, bounded direct prompts without OpenCode and exact owned cleanup. App/CLI release remains held. Prepare the complete image before resolving the separate earlier public-image promotion hold. Track plan v1.8.

Complete Q6 candidate is built and independently verified on Dell: source `528322e`, digest `sha256:83db6998106ca53b67b2bcec9cba445f91924f942b664958eebb742a8539a2d5`, 24.69 GB. Artifact root: `/storage/lobocode-cloud-e2e-q6-528322e-20260930/artifacts`. Verification-only CI run `36730862442` passed at verifier revision `505c255`; publication steps were skipped. The image is unpublished, both required public tags return 404, and live cloud acceptance remains pending. New builder is stopped; original Q8 jobs remain paused. Keep prepared native Task11 changes separate.

## RunPod execution approved — 2026-10-01

The user explicitly said “do test e2e on runpod. do image etc. docker is available on dell”. This authorizes publishing the verified Q6 candidate and promoting `latest-q6`, then executing the planned TUI and Mac cloud tests. It supersedes the earlier image-publication and cloud-rental holds for this work. Use one GPU at a time and exact task-owned cleanup. Local inference, OpenCode launch, app/CLI releases, Q8 work and updater work remain held. Do not request the same approval again.

Current plan is v1.8. Direct monolithic upload was too slow; regctl chunks failed with HTTP416. A single CI artifact then failed authentication after 3,609 seconds despite a valid twelve-hour runtime token. No image was published. Implement independent 512 MiB artifacts with completed-part reuse, exact reconstruction and full verification before hosted publication. Use a task-owned scratch mount under Dell `/storage`; its Docker root lacks space for a full part copy. Keep the exact Q6 image unchanged and prove completed-part reuse before another long wait. No live cloud success is established yet. A custom image removes bootstrap downloads but does not guarantee cached layers on a new RunPod host. Measure actual startup; do not promise instant starts.
