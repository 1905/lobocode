# GitHub-only updates for the Lobocode Mac app

**Date:** 2026-09-30
**Scope:** /Users/kass/dev/lobocode
**Status:** pending review

## TL;DR

**P1 — App updater.** The app checks on launch and every six hours, then downloads signed updates automatically.
**Why:** users currently have to replace the app manually.
**You do:** approve the proposed behavior; click **Install and restart** when no runtime is active.
**Does not do:** forced restarts, Homebrew CLI updates, or GPU image updates.

**P2 — GitHub delivery.** GitHub Actions builds update files; GitHub Releases hosts them without your server or bucket.
**Why:** publishing before the app files are ready can break update checks.
**You do:** retain a backup of the update-signing key before the first release; no credentials are needed for this spec.
**Does not do:** Apple notarization or removal of the current Gatekeeper warning. Publication stays on hold.

**P3 — Verification and rollout.** Deliver native UI smoke results first, then finish update, failure, and recovery tests on remote hosts.
**Why:** a download alone does not prove that installation and restart work.
**You do:** install the first updater-enabled DMG once; explicitly lift the release hold before public delivery.
**Does not do:** inference or backend/lifecycle testing on this Mac, or claim automatic recovery from every interrupted installation.

## Problem(s)

1. **The app has no updater.** The Tauri dependencies and startup registration contain no update plugin. Users cannot discover or install app releases from the app. Evidence: `app/src-tauri/Cargo.toml:25`, `app/src-tauri/src/lib.rs:23`, `app/src-tauri/tauri.conf.json:18`.
2. **Release publication happens too early for an update feed.** GoReleaser finishes before the dependent DMG job builds and uploads the app. A new Latest release could therefore lack its update manifest or archive. Evidence: `.github/workflows/release.yml:32`, `.github/workflows/release.yml:42`, `.goreleaser.yaml:6`.
3. **Restart cannot bypass application lifecycle rules.** Successful Quit cancels the polling loops. Starting that shutdown before a fallible installation would require recovery that the current controller does not provide. Evidence: `app/src-tauri/src/controller.rs:281`, `app/src-tauri/src/commands.rs:121`, `app/src-tauri/src/lib.rs:95`.
4. **Version and process state need separate treatment.** The panel displays the runtime agent version, while the build overrides the desktop app version from the tag. An updater must compare desktop versions and must account for detached runtimes and operations from another process. Evidence: `app/ui/src/panel/Header.svelte:21`, `Makefile:47`, `crates/lobo-core/src/control/operation_state.rs:51`, `crates/lobo-core/src/control/status.rs:3`.
5. **Distribution and testing have explicit limits.** The app is ad-hoc signed. Backend and lifecycle checks cannot run on this Mac, and public delivery remains on hold. Evidence: `app/src-tauri/tauri.conf.json:29`, `AGENTS.md:16`, `docs/implementation-mistakes.md:83`.

Source references describe the inspected worktree. Public-image implementation is changing in parallel. Re-read touched files before implementation; preserve that work.

## Goals

1. Fetch app update metadata and archives anonymously from public GitHub infrastructure. No customer GitHub account, developer host, bucket, or shipped token. Addresses problem 1.
2. Install only a newer, correctly signed archive for the supported app architecture. Addresses problems 1 and 4.
3. Publish a complete release before advertising it as Latest or updating the Homebrew formula. Addresses problem 2.
4. Keep Start, Stop, normal Quit, and polling usable after a failed update attempt. Preserve config, credentials, models, and runtime state. Addresses problem 3.
5. Show the desktop version and all update controls in a native window without scrolling. Addresses problems 1 and 4.
6. Record native installation evidence separately from UI tests, fixture tests, and public GitHub delivery evidence. Addresses problem 5.

## Non-goals

- Updating the optional Homebrew CLI from the app.
- Updating a running GPU image, downloading model weights, or changing the latest-image policy.
- Replacing the updater with a custom archive installer or a private update service.
- Silent installation, automatic restarts, beta channels, percentage rollouts, or differential patches.
- Apple account enrollment, Developer ID certificate provisioning, or notarization in this feature.
- Windows, Linux, Intel Mac, or universal desktop packages in the initial release. Existing CLI targets remain unchanged.

