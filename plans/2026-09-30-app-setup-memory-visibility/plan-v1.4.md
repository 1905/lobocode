# App Setup and Runtime Visibility Implementation Plan v1.4

**Date:** 2026-09-30
**Status:** in-progress
**Spec:** ./spec.md (approved)
**Authorization:** The user approved implementation and testing in the expanded spec. Prior full-auto delivery authorization persists. Parent self-review completed. Execute under the approved implementation and testing instruction; no new scope permission is needed.
**Goal:** Safely operate the app-owned runtime, configure OpenCode, and show accurate memory/activity in a normal native Mac app.
**Architecture:** Add explicit app-scoped operations beside the unchanged CLI operations. Keep JSONC updates and measured telemetry in shared Rust modules; expose sanitized results through the existing Tauri backend and store. Run display sampling independently from the existing watchdog.
**Tech Stack:** Rust/Tokio/serde, jsonc-parser 0.33.2 CST, macOS libproc, Svelte/Tauri, Vitest, WebdriverIO and Python native harnesses.

> For agentic workers: use superpowers:subagent-driven-development to implement task-by-task. Checkbox syntax tracks completion.

## CI path correction

GitHub requires a dispatch workflow to exist on the default branch. `Pod image` is already registered as workflow369370768; a new workflow is not. Extend its branch version with a prebuilt mode. Normal reusable Q6/Q8 builds and release promotion remain unchanged. The temporary runner gets only the completed OCI directory read-only, no host Docker socket or private home.

## Priority cloud E2E — latest user instruction

The user now defers local tests and explicitly requests cloud E2E after asking about both TUI and Mac. RunPod rental and cloud-client checks are authorized. Preserve all previous implementation work; no local inference, OpenCode launch, app installation or app/CLI public release.

File map: create `tools/cloud_runtime_e2e.py`, the prebuilt mode in `.github/workflows/pod-image.yml`, and `tools/verify_cloud_candidate.py`;  update this plan, spec, results/investigation, AGENTS.md and CHANGELOG.md. Reuse bounded HTTP and synthetic-token code. No new UI or provider feature is planned.

- [ ] C1: Verify RunPod credentials, account inventory and current public image tags. Prepare one complete Q6 image on Dell with catalog hashes, offline image validation, exact source revision and digest. Preserve paused jobs and document any necessary resume. Do not rent until image access succeeds. Resolve public publication/promotion hold only after the concrete image is ready.
- [ ] C1a: Prepare the prebuilt mode in `.github/workflows/pod-image.yml` with a read-only candidate verification default. Use a temporary isolated Dell runner, pinned official runner image and ORAS. Require exact source/image digests and verify OCI hashes/config. Keep `publish` and `promote_latest` false by default; promotion requires successful immutable publication and anonymous digest verification. No app/CLI release or latest-q8 changes. Run static checks and a verification-only CI job before requesting image publication/promotion approval.
- [x] C2: Add CloudRuntime adapter for expected RunPod instance/boot and private saved SSH connection. Require Q6/65,536 context, authenticated model metadata and unchanged identity. Run fake-only checks on Dell. No local weights, retry or arbitrary prompts.
- [ ] C3: Build the current CLI on Dell. Run real startup and status TUI under a PTY with isolated config/state. Resolve latest-q6 normally, then send one direct 47,000-input/32-output request. Capture time to first content, rates and exact counters. Delete only the task-owned pod and verify cleanup.
- [ ] C4: Run the verified hosted Mac app against RunPod with isolated provider config/state and safely preserved preferences. Check visible Cloud selection, Start, loading, Ready, short streamed response, scoped Stop and final Off without scrolling. No local model. Verify the exact cloud pod and tunnel are gone.
- [ ] C5: Record actual passed/failed checks, cloud cost and remaining limits. Update changelog and saved memory. Keep release/updater/local-model acceptance held. Do not infer model success from fake checks or provider creation alone.

C1 preflight: direct authenticated REST v1/v2 and GraphQL pod inventory all pass, zero pods. An initial urllib check returned 403; direct requests did not reproduce it. Anonymous latest-q6/latest-q8 manifests return 404. No rental has started.

Cloud checkpoint: C2 parent review and Dell fake-only checks pass. Actual Mac production artifact and Dell 80×24 TUI both fail cleanly at latest-q6 HTTP 404, before rental. Account remains empty. Task Mac app was quit and original target preference restored. Q6 build preparation is active in an isolated Dell worker; old paused image jobs are untouched. CLI binary 528322e is ready. Local GHCR upload capability probe returned 403 DENIED, so CI publication preparation is required before live provisioning can proceed.

Publication checkpoint: the direct GHCR upload-session probe returned 403 DENIED. Existing `1905/lobocode` is already public, so no new package or visibility change is planned. C1a adds a normal repository CI path using a temporary isolated runner; no registry write is authorized by this preparation alone.

C1a source checkpoint: candidate mode and the OCI verifier are prepared. Parent review plus Dell validation pass: 18 fake cases, Python compilation, actionlint and shell syntax. Actual candidate verification and verification-only CI remain pending. The pinned official runner image is downloaded on Dell; no runner is registered.

C1a review follow-up: reject OCI descriptor platform declarations that disagree with the runnable Linux AMD64 config, including enclosing indexes. Parent reviewed the fix; all 23 focused fixtures, Python compilation and workflow lint pass on Dell. Candidate image source is unchanged.

## Earlier local diagnostic — now deferred

The user explicitly requested a direct comparison without OpenCode using the same input size. This narrow authorization supersedes the earlier inference hold and 64-token input cap for one diagnostic. All other deferred work remains held. Existing Tasks 1–23 and their completed evidence remain unchanged below.

File map: create `tools/same_prompt_runtime_e2e.py`; modify only required transport seams in `tools/bounded_runtime_e2e.py`, this feature's evidence/docs, project memory and changelog. Use no new runtime feature or dependencies.

- [x] D1: Implement one-request Q6/65,536-context diagnostic with exactly 47,000 synthetic input IDs, at most 32 output tokens, no retries and a 30-minute generation limit. Keep the existing short harness defaults. Verify exact construction, bounds and failure handling with fake-only checks on Dell.
- [ ] D2: Retrieve hosted artifact `11095757309` from successful run `36713757006`, revision `b5617ae`. Verify bundle identity/signature. Prepare private task config/state using existing model/runtime. Start only its normal supervisor and require the production memory guard to pass. Run the diagnostic once; preserve numerical timing and token evidence. Stop the owned runtime and verify cleanup.
- [ ] D3: Record admission, exact input/output counts, time to first output, processing/generation rates, memory observations and comparison limits. If it succeeds, inspect OpenCode request construction/stream handling next. Do not launch OpenCode automatically. Keep native E2E/install/release deferred.

