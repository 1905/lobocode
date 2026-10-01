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
not observe movement through the input automation. A controlled native position
change was measured correctly. Exact-process diagnostics found that the drag
tool's mouse press reports `buttons: 0`, so it does not establish a normal held
button. Manual movement acceptance remains open. Rounded corners were inspected
in the actual native window. Do not describe all native behavior as verified.

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

The full Q8 build verified and split the model, then failed while downloading
the Apache license text. The license is now bundled in the build context and
copied before model processing. Its canonical ASF source has SHA-256
`cfc7749b96f63bd31c3c42b5c471bf756814053e847c10f3eb003417bc523d30`.
The rebuild reuses the verified original model bytes. Full image validation
remains in progress; this fix does not establish public release readiness.

“Implemented” described local source changes and fixture checks. It did not mean
that a new user could download a public DMG and start a cloud model. The installed
app, published GPU image, model distribution and release artifacts must agree.

The release hold remains active. No new image, agent or public release was
published for the domain-free implementation. Do not remove this limitation from
handoffs merely because a local build passes.

## 6. Local memory rejection happens too late

The app offered local Start without checking available memory. The backend
checked a fixed GPU budget after runtime preparation and supervisor startup.
That check ignored memory used by other apps and the selected context size.
A downloaded model is not proof that this Mac can load it safely.

User correction: inspect Mac memory first. If the selected model cannot fit,
show an error instead of starting it. The approved
[memory-check spec](../plans/2026-09-30-local-memory-check/spec.md) now has a shared
physical-memory and Metal guard. It checks before runtime preparation and again
before loading. The app displays its model/context estimate, rejects stale
results, and keeps Cloud as an explicit choice.

Dell checks pass: 449 workspace tests, with two existing opt-in tests ignored,
and Clippy. The new denial test first reproduced an unwanted fake-runtime start;
it now proves insufficient and unknown memory cause no startup side effects.
Native fixture compilation and 11 frontend tests pass. Native E2E and hosted
app checks are still pending. This Mac remains limited to UI checks and builds.
The memory reserve is a conservative policy, not a measured inference peak.

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

## Research: desktop updates through GitHub — 2026-09-30

Status: researched, not implemented or tested. No release was published.

The public `1905/lobocode` repository can host desktop updates without a private
server. The app already uses Tauri 2.12.0. It has no updater dependency or
configuration. The current release workflow runs GoReleaser before the DMG job.
The published `v0.1.0` assets currently contain CLI archives and checksums only.

