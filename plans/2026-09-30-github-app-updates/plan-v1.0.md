# GitHub-only App Updates Implementation Plan v1.0

**Date:** 2026-09-30
**Status:** draft
**Spec:** [./spec.md](./spec.md) — approved on 2026-09-30
**Goal:** Update the standalone ARM64 Mac app from signed public GitHub Releases without a developer-owned server.
**Architecture:** One Rust service owns discovery, verified download bytes, installation, and restart. The existing controller and shared operation lock prevent conflicting runtime work. GitHub Actions assembles and validates a draft release before publication and Homebrew formula delivery.
**Tech Stack:** Rust 1.98.1, Tauri 2.12.0, tauri-plugin-updater 2.13.1, Tokio, Svelte 5, TypeScript, Vitest, native WebdriverIO, Python 3.12+, GoReleaser 2.13.3, GitHub Actions, Minisign 0.12.

> For agentic workers: use superpowers:subagent-driven-development to implement task-by-task. Checkbox syntax for tracking.

This is the current plan. No task has executed. Self-review is complete; no external model review was requested or run. A revision creates `plan-v1.1.md`; it does not overwrite this version.

## Global constraints

- Production endpoint: `https://github.com/1905/lobocode/releases/latest/download/latest.json`.
- Target: `aarch64-apple-darwin`. Keep the app identifier `io.github.1905.lobocode` and minimum macOS 13.0.
- Initial check after 15 seconds; normal interval six hours. Metadata timeout 30 seconds; download timeout 15 minutes.
- Transient retries after 15 minutes, then one hour, then six hours. Never install an equal or older version.
- Download automatically. Install only after **Install and restart**, with a fresh idle check.
- Keep downloaded bytes in memory for this session only. Closing a window keeps the task alive; ordinary Quit cancels a check/download.
- A shared operation-lock wait has a two-second budget. Read-only runtime inspection has a 15-second total budget.
- No update operation rents, deletes, stops, or restarts a GPU/model runtime. Do not change the optional CLI binary.
- Do not change latest-public-image resolution. Do not pin GPU images to the desktop version.
- Keep macOS windows native, movable, rounded, dark, and without scrolling. Updates content is 440 logical pixels wide and at most 440 high.
- This laptop runs UI tests and builds only. Dell runs host-independent core/script tests. macOS backend and installation tests run on GitHub-hosted macOS.
- Public release, tag push, image publication, and GPU rental remain on hold. Spec/plan approval does not lift that hold.
- No PRs. No automatic plan model review or per-task model reviews. The orchestrator runs integration/smoke checks; implementers run their focused unit tests only.
- Preserve all unrelated work in the primary checkout. Never use reset, clean, stash, or broad staging to make it appear clean.

Commands below are job commands executed at the feature checkout root on the named host. When invoking them through the local agent shell, use the installed RTK wrapper. GitHub runners execute their workflow commands directly.

## Execution base and permissions

The inspected committed Rust baseline is `adf8a25` on `feat/rust`. The primary checkout also contains uncommitted public-image work. Starting from the current `master` would not supply the same Tauri application.

**Proposed exception for approval:** create a clean `feat/github-app-updates` worktree from `adf8a25`, then bring in only this feature's approved planning documents. Do not copy the uncommitted image changes. This is an explicit exception to the `plan` skill's clean `master`/`dev`/`main` prerequisite. Ask for this exception with plan approval, once. Without that approval, wait for an approved clean Rust base; do not begin code changes.

The proposed worktree path is `/Users/kass/dev/lobocode-github-app-updates`. Check for an existing worktree/branch before creating either. If one exists, inspect it and reuse it only if it belongs to this feature.

Plan approval authorizes preparation and implementation in that isolated worktree. It does not authorize pushing a branch or publishing. GitHub-hosted macOS tests require an authorized CI push/dispatch. If that access is still held, finish the available code/UI/Dell work and leave the macOS gates explicitly pending. Do not run those tests on the laptop to bypass the restriction.

## File map

### Modify

| Paths | Responsibility |
|---|---|
| `app/src-tauri/Cargo.toml`, `app/src-tauri/Cargo.lock` | Pinned updater dependencies and test-only feature. |
| `app/src-tauri/tauri.conf.json` | Fixed production endpoint and updater public key. |
| `app/src-tauri/src/lib.rs`, `app/src-tauri/src/commands.rs` | Service startup, IPC, events, test-only registration, and restart integration. |
| `app/src-tauri/src/controller.rs`, `app/src-tauri/src/controller/tests.rs` | Reversible installation reservation and lifecycle/config/poll exclusion. |
| `app/src-tauri/src/backend.rs` | Advisory activity check and installation callback under the shared operation lock. |
| `app/src-tauri/src/types.rs` | Serializable update types and exact TypeScript export inventory. |
| `app/src-tauri/src/windows.rs`, `app/src-tauri/capabilities/default.json` | Updates window and minimal window/event capabilities. |
| `crates/lobo-core/src/control/mod.rs` | Export the new read-only activity API. |
| `crates/lobo-core/src/local/state.rs`, `crates/lobo-core/src/local/state/tests.rs` | Non-mutating state read. |
| `crates/lobo-core/src/connection.rs` | Read saved cloud ownership without connecting, deleting, or exposing keys. |
| `app/ui/src/App.svelte`, `app/ui/src/lib/api.ts` | Updates route, event subscription, and typed commands. |
| `app/ui/src/panel/Footer.svelte`, `app/ui/src/panel/Panel.svelte`, `app/ui/src/panel/SetupCard.svelte` | Update entry points before and after setup. |
| `app/ui/src/render/Render.svelte`, `app/src-tauri/tests/fixtures.rs` | Generated update fixtures and render routes. |
| `app/e2e/app.spec.js`, `app/e2e/wdio.conf.js`, `app/e2e/tauri.conf.json`, `tools/native_app_e2e.py` | Native update UI smoke, window driver access, and explicit test selection. |
| `Makefile`, `.github/workflows/rust.yml` | Build/package separation, test targets, CI checks, and path filters. |
| `.github/workflows/release.yml`, `.goreleaser.yaml` | Complete-draft publication and formula publication after public smoke. |
| `README.md`, `CHANGELOG.md`, `docs/implementation-mistakes.md` | Installation instructions and accurate delivery/validation status. |

### Create

| Exact paths | Responsibility |
|---|---|
| `crates/lobo-core/src/control/update_safety.rs` | Runtime activity inspection and its unit tests. |
| `app/src-tauri/src/updater/mod.rs`, `app/src-tauri/src/updater/state.rs`, `app/src-tauri/src/updater/tests.rs` | Update service, adapter, scheduler, candidate ownership, installation, receipts, and tests. |
| `app/src-tauri/tauri.release.conf.json` | Production update-artifact overlay. |
| `app/ui/src/gen/UpdateState.ts`, `UpdatePhase.ts`, `UpdateBlocker.ts`, `UpdateFailure.ts` | Rust-generated types under `app/ui/src/gen/`. |
| `app/ui/src/updates/Updates.svelte`, `app/ui/src/updates/view.ts`, `app/ui/src/updates/Updates.test.ts` | Native view and pure presentation tests. |
| `app/ui/src/fixtures/updates.json` | One generated inventory of update UI states. |
| `app/e2e/tauri.updater-test.conf.json`, `tools/test_app_updater.py` | Disposable signed A/B builds and remote macOS acceptance orchestration. |
| `tools/verify_app_release.py`, `tools/publish_brew_formula.sh` | Artifact/manifest/signature validation and idempotent formula delivery. |
| `docs/app-updates.md` | Release, signing-key backup, diagnosis, and recovery runbook. |

