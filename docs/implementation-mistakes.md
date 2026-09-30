# Implementation mistakes and corrections

Updated: 2026-09-30. Development record, not release notes.

The implementation treated a developer setup as if it were suitable for a public
application. That was the main mistake. Users must not need the developer's
bucket, private hosts, credentials or configuration.

## 1. Native UI acceptance was incomplete

The user reported an immovable window, scrolling, clipped content, an unsuitable
layout and missing rounded corners. Browser screenshots and successful builds
did not establish native window behavior. Native startup also exposed a tray
mutex deadlock that earlier checks missed.

Corrections already implemented: native title bars, measured content sizing,
compact Settings tabs, rounded corners, and tray mutations on the main thread.
Native setup/save and all 23 browser layouts pass. A new native drag check did
not observe movement through the input automation. The cause is unresolved;
manual movement acceptance remains open. Rounded corners were inspected in the
actual native window. Do not describe all native behavior as verified.

The user also limited testing on the Mac because of insufficient memory. Since
that correction, further backend and lifecycle checks belong on Dell or an
authorized remote host. UI-only checks and builds may run on the Mac.

## 2. Cloud setup required a domain

The original cloud connection depended on a named Cloudflare tunnel. That made a
domain and tunnel token prerequisites for ordinary users. This was unnecessary
for a desktop client connecting to its rented GPU.

Correction implemented locally: automatic SSH forwarding, dedicated per-instance
keys, pinned server identity and a loopback endpoint. New setup no longer asks
for a domain or tunnel token. Dell SSH fixtures pass. Live RunPod/Vast acceptance
has not run for this implementation.

## 3. Removing the domain did not remove the private infrastructure dependency

The SSH implementation retained `LOBO_BUCKET_URL` in setup. Cloud startup still
resolved an agent release and model files from that configured bucket. Asking a
public-app user to configure this storage was the wrong distribution design.

The README documented that requirement instead of removing it. The implementation
was saved and installed as a development build, but that did not make cloud use
ready for a new user. The matching new remote agent was not published either.

User correction: everything the GPU needs must be in the Docker image it pulls.
The replacement design puts the agent, inference runtime, SSH daemon and model
weights in public, model-specific images. The app selects the image. No user
bucket, personal download server or manual agent install is part of onboarding.

Status: implemented in the Rust source under the user's approved
[spec](../plans/2026-09-30-public-pod-images/spec.md). The source removes private
storage from normal startup. Settings and the wizard no longer ask for a bucket.
All 434 workspace checks and clippy pass on Dell. Both opt-in checks (SSH
integration and runtime archive) and three explicit Go interoperability checks
also pass. Image builds are still running.
This is not an available public release.

The first replacement plan also tied image selection to the app release. The
user corrected that: each new cloud start must resolve the latest public image
for the selected model. The app must not silently use an older cached image.
Tests now cover changing registry manifests, two starts with different digests,
and registry failure after a previous success with no extra provider create.
The provider receives the resolved digest. Local mode avoids registry access.

CI now builds both model images before the app/CLI release jobs. Stable latest
tags move only after both builds and anonymous manifest checks succeed. CI needs
a Linux AMD64 runner with 200 GiB free Docker storage per concurrent job.
Actual anonymous layer pulls and live providers remain release acceptance.

## 4. The DMG and CLI delivery paths were confused

The user requested a fresh Mac install. The app was rebuilt from `3dec9ad` and
installed in `/Applications/lobocode.app`. Its signature and native launch were
verified. Existing config and models were preserved. The local DMG was then
rebuilt and verified against that installed binary.

During the later “update all” request, a manual Rust CLI installation was started.
The user clarified that the CLI installs separately through Homebrew. The command
was stopped before its install step. No CLI binary was changed.

Required delivery contract: the DMG contains the standalone desktop app. It does
not require or install the CLI. Homebrew owns the separate CLI installation.

The public-image source update was installed locally from `2bd9538`. The matching
DMG is 6,128,059 bytes; its app binary matches the installed app and its signature
verifies. The app opens idle. CLI/config hashes and model file metadata remain
unchanged. Cloud image publication and live acceptance are still pending.

## 5. Local completion was not public release readiness

“Implemented” described local source changes and fixture checks. It did not mean
that a new user could download a public DMG and start a cloud model. The installed
app, published GPU image, model distribution and release artifacts must agree.

The release hold remains active. No new image, agent or public release was
published for the domain-free implementation. Do not remove this limitation from
handoffs merely because a local build passes.

## 6. Local memory rejection happens too late

The app currently offers local Start without checking available memory. The
backend checks a fixed GPU budget after runtime preparation and supervisor
startup. That check ignores memory used by other apps and the selected context
size. A downloaded model is not proof that this Mac can load it safely.

User correction: inspect Mac memory first. If the selected model cannot fit,
show an error instead of starting it. The
[memory-check spec](../plans/2026-09-30-local-memory-check/spec.md) records fresh
physical-memory and Metal checks, context-dependent estimates, startup rejection
and a second check before loading. It is pending approval, not an implemented
fix. This Mac remains limited to UI checks with no inference.

## Acceptance required before public delivery

- Install from the DMG with a fresh configuration. No CLI installation is required.
- Enter only a GPU provider key for cloud setup. Generate the client API key automatically.
- Select the correct public Q8 or Q6 image without a bucket or image field in normal setup.
- Pull both complete images anonymously and verify their contents and model hashes.
- Move the public image tag and verify the next start uses the new digest. If the
  registry cannot resolve it, fail before renting instead of using a stale image.
- Start each supported model on RunPod and Vast. Verify streaming, tool calls,
  reconnect, Stop, cancellation, failed-boot cleanup and automatic shutdown.
- Verify actual native window movement, rounded corners and every view without scrolling.
- Verify local memory rejection happens before downloads or processes start;
  an unavailable measurement must also block Start.
- Publish the compatible GPU images before distributing the app and Homebrew release.

Backend/container acceptance must run away from the development Mac. Publication
and GPU rental require the release hold to be lifted; these checks are pending.
