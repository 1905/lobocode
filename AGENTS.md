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
  [memory-check spec](plans/2026-09-30-local-memory-check/spec.md) is pending
  approval; this guard is not implemented yet.
- Release remains on hold from the user's explicit instruction. Do not infer
  permission to push, publish images, tag a release or rent GPUs from a local
  install, documentation update or implementation correction.
- Keep mistakes and unresolved acceptance visible in
  [docs/implementation-mistakes.md](docs/implementation-mistakes.md).

Current state: SSH transport, complete public-image startup and fresh latest-image
resolution are implemented locally. Agent/core/CLI fixture checks and clippy pass
on Dell. Native setup/save and all browser layouts pass. Native drag automation
did not observe movement; manual acceptance remains open. Full image builds are
still running, and images remain unpublished. The installed app and verified
`bin/lobocode.dmg` now contain `2bd9538`.
The CLI, config and model files remain unchanged. Live provider acceptance is held.
The user approved the [written spec](plans/2026-09-30-public-pod-images/spec.md)
on 2026-09-30. Continue the authorized implementation and preserve existing work.
The [earlier plan](plans/2026-09-30-public-pod-images.md) remains as a record.