D1 completed: parent source review plus Dell Python compilation, existing HTTP fake suite and 12 new diagnostic scenarios pass. D2 attempted: verified hosted artifact and normal supervisor admission. Actual guard blocked at 26.7 GiB required / 9.4 GiB available after reserves. Zero model loads/generations; owned cleanup passed. D2 remains incomplete until a same-size request can run. D3 evidence is recorded in `results.md`; model throughput and the conditional OpenCode investigation remain pending. No admission override or smaller substitute test was used.

## Execution boundaries

- Earlier host constraint (overridden only by D1–D3 above): the user reported no available Mac memory. Keep this Mac to code edits for now. Run compilation and tests on Dell or hosted macOS. Local build/install/UI-check commands below remain pending until memory is available; do not start an app or model. E2E remains deferred.

- Earlier instruction (all other E2E remains deferred): E2E comes later, after fixes. Do not run native fixtures or scripted inference during implementation, and do not merge unverified memory code into master first.
- Create `/Users/kass/dev/lobocode-app-runtime` on `fix/app-setup-memory-visibility` from clean `master`, then merge memory checkpoint `597fb1c` into this isolated branch. Its code is `b9a1966`; native E2E remains pending. Preserve primary checkout and unrelated work. Do not restart image jobs or the updater.
- Use `/storage/lobocode-app-runtime` on Dell, copied to `/app-runtime` in the existing `lobo-public-image-check` container. Reuse `/memory-target` with two Cargo jobs; do not alter the image-job checkout or container lifecycle. App Rust and native adapter suites run on hosted macOS through normal Rust CI.
- During implementation this Mac runs only UI unit checks and builds. Native fixture/visual E2E and the explicitly authorized bounded live E2E are deferred until fixes are complete. Do not run `make app-test`, `make rust-test`, lifecycle suites, Docker or broad benchmarks here.
- First usable build: scoped Start/status/Stop, OpenCode configuration and Dock behavior. Deliver unit/build evidence before telemetry and broad acceptance. Native QA and all E2E remain deferred until fixes are complete.
- Normal branch push, required CI, merge and final push are authorized. No PRs. Public release, tags, image publication and GPU rental remain held.
- Install only the standalone app after its focused checks. Preserve user config, CLI, models and existing runtime ownership. Do not replace the optional CLI.
- Parent verifies each phase. One whole-branch Rival Codex code review after implementation; no per-task model reviews or automatic Rival plan review. Use workflow dispatch with `native_e2e=false` until deferred E2E resumes.
- Update the changelog in each delivery commit. Track unfinished validation in this file and `results.md`; do not mark acceptance complete early.

## File map

Create:

- `crates/lobo-core/src/control/app_scope.rs`
- `crates/lobo-core/src/control/app_scope/tests.rs`
- `crates/lobo-core/src/opencode.rs`
- `crates/lobo-core/src/opencode/tests.rs`
- `crates/lobo-core/src/local/process_memory.rs`
- `crates/lobo-core/src/local/process_memory/tests.rs`
- `crates/lobo-proto/src/telemetry.rs`
- `crates/lobo-agent/src/telemetry.rs`
- `app/ui/src/settings/Clients.svelte`
- `app/ui/src/panel/RuntimeMemory.svelte`
- `app/e2e/runtime.spec.js`
- `tools/bounded_runtime_e2e.py`
- `plans/2026-09-30-app-setup-memory-visibility/results.md`

Modify:

- `Cargo.lock`, `crates/lobo-core/Cargo.toml`, `crates/lobo-core/src/lib.rs`
- `crates/lobo-core/src/control/{mod,up,status,cleanup,operation_state,agent_http}.rs`
- `crates/lobo-core/src/control/{testkit,tests,up_tests}.rs`
- `crates/lobo-core/src/provider/mod.rs` (owned identity/delete trait hooks), `crates/lobo-core/src/connection.rs`, `crates/lobo-core/tests/cloud_connection.rs`
- `crates/lobo-core/src/local/{mod,deps,supervise,provider}.rs`, `crates/lobo-core/src/local/deps/tests.rs`, `crates/lobo-core/tests/local_provider.rs`
- `crates/lobo-proto/src/{lib,agent}.rs`
- `crates/lobo-agent/src/{lib,metrics,runner,pod,api,testutil}.rs`
- `app/src-tauri/Cargo.toml` (authenticated setup HTTP client/test dependencies if needed), `app/src-tauri/Cargo.lock`, `app/src-tauri/Info.plist`
- `app/src-tauri/src/{backend,commands,controller,store,types,lib,windows,tray}.rs`
- `app/src-tauri/src/{controller,store}/tests.rs`, `app/src-tauri/src/e2e_memory.rs`
- `app/src-tauri/examples/generate_ui.rs`, `app/src-tauri/tests/fixtures.rs`
- `app/ui/src/lib/{api,view,fmt}.ts`, `app/ui/src/lib/{view,fmt}.test.ts`
- `app/ui/src/settings/Settings.svelte`, `app/ui/src/panel/{BootLog,ReadyCard,Panel}.svelte`
- `app/ui/src/render/Render.svelte`, generated `app/ui/src/{proto,gen}/` and fixture files under `app/ui/src/fixtures/`
- `app/e2e/{wdio.conf.js,fixture.py,app.spec.js}`, `tools/native_app_e2e.py`
- `Makefile`, `.github/workflows/rust.yml`, `README.md`, `CHANGELOG.md`, `docs/implementation-mistakes.md`

Out of scope: Docker/image files, updater files, release workflow, CLI installation, personal config/keys, model payloads and unrelated plans. Add new files to the spec file map before implementation. Do not modify the legacy Swift app.

## Locked interfaces and invariants

### App ownership

Module `lobo_core::control::app_scope`, re-exported by `control`:

- `RuntimeTarget { provider: String, instance_id: Option<String>, boot_id: String, agent_url: Option<String>, api_url: Option<String>, local_pid: Option<i32>, local_start_id: Option<u64> }`, serde/Clone/Debug/PartialEq. Empty boot identity is never sufficient for destructive app operations.
- `OwnerSink = Arc<dyn Fn(RuntimeTarget) -> Result<()> + Send + Sync>`.
- `up_app(d: Deps, opts: UpOpts, previous: Option<RuntimeTarget>, cancel: CancellationToken, owner: OwnerSink) -> UpOperation`.
- `discover_app(d: &Deps, provider: &str) -> Result<Option<RuntimeTarget>>`, selected provider only; return an error for ambiguous ownership, never choose an arbitrary instance.
- `snapshot_app(d: &Deps, target: &RuntimeTarget) -> Result<Snap>` and `down_app(d: &Deps, target: &RuntimeTarget) -> Result<f64>`.
- `sample_app(d: &Deps, target: &RuntimeTarget) -> Result<lobo_proto::Status>` reads only the recorded agent endpoint. It never lists providers or changes connections.

