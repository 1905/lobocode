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
  memory-check spec and implementation are in the separate
  `feat/local-memory-check` worktree at `b9a1966`. Native E2E, final review and
  merge remain pending. The installed app does not include this guard yet.
- Release remains on hold from the user's explicit instruction. Do not infer
  permission to push, publish images, tag a release or rent GPUs from a local
  install, documentation update or implementation correction.
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
worktree and its uncommitted documentation.

New requests: configure or repair OpenCode from the app, show measured memory
through loading and normal use, and appear in the macOS app switcher. The user
also reported misleading `0 / 0` token rates while a request was still reading
its prompt. Track these additions in
[the app setup and visibility spec](plans/2026-09-30-app-setup-memory-visibility/spec.md).
This spec is pending review; these additions are not implemented.

The local OpenCode config now uses the running Q6 model at
`http://127.0.0.1:8931/v1`, a private credential-file reference, and the existing
`lobo` agent as its default. Authenticated model metadata and config loading pass.
No new inference request was sent. Existing user-started inference was left alone.
The observed long wait was prompt processing: an 18-character user message became
roughly 47,000 input tokens. The app's zero rates did not prove the server was idle.
Do not save credentials in project documents.

The user then reported a Vast error during Local Stop. Source inspection confirms
that app Stop and status call all-provider operations; Stop can delete every
listed runtime. The new spec must isolate Start/status/Stop by runtime identity,
including pending-operation and saved-connection cleanup. Do not contact a real
cloud provider to reproduce the error.

The user explicitly authorized a limited real-model E2E after these fixes on
2026-09-30: use direct scripts, not OpenCode; keep token counts small, measure
speed and inspect native visuals. Planned limit: two sequential requests, each
at most 64 input and 32 output tokens. This is the only exception to the Mac
inference restriction for this feature. The memory guard must pass normally.
Backend/full/lifecycle suites still run off this Mac. No cloud rental or release.
The user has now quit OpenCode and stopped the local runtime; leave it stopped
until the authorized post-fix E2E.
