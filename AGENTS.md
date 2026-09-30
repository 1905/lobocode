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
- Release remains on hold from the user's explicit instruction. Do not infer
  permission to push, publish images, tag a release or rent GPUs from a local
  install, documentation update or implementation correction.
- Keep mistakes and unresolved acceptance visible in
  [docs/implementation-mistakes.md](docs/implementation-mistakes.md).

Current state: SSH transport is implemented locally. Complete public-image
distribution and latest-image resolution have partial, uncommitted implementation.
The first Dell compile check passed for an earlier source snapshot. The latest
changes, full image builds and live provider acceptance remain unverified.
The installed app still uses `3dec9ad`; it does not contain the image changes.
The [written spec](plans/2026-09-30-public-pod-images/spec.md) is pending review
under the planning workflow introduced on 2026-09-30. Preserve existing work.
The [earlier plan](plans/2026-09-30-public-pod-images.md) remains as a record.