## Update architecture

### Approach and evidence

| Approach | Fit | Decision |
|---|---|---|
| Official Tauri updater with a static GitHub release manifest | Fits the existing Tauri app and its Rust command layer. | Use it. |
| Custom GitHub release lookup and custom installation | Requires Lobocode to own version selection, signature handling, archive replacement, and restart integration. | Reject the duplicated installer work. |

The [Tauri updater guide](https://v2.tauri.app/plugin/updater/) documents static metadata, mandatory artifact signatures, and macOS update archives. Its [API reference](https://v2.tauri.app/reference/javascript/updater/) separates download, installation, and relaunch. These are documented capabilities; this repository has not verified them end to end.

```text
Git tag -> Actions builds -> draft GitHub release -> asset validation
                                                    |
                                              publish complete release
                                                    |
App check -> latest.json -> signed app archive -> verified download
                                                    |
                                  Install and restart + idle check
                                                    |
                                    replace app -> restart -> version check
```

The production endpoint is fixed:

```text
https://github.com/1905/lobocode/releases/latest/download/latest.json
```

GitHub supports a stable URL for an asset on the Latest release. It does not select the newest release containing a particular filename. Every release promoted to Latest must therefore include the updater assets. See [GitHub's release-link documentation](https://docs.github.com/en/repositories/releasing-projects-on-github/linking-to-releases).

### Ownership

- Add one Rust update service under `app/src-tauri/src/updater/`. It owns checks, verified download bytes, operation state, installation, and progress events.
- Register `tauri-plugin-updater` beside the existing plugins in `lib.rs`. Pin a compatible v2 release and its resolved dependencies during implementation.
- Use the existing Rust IPC pattern. JavaScript sends commands and renders state; it does not select URLs, keys, signatures, filesystem destinations, or restart commands.
- Add no JavaScript updater/process plugin permissions. Preserve the existing restrictive webview network policy.
- Production builds accept only the embedded endpoint and public key. Test endpoints and ephemeral keys require a test build configuration.
- Serialize update operations within the process. Serialize installation between app processes using a task-owned advisory lock in the app data directory. Do not delete a lock file to break a live lock.

Why: `app/ui/src/lib/api.ts:9` already routes actions through Rust. Reusing that boundary avoids a second installation path that bypasses the controller.

### Versions and artifacts

| Item | Contract |
|---|---|
| Desktop version | Read Tauri package metadata from the running app. Do not use `Snap.version`, which belongs to the runtime. |
| Production tag | Stable `vMAJOR.MINOR.PATCH`; the app version and manifest version must equal it after removing `v`. |
| Development build | Automatic production checks disabled for debug builds and `0.0.0-dev`. A test bundle must opt into its fixture feed. |
| Platform | `aarch64-apple-darwin`; manifest entry `darwin-aarch64`, with compatible bundle-specific entries if the pinned action generates them. |
| First-install asset | `lobocode.dmg`, containing the same app as the update archive. |
| Update asset | Versioned `.app.tar.gz` and its `.sig`, generated from the built app. Use the pinned action's actual filenames. |
| Manifest | `version`, optional short `notes` and `pub_date`, and platform entries containing an exact-tag archive URL and signature contents. |
| Downgrade | Reject equal and older versions. A corrective release gets a higher patch version. |

The manifest generator must use the actual tag and actual uploaded artifact names. Do not derive app filenames from the existing CLI archive naming convention. Validate the archive's embedded app identifier, desktop version, and architecture in CI.

### Schedule and download policy

1. Start the first check 15 seconds after application startup. It must not delay window creation or require provider configuration.
2. Check every six hours while the app runs. Sleep/wake produces at most one overdue check, with no replay of missed intervals.
3. Manual **Check now** bypasses the interval. Concurrent requests share the current operation rather than issuing another request.
4. Set a 30-second total budget for metadata and a 15-minute budget for a complete archive download. Use cancellable tasks.
5. Automatically download a newer supported version and verify it. Keep the verified bytes only for the current app session; no persistent download cache in v1.
6. Report progress as bytes when length is unknown. Report a percentage only when total length is known.
7. Retry transient check/download failures after 15 minutes, then one hour, then return to the six-hour schedule. Manual retry remains available.
8. A signature or malformed-release failure does not trigger repeated downloads of the same candidate. A manual retry or changed candidate can retry it.
9. Closing the Updates window does not cancel the task. Normal app Quit cancels a check/download and follows the existing Quit behavior.
10. A newly discovered release does not replace verified bytes during installation. Discard an older candidate only after the service accepts the new candidate.

Automatic checks and downloads are the proposed default. Installation always requires the app user's action. A downloaded update is lost when the app exits; a later session can download it again. This avoids adding cache recovery and disk cleanup to the first feature.

## Native update interface

Add an **Updates** action to the main panel and setup view. In the existing panel footer, replace the direct config shortcut with Updates; configuration remains accessible through Settings. Add a dedicated `updates` window and `?view=updates` route.

Use the existing native title bar, dark appearance, rounded corners, drag behavior, and measured title-bar inset. Target 440 logical pixels in width and at most 440 in content height. Verify controls at the supported display scale. No vertical or horizontal scrolling.

| State | Visible content and action |
|---|---|
| Idle / current | Installed desktop version, last successful check, **Check now**. |
| Checking | Installed version and **Checking for updates**. |
| Downloading | Installed/new versions and download progress. |
| Downloaded, busy | **Update downloaded**; explain that active runtime/startup/cleanup must finish. Installation is disabled. |
| Downloaded, idle | **Install and restart** and **Later**. Later hides the window and keeps the download. |
| Installing / restarting | One progress state; disable competing app lifecycle actions until success or recoverable failure. |
| Failed | Short specific reason and **Retry**, or **Open release page** for manual recovery. |

Do not render arbitrary release Markdown in the window. **Release notes** opens the exact tag page on `github.com/1905/lobocode`. Long errors remain bounded in the UI; sanitized details go to local diagnostics.

Expose commands `get_update_state`, `check_for_updates`, `install_update`, and `open_updates`. Emit `lobo://update-state` using types generated from Rust, matching the existing export pattern in `app/src-tauri/src/types.rs:232`.

Example state delivered to the frontend:

```json
{
  "phase": "downloaded",
  "current_version": "0.2.0",
  "available_version": "0.2.1",
  "downloaded_bytes": 18874368,
  "total_bytes": 18874368,
  "install_blocked": "runtime_active",
  "last_checked_at": "2026-09-30T12:00:00Z",
  "error": null
}
```

The numbers above are a shape example, not measured artifact sizes or a chosen release number. Phase values are `idle`, `checking`, `current`, `downloading`, `downloaded`, `installing`, `restarting`, and `failed`. Optional fields serialize as `null`. Blocking reasons are `runtime_active`, `operation_active`, `state_unknown`, `another_installer`, or `null`.

## Installation and restart

### Preflight and exclusion

The frontend's enabled button is only a hint. Rust rechecks eligibility when the user clicks it.

1. Acquire the update-install lock and set a reversible controller installation guard. This rejects a new app Start, Stop, config mutation, or competing Quit.
2. Acquire the existing shared `OperationState` guard with a two-second cancellable wait. Hold it through the final idle check and installation. This prevents a cooperating CLI/app process from starting a new operation in that interval. Avoid nested acquisition of the same operation lock.
3. If the operation record is pending or corrupt, stop preflight with an error. Never clear, adopt, or delete runtime resources merely to install an update.
4. Inspect local process ownership and configured provider instances through a read-only backend check, with a 15-second total timeout. Active or unknown state blocks installation. Check all configured providers, not the selected panel target.
5. Do not call the current `snapshot()` as the preflight probe: it can attach or stop connections (`crates/lobo-core/src/control/status.rs:9`). Add a small read-only inspection entry point and reuse existing provider listing and process-identity code.
6. An unconfigured fresh app can update if no local process or pending ownership record exists. Malformed existing state fails closed; missing provider keys must not be interpreted as proof that a saved runtime is absent.
7. Reject execution from a mounted DMG or a non-writable installation directory. Offer instructions to install the DMG into `/Applications` or `~/Applications`. Do not add a privileged installation helper.

This protects operations that use the existing shared lock and known app state. It does not claim control over another user account, an unrelated config, or unmanaged processes. The updater never sends a provider delete request, kills a model, or rewrites the optional CLI.

### Commit installation, then restart

- Verify the selected download before entering installation. Keep the verified candidate owned by the Rust service, unavailable to frontend mutation.
- Run the plugin's blocking installation work off the UI thread. Keep native dispatch available for the plugin.
- Do not call the current irreversible `Controller::quit()` before installation succeeds. Polling and ordinary app use must resume after a preflight/download/install failure when the installed bundle is intact.
- On success, finalize app shutdown and request Tauri restart. Adapt `ExitRequested` handling to distinguish this restart from ordinary Quit and prevent duplicate cleanup.
- Before restart, atomically save a small receipt with previous version and expected version. On the next launch, compare it with Tauri package metadata, record success or mismatch, and remove the receipt only after recording the result.
- Do not restart in a loop on mismatch. Show the installed version and a manual recovery action.
- If the bundle replacement is incomplete, do not claim rollback or keep offering normal Start as if the app were healthy. Keep the error visible and direct the user to the complete DMG.

Why: the updater plugin supplies bundle installation; Lobocode still owns the lifecycle around it. The current successful Quit cancels a token that cannot be reset (`app/src-tauri/src/controller.rs:285`). Use a separate reversible guard rather than trying to uncancel it.

Power loss or forced termination during bundle replacement may require manual DMG recovery. That limitation must remain documented until a pinned plugin version passes an interruption test that proves more.

## Signing and GitHub release pipeline

### Key lifecycle and build modes

- Generate one production updater keypair during authorized implementation. Store the public key in the production config and the private key/password in GitHub Actions secrets.
- Keep a recoverable private-key backup outside the repository. Record its fingerprint and backup location, never the key material, in the release runbook.
- Never print secrets, commit private keys, include them in artifacts, or pass them to ordinary pull-request builds.
- Missing signing secrets fail the production release before publication. Test builds use disposable keys and a separate bundle identifier.
- Keep normal `make mac` and native UI builds usable without production secrets. Put `createUpdaterArtifacts: true` in a release configuration overlay; test signing has its own overlay.
- Losing the private key breaks the installed clients' normal trust path. Document a manual DMG migration; planned key rotation and multi-key trust are outside v1.

The app currently uses ad-hoc Apple signing. Updater artifact signing is separate. Keep the existing first-open warning documented; do not promise that this feature removes it. Developer ID signing and notarization require a separate follow-up. See [Tauri's macOS signing guidance](https://v2.tauri.app/distribute/sign/macos/).

### Pipeline ordering

Use one serialized release workflow per repository without cancelling an active release. Reruns for the same tag may repair a draft; they must not overwrite a published updater archive. Before promotion, reject a version older than the current stable Latest release.

| Stage | Required result |
|---|---|
| 1. Validate source | Stable tag, intended commit, required CI, version agreement, release authorization, and compatible public GPU-image prerequisites. |
| 2. Build CLI | GoReleaser produces the existing four archives, checksums, and formula. Set draft mode and reuse an existing draft without deleting it; defer formula upload. |
| 3. Build app | Build explicit ARM64 app once. Generate the DMG and signed update archive from those same app bytes. |
| 4. Upload app | Attach archive/signature/DMG and generate `latest.json` using `tauri-apps/tauri-action`, pinned to a reviewed commit. |
| 5. Validate draft | Check asset inventory, signatures, app version/identifier/architecture, manifest targets and exact-tag URLs. Download the uploaded bytes with CI credentials and compare them. |
| 6. Publish | Publish the complete draft and make it Latest only after all prior stages pass. |
| 7. Public smoke | Fetch manifest and archive anonymously from GitHub. Verify status, redirects, digest, signature, and app metadata. |
| 8. Publish formula | Push only the generated formula to `1905/homebrew-tap` using the existing tap credential. Do not create a PR. |

The [official Tauri action](https://github.com/tauri-apps/tauri-action) supports updater JSON generation, an existing release ID, explicit tag, and matching draft status. Supply both release ID and tag so the manifest identifies that release's archive.

GoReleaser supports leaving a release as a draft and reusing an existing draft. Its formula configuration supports generating a formula without uploading it. Validate those settings against this repo's pinned GoReleaser v2.13.3 before implementing the workflow. References: [release settings](https://goreleaser.com/customization/publish/scm/), [formula upload settings](https://goreleaser.com/customization/publish/homebrew_formulas/).

Preserve `LOBO_VERSION`, `LOBO_COMMIT`, `LOBO_DATE`, locked builds, and the Makefile's tag-derived app version. Changing the app target adds a target-specific output directory; update the DMG packaging path accordingly.

The Tauri action performs the single app build and uploads its update artifacts to the draft. Split DMG packaging from the current build prerequisite so it packages that exact bundle without rebuilding. Retain `make dmg` as the convenient local build-plus-package target.

Give `contents: write` only to release jobs. Scope the updater signing key to app signing and the tap credential to the final formula job. GitHub tokens stay in CI; production app requests remain anonymous.

If public smoke fails after publication, stop formula publication and report the failed release. Do not silently delete public assets or assume that no client downloaded them. Correct app behavior through a higher-version release. If only formula publication fails, retry that step with the same verified formula; do not rebuild or republish the app.

## File-level changes

| File path | Planned change |
|---|---|
| `app/src-tauri/Cargo.toml`, `app/src-tauri/Cargo.lock` | Pin the updater plugin and required direct locking dependency. Preserve Tauri 2 compatibility. |
| `app/src-tauri/tauri.conf.json` | Embed the production updater endpoint/public key. Preserve bundle identity and existing code-signing behavior. |
| `app/src-tauri/tauri.release.conf.json` | New overlay enabling production updater artifacts. Ordinary development builds do not require its signing secrets. |
| `app/src-tauri/src/updater/mod.rs`, `state.rs`, `tests.rs` | Coordinator, injectable transport/installer boundaries, scheduler, candidate ownership, progress, receipt, and meaningful failure tests. |
| `app/src-tauri/src/lib.rs`, `commands.rs` | Register service/plugin/commands/events and integrate restart handling. |
| `app/src-tauri/src/controller.rs`, `controller/tests.rs` | Reversible installation guard, command exclusion, normal-Quit preservation, and failure recovery. |
| `app/src-tauri/src/backend.rs` | Expose read-only update safety and execute installation while the shared operation guard is held. Never reuse destructive cleanup as preflight. |
| `crates/lobo-core/src/control/update_safety.rs`, `control/mod.rs` | Minimal read-only runtime inspection shared with app backend; reuse existing listing and ownership rules. |
| `app/src-tauri/src/types.rs`, `app/ui/src/gen/` | Typed update state and generated TypeScript. Extend the exact generated-file test expectations. |
| `app/src-tauri/src/windows.rs`, `capabilities/default.json` | Native Updates window and its permitted window/event operations. No direct updater/process IPC grant. |
| `app/ui/src/App.svelte`, `lib/api.ts` | Updates route, sizing, commands, and event subscription with initial-state race handling. |
| `app/ui/src/updates/Updates.svelte`, `updates/Updates.test.ts` | Compact update view and state/action tests. |
| `app/ui/src/panel/Footer.svelte`, `panel/Panel.svelte`, `panel/SetupCard.svelte` | Update entry points available before provider setup. Preserve essential runtime actions. |
| `app/ui/src/render/`, `app/src-tauri/tests/fixtures.rs` | Representative update states in the established fixture/render pipeline. |
| `app/e2e/app.spec.js`, `tools/native_app_e2e.py` | UI-only update tests on Mac; explicit separate install mode for remote macOS acceptance. |
| `tools/test_app_updater.py`, `app/e2e/tauri.updater-test.conf.json` | Isolated A-to-B bundle test with disposable keys, receipt inspection, and task-owned cleanup. |
| `Makefile`, `.github/workflows/rust.yml` | Release overlay/output paths, remote updater tests, UI fixture checks, and relevant CI path filters. |
| `.github/workflows/release.yml`, `.goreleaser.yaml` | Complete-draft publication, updater assets, serial release ordering, and delayed formula upload. |
| `tools/verify_app_release.py`, `tools/publish_brew_formula.sh` | Validate manifests and uploaded artifacts; publish only the already-generated formula after public smoke. |
| `README.md`, `CHANGELOG.md`, `docs/implementation-mistakes.md` | Update instructions, exact acceptance limits, and correction of the missing-updater finding. |
| `docs/app-updates.md` | Release/key-backup runbook, bootstrap installation, failure diagnosis, and manual recovery. |

These are planned files, not completed changes. Final module boundaries may be smaller if the existing code supports them without duplicating responsibilities.

## Tests

### Host and delivery order

- This Mac: frontend checks, builds, fixture renders, and actual native UI-only checks. No installation/restart lifecycle suite, backend tests, or inference.
- Dell: host-independent core fixtures and release-validation scripts. Linux cannot prove macOS bundle installation.
- GitHub-hosted macOS: app Rust tests, signed A-to-B bundle replacement, restart, and native integration. Fixture runtimes only; no rented GPU or model inference.
- Public GitHub: anonymous download smoke only after publication is explicitly authorized.

First deliver the smallest usable app update UI with focused smoke and required checks. Report the exact revision and remaining tests. Then finish the failure/regression matrix. A fixture-only UI build is QA-ready for the UI, not evidence that production auto-update works.

### Unit and fixture tests

| ID | Area | Required proof |
|---|---|---|
| U1 | Update service | Launch delay, six-hour interval, sleep/wake coalescing, manual check, and concurrent request coalescing. Use a fake clock. |
| U2 | Candidate selection | Equal/older version does not install; wrong target, invalid manifest, and malformed signature produce distinct failures. |
| U3 | Download | Progress with/without length, interrupted stream, timeout, normal Quit cancellation, retry timing, and failed-candidate suppression. |
| U4 | Controller | Start versus install and Quit versus install races have one owner. Recoverable failure releases guards and keeps polling usable. |
| U5 | Runtime safety | Active local/cloud runtime, pending/corrupt ownership record, failed provider listing, fresh no-config install, and second-process lock contention. |
| U6 | Receipt | Success only when the next process reports the expected desktop version. Mismatch produces one visible error, not repeated restarts. |
| U7 | Generated types/UI | All states render; invalid actions stay disabled; windows opening during events show the newest state; desktop/runtime versions remain distinct. |
| U8 | Release scripts | Missing asset, version/architecture/identifier mismatch, invalid signature, wrong repo/tag URL, and premature formula publication are rejected. |

### Remote macOS integration

1. Build two isolated app versions, A and B, with a disposable signing key and fixture feed. Use a separate bundle identifier and temporary user-data paths.
2. Install A in a task-owned writable Applications directory. Fetch B, verify it, perform replacement, restart, and read the B receipt.
3. Verify configuration, preferences, model-directory references, model bytes, and a sentinel optional CLI binary remain unchanged.
4. Serve an archive signed by a different key. Confirm rejection before bundle replacement.
5. Break the connection during download and during the next metadata check. Confirm the original app remains usable and retry succeeds.
6. Simulate active runtime and pending cleanup through fixtures. Confirm installation is blocked without delete/kill calls.
7. Inject an installation error before replacement and exercise an interrupted replacement in an isolated bundle. Record which failures preserve a launchable old app and which require the DMG.
8. Test read-only/mounted-image preflight, second app instance contention, restart failure/mismatch, and a verified candidate surviving an ordinary window close.
9. Prove the test endpoint/key cannot be enabled in the production package through environment variables, URL parameters, or IPC.

### Native UI acceptance

Verify the actual app windows move normally and retain native rounded corners. Check every update state with long versions/errors and unknown download length. Confirm no scrolling, clipped buttons, stolen focus, or automatic restart. Open Updates before setup and while the runtime view is active. Existing Settings, Start, Stop, and Quit UI behavior must still pass.

### Production acceptance

After the release hold is lifted, validate the actual uploaded assets and anonymous GitHub route. Install the first updater-enabled DMG manually. Prove a subsequent higher version updates that installation using the production public key. Do not publish an artificial empty update solely to mark the feature complete; keep production A-to-B acceptance pending until a real subsequent release exists.

## Failure modes & decisions

| Failure | Behavior |
|---|---|
| GitHub unavailable, rate-limited, or offline | Background failure is non-blocking. Manual check shows the reason; retry follows the documented schedule. |
| Latest lacks `latest.json` or a target | Treat it as release configuration failure, not “up to date.” Never guess an older asset. |
| Signature mismatch or malformed metadata | Reject the candidate and retain the installed app. No automatic retry loop for identical rejected metadata. |
| Runtime busy or state unknown | Keep the verified download. Disable installation until a fresh preflight proves idle. |
| Another process holds the operation/install lock | Report busy promptly. Do not wait forever or remove another process's lock. |
| Installation fails before replacement | Release reversible guards and retain ordinary app use. |
| Replacement is incomplete | Report recovery required. Offer the complete DMG; do not claim transactional rollback. |
| User closes the update window | Keep checking/downloading in the background. No installation consent is implied. |
| User quits during download | Cancel download; ordinary controller Quit rules still apply. |
| User quits during installation | Keep one install owner; defer competing graceful Quit until the install outcome is known. OS force-quit remains outside that guarantee. |
| App is running from a DMG or unwritable location | Block in-app replacement and explain the supported install location. |
| Missing production key or failed app build | Keep the release draft and the previous Latest release. |
| Rerun targets a published release | Refuse archive replacement. Fix code with a higher version; retry only non-mutating checks or the formula step. |
| Update key lost | Existing clients require manual migration unless a working old key remains. No server can repair a missing trust key. |
| Gatekeeper blocks a new installation | Show the existing documented macOS first-open instructions. Updater signing does not remove the warning. |
| App upgrade requires a model/provider change | Do not couple image selection to app version. Enforce compatibility through release validation and the separate public-image contract. |

## Out of scope

- This planning turn does not generate production keys, change GitHub secrets, run builds, commit product code, push, publish, or rent infrastructure.
- Spec approval does not lift the explicit release hold in `AGENTS.md`.
- Existing unrelated public-image work and other plans are not migrated or rewritten.
- No app-store distribution, paid update service, telemetry backend, beta feed, multi-key rotation, or privileged updater daemon.
- No migration of the optional CLI to a bundled executable or Homebrew cask.
- Apple notarization remains a separate release-quality improvement. Its absence must remain visible in user-facing install documentation.

## Rollout

- **P1 — one commit, clean review:** app service, runtime guard, native UI, and focused smoke; deliver a usable UI QA build and list untested installation behavior.
- **P2 — one commit, clean review:** signing/build overlays, complete-draft release pipeline, delayed Homebrew publication, and isolated remote A-to-B smoke; no public release while the hold remains.
- **P3 — one commit, clean review:** full failure/regression acceptance, documentation/changelog, and any required fixes; public delivery and production A-to-B evidence only after their stated prerequisites.

After spec approval, `/plan` writes one versioned implementation plan for these three phases. Each phase updates its status and records exact validation evidence. Required CI and focused smoke precede QA delivery; broader follow-up tests remain visible until complete. If later tests find defects, deliver focused fix commits through the same authorized workflow.