The existing `control::{up,down,snapshot}` signatures and behavior stay unchanged for CLI compatibility. Share the worker implementation through an internal explicit scope enum; do not duplicate the startup state machine.

Persist `RuntimeTarget` atomically as `config_path.with_extension("app-runtime.json")`, mode `0600`, in the existing backend. No separate ownership service. The owner callback persists boot identity before provider creation and updates instance/endpoints immediately after creation. A persistence failure prevents creation or retains the existing worker's cleanup responsibility. Controller/store retain immutable operation identity while UI selections change.

An app operation may reconcile `PendingCreate` only when provider and boot match its recorded owner. Foreign pending records stay byte-for-byte unchanged, even for the same provider. Local paths must not call `connection.stop_saved`, attach cloud connections or discard unrelated keys. Cloud Stop uses only the recorded provider/instance. Preserve worker cancellation, panic handling and uncertain-create cleanup. A missing target is successful Stop; a replaced local PID/boot is never signalled.

Backend signatures become `snapshot(&self, provider: &str)`, `down(&self, target: RuntimeTarget)` and `telemetry(&self, target: RuntimeTarget)`. All return existing app `Result` aliases (`Snap`, `f64`, `Status`). Keep `up` returning `UpOperation`; production wires `OwnerSink`, and fakes implement the same ownership events. Keep `Store.runtime: Option<RuntimeTarget>` private and outside PanelState/IPC. Add private `Controller::runtime_target(&self) -> Result<RuntimeTarget>` to capture the immutable owner for Stop/setup/polling. New ownership and child process identity fields stay private; they never enter PanelState or IPC. Preserve existing Snap instance IDs and Status.boot_id for compatibility and telemetry correlation. Credentials remain private.

Capture `StartSubmission { request: UpRequest, config_generation: u64, selection_generation: u64 }` and reserve startup synchronously before queueing the blocking memory probe. A queued worker must consume that submission, never reread mutable target/model state. If either generation changes before admission completes, reject the stale submission without provider calls; never redirect a Local click into a Cloud rental.

### OpenCode