A shortened filename in the generated-types row has the same `app/ui/src/gen/` parent. No handwritten copies of Rust types.

### Out of scope

`docker/`, model catalogs, provider rental behavior, normal image resolution, CLI installer behavior, Apple certificate provisioning, and unrelated plans. Do not modify the installed `/Applications/lobocode.app` until a UI QA installation is authorized. Test bundles use their own identity and directories.

## Review focus

1. **Stale local state:** inspection must not delete it or confuse a reused PID with the app's supervisor. Task 1 tests file bytes and process identity.
2. **A poll or config save already in progress:** installation must not race a late connection attach or settings write. Tasks 3 and 8 test reservations around awaited work.
3. **Quit while the installer thread is running:** dropping an async future must not release locks while replacement continues. Task 8 tests ownership until the blocking worker finishes.
4. **New metadata while a candidate is ready:** a failed newer download must not discard the still-valid old candidate or change the bytes being installed. Tasks 7 and 18 test candidate identity and retention.
5. **Release rerun after partial success:** the app must not be rebuilt or overwritten merely because the formula push failed. Tasks 14, 15, and 18 test repeat publication and declining version order.

## Locked interfaces and names

These declarations specify interfaces, not implementation bodies. Every `Result` in the app uses `crate::backend::Result<T>` and `AppError`; core APIs use `lobo_core::Result<T>`.

### Core inspection

```rust
// local/state.rs: parse and validate, but never remove or rewrite a record.
pub fn inspect(&self) -> Result<Option<lobo_proto::LocalState>>; // StateFile

// connection.rs: contains no credentials or key paths.
pub struct SavedConnectionRef { pub provider: String, pub id: String }
pub fn saved_reference(config_path: &std::path::Path)
    -> Result<Option<SavedConnectionRef>>;

// control/update_safety.rs
pub enum RuntimeActivity { Idle, Active, Unknown }
pub async fn inspect_runtime(
    deps: &super::Deps,
    local_state: &crate::local::StateFile,
    saved: Option<&crate::connection::SavedConnectionRef>,
) -> Result<RuntimeActivity>;
```

`inspect_runtime` does not acquire the operation lock. Its caller owns locking. It lists only cloud providers; local state uses `StateFile::inspect`, because `LocalProvider::list` currently calls the mutating reader. Any unavailable provider or unverified ownership yields Unknown. A saved provider absent from configuration also yields Unknown. A saved cloud reference with no matching instance remains Unknown until ordinary lifecycle cleanup resolves it; the updater cannot prove a detached helper has stopped. Idle requires all relevant checks to succeed.

### App state

`UpdatePhase`: `Idle`, `Checking`, `Current`, `Downloading`, `Downloaded`, `Installing`, `Restarting`, `Failed`.

`UpdateBlocker`: `RuntimeActive`, `OperationActive`, `StateUnknown`, `AnotherInstaller`.

All enums serialize as the snake_case strings in the spec. `UpdateFailure` contains `code: String`, `message: String`, and `retryable: bool`. Codes used by this plan are `network`, `manifest`, `signature`, `unsupported_target`, `busy`, `location`, `disk`, `install`, `recovery_required`, `restart_mismatch`, and `disabled`.

`UpdateState` has exactly the spec's fields: `phase: UpdatePhase`, `current_version: String`, `available_version: Option<String>`, `downloaded_bytes: u64`, `total_bytes: Option<u64>`, `install_blocked: Option<UpdateBlocker>`, `last_checked_at: Option<String>`, `error: Option<UpdateFailure>`. Timestamp strings are UTC RFC 3339; byte counters export as TypeScript `number`, not `bigint`.

### Controller and backend

```rust
pub type InstallJob = Box<dyn FnOnce() -> Result<()> + Send + 'static>;

// Add these methods to Backend and its production/fake implementations.
async fn update_activity(&self) -> Result<lobo_core::control::update_safety::RuntimeActivity>;
async fn install_when_idle(&self, job: InstallJob, cancel: CancellationToken) -> Result<()>;

// Controller methods. UpdateGuard owns an Arc<Controller>, not a borrowed mutex.
pub async fn begin_update(self: &Arc<Self>) -> Result<UpdateGuard>;
pub async fn save_config(self: &Arc<Self>, set: BTreeMap<String, String>) -> Result<()>;
pub fn update_reserved(&self) -> bool;
// Dropping an uncommitted guard restores operation; success consumes it.
pub fn commit_restart(self); // UpdateGuard
pub fn require_recovery(self); // UpdateGuard: retain mutation exclusion, allow Quit.
```

`install_when_idle` acquires the persistent operation lock, checks pending ownership, performs fresh inspection, and runs the closure on a blocking worker. It retains the lock until that worker has returned. It never runs the closure if preflight fails. Do not add a timeout that drops the running worker.

### Update service and plugin adapter

```rust
pub type UpdateProgress = Arc<dyn Fn(u64, Option<u64>) + Send + Sync>;
pub type UpdateEmitter = Arc<dyn Fn(UpdateState) + Send + Sync>;
pub type RestartApp = Arc<dyn Fn() + Send + Sync>;
pub enum CheckReason { Startup, Scheduled, Manual }
pub struct CandidateMeta { pub identity: String, pub version: String, pub release_url: String }

#[async_trait]
pub trait UpdatePackage: Send + Sync {
    fn metadata(&self) -> CandidateMeta;
    async fn download(&self, progress: UpdateProgress, cancel: CancellationToken) -> Result<Vec<u8>>;
    fn install(&self, bytes: &[u8]) -> Result<()>;
}
#[async_trait]
pub trait UpdateClient: Send + Sync {
    async fn check(&self, cancel: CancellationToken) -> Result<Option<Arc<dyn UpdatePackage>>>;
}

// UpdateService methods; constructor dependencies are named in Task 5.
pub fn state(&self) -> UpdateState;
pub fn start(self: &Arc<Self>);
pub async fn check(self: &Arc<Self>, reason: CheckReason) -> Result<()>;
pub async fn install(self: &Arc<Self>) -> Result<()>;
pub async fn prepare_quit(&self);
pub fn runtime_changed(self: &Arc<Self>);
```

`TauriUpdateClient` and `TauriUpdatePackage` implement these traits. Package identity is a SHA-256 digest of version, initial archive URL, and signature with unambiguous separators. Keep plugin objects and bytes private to Rust. A private `VerifiedUpdate` pairs one package with bytes returned successfully by its verified download.