Use Tauri's official updater with a static manifest at
`https://github.com/1905/lobocode/releases/latest/download/latest.json`.
Enable `bundle.createUpdaterArtifacts`, embed the updater public key, and keep
`TAURI_SIGNING_PRIVATE_KEY` and its password in GitHub Actions secrets.
Keep a secure backup of the signing key. macOS updates use a signed `.app.tar.gz`
archive; the DMG remains the first-install download.
See the [Tauri updater guide](https://v2.tauri.app/plugin/updater/).

Proposed implementation:

- Add `tauri-plugin-updater` to `app/src-tauri/Cargo.toml` and register it in
  `lib.rs`. Keep update operations in Rust alongside the existing controller.
- Add automatic checks after launch and every six hours, plus a manual check.
  Show version, download progress, retry and an install/restart action in a
  compact native view. Preserve the no-scrolling requirement.
- Serialize installation against Start, Stop and Quit. Defer installation while
  a runtime is active or startup/cleanup is pending. The existing controller's
  successful `quit()` cancels its polling loops; installation failure must not
  leave the open app in that state.
- Extend `.github/workflows/release.yml` to assemble a draft release first.
  Publish it only after the CLI, DMG, update archive, signature and manifest are
  complete. Coordinate Homebrew publication with that final step.
- Use `tauri-apps/tauri-action` to generate and upload `latest.json`. Set the
  actual release tag so archive URLs point to that version. Match its draft
  setting to the existing release. Pin the chosen action revision.
  These options are documented in the [official action](https://github.com/tauri-apps/tauri-action).
- Build an explicit Apple Silicon target. Add an Intel manifest entry only if
  an Intel app is also built and validated. Keep the tag, app version and
  manifest version consistent; retain the Makefile's version override.

Every release selected as Latest must contain the desktop update assets.
GitHub's [latest-asset URL](https://docs.github.com/en/repositories/releasing-projects-on-github/linking-to-releases)
does not search older releases for a missing file. Publish a higher patch version
for a corrective update instead of relying on automatic downgrades.

Updater signatures do not replace Apple Developer ID signing and notarization.
The current app uses `signingIdentity: "-"`. Ad-hoc signing still requires users
to allow installation in macOS security settings. Use Developer ID and
notarization for public distribution without those warnings.
See [Tauri's macOS signing guide](https://v2.tauri.app/distribute/sign/macos/).

Acceptance remains pending: signed version A to B, wrong signature, interrupted
download, unavailable GitHub, wrong architecture, installation failure and
restart. Native UI checks may run on this Mac. Installation/lifecycle acceptance
needs an authorized remote macOS host; Linux Dell fixtures cannot prove macOS
bundle replacement. Existing builds need one manual install to gain an updater.

The updater replaces only the standalone desktop app. Homebrew continues to own
the optional CLI. Each new cloud start must independently resolve and pull the
latest public image for its selected model, as required by the image plan.

## 7. App actions used global runtime operations

The app used the CLI's all-provider status and Stop operations. A Local action
could contact a cloud provider and report its failure. The exact Vast error from
the user's Stop was not captured; do not invent an HTTP status for that report.

Core corrections `d4b5c6c` and `ee60fbf` add selected-provider discovery and
identity-scoped actions. Dell ownership and process fixtures pass. Cleanup checks
provider, instance and boot identity; local cleanup also checks process identity.
An unproven cloud create remains unresolved instead of deleting another instance.
The app now records its runtime privately and uses that same identity through
Start, status and Stop. Hosted checks pass: 53 app tests, two fixture tests,
11 UI tests, lint and the native bundle. Native acceptance remains pending.

The Start command also queued work before capturing the UI selection. A later
Cloud selection could change a queued Local request. The app now reserves Start
synchronously, captures its request, and checks generations again after memory
admission. The hosted regression passes. These changes are not installed yet.

## 8. Zero token gauges did not explain request progress

The user's main OpenCode request carried about 47,000 input tokens after its
agent context was added. A separate short title request completed, but that did
not prove the main request had produced an answer. Zero throughput gauges could
still mean prompt processing. They must not be labeled Idle.

The approved fix adds measured memory and request activity. Pinned llama.cpp
slots do not expose queue length. Zero active slots will display `No active
request`; unknown queue length stays unknown. The display must not scrape metrics
more often because those reads affect the existing throughput counters.

In-app OpenCode repair will create a lean agent only when that agent is absent.
Existing custom agent controls stay intact. Disabling MCP resource use does not
remove every resource schema from OpenCode's prompt. The core repair writer now
preserves JSONC and unrelated settings, with private key files and backups.
Fifteen repair fixtures and six unchanged CLI fixtures pass on Dell. App command
wiring is in progress. Generation speed and the complete flow remain unverified.

### Current host limit

The user now reports no available Mac memory. Keep local work to source edits.
Run builds and unit checks on Dell or hosted macOS. Local install and all E2E
remain pending. Do not start a model or infer GPU-rental permission.

### Memory feature validation checkpoint

The memory guard is implemented through `b9a1966`; it is not installed in the user's production app yet. Native fixture E2E passed five cases, then the app disconnected during denied Start. Cleanup passed and no model started. Preserve the failed run at `bin/app-e2e/lobo-native-e2e-zlqngx7s`; do not report complete native acceptance. The user deferred further E2E while the approved app setup, live memory/activity, native activation and scoped Stop fixes are implemented.

### Direct comparison stopped by real memory admission

The later explicit request authorized one 47,000-input-token direct test without OpenCode. The isolated supervisor from hosted build `b5617ae` rejected Q6/65,536 context: 26.7 GiB required, 9.4 GiB available after reserves. Zero models or generation requests started. The supervisor stopped with zero errors and no remaining task processes. Do not report this as either a model failure or a successful reply test. The original title completion still does not establish the main request can finish.

## 9. A valid complete image did not prove it could be published

The complete Q6 image passed offline verification on Dell. Its OCI export is
24.69 GB, with every layer below GHCR's 10 GB limit. That size check missed a
second constraint: GHCR limits each upload to ten minutes. At the measured
Dell rate, a 3.99 GB monolithic layer cannot finish within that limit.
See [GitHub's registry limits](https://docs.github.com/en/packages/working-with-a-github-packages-registry/working-with-the-container-registry).

The first ORAS attempt was cancelled before publication. The replacement
regctl attempt used 32 MiB chunks but GHCR returned HTTP 416. The available
logs do not establish which chunk failed or why. Neither attempt published a
manifest. Offline copy fixtures did not prove live registry compatibility.

The revised workflow stages the exact verified image through a temporary
GitHub Actions artifact. A hosted runner rechecks all image hashes before
publishing to GHCR. This artifact is build transport; the application never
uses it or asks users for a bucket. The first staging run acknowledged about
1.2 MB/s. Its six-hour limit left too little margin for the full transfer.
The replacement allowed twelve hours and verified a matching runtime credential.
That still failed: upload authentication was rejected after 3,609 seconds.
No artifact was finalized, so 3.87 GB of acknowledged transfer was not reusable.
Signed upload URL expiry fits the timing but is not proved by the generic error.

The next correction uses independent 512 MiB artifacts. Each part completes
with its own upload authorization. A retried job can reuse completed parts
from the same run and exact content hashes. Reconstruction verifies every part
and original image byte before publication. Prove part completion and reuse
before another long transfer. This workflow is still under implementation;
publication remains pending until anonymous digest checks pass.

The user authorized Q6 publication and RunPod cloud E2E on 2026-10-01. Test one
GPU at a time, use direct bounded prompts, and delete only the recorded test
instance. Local inference, OpenCode and app/CLI releases remain on hold.

A complete custom image removes package installation and separate model
downloads during startup. It does not guarantee cached layers on a new GPU
host. Measure image startup and model loading during the live tests before
claiming fast startup.