Use `jsonc-parser = { version = "=0.33.2", features = ["cst"] }` in core. Its [official CST documentation](https://docs.rs/jsonc-parser/0.33.2/jsonc_parser/cst/index.html) documents retention of comments and whitespace. Do not use a generic JSON rewrite or a subprocess dependency.

- `Binding { provider: String, model_alias: String, context: u64, endpoint: String, api_key: String }` stays Rust-only and must not derive Debug/Serialize.
- `ConfigChoice { path: PathBuf }`; `ConfigOutcome { path: PathBuf, provider: String, model_alias: String, changed: bool }` contains no key.
- `discover_config(config_home: &Path) -> Result<ConfigChoice>`; `patch_config(source: &str, binding: &Binding, key_reference: &str, make_default: bool) -> Result<String>`.
- `configure(path: &Path, key_dir: &Path, binding: &Binding, make_default: bool, validate: &dyn Fn() -> Result<()>) -> Result<ConfigOutcome>` validates immediately before replacement. Preserve JSONC, detect concurrent edits, create private backup/key files and atomically replace only after validation.
- Tauri commands: `opencode_info`, `choose_opencode_config`, `configure_opencode(path: String, make_default: bool)`; Rust-only backend setup performs fresh scoped status and authenticated `/v1/models` first.

Use the running alias/context; provider IDs remain `lobo-local`/`lobo`. Preserve unrelated values and custom `agent.lobo` controls. Checked option changes top-level `model` and `default_agent`; unchecked preserves both. A new lean agent uses agent-scoped tools `"*": false` followed by an explicit core coding allowlist (`read`, `edit`, `write`, `bash`, `glob`, `grep`); validate supported OpenCode tool names against its schema. Version 1.18.33 has no built-in `list`; edit/write also authorize its `apply_patch` alias. Do not grant MCP/skill wildcard access or alter global tools. Existing agents only receive the model change.

Config key files use `{file:<absolute-private-path>}`. An unchanged repair creates no backup/key churn. Return only bounded metadata. Hold a setup-generation guard through commit so Stop/config/selection changes invalidate stale setup without holding store locks across network awaits.

### Display telemetry

New protocol module re-exports `RuntimeTelemetry`, `MemorySample`, `ActivitySample`, `MemoryState`, `MemoryScope`, `ActivityState`. `Status.telemetry: Option<RuntimeTelemetry>` defaults to None and is omitted when absent.

- `RuntimeTelemetry { version: u32, boot_id: String, memory: Option<MemorySample>, activity: Option<ActivitySample> }`.
- Samples carry `collected_at: GoTime`, `sample_age_ms: u64`, `sequence: u64`. Memory adds state, scope and nullable `used_bytes`, `total_bytes`, `mac_available_bytes`. Activity adds state, nullable `processed_tokens`, `total_tokens`, `tokens_per_second`, and measured `active_requests`, `queued_requests` counts.
- `MemoryState`: available/not_started/unavailable; `MemoryScope`: model_process/device_vram. `ActivityState`: reading_prompt/generating/processing_request/no_active_request/idle/unavailable. Wire enum names use snake_case. `total_tokens` remains None for supported slots versions.
- `local::process_memory::ProcessIdentity { pid: i32, start_id: u64 }`; `identity(pid: i32) -> Result<ProcessIdentity>`; `footprint(identity: &ProcessIdentity) -> Result<u64>`. Inject both through function traits in tests. Query the actual child, never `LocalState.pid`.
- `agent::telemetry::SlotSample` holds only slot/task IDs, processing and whitelisted counters; `parse_slots(text: &str) -> Result<Vec<SlotSample>>`; `ActivityTracker::observe(boot_id: &str, slots: &[SlotSample], now: Instant) -> ActivitySample`.
- Extend the existing `Metrics` trait with `display_memory()` and `slots()` methods returning typed samples. The runner owns one two-second task with missed-tick Skip, bounded reads and cancellation. Tests/fakes implement these methods explicitly.

The fast path never scrapes `/metrics`. Preserve its single 30-second reader and watchdog policy. Thirty-second status writes must not overwrite newer display fields. Age uses producer monotonic time plus UI time since receipt. More than six seconds old means Stale memory/Unavailable activity. New boot, backwards sequence, reset task or counter regression invalidates old values. Never compute rates across task identities.

Check `is_processing` before retained counters. Active decoded > 0 means Generating; active prompt work with decoded == 0 means Reading prompt; ambiguous active means Processing request. Inactive plus no queued work means Idle. Normalize `next_token` object or one-element array. Do not use `n_prompt_tokens` as processed count, denominator or ETA. No raw slots/prompt/token contents enter status or logs.

## Commands and evidence locations

Run all commands from `/Users/kass/dev/lobocode-app-runtime` unless marked Dell. `rtk proxy` is used for SSH, rsync and Python harnesses because no suitable filtered wrapper exists. Read-only verification must confirm the existing container mount before syncing; if `/app-runtime` is not present, copy this task-owned tree into that path with `docker cp`, without restarting the container.

Sync current code before each Dell check; this excludes credentials and build output:

```sh
rtk proxy rsync -az --exclude .git --exclude='.env*' --exclude=target --exclude=node_modules --exclude=/bin/ --exclude='.superpowers' --exclude='*.gguf' --exclude=.DS_Store /Users/kass/dev/lobocode-app-runtime/ dell:/storage/lobocode-app-runtime/
```

Use explicit remote commands below; do not reuse another task's directory. Native evidence goes under the harness's printed task directory and sanitized summaries in this feature's `results.md`. Never copy personal config or keys into evidence.

## Self-test sanity check

- [ ] Record clean master SHA and memory checkpoint `597fb1c`. Existing native run passed five cases, then disconnected at forced-denied Start; the following case failed consequently. This is unresolved acceptance, not a passing baseline.
- [ ] Create the worktree: `rtk git worktree add /Users/kass/dev/lobocode-app-runtime -b fix/app-setup-memory-visibility master`. Inside it, merge `597fb1c` with `rtk git merge --no-edit 597fb1c`. Do not merge code into primary master now.
- [ ] Mac baseline: `rtk pnpm -C app/ui check`, `rtk pnpm -C app/ui test`, `rtk pnpm -C app/ui build` all pass.
- [ ] Sync Dell and run `rtk proxy ssh dell 'docker exec -e CARGO_TARGET_DIR=/memory-target -e CARGO_BUILD_JOBS=2 -w /app-runtime lobo-public-image-check sh -c "cargo test --locked -p lobo-core control::"'`; all existing scoped-area tests pass before new failures are added.

## Task 1: Record ownership regressions

Files: Modify `crates/lobo-core/src/control/testkit.rs`; Create `control/app_scope.rs`, `control/app_scope/tests.rs`; Modify `control/mod.rs`.

- [x] Add recording local/runpod/vast fakes and foreign pending/saved-connection fixtures. Tests fail because the app-scoped API is absent.
- [x] Lock tests: Local discovery/status/Stop call zero cloud methods; cloud Stop deletes only recorded instance; foreign pending records survive; reused local identity is untouched.
- [x] Dell: `rtk proxy ssh dell 'docker exec -e CARGO_TARGET_DIR=/memory-target -e CARGO_BUILD_JOBS=2 -w /app-runtime lobo-public-image-check sh -c "cargo test --locked -p lobo-core control::app_scope::"'`. Expect named red tests before implementation.

## Task 2: Implement selected-provider discovery and owned status

Files: Modify `control/app_scope.rs`, `control/agent_http.rs`, `control/mod.rs`; Test `control/app_scope/tests.rs`.

- [x] Implement `RuntimeTarget`, selected-provider discovery and identity-checked `snapshot_app`. Ambiguous discovery fails; never choose first across providers.
- [x] Add `sample_app` direct-agent read with boot validation and no provider/connection calls.
- [x] Repeat Task 1's exact Dell command; discovery/status/fast-sample tests pass. Keep Stop red until Task 3.

## Task 3: Implement identity-scoped Stop

Files: Modify `control/app_scope.rs`, `control/cleanup.rs`, `control/operation_state.rs`, `connection.rs`, `local/provider.rs`; Test `control/app_scope/tests.rs`, `tests/cloud_connection.rs`, `tests/local_provider.rs`.

- [x] Add failing identity tests at the last signal/delete boundary, including PID reuse and a new local boot at the same port.
- [x] Implement `down_app` with exact owner matching, scoped verification and no global cleanup fallback. Already absent succeeds; unrelated connections/pending records remain unchanged.
- [x] Dell: `rtk proxy ssh dell 'docker exec -e CARGO_TARGET_DIR=/memory-target -e CARGO_BUILD_JOBS=2 -w /app-runtime lobo-public-image-check sh -c "cargo test --locked -p lobo-core control::app_scope:: && cargo test --locked -p lobo-core --test local_provider"'`. Expect zero unrelated calls and all targeted tests green.

## Task 4: Scope app Start without changing CLI

Files: Modify `control/up.rs`, `control/app_scope.rs`, `control/cleanup.rs`; Test `control/up_tests.rs`, `control/app_scope/tests.rs`.

- [x] Add failing tests for Local Start with broken cloud keys, foreign pending operation, cancellation before/after rent, panic and owner-record failure.
- [x] Implement explicit internal operation scope and `up_app`. Persist boot ownership before rent, then instance/endpoints. Existing CLI `up` retains its global behavior.
- [x] Dell: `rtk proxy ssh dell 'docker exec -e CARGO_TARGET_DIR=/memory-target -e CARGO_BUILD_JOBS=2 -w /app-runtime lobo-public-image-check sh -c "cargo test --locked -p lobo-core control::"'`. Both existing CLI-contract tests and new scope tests pass; no cloud create occurs.

## Task 5: Wire app ownership and generation guards

Files: Modify app `backend.rs`, `controller.rs`, `store.rs`, `types.rs`, `controller/tests.rs`, `store/tests.rs`.

- [x] Add failing app tests for target changes during Stop/setup, persisted restart ownership, foreign operation and local Ready despite a failing Vast fake.
- [x] Add the whole-branch review's queued-worker regression: hold the blocking worker, submit Local Start, select Cloud/change model/config, then release it. Assert no Cloud rent, no second accepted submission and a stale-submission error or correctly bound local request. Reserve startup and capture StartSubmission before queueing, preserving error recovery.
- [x] Persist ownership in backend, capture immutable Stop identity, wire scoped Start/status/Stop and retain worker-owned startup cleanup. Stop's final refresh uses the same identity.
- [x] Hosted macOS command: `CARGO_BUILD_JOBS=2 cargo test --locked --manifest-path app/src-tauri/Cargo.toml`. Expect named ownership cases green. Do not run this on the laptop.

## Task 6: Preserve JSONC with exact provider/agent patches

Files: Create `opencode.rs`, `opencode/tests.rs`; Modify core `Cargo.toml`, `src/lib.rs`, root/app lockfiles.

- [x] Add JSONC/comment/trailing-comma fixtures as test strings; assert unrelated byte slices and values survive. Include both checkbox states, custom agents and Q6/Q8 contexts.
- [x] Add/pin CST dependency and implement `patch_config` without whole-document serialization. Reject ambiguous duplicate keys and non-object containers.
- [x] Dell: `rtk proxy ssh dell 'docker exec -e CARGO_TARGET_DIR=/memory-target -e CARGO_BUILD_JOBS=2 -w /app-runtime lobo-public-image-check sh -c "cargo test --locked -p lobo-core opencode::"'`. New tests fail first, then pass. Keep CLI export fixtures unchanged.

## Task 7: Add atomic private config writes

Files: Modify `opencode.rs`, `opencode/tests.rs`.

- [x] Test config discovery precedence, file-key substitution, private permissions, no-op repair, changed key backups, concurrent edits and interrupted replacement.
- [x] Implement `discover_config` and `configure`; call validation immediately before commit. No error includes secrets or parser excerpts. Unsupported symlink target leaves original intact.
- [x] Repeat Task 6's Dell command. Assert byte-identical originals on failures and no leaked key/backup files outside the task tempdir.

## Task 8: Authenticate current OpenCode binding in Rust

Files: Modify app `backend.rs`, `commands.rs`, `types.rs`, `lib.rs`, `controller/tests.rs`; Test existing app backend test module.

- [ ] Add failing HTTP fixtures for wrong key, missing alias, stale boot, rotated key and Stop during verification.
- [ ] Register the three locked commands. Resolve actual model/context and GET `/v1/models`; enforce setup-generation validation before file commit. No completion request or key rotation.
- [ ] Hosted macOS: `CARGO_BUILD_JOBS=2 cargo test --locked --manifest-path app/src-tauri/Cargo.toml`. Expect successful setup returns metadata only and failure cases write nothing.

## Task 9: Add compact Clients controls

Files: Create `settings/Clients.svelte`; Modify `Settings.svelte`, `ReadyCard.svelte`, `Panel.svelte`, `lib/api.ts`, `lib/view.test.ts`, `render/Render.svelte`.

- [ ] Add view tests for checked-by-default option, explicit uncheck preservation, duplicate-action disable, path/model display and bounded failure text.
- [ ] Add Clients tab and Ready shortcut. Show config path before Configure/Repair and restart/project-override caveat after success.
- [ ] Mac: `rtk pnpm -C app/ui test`, `rtk pnpm -C app/ui check`, `rtk pnpm -C app/ui build`. All pass; preserve existing fixed widths and content-measured native height, with every control visible and no scrolling.

## Task 10: Restore regular native app behavior

Files: Modify `Info.plist`, app `lib.rs`, `windows.rs`; Test native harness in Task 11.

- [ ] Remove agent-app plist setting; set regular activation. Preserve tray panel settings, close-to-hide, background launch and reopen.
- [ ] Build with `rtk make mac` and inspect the actual bundle plist. Prepare Dock/Cmd+Tab/reopen/drag checks for deferred Task 21. No E2E execution or implementation-mirroring unit tests now.

## Task 11: Prepare native smoke and CI deferral

Files: Create `app/e2e/runtime.spec.js`; Modify `app/e2e/{fixture.py,wdio.conf.js,app.spec.js}`, `tools/native_app_e2e.py`, `e2e_memory.rs`, `generate_ui.rs`, `tests/fixtures.rs`, `Makefile`, `.github/workflows/rust.yml` and generated files.

- [ ] Add `--runtime-ui-only` to the harness. Fixtures provide owned local Ready/startup/Stop and failing cloud endpoints; they never launch real inference. Keep real Start denied in UI-only fixtures.
- [ ] Native checks cover config action/preservation, Local Stop reaching Off with zero cloud requests, Dock/Cmd+Tab/reopen, dragging/corners and no scroll. Use actual native UI evidence, not browser-only claims.
- [ ] Prepare but do not execute these deferred commands: `rtk make app-e2e-build`; `rtk proxy python3 tools/native_app_e2e.py --runtime-ui-only`; `rtk proxy python3 tools/native_app_e2e.py --setup-only`; `rtk proxy python3 tools/native_app_e2e.py --memory-only`.
- [x] Add workflow_dispatch boolean input `native_e2e`, default true. Guard the native E2E step with `github.event_name != 'workflow_dispatch' || inputs.native_e2e`. Dispatch false skips only E2E, not app/core tests, lint, generation or builds. Push behavior remains unchanged.
- [ ] Add runtime-only mode to that guarded step. Generate core TS on Dell (`cargo test --locked -p lobo-proto export_bindings`) and app fixtures on hosted macOS (`make app-fixtures`); import only generated files. Require clean generation diff.

## Task 12: Deliver phase-one QA build

Files: Modify `CHANGELOG.md`, `README.md`, `docs/implementation-mistakes.md`; Create `results.md`; commit feature changes.

- [ ] Parent self-review scope/config/native changes and unit/build evidence. Record exact source SHA; mark native smoke, memory disconnection acceptance, telemetry and full regression pending.
- [ ] Commit on clean review: `rtk git add -A`; `rtk git commit -m 'fix(app): scope runtime actions and configure OpenCode'`.
- [ ] Push through explicit identity: `rtk proxy env GIT_SSH_COMMAND='ssh -o BatchMode=yes -o IdentitiesOnly=yes -i /Users/kass/ssh/github-kass' git push git@github.com:1905/lobocode.git HEAD:refs/heads/fix/app-setup-memory-visibility`. This branch is outside automatic Rust push filters.
- [ ] Dispatch tests/builds: `rtk gh workflow run rust.yml --repo 1905/lobocode --ref fix/app-setup-memory-visibility -f native_e2e=false`. List with `rtk gh run list --repo 1905/lobocode --branch fix/app-setup-memory-visibility --workflow rust.yml --limit 3`; watch the returned exact run ID using `rtk gh run watch <run-id> --repo 1905/lobocode --exit-status`.
- [ ] After dispatched checks pass, `rtk make install-mac` builds/installs only the app. Verify bundle signature/revision without launching E2E. Preserve CLI/config/weights.
- [ ] Notify build availability through notify, with unit/build results and explicit deferred native QA/E2E. Do not call the build fully QA-verified. No public release or model requests yet.

## Task 13: Add optional telemetry protocol

Files: Create proto `telemetry.rs`; Modify proto `lib.rs`, `agent.rs`, agent `api.rs` tests and generated TS.

- [ ] Add serialization tests for absent old fields, new enums, optional/null values, boot/sequence and old-client tolerance.
- [ ] Implement locked types. Keep legacy serialized status unchanged when telemetry is absent.
- [ ] Dell: `rtk proxy ssh dell 'docker exec -e CARGO_TARGET_DIR=/memory-target -e CARGO_BUILD_JOBS=2 -w /app-runtime lobo-public-image-check sh -c "cargo test --locked -p lobo-proto && cargo test --locked -p lobo-agent api::"'`. Existing Go-compatible fixtures remain valid.

## Task 14: Measure actual local child footprint

Files: Create `local/process_memory.rs`, its tests; Modify local `mod.rs`, `deps.rs`, `deps/tests.rs`.

- [ ] Add injected tests for footprint, start identity, supervisor confusion, PID reuse, exit, invalid values and permission failure. Capture/clear identity at actual model-child spawn/exit.
- [ ] Implement `proc_pid_rusage` physical footprint plus existing `memory::snapshot` availability. No RSS/Metal addition and no admission-policy changes.
- [ ] Dell: `rtk proxy ssh dell 'docker exec -e CARGO_TARGET_DIR=/memory-target -e CARGO_BUILD_JOBS=2 -w /app-runtime lobo-public-image-check sh -c "cargo test --locked -p lobo-core local::process_memory:: && cargo test --locked -p lobo-core local::deps::"'`.
- [ ] Hosted `core-macos` job verifies native calls against a harmless fixture child; no model required. Mac laptop runs no adapter test suite.

## Task 15: Parse slots and derive request activity

Files: Create agent `telemetry.rs`; Modify agent `lib.rs`, `metrics.rs`; tests colocated in telemetry module.

- [ ] Add fixtures for zero gauges with prompt work, generation, inactive retained counters, queued/multiple requests, object/array `next_token`, unsupported fields and malformed numbers.
- [ ] Implement whitelist parser and `ActivityTracker`. Unknown denominator stays null; reject/reset invalid deltas. Never retain prompt, params, token IDs or arbitrary fields.
- [ ] Dell: `rtk proxy ssh dell 'docker exec -e CARGO_TARGET_DIR=/memory-target -e CARGO_BUILD_JOBS=2 -w /app-runtime lobo-public-image-check sh -c "cargo test --locked -p lobo-agent telemetry::"'`. Assert exact activity and absence of content in serialized results.

## Task 16: Run independent two-second display sampling

Files: Modify agent `runner.rs`, `metrics.rs`, `pod.rs`, `testutil.rs`, `api.rs`; local `deps.rs`, `supervise.rs`.

- [ ] Add fake-clock tests proving 2-second slots/memory sampling, single in-flight task, timeout/skip/cancel behavior and independent partial failures.
- [ ] Keep exactly the existing 30-second `/metrics` reader/watchdog. Add producer age/sequence and prevent old tick writes replacing fresh telemetry.
- [ ] Dell: `rtk proxy ssh dell 'docker exec -e CARGO_TARGET_DIR=/memory-target -e CARGO_BUILD_JOBS=2 -w /app-runtime lobo-public-image-check sh -c "cargo test --locked -p lobo-agent runner:: && cargo test --locked -p lobo-agent telemetry:: && cargo test --locked -p lobo-core local::deps::"'`. Assert unchanged watchdog decisions and no extra metrics scrapes.

## Task 17: Poll display without provider enumeration

Files: Modify app `backend.rs`, `controller.rs`, `store.rs`, `types.rs`, controller/store tests.

- [ ] Add controlled tests for visible 2-second status reads, reopen refresh, foreign boot, backwards sequence, clock skew and repeated stale responses.
- [ ] Use `sample_app` for fast display and retain separate scoped reconciliation. Add no new rental/provider-list calls. Discard telemetry after ownership changes.
- [ ] Hosted app test command from Task 5 passes, including Stop/quit responsiveness under a hung sample. Hidden windows may reduce UI polling without changing the agent sampler or watchdog.

## Task 18: Render memory and activity in startup/Ready/tray

Files: Create `RuntimeMemory.svelte`; Modify `BootLog.svelte`, `ReadyCard.svelte`, `Panel.svelte`, `lib/view.ts`, `lib/fmt.ts`, tests, `store.rs`, `tray.rs`, fixture generator/render page.

- [ ] Add failing presentation cases: pre-child, loading footprint, partial unknown, zero valid measurement, device VRAM, >6-second stale, processed tokens and ambiguous active work.
- [ ] Render compact correctly labeled GiB values and activity. Replace rate tiles; show no percentage/ETA. Tray shows active work even with zero rate.
- [ ] Mac: `rtk pnpm -C app/ui test`; `rtk pnpm -C app/ui check`; `rtk pnpm -C app/ui build`. Hosted generator and Rust tray tests pass; all layouts fit without scroll.

## Task 19: Prepare telemetry smoke and refresh the build

Files: Modify `app/e2e/runtime.spec.js`, fixture/harness data, `CHANGELOG.md`, `results.md`.

- [ ] Extend runtime-only fixture mode through startup→prompt read→generation→idle→stale→Stop and both protocol generations. Assert visible updates and zero cloud calls for Local.
- [ ] Prepare the deferred command `rtk proxy python3 tools/native_app_e2e.py --runtime-ui-only`; do not run it before fixes finish. Build/type/unit checks continue now.
- [ ] Parent verifies focused Dell/hosted/unit/build evidence; commit telemetry batch on clean review. Repeat Task 12's exact push/dispatch-false/install sequence and report deferred native/live/full-regression checks.

## Task 20: Add bounded direct-request E2E harness

Files: Create `tools/bounded_runtime_e2e.py`; Modify `Makefile`, workflow path filters, `results.md`.

- [ ] Script flags: `--config PATH --requests 2 --max-input-tokens 64 --max-output-tokens 32 --evidence-dir PATH`; hard reject larger counts/limits or concurrency. No OpenCode subprocess.
- [ ] Resolve existing local runtime/config internally without printing keys. Use two fixed short completion prompts, validate actual input counts with the runtime tokenizer, request max 32 outputs and record usage/timings only. No retries that create extra generation requests.
- [ ] If runtime/model is absent, report blocked; the script never starts a runtime, downloads a model or rents anything. Start, if needed, uses the actual app and the existing admission guard before script execution.
- [ ] Prepare the deferred harness self-test; execute it only when E2E resumes in Task 21. It uses an HTTP fake, not inference: `rtk proxy ssh dell docker exec -w /app-runtime lobo-public-image-check python3 tools/bounded_runtime_e2e.py --self-test`. Add `--self-test` with fake auth/count/timeout endpoints and assert no real provider access.

## Task 21: Deferred native and bounded live E2E after fixes

Files: Modify `results.md` only, unless fixing a demonstrated bug through a reviewed code change.

- [ ] Only after fixes are complete, resume deferred native fixtures and bounded live E2E. Before then leave this entire task pending. First run Task 11's four native commands and Task 19's runtime fixtures; resolve the earlier memory Start-disconnection failure.
- [ ] Dispatch complete CI with `rtk gh workflow run rust.yml --repo 1905/lobocode --ref fix/app-setup-memory-visibility -f native_e2e=true`; verify the exact revision and expected native steps executed.
- [ ] Verify installed revision, existing model and fresh admission. Do not bypass denied Start or download absent weights. Preserve the running target identity.
- [ ] Run once: `rtk proxy python3 tools/bounded_runtime_e2e.py --config /Users/kass/.config/lobo/config.env --requests 2 --max-input-tokens 64 --max-output-tokens 32 --evidence-dir /tmp/lobocode-app-runtime-live`.
- [ ] Record actual prompt/output counts, prompt/output speed, wall time, model alias, boot identity and revision. Capture native memory/activity before/during/after; short requests may complete between frames, so fixtures remain evidence for long Reading prompt presentation.
- [ ] Use app Stop and verify local Off plus zero cloud-provider calls in the fixture proof. Never stop an unrelated runtime. Keep real-request content and credentials out of evidence.
- [ ] Failure or admission denial remains explicit; do not expand request budgets, switch models, rent or rerun automatically. Focused fixes get smoke checks and the normal reviewed delivery sequence.

## Task 22: Complete broad acceptance and one branch review

Files: Existing affected tests, `results.md`, scoped fixes if required.

- [ ] Dell full suite: `rtk proxy ssh dell 'docker exec -e CARGO_TARGET_DIR=/memory-target -e CARGO_BUILD_JOBS=2 -w /app-runtime lobo-public-image-check sh -c "cargo test --locked --workspace --features lobo-cli/test-fakes && cargo clippy --locked --workspace --all-targets --features lobo-cli/test-fakes -- -D warnings"'`.
- [ ] Hosted Rust workflow must pass app/backend/native adapter/generator jobs for the exact revision. Deferred native E2E must also pass before final acceptance/merge. Check old CLI bulk down/up tests, stale/identity races, config recovery and unchanged watchdog policy.
- [ ] Mac UI only: `rtk pnpm -C app/ui test`; `rtk pnpm -C app/ui check`; `rtk pnpm -C app/ui format:check`; repeat native modes only after affected fixes.
- [ ] Read/invoke the rival-codex skill once for the complete branch against the recorded merged base. Fix substantive findings, run affected checks and record review limits. Do not run automatic per-task reviews.

## Task 23: Reconcile, merge and verify final delivery

Files: `README.md`, `CHANGELOG.md`, `docs/implementation-mistakes.md`, feature spec/plan/results and existing project memory as needed.

- [ ] Reconcile actual interfaces and limits with the spec. Record completed checks separately from cloud-image/provider acceptance still held. Parent performs final type/name consistency review across Rust, TS, fixtures and commands.
- [ ] After deferred E2E and required checks pass, commit final fixes/docs and push the branch. Merge directly into clean primary `master`; no PR. Push master using Task 12's explicit SSH identity and destination `HEAD:refs/heads/master`. Its normal CI includes native E2E.
- [ ] Verify final merged app revision and repeat focused native smoke after local app build/install. Do not modify CLI, config or weights. No tag/image/public release is created.
- [ ] Send final notification through notify with shipped scope, measured results and any blocked acceptance. Keep updater/image jobs paused.
- [ ] Mark plan done only when all authorized acceptance is complete. If bounded live E2E is blocked, retain pending status and explain the blocker. On full completion, move the feature directory under `plans/done/` per the skill.

## Type and requirement consistency check

- [ ] RuntimeTarget and app-scoped APIs match Tasks 1–5, 8 and 17; no app path calls legacy bulk operations.
- [ ] Owner matching protects pending records, saved cloud connections, PID/start identity and boot identity at mutation boundaries.
- [ ] OpenCode commands and checkbox payload match Rust/Svelte; Binding never crosses IPC or logs.
- [ ] Telemetry enums/units/nullability/age/sequence match protocol, collectors, UI and old-version fixtures.
- [ ] Two-second display reads use slots/memory only; metrics/watchdog stay at 30 seconds.
- [ ] All five approved TL;DR items map to tasks; every created/modified file is recorded above and reconciled into the spec before implementation.
- [ ] No task authorizes broader Mac inference, publication, rental, CLI installation, updater work or image-job resumption.

New-agent permission detail: set agent-scoped `permission.read` to `{ "*": "allow", "mcp:*": "deny" }` after the lean tools map. OpenCode v1.18.33 resource tools use `read` permission patterns `mcp:<server>:...`; this denies their use while keeping file reads. Some resource tool schemas remain visible to OpenCode. Do not promise zero MCP schema overhead. Preserve all existing custom-agent permissions and never change global MCP settings.

App ownership implementation detail: add Rust-only `PreparedUp` without Debug/Serialize. `Backend::prepare_up(request: UpRequest) -> Result<PreparedUp>` captures config/options and performs blocking admission without provider calls or worker creation. `Backend::up(prepared: PreparedUp, previous: Option<RuntimeTarget>, cancel: CancellationToken, owner: OwnerSink) -> Result<UpOperation>` launches from that captured config after the final generation check. Keep the core provider's independent admission guard. Track reservation/preparation/operation as one owned job so Stop/Quit can cancel before launch without blocking on Active or Store.

Add private `snapshot_owned(provider, captured: Option<RuntimeTarget>) -> Result<(Option<RuntimeTarget>, Snap)>` and `load_owner() -> Result<Option<RuntimeTarget>>`. Keep `snapshot(provider) -> Snap` as a wrapper. Pair owner and snapshot in one result, persist owner before delivery, and discard stale replies. Remove an owner record only after scoped absence and only when its identity still matches. These helpers stay inside Rust and do not change CLI or frontend contracts.

Telemetry source correction: pinned llama.cpp b11118 `/slots` has no queued-request count. Its high-priority slot query can overtake pending work. Set `ActivitySample.queued_requests: Option<u64>` and use null for this adapter; do not infer zero or scrape `/metrics` on the fast path. Add `ActivityState::NoActiveRequest` (`no_active_request`) and display `No active request` when a valid slots response reports no active slots and queue state is unknown. `Idle` is reserved for a future documented source that measures both counts as zero; the current adapter never emits Idle. Keep parse_slots/observe signatures unchanged. Active prompt/generation phases work with unknown queue count. The current shared local/pod flags leave `/slots` enabled by its documented default.

The runner owns sample sequence per two-second batch. It saves independent monotonic observation instants for memory/activity and recomputes sample_age_ms on every status response. ActivityTracker does not own a competing sequence. Local footprint identity uses rusage ri_proc_start_abstime from the same native read as footprint; it is separate from the supervisor ownership start identifier. Identity-probe failure yields unavailable telemetry and must not drop the child exit monitor.

App uncertain-create recovery must verify ownership of a discovered candidate. A single ID absent from the pre-create list is not proof: another client may have created it. Require matching local boot identity or an exact saved cloud connection record for provider/instance/boot. Without proof, keep the unresolved journal, do not delete the candidate, and do not rent again. Current cloud provider Instance data has no boot metadata; app cloud uncertain-create recovery can remain unresolved. Existing CLI adoption behavior remains unchanged.

Discovery commit clarification: `snapshot_owned` returns the candidate owner and snapshot without adopting it. Add private Rust-only `Backend::adopt_owner(expected: Option<RuntimeTarget>, target: RuntimeTarget) -> Result<()>` for an atomic on-disk identity comparison and local write. Controller validates captured generations under Active, releases Store, persists the candidate, then applies the paired owner/snapshot. Keep Active only through this short local commit, never across network requests. This prevents a discarded stale discovery reply from changing persistent ownership. Persistence still precedes Store/IPC delivery.


OpenCode metadata contract (Tasks 6–9):

```rust
ConfigRestrictions { provider_disabled: bool, provider_not_enabled: bool }
inspect_restrictions(source: &str, provider: &str) -> Result<ConfigRestrictions>
OpenCodeInfo { path: String, endpoint: Option<String>, provider: Option<String>, model_alias: Option<String>, context: Option<u64>, can_configure: bool, reason: Option<String>, warnings: Vec<String> }
OpenCodeResult { path: String, provider: String, model_alias: String, changed: bool, message: String, warnings: Vec<String> }
```

The shared restriction inspector uses the same strict JSONC parser and validates string arrays. It reports existing provider restrictions without changing them. IPC warnings/reasons are bounded static messages. ConfigOutcome retains its four fields. `opencode_info` still reports the discovered config path when no Ready runtime exists, with configure disabled and a clear reason.

Use a setup-specific bounded HTTP client with redirects disabled. Capture the private raw Lobocode config generation/fingerprint as well as Store generations; masked ConfigShow cannot detect external key rotation. Hold Active and Store through the bounded local configure/replace call, including its validation callback; never hold either across network awaits. Releasing guards after validate but before replacement would allow Stop to race the write. No runtime start or generation request is permitted.

Setup module file-map addition: create `app/src-tauri/src/opencode.rs` (and its `opencode/tests.rs` if useful) for private binding/revision/HTTP helpers. Keep backend wiring, command registration and controller generation gates in their existing files. This avoids adding all parsing/HTTP/revision logic to backend.rs; it adds no new public API beyond the locked metadata. Read `/tmp/lobocode-setup-binding-design.md` as source guidance; the two-flag ConfigRestrictions contract above is authoritative.

Ready shortcut delivery clarification: an existing Settings webview may not have subscribed yet. Retain the requested tab until Settings consumes it after subscribing; events prompt consumption rather than carry the only copy. A small safe native consume-settings-tab command/state is allowed in windows.rs/commands.rs/lib.rs. This preserves unsaved fields and covers rapid ordinary-open followed by Ready shortcut without navigation or window recreation. Add App.svelte to Task9 files for initial tab/event wiring.

Task-order clarification: prepare Task20 in parallel with Task8 using a separate author limited to tools/bounded_runtime_e2e.py, Makefile and workflow path filters. It has no shared product files or dependency changes with the app adapter. No script, self-test or inference executes. Parent reviews and commits its isolated diff; Task21 stays deferred. This source-only preparation does not delay phase-one delivery or claim acceptance.

Task11 may prepare its disjoint native-test files and new E2E-only modules alongside Tasks8–10. Existing Rust, generated files, Makefile and workflow edits wait for the prior owners to finish and parent handoff. No fixture executes. Parent serializes commits and reviews the combined dependency seam before hosted compilation.

Task11 may add `app/e2e/native-helpers.js` to share sanitized button-click tracing and control-bound/no-scroll assertions across the existing setup, memory and new runtime specs. This adds no runtime behavior or execution authorization.

First-delivery clarification: the user's E2E deferral and smoke-first delivery order allow Task12's hosted unit/build artifact once Tasks8–10 pass. Unfinished Task11 fixture preparation must not delay that usable artifact. Keep Task11 and native acceptance pending, complete its source and hosted compilation separately, and retain the local install hold. Telemetry implementation may begin after that first hosted artifact passes. No E2E or master merge is authorized by the artifact alone.

Selected-file metadata clarification: `opencode_info(path: Option<String>)` accepts an optional explicit config path. Omitted path discovers the global file; supplied path validates and inspects that selected JSON/JSONC file. Task9 sends the user-selected path on refresh, so an invalid default file cannot block a valid chosen file or produce warnings for the wrong file. Info/result shapes and the other commands remain unchanged. Add the invalid-default/valid-explicit-path case to app units.

Native fixture file-map clarification (Task11): create `app/src-tauri/src/e2e_runtime.rs`, `e2e_trace.rs`, `app/e2e/runtime_fixture.py`, and `trace-service.js` alongside the planned runtime.spec.js. Use validated E2E-only dependency injection in CoreBackend; keep real scoped Stop and real OpenCode authentication/config writing. Deny both prepare_up and up before real dependency construction. Synthetic process identities never reach LocalProvider or local.json. Isolate OpenCode config, state and app preferences under the task root. The HTTP fixture never serves generation.

Split hosted `make app-e2e-build` into an ungated compilation-only step. Keep all Python native launches, including runtime-ui-only, under the existing native_e2e guard. This compiles prepared fixture code remotely without executing E2E. Add E2E-only native/driver exit diagnostics and remove the invalid memory-case invoke replacement. Native drag/corners/Dock/Cmd+Tab remain pending actual acceptance; webview screenshots or plist inspection cannot establish them.