The Tauri source inspected for this plan documents `Update::download(...) -> Result<Vec<u8>>` and `Update::install(bytes) -> Result<()>`. Download verification finishes before bytes are returned. Do not treat the download-finished progress callback as proof of a valid signature. See [updater source](https://github.com/tauri-apps/plugins-workspace/blob/updater-v2.13.1/plugins/updater/src/updater.rs).

### IPC, frontend, and receipt

| Name | Signature/contract |
|---|---|
| `get_update_state` | No frontend arguments; returns `UpdateState`. |
| `check_for_updates` | No frontend arguments; calls `check(Manual)`, returns `Result<()>`. |
| `install_update` | No frontend arguments; calls `install()`, returns `Result<()>`. |
| `open_updates` | No arguments; opens/focuses the native `updates` window. |
| `open_update_release` | No arguments; opens the candidate's validated tag page, or the fixed Latest page when no candidate exists. |
| `api.getUpdateState`, `api.checkForUpdates`, `api.installUpdate`, `api.openUpdates` | Typed wrappers for those exact commands. |
| `api.openUpdateRelease` | Typed wrapper for `open_update_release`. |
| `onUpdateState(cb)` | Subscribe to `lobo://update-state`, return the unlisten handle. |
| `updateView(state: UpdateState): UpdateView` | Pure presentation function in `updates/view.ts`. |
| `UpdateView` | `{ headline: string; detail: string; progress: number | null; primary: 'check' | 'install' | 'retry' | 'none'; primaryDisabled: boolean; recoveryRequired: boolean }`. |
| `update-pending.json` | Private app-config receipt: `{ previous_version, expected_version, prepared_at }`. Write atomically before replacement starts. |
| `update-result.json` | `{ previous_version, expected_version, observed_version, result, checked_at }`; result is `success` or `mismatch`. Persist before removing pending receipt. |

Derive release links from the validated candidate version under `https://github.com/1905/lobocode/releases/tag/v...`. Without a candidate, use only `https://github.com/1905/lobocode/releases/latest`. The view uses the existing `@tauri-apps/api` and Rust commands; it does not receive arbitrary opener URLs or signatures.

## Self-test sanity check before Task 1

This gate runs at execution time, not during this planning turn.

- [ ] Confirm plan and the explicit worktree-base exception are approved. Read the current project/global instructions and both feature documents.
- [ ] Inspect `git status --short`, `git worktree list`, and the committed Rust baseline. Preserve the dirty primary checkout.
- [ ] Create the isolated worktree at the approved base and copy only this feature's planning documents from their committed revision. Record base/plan commit IDs in the execution notes inside this plan.
- [ ] On Dell, run `cargo test --locked -p lobo-core --lib local::state::tests` and `cargo check --locked -p lobo-core`. Expect the existing state tests and compile check to pass.
- [ ] On remote macOS, run `cargo check --locked --manifest-path app/src-tauri/Cargo.toml` and the existing `controller::tests` suite. On the laptop run `pnpm -C app/ui check`, `pnpm -C app/ui test`, and `pnpm -C app/ui build`.
- [ ] Record named test results and exit codes. A filter matching zero tests is not a pass. If baseline fails, identify pre-existing versus feature failures before editing.
- [ ] If remote macOS execution is unavailable because CI pushes are held, label that gate pending. Do not claim a green backend baseline or run the app Rust suite on the laptop.

## P1 — Usable app updater

### Task 1: Read saved state without changing it

**Files:** Modify `crates/lobo-core/src/local/state.rs`, `crates/lobo-core/src/local/state/tests.rs`, `crates/lobo-core/src/connection.rs`. Test in those existing modules.
**Interfaces:** Produce `StateFile::inspect` and `connection::saved_reference` from the locked interfaces.

- [ ] Add `inspect_keeps_stale_record`, `inspect_rejects_invalid_pid`, and `saved_reference_is_read_only`. Use temporary records; compare bytes and directory entries before/after.
- [ ] On Dell run `cargo test --locked -p lobo-core inspect_` and `cargo test --locked -p lobo-core saved_reference_`. Expect the named tests to fail before implementation.
- [ ] Implement raw parse/validation without calling `remove_if`, taking a connection mutation lock, spawning a helper, or reading private key files.
- [ ] Keep `StateFile::read` behavior unchanged for existing callers. A dead PID does not erase the inspected record.
- [ ] Rerun both commands and existing `cargo test --locked -p lobo-core local::state::tests`. Expect named tests to pass and existing state behavior unchanged.
- [ ] Orchestrator checks the diff for accidental runtime changes. Retain this work for the P1 commit.

### Task 2: Classify idle, active, and unknown runtime state

**Files:** Create/Test `crates/lobo-core/src/control/update_safety.rs`; Modify `crates/lobo-core/src/control/mod.rs`.
**Interfaces:** Consume Task 1; produce `RuntimeActivity` and `inspect_runtime`.

- [ ] Add table cases `idle_without_records`, `active_cloud_blocks`, `active_local_blocks`, `saved_provider_without_key_is_unknown`, `unresolved_saved_reference_is_unknown`, and `inspection_never_calls_delete`. Fake providers count list/rent/delete calls.
- [ ] On Dell run `cargo test --locked -p lobo-core --features testkit update_safety::tests`. Expect the new API tests to fail initially.
- [ ] Inspect the local record with `inspect`, then validate PID and boot ID using existing process-identity rules. A dead or unrelated PID is not an active owned supervisor; retain its record.
- [ ] If process identity cannot be inspected, return Unknown. If a configured cloud provider cannot list or saved ownership cannot be resolved, return Unknown. If any known runtime is active, return Active; otherwise require complete evidence for Idle.
- [ ] Skip `LocalProvider::list`, `control::snapshot`, `connection::attach`, cleanup, and image/release resolution. No agent health request is needed to know that an instance exists.
- [ ] Rerun the named suite. Verify zero rent/delete/connection mutations in every case; keep the change for the P1 commit.

### Task 3: Add the reversible controller reservation

**Files:** Modify `app/src-tauri/src/controller.rs`, `app/src-tauri/src/controller/tests.rs`, `app/src-tauri/src/commands.rs`.
**Interfaces:** Produce `begin_update`, `UpdateGuard::commit_restart`, `UpdateGuard::require_recovery`, `update_reserved`, and `save_config`.

- [ ] Add focused tests `update_guard_rejects_start_stop`, `update_guard_waits_for_poll`, `update_guard_excludes_config_save`, and `dropping_update_guard_keeps_polling`.
- [ ] On remote macOS run `cargo test --locked --manifest-path app/src-tauri/Cargo.toml controller::tests::update_` and `cargo test --locked --manifest-path app/src-tauri/Cargo.toml controller::tests::dropping_update_guard`. Expect failures before the reservation exists.
- [ ] Track an update reservation and in-flight poll/config activity under controller synchronization. Reserve before awaited work; release on every return path. Prevent new polls while installation owns the reservation.
- [ ] Give draining existing poll/config activity a two-second cancellable budget. On timeout release the reservation and return a busy error. Never overlap an old connection-attach result with installation.
- [ ] Route `config_save` through `Controller::save_config`. Preserve its existing validation, refresh, and errors outside an update.
- [ ] Make `UpdateGuard` release the reservation on ordinary drop. `commit_restart` cancels polling after successful installation. `require_recovery` stops further polls and runtime/config mutations after an incomplete replacement; it permits ordinary Quit and release-page access. Neither terminal path may resume Start.
- [ ] Rerun the focused tests and existing `controller::tests` remotely. Keep ordinary ready-runtime Quit behavior unchanged. Retain for P1.

### Task 4: Execute installation only under a fresh idle check

**Files:** Modify/Test `app/src-tauri/src/backend.rs`; Modify fake Backend implementations in `app/src-tauri/src/controller/tests.rs`.
**Interfaces:** Add `InstallJob`, `Backend::update_activity`, and `Backend::install_when_idle`.

- [ ] Add `install_job_not_run_when_busy`, `install_job_holds_operation_lock`, `fresh_config_can_update`, and `install_worker_keeps_lock_until_finished`.
- [ ] On remote macOS run `cargo test --locked --manifest-path app/src-tauri/Cargo.toml backend::tests::install_` and `cargo test --locked --manifest-path app/src-tauri/Cargo.toml backend::tests::fresh_config_can_update`. Expect the new tests to fail first.
- [ ] For update checks only, construct default inspection dependencies when config is absent. Malformed existing config/state remains Unknown; do not change startup readiness.
- [ ] Acquire the same persistent `operation.json.lock` used by existing Start/Stop, with a two-second deadline. Refuse pending/corrupt ownership without reconciling or deleting it.
- [ ] Run Task 2 inspection under a 15-second total deadline while holding the lock. Only Idle can run `InstallJob`.
- [ ] Run the job through `spawn_blocking` and await its completion without abandoning it on cancellation. Keep the operation guard alive through completion and propagate join failures as installation failures.
- [ ] Rerun focused tests; assert no nested operation-lock acquisition and no destructive provider calls. Retain for P1.

### Task 5: Define update state and service ownership

**Files:** Create `app/src-tauri/src/updater/mod.rs`, `state.rs`, `tests.rs`; Modify `app/src-tauri/src/types.rs`; generate the four new files in `app/ui/src/gen/`.
**Interfaces:** Produce all update state types, `UpdateService`, `UpdateClient`, `UpdatePackage`, `VerifiedUpdate`, and callback types.

- [ ] Add `update_state_json_contract`, `candidate_bytes_are_owned`, and `quit_cancels_check_only`. Assert exact snake_case values, nulls, and private ownership of candidates.
- [ ] On remote macOS run `cargo test --locked --manifest-path app/src-tauri/Cargo.toml updater::tests::update_state_`. Expect failure before types exist.
- [ ] Define `UpdateService::new(current_version: String, client: Arc<dyn UpdateClient>, controller: Arc<Controller>, state_dir: PathBuf, clock: Arc<dyn lobo_core::clock::Clock>, emit: UpdateEmitter, restart: RestartApp) -> Arc<Self>`.
- [ ] Use one owned worker per check/download or installation. `prepare_quit()` cancels and joins check/download work; if installation has started, await its worker rather than aborting it.
- [ ] Export the update types through the existing `ts-rs` fixture test. Update its exact inventory; keep old generated types stable.
- [ ] Run `cargo test --locked --manifest-path app/src-tauri/Cargo.toml app_types_export_without_bigint` remotely, then `pnpm -C app/ui check` on the laptop or CI. Expect the new files to use `number` and no errors.
- [ ] Inspect only task-owned generated diffs. Retain for P1.

### Task 6: Wrap the real Tauri plugin and check versions

**Files:** Modify `app/src-tauri/Cargo.toml`, `app/src-tauri/Cargo.lock`, `app/src-tauri/src/updater/mod.rs`, `tests.rs`, `app/src-tauri/src/lib.rs`.
**Interfaces:** Implement `TauriUpdateClient` and `TauriUpdatePackage`; keep the service traits unchanged.

- [ ] Pin `tauri-plugin-updater = "=2.13.1"`, `nix = { version = "=0.31.3", features = ["fs"] }`, and compatible locked `semver`, `sha2`, and `url` dependencies. Keep Tauri itself at `=2.12.0`.
- [ ] On remote macOS run `cargo check --manifest-path app/src-tauri/Cargo.toml`, then `cargo check --locked --manifest-path app/src-tauri/Cargo.toml`. Expect dependency resolution and compilation. If the exact plugin pin conflicts, record the evidence and revise the plan; do not silently upgrade Tauri.
- [ ] Add `adapter_rejects_equal_or_older`, `adapter_rejects_wrong_target`, `adapter_rejects_signed_version_mismatch`, `adapter_rejects_missing_signed_version`, `download_finishes_before_signature_acceptance`, and `production_configuration_is_fixed`. Unit tests create their own test app config; they do not depend on Task 12's packaged fixture.
- [ ] Register the plugin only when a valid embedded key/config exists. Before Task 12 provides the production key, ordinary development/UI startup uses a disabled adapter and must remain usable. A production release without its key fails validation; never ship a placeholder key.
- [ ] Build the metadata client with a 30-second request timeout; set the checked Update object's download timeout to 15 minutes. Enforce stable SemVer and the ARM64 target. Set `requireSignedVersion: true` and `allowDowngrades: false`. Signature-version mismatch or absence is a verification failure.
- [ ] Require the initial artifact URL to identify an exact release in `1905/lobocode`. Permit GitHub's normal HTTPS asset redirects; do not require the final CDN hostname to equal `github.com`.
- [ ] Map known plugin errors to `UpdateFailure` without including credentials or complete signed redirect query strings. Verify the downloaded bytes before constructing `VerifiedUpdate`.
- [ ] Run `cargo test --locked --manifest-path app/src-tauri/Cargo.toml updater::tests::adapter_` and `cargo test --locked --manifest-path app/src-tauri/Cargo.toml updater::tests::download_finishes_` remotely. Retain for P1.

### Task 7: Schedule checks and retain verified downloads

**Files:** Modify/Test `app/src-tauri/src/updater/mod.rs`, `state.rs`, `tests.rs`.
**Interfaces:** Implement `start`, `check`, and progress through the already-defined state and callbacks.

- [ ] Add focused tests `schedule_delays_first_check`, `schedule_coalesces_sleep`, `manual_check_coalesces`, `signature_failure_keeps_installed_app`, and `new_download_failure_keeps_verified_candidate`.
- [ ] Run `cargo test --locked --manifest-path app/src-tauri/Cargo.toml updater::tests::schedule_` and `cargo test --locked --manifest-path app/src-tauri/Cargo.toml updater::tests::manual_check_` remotely. Expect failures before scheduling exists.
- [ ] Use cancellable Tokio timers with missed intervals skipped. Use the injected clock for displayed timestamps. Reset transient backoff after success.
- [ ] Implement the exact schedule/timeouts from Global constraints. A manual retry may retry a rejected candidate; background checks do not redownload an identical rejected candidate.
- [ ] Accumulate chunk deltas into byte totals. Unknown totals remain null. Keep the old verified candidate until a new candidate completes verification; installation snapshots one candidate and cannot switch midway.
- [ ] Disable production automatic checks in debug and `0.0.0-dev` builds. Manual check in an ordinary dev build returns `disabled`; a compiled test configuration can opt into its fixture source.
- [ ] Implement `runtime_changed()` to coalesce advisory blocker refreshes after controller activity transitions and opening Updates. Until the first inspection completes, report `state_unknown`. Compare a small activity summary before refreshing; routine one-second progress events must not cause repeated provider listings. A fresh install preflight remains mandatory.
- [ ] Rerun the declared focused tests remotely. Verify the worker count never exceeds one and a check needs no provider credentials. Retain for P1.

### Task 8: Install, recover guards, and verify restart receipts

**Files:** Modify `app/src-tauri/src/updater/mod.rs`, `state.rs`, `tests.rs`, `controller.rs`, `controller/tests.rs`, `commands.rs`, `lib.rs`.
**Interfaces:** Implement `UpdateService::install`, pending/result receipts, and `RestartApp`.

- [ ] Add `install_requires_user_command`, `install_failure_releases_guards`, `install_partial_failure_blocks_start_but_allows_quit`, `quit_waits_for_installer`, `receipt_success_records_observed_version`, and `receipt_mismatch_does_not_loop`.
- [ ] On remote macOS run `cargo test --locked --manifest-path app/src-tauri/Cargo.toml updater::tests::install_` and `cargo test --locked --manifest-path app/src-tauri/Cargo.toml updater::tests::receipt_`. Expect failures before installation/receipt behavior exists.
- [ ] Acquire `update-install.lock` in the app config directory and the reversible controller guard. Use nonblocking file-lock acquisition with the two-second budget. Never unlink a lock held by another process.
- [ ] Check the canonical running bundle and destination parent. A mounted disk image, unsupported standalone binary location, or non-writable path fails with `location` before replacement. Do not elevate privileges.
- [ ] Call `Backend::install_when_idle` with a job that writes the pending receipt atomically, fingerprints the old bundle, and installs the private verified bytes.
- [ ] On installer error, compare the complete old bundle tree, including file bytes, relative paths, symlink targets, and executable permissions. Resume ordinary operation only if it is intact; otherwise consume `UpdateGuard::require_recovery`, report `recovery_required`, and block Start. If inspection fails, require recovery. Do not claim rollback.
- [ ] On success, consume `UpdateGuard::commit_restart` and call Tauri `request_restart`. Do not route the restart through ordinary Quit cleanup or `app.exit(0)`.
- [ ] During ordinary Quit, call `prepare_quit()` before the existing controller Quit path. Repeated clicks or OS graceful Quit cannot start a second installer or release a worker's locks early.
- [ ] On next launch, compare actual Tauri package version with the pending receipt. Atomically record success/mismatch before removing pending state. No automatic restart retry.
- [ ] Rerun the named tests remotely; also run existing Quit regression tests. Retain for P1.

### Task 9: Build the compact update view

**Files:** Create `app/ui/src/updates/Updates.svelte`, `view.ts`, `Updates.test.ts`.
**Interfaces:** Implement `updateView`; `Updates.svelte` accepts optional `fixture: UpdateState` and `rendering: boolean` props for renders, using real IPC otherwise.

- [ ] Add presentation tests for all eight phases, four blocking reasons, unknown totals, long version strings, and recovery-only errors. Test primary action selection rather than mirroring component internals.
- [ ] On laptop/CI run `pnpm -C app/ui test -- src/updates/Updates.test.ts`. Expect missing-function failures before implementation.
- [ ] Implement the pure `UpdateView` mapping with exact operational labels from the spec. Use existing widgets and Node-based Vitest; do not add a DOM test framework.
- [ ] Implement the view at 440 logical pixels. Bound error details and link to release notes; no Markdown renderer or scroll region.
- [ ] Subscribe before reading initial state. Ignore a late initial snapshot after a newer event; dispose listeners on view destruction.
- [ ] Rerun the focused test, `pnpm -C app/ui check`, and `pnpm -C app/ui build`. Retain for P1.

### Task 10: Connect IPC, windows, and entry points

**Files:** Modify `app/src-tauri/src/lib.rs`, `commands.rs`, `windows.rs`, `capabilities/default.json`, `app/ui/src/lib/api.ts`, `App.svelte`, `panel/Footer.svelte`, `panel/Panel.svelte`, `panel/SetupCard.svelte`.
**Interfaces:** Implement the commands/event/wrappers in the interface table. Implement no-argument `open_update_release` as the constrained release-page action.

- [ ] Add focused command/window tests for update access without config, the exact `updates` label, and rejection of unsupported mutation during installation.
- [ ] Register service startup only for the GUI entry path. The existing helper/supervisor entry paths must not create an update service.
- [ ] Call `runtime_changed()` when Updates opens and when the controller emits an activity transition. Resolve the managed service through the app handle after setup; do not create a strong reference cycle between controller and service.
- [ ] Add `updates` to native window creation and minimal capabilities. Preserve native title bars and the existing measured inset; use the 440-pixel width only for the new view.
- [ ] Add `?view=updates` handling and exact typed IPC wrappers. Replace the footer config shortcut with Updates; keep config actions in Settings. Include the setup entry point.
- [ ] Display desktop version in the Updates view using service state. Do not relabel the existing runtime-version display as the desktop version.
- [ ] On laptop/CI run `pnpm -C app/ui check`, `pnpm -C app/ui build`, and `pnpm -C app/ui test -- src/updates/Updates.test.ts`. On remote macOS run focused `windows::tests` and `updater::tests` checks.
- [ ] Verify production capabilities contain no updater/process plugin grants or arbitrary frontend URL/key arguments. Retain for P1.

### Task 11: Deliver P1 native UI QA and commit

**Files:** Modify `app/src-tauri/tests/fixtures.rs`, `app/ui/src/render/Render.svelte`, `app/e2e/app.spec.js`, `app/e2e/wdio.conf.js`, `app/e2e/tauri.conf.json`, `tools/native_app_e2e.py`, `Makefile`, `CHANGELOG.md`; generate `app/ui/src/fixtures/updates.json`.
**Interfaces:** `tools/native_app_e2e.py --setup-only --updates-only`; `make app-e2e-build` remains an isolated UI bundle build. A test-only `set_update_fixture(state: UpdateState)` command exists only behind `e2e`.

- [ ] Generate one fixture inventory covering all update states, long content, known/unknown totals, and no-config startup. Regenerate on remote macOS using `make app-fixtures`; commit the generated data with P1.
- [ ] Extend the render route to select an update fixture from that inventory. Never send production network requests while rendering fixtures.
- [ ] In `e2e` without the separate `updater-test` feature, use an inert update adapter: fixture states may render, but archive replacement is impossible.
- [ ] Extend the driver config so `--updates-only` selects update UI tests and can access the `updates` window. Preserve `--setup-only` as the laptop safety requirement.
- [ ] Run `pnpm -C app/ui check`, `pnpm -C app/ui test`, `pnpm -C app/ui build`, then `make app-e2e-build` and `python3 tools/native_app_e2e.py --setup-only --updates-only` on the laptop.
- [ ] Inspect the actual native window, movement, corners, and every control. Record layout JSON/screenshots with no document overflow or clipped controls. A browser render alone is insufficient.
- [ ] Run required focused backend checks on the allowed remote hosts. If remote macOS CI is held, leave that gate pending; do not report backend acceptance.
- [ ] Add a short Unreleased changelog entry for shipped UI behavior and pending real-install tests. Orchestrator reviews the full P1 diff and commits only the listed feature files with `feat(app): add signed update checks and native update controls` once its required gates pass.
- [ ] Deliver the P1 QA artifact/revision and exact smoke results. State that release packaging, full failure testing, and production A-to-B acceptance remain pending.

## P2 — Signed GitHub delivery

### Task 12: Set up signing and package the same app once

**Files:** Modify `app/src-tauri/tauri.conf.json`, `app/src-tauri/Cargo.toml`, `Makefile`; Create `app/src-tauri/tauri.release.conf.json`, `app/e2e/tauri.updater-test.conf.json`, `docs/app-updates.md`.
**Interfaces:** `make package-dmg APP_BUNDLE=...` packages an existing bundle without rebuilding. `make dmg` keeps its current build-plus-package purpose. Feature `updater-test = ["e2e"]` enables real installation only in isolated test builds.

- [ ] Generate a disposable signing key for fixture acceptance. For production, generate the permanent key only after implementation/key setup is authorized; never generate it during plan preparation.
- [ ] Embed only the production public key in the production config. Enable `requireSignedVersion: true` and keep `allowDowngrades: false` in production and real-install fixture configs. Configure `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` as GitHub secrets only when external secret setup is authorized.
- [ ] Record public-key fingerprint and private backup location in the private execution record. Public docs describe backup procedure without machine-specific secret paths. Verify the backup by signing a tiny non-release fixture.
- [ ] Add the release overlay with `bundle.createUpdaterArtifacts: true`. Ordinary development and UI builds must not require production secrets.
- [ ] Add the test overlay with identity `io.github.1905.lobocode.updater-test`, generated test public key/endpoint through build-time overlays, and the existing native driver permission. Production builds reject test overrides.
- [ ] Split `package-dmg` from `mac`, preserving its Applications shortcut. Accept the explicit ARM64 target output path; package that bundle directly after the action builds it.
- [ ] On remote macOS build a disposable signed release fixture and run `codesign --verify --deep --strict` on the app. Expect archive and signature alongside the app, and a DMG containing the same bytes.
- [ ] On laptop run ordinary `make mac` with no signing-secret variables. Expect a successful UI build without production updater artifacts. Do not run installation tests here. Retain for P2.

### Task 13: Validate release contents before publication

**Files:** Create/Test `tools/verify_app_release.py`; Modify `docs/app-updates.md`.
**Interfaces:** CLI modes `--self-test` or `--mode local|draft|public --tag TAG --asset-dir DIR --config FILE --output FILE`. Repository is fixed to `1905/lobocode`. Exit 0 means verified; any required check failure exits nonzero and writes a sanitized result report.

- [ ] Add embedded self-tests for missing assets, duplicate platform keys, wrong tag/repo URL, embedded-version mismatch, wrong architecture, unsafe archive entries, and invalid signatures.
- [ ] On Dell run `python3 tools/verify_app_release.py --self-test`. Expect the declared negative/positive cases to fail before their checks are implemented, then pass after implementation.
- [ ] Parse JSON strictly, including duplicate-key rejection. Require the DMG, update archive, signature, manifest, four CLI archives, and CLI checksums. Do not pretend the CLI checksum file covers app files it did not generate.
- [ ] Decode Tauri's public-key and signature wrappers into temporary Minisign files. Run `minisign -V -m ARCHIVE -p PUBLIC_KEY -x SIGNATURE` through subprocess arguments. A signature string match alone is not verification.
- [ ] After cryptographic verification succeeds, require exactly one `version:` field in the signed trusted comment. It must equal the tag, manifest, and archive's embedded version. Reject a legacy signature without a version. The pinned Tauri CLI supplies the field during `tauri build`.
- [ ] Inspect the archive without unsafe extraction. Require one expected `.app`, correct identifier, stable tag/version, ARM64 executable, and valid code signature on macOS. Compare normalized app contents with the mounted read-only DMG in macOS validation.
- [ ] In draft mode, use CI's authenticated `gh release download` to obtain uploaded bytes, then validate them locally against build hashes. Never pass CI tokens into the generated manifest.
- [ ] In public mode, clear authentication from the download client, follow HTTPS redirects, and fetch the Latest manifest plus its exact-tag archive. Verify the same signature/digest/version assertions and retain a JSON report.
- [ ] Rerun self-tests on Dell and macOS-specific fixture validation remotely. Retain for P2.

### Task 14: Publish the existing formula only after public verification

**Files:** Create `tools/publish_brew_formula.sh`; Modify `.goreleaser.yaml`, `tools/verify_app_release.py`, `docs/app-updates.md`.
**Interfaces:** `publish_brew_formula.sh --tag TAG --formula FILE --verification FILE [--dry-run]`; `--self-test` uses task-owned temporary git repositories and never contacts GitHub.

- [ ] Add self-test cases for failed/missing public verification, wrong tag/digest, repeated identical formula, stale version, unrelated tap files, and dry-run without push.
- [ ] On Dell run `bash tools/publish_brew_formula.sh --self-test` and `bash -n tools/publish_brew_formula.sh`. Expect missing/invalid evidence to be rejected.
- [ ] Set GoReleaser release `draft: true`, `use_existing_draft: true`, and `replace_existing_draft: false`. Set `brews[].skip_upload: true`; keep generating the formula and existing archives.
- [ ] Require a successful public report for the same tag and formula digest. The verifier records that digest during draft verification and carries it into public verification.
- [ ] Use the existing `HOMEBREW_TAP_KEY` only in the publication job. Write only `lobo.rb` in `1905/homebrew-tap`; use explicit per-command SSH configuration and preserve other tap content.
- [ ] If the tap already contains the exact formula, succeed without a new commit. If it contains a newer version, refuse to overwrite it. On a push conflict, fetch and reassess before a narrow retry; never force-push.
- [ ] Run the self-tests and `goreleaser check` with the pinned GoReleaser remotely. No paid upgrade or cask migration. Retain for P2.

### Task 15: Assemble and publish complete releases

**Files:** Modify `.github/workflows/release.yml`, `.goreleaser.yaml`, `Makefile`; Test through Task 13/14 scripts and an authorized draft workflow run.
**Interfaces:** Workflow jobs `prepare`, `cli`, `app`, `verify`, `publish`, `public-smoke`, `homebrew`.

- [ ] Set repository-wide release concurrency without cancelling a running release. Keep draft repair separate from publication of an already-public tag.
- [ ] `prepare`: validate stable tag, intended revision, required check results, allowed release action, and current release state. Refuse older-than-Latest promotion and published-asset replacement.
- [ ] `cli`: generate/upload CLI archives, checksums, and formula to a reusable draft. Upload the generated formula as a workflow artifact for the final job; expose the draft release ID.
- [ ] `app`: use `tauri-apps/tauri-action@1deb371b0cd8bd54025b384f1cd735e725c4060f`, explicit tag/release ID, matching draft status, project path `app`, and the project's pinned Tauri CLI. Build the UI first and pass the exact tag-derived version/build metadata.
- [ ] Build only `aarch64-apple-darwin` with the release overlay. Let the action generate updater JSON and upload its archive/signature; run `package-dmg` on that exact output and upload `lobocode.dmg` to the draft.
- [ ] `verify`: run Task 13 against all uploaded draft assets. Validate the manifest contains the supported target and exact-tag URLs. Never publish if any required job fails.
- [ ] `publish`: reread Latest and release state, then publish the completed draft. This is the only job allowed to promote Latest.
- [ ] `public-smoke`: run anonymous Task 13 validation. `homebrew`: run Task 14 only after successful public verification, using the preserved formula artifact.
- [ ] Scope updater signing secrets to `app`, release write permission to release jobs, and tap credentials to `homebrew`. Do not expose secrets in pull-request jobs or public reports.
- [ ] Validate workflow syntax and pinned GoReleaser configuration. When CI/release testing is authorized, exercise a draft-only run that cannot promote Latest or push the tap. Retain for P2.

### Task 16: Prove a real signed A-to-B update remotely

**Files:** Create/Test `tools/test_app_updater.py`; Modify `app/e2e/app.spec.js`, `app/e2e/wdio.conf.js`, `app/e2e/tauri.updater-test.conf.json`, `app/src-tauri/src/updater/tests.rs`.
**Interfaces:** `python3 tools/test_app_updater.py --suite smoke|full --output DIR`. Installation suites require macOS and `GITHUB_ACTIONS=true`; refuse local laptop execution. A `--self-test` mode only validates harness planning and temporary-path cleanup without launching an app.

- [ ] Add harness self-tests for refusing the wrong host, preserving unrelated paths/processes, and returning failure when a restarted app never reports its expected version.
- [ ] Create a unique temporary root, disposable keys, and isolated A/B bundles. Use fixture-only versions `0.0.1` and `0.0.2`, test identifier, temporary config/state, and test-only HTTP transport. No production secrets.
- [ ] Launch a loopback fixture feed bound to an allocated port. Embed its endpoint and test key at build time. The production app cannot accept those values through environment, IPC, or query parameters.
- [ ] On GitHub-hosted macOS, run `python3 tools/test_app_updater.py --suite smoke --output bin/app-updater-smoke`. Drive the real native UI to download B and click Install and restart.
- [ ] Reconnect to the restarted B process using the existing embedded driver, then inspect the receipt, actual executable version, and normalized bundle contents.
- [ ] Assert config, preferences, model sentinel bytes, and optional CLI sentinel bytes are unchanged. No model or GPU process starts.
- [ ] Stop only the fixture server, drivers, and child PIDs owned by this run. Preserve evidence on failure; never issue broad process kills or delete another app installation.
- [ ] Require `result.json` with A/B versions, signature outcome, observed restart version, file-preservation checks, and cleanup result. Keep screenshots/logs as artifacts. Retain for P2.

### Task 17: Wire required CI, deliver P2 QA, and commit

**Files:** Modify `.github/workflows/rust.yml`, `Makefile`, `CHANGELOG.md`, `docs/app-updates.md`.
**Interfaces:** `make app-updater-smoke` invokes Task 16 smoke on GitHub macOS; `make app-updater-full` invokes the full suite there. Both refuse unsupported execution through the harness.

- [ ] Add relevant `tools/verify_app_release.py`, `tools/publish_brew_formula.sh`, and `tools/test_app_updater.py` paths to the Rust workflow triggers.
- [ ] Run updater Rust tests on macOS; core read-only tests on the Linux core job; verification-script self-tests on Linux. Keep updater installation in a separate macOS job with no provider credentials.
- [ ] Install verified Minisign 0.12 tooling for artifact tests. Record its actual version; fail the verification job if the required tool is absent.
- [ ] Keep the UI smoke target UI-only on every host. Installation suites are invoked only by the explicit updater job, never by `make app-e2e` on the laptop.
- [ ] Run all normal required CI checks plus signed A-to-B smoke on the exact candidate commit, when CI execution is authorized. Do not broaden into full failure tests before the first usable QA delivery.
- [ ] Update Unreleased changelog and the runbook with verified behavior and pending full/public acceptance. Orchestrator reviews P2 and commits the listed files with `feat(release): distribute signed app updates through GitHub` after its required gates pass.
- [ ] Deliver the app/DMG artifact and exact revision for QA. State that fixture A-to-B succeeded only if its report proves it. Public Latest and Homebrew publication remain held.

## P3 — Full acceptance and delivery

### Task 18: Complete the failure and concurrency matrix

**Files:** Modify/Test `app/src-tauri/src/updater/tests.rs`, `app/src-tauri/src/controller/tests.rs`, `app/src-tauri/src/backend.rs`, `crates/lobo-core/src/control/update_safety.rs`, `tools/test_app_updater.py`, `tools/verify_app_release.py`, `tools/publish_brew_formula.sh`, `app/ui/src/updates/Updates.test.ts`, `app/e2e/app.spec.js`.
**Interfaces:** Extend existing test cases and `--suite full`; do not change approved production behavior.

- [ ] Add failing regression tests for the matrix rows below before each affected fix. Keep test assertions about externally visible outcomes and forbidden mutations.
- [ ] Run core/script tests on Dell and app Rust/lifecycle tests on remote macOS. Run frontend/native UI-only checks on the laptop if needed.
- [ ] Run `cargo test --workspace --locked --features lobo-cli/test-fakes -j 2` and `make rust-lint` on the authorized remote host; run `make app-lint app-test` and `make app-updater-full` on GitHub macOS. These are follow-up checks after QA delivery.
- [ ] Run `pnpm -C app/ui test`, `pnpm -C app/ui check`, `pnpm -C app/ui build`, and native setup/update UI smoke. Check existing Start/Stop/Quit UI regression behavior.
- [ ] Any discovered fix gets focused verification, a clear changelog entry, and normal authorized QA delivery before rerunning only the affected broader checks.

| Case | Required result |
|---|---|
| Equal/older/prerelease/wrong-platform metadata | No installation; correct current/error state, no accidental downgrade. |
| Invalid JSON, duplicate target, missing field, missing Latest manifest | Explicit release error, never false “up to date.” |
| Wrong signature or corrupted payload | Rejected before installer call; installed bundle unchanged. |
| Old signed archive advertised as a newer version, or signature without a version | Rejected before installation; signed version must match the manifest and tag. |
| Unknown length, mid-stream disconnect, slow metadata/download | Accurate progress, bounded failure, scheduled/manual retry. |
| Sleep/wake, rapid manual checks, two visible windows | One active operation, no missed-event overwrite or interval replay storm. |
| New candidate fails after an older candidate is verified | Keep the older verified package; no version/byte mismatch. |
| Start, Stop, config save, poll, and Quit compete with install | One owner; locks survive worker completion; no leaked reservation. |
| Local PID reused, dead PID record, unreadable process identity | No kill/delete; Idle only on sufficient evidence, Unknown on uncertainty. |
| Missing provider credentials plus saved cloud ownership | Installation blocked as unknown; no hidden fallback or cleanup. |
| Two app processes attempt installation | One installer; other receives busy, no lock-file deletion. |
| Read-only location, mounted DMG, insufficient write space | Preflight or install error with correct recovery instructions; no privilege escalation. |
| Error before replacement | Old app remains launchable; controller can Start/Stop/Quit again. |
| Interruption during replacement, restart never reaches B | Honest failure result and documented manual DMG recovery; no rollback claim or restart loop. |
| Receipt corrupt or disk full while writing receipt | Refuse replacement before a durable receipt exists; preserve config/model files. |
| Restart reports old version | Persist mismatch once; no automatic retry. |
| Draft build/upload failure | Previous public Latest unchanged; no formula publication. |
| Published-tag rerun, older tag, formula push failure | No overwrite/downgrade; formula-only retry uses preserved verified input. |
| Production environment/query/IPC attempts to set test endpoint/key | Ignored or rejected; production configuration stays fixed. |

### Task 19: Final review, documentation, and P3 commit

**Files:** Modify `README.md`, `CHANGELOG.md`, `docs/app-updates.md`, `docs/implementation-mistakes.md`, this plan's status/evidence fields, and spec As-built notes after delivery.

- [ ] Complete the required full checks from Task 18. Record exact revisions, commands, hosts, and retained reports; do not mark held public acceptance as passed.
- [ ] Orchestrator self-reviews scope, source-of-version, secret handling, lifecycle ownership, and archive/signature validation. Run the one final whole-branch Codex review required by the execution skill; do not add per-task, per-phase, or Claude reviews.
- [ ] Fix supported findings with focused tests. Preserve the rule that app updates cannot modify the installed Homebrew CLI or select a GPU image version.
- [ ] Document one-time bootstrap DMG installation, background downloads, restart consent, Gatekeeper warning, signing-key backup, lost-key migration, and manual recovery.
- [ ] Update the changelog with concrete shipped behavior and material validation limits. Reconcile spec As-built notes only with implemented evidence; retain approval history.
- [ ] Commit the reviewed P3 changes with `test(app): verify updater failure and recovery behavior`. Deliver final local/fixture acceptance results. Leave public release tasks pending until explicitly authorized.

### Task 20: Authorized public delivery

**Files:** No new product scope; use release workflow and runbook. Update status/evidence in `docs/app-updates.md`, `CHANGELOG.md`, and this plan when results exist.

- [ ] Confirm the release hold is explicitly lifted and the relevant public GPU-image prerequisites are satisfied. Plan approval alone is insufficient.
- [ ] Merge tested work into the approved target branch without a PR. For workflow-file pushes, use the explicit GitHub SSH identity from global instructions; never force-push.
- [ ] Tag the intended version from the exact verified revision. Run the complete draft/build/verify/publish/public-smoke/formula sequence.
- [ ] Verify anonymous Latest download, actual app version/signature, and the formula's intended CLI archive checksums. Confirm the user's installed CLI binary was not replaced by the app.
- [ ] Notify QA readiness with the exact release and checks. Use the authorized `notify` skill for release notification; do not send local links to the phone.
- [ ] If public smoke fails, stop formula delivery and report the failed release. Do not remove or rewrite public artifacts silently. Use a higher app version for corrective delivery.

### Task 21: Verify the first real production upgrade

**Files:** Update acceptance evidence in `docs/app-updates.md` and plan/spec status.

- [ ] Manually install the first updater-enabled public DMG once in an approved test environment.
- [ ] When a real higher release exists, prove that installation downloads from public GitHub, verifies the production signature, replaces the app, and restarts into the expected version.
- [ ] Confirm config, models, optional CLI, and latest-image behavior remain correct. Record the public source/target versions and receipt report.
- [ ] Keep this acceptance pending until evidence exists. Do not publish an empty artificial update solely to close the plan.
- [ ] After every required task and acceptance gate passes, set plan status to `done`, reconcile spec As-built notes, and move the feature directory to `plans/done/` through the normal delivery commit.

## Spec coverage and delivery gates

| Spec requirement | Tasks | Gate/evidence |
|---|---|---|
| GitHub-only endpoint, signed supported newer app | 5–7, 12–13, 15–16 | Adapter tests, verified archive, fixture A-to-B, later public A-to-B. |
| Six-hour schedule, retries, cancellation, retained candidates | 5, 7, 18 | U1–U3 and Review focus 4. |
| Native compact interface before setup | 9–11, 18 | U7, real native screenshots/layout/movement evidence. |
| Read-only all-provider preflight and shared locking | 1–4, 8, 18 | U4–U5, no mutation counters, lock/concurrency tests. |
| Recoverable failure and restart receipts | 3, 5, 8, 16, 18 | U4/U6; expected versus observed version; honest interruption limits. |
| Same app in DMG and update archive | 12–13, 15 | Normalized bundle equality and architecture/signature checks. |
| Draft completeness before Latest and formula | 13–15, 17–18, 20 | U8, draft failure and formula-only retry evidence. |
| Key backup, no production secrets in tests/app | 6, 12–13, 15–18 | Secret scope, disposable fixture key, production override rejection. |
| Correct host and smoke-before-full-test order | Baseline, 11, 16–19 | Host-tagged reports; QA delivery before broad follow-up. |
| Separate CLI and GPU-image update paths | 2, 8, 16, 18–21 | Unchanged sentinel/installed CLI and no image-version coupling. |
| Docs, changelog, first manual install, held release | 11–12, 17, 19–21 | Explicit delivery status and bootstrap/recovery instructions. |

P1/P2/P3 are delivery commits after clean orchestrator review. Individual task steps stay small and independently checked; they do not add a separate model review or public release. If a task reveals an unanticipated requirement, create a new plan version before implementing that scope.

## Sources and validation limits

- [Updater v2.13.1 release](https://github.com/tauri-apps/plugins-workspace/releases/tag/updater-v2.13.1): published 2026-09-29; selected plugin pin. Compatibility compilation has not run.
- [Tauri updater documentation](https://v2.tauri.app/plugin/updater/): supported static manifest and signing workflow.
- [Updater v2.13.1 configuration](https://github.com/tauri-apps/plugins-workspace/blob/updater-v2.13.1/plugins/updater/src/config.rs) and [Tauri CLI v2.12.0 signing source](https://github.com/tauri-apps/tauri/blob/tauri-cli-v2.12.0/crates/tauri-cli/src/helpers/updater_signature.rs): version-bound signatures and `requireSignedVersion` are present in these pinned sources. Packaged behavior still needs fixture proof.
- [Pinned Tauri action](https://github.com/tauri-apps/tauri-action/tree/1deb371b0cd8bd54025b384f1cd735e725c4060f): resolved from `v1` during planning. Its build/upload behavior still needs the planned fixture validation.
- [Minisign verification](https://jedisct1.github.io/minisign/): CLI verification interface; v0.12 selected from its official release.
- [GoReleaser release settings](https://goreleaser.com/customization/publish/scm/) and [formula settings](https://goreleaser.com/customization/publish/homebrew_formulas/): draft reuse and deferred formula publication. Check against pinned v2.13.3 during execution.

Planning verified repository structure, current control flow, tool help, and upstream interfaces. It did not run builds, backend/lifecycle suites, native update installation, signing-key setup, or publication. Those checks remain unchecked tasks above.
