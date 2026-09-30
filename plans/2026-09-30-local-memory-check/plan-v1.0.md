# Local Mac Memory Check Implementation Plan v1.0

**Date:** 2026-09-30
**Status:** in-progress
**Spec:** ./spec.md (approved)
**Authorization:** User requested spec, plan, implementation, E2E and merge in full auto. No repeated approval is required.
**Goal:** Block local startup before side effects when the selected model cannot fit in current Mac memory.
**Architecture:** A shared core probe produces a read-only memory snapshot. A pure calculator applies the model/context requirement. The native app displays the result; shared startup and pre-load checks enforce it independently.
**Tech Stack:** Rust, Mach VM statistics, Metal, Tokio, Svelte/Tauri, Vitest, native WebdriverIO.

> For agentic workers: use superpowers:subagent-driven-development to implement task-by-task. Checkbox syntax tracks completion.

## Execution and constraints

- Existing work merged and pushed to `master` at `d2557c3` before this feature.
- Worktree: `/Users/kass/dev/lobocode-local-memory-check`; branch `feat/local-memory-check`; implementation base `461372f` (includes the merged-baseline CI fixture lint fix).
- This sibling worktree keeps feature files outside the primary Docker build context. Preserve the primary checkout and ongoing image builds.
- Backend and lifecycle tests run on Dell. App Rust tests run on hosted macOS CI. This Mac runs UI checks/builds only, never inference.
- E2E must cover forced insufficient memory and probe failure even if this Mac has enough memory. A passing UI fixture never starts an inference process.
- Public release, tag push, image publication and GPU rental remain held. Branch pushes and normal CI are authorized.
- No PRs. Do not change the installed CLI or user config/models.
- Self-review each batch; parent verifies integration. No automatic per-task model review. Fix required checks before merge.
- After this feature is tested and merged, start `plans/2026-09-30-github-app-updates/plan-v1.0.md` in its own worktree. Revise that plan for the merged base and new implementation authorization first.

## File map

Create: `crates/lobo-core/src/local/memory.rs`; focused tests may use `crates/lobo-core/src/local/memory/tests.rs`; optional app E2E-only helper `app/src-tauri/src/memory_fixture.rs`.

Modify: `crates/lobo-core/src/local/{mod,platform,provider,deps}.rs`;
`crates/lobo-core/src/local/deps/tests.rs`; `crates/lobo-core/tests/local_provider.rs`;
`crates/lobo-core/Cargo.toml`; root and app Cargo locks;
`app/src-tauri/src/{backend,controller,store,types,commands,lib}.rs` and their existing tests;
`app/src-tauri/tests/fixtures.rs`; `app/src-tauri/Cargo.toml`;
`app/ui/src/{gen/*,fixtures/*,panel/StartCard.svelte,panel/LocalStart.svelte,panel/FailCard.svelte,lib/view.ts,lib/view.test.ts,render/Render.svelte}`;
`app/e2e/{app.spec.js,wdio.conf.js}`; `tools/native_app_e2e.py`; `.github/workflows/rust.yml`; `Makefile`;
`README.md`; `CHANGELOG.md`; `AGENTS.md`; `docs/implementation-mistakes.md`; affected `docs/img` assets.

Out of scope: Docker/image source, cloud admission, updater implementation, installed CLI, real model execution, unrelated plans.

## Locked core interface

Module: `lobo_core::local::memory`.

- `MemorySnapshot { total_bytes: u64, available_bytes: u64, metal_limit_bytes: u64 }`.
- `MemoryAssessment { model: String, ctx: i64, total_bytes: u64, available_bytes: u64, metal_limit_bytes: u64, required_bytes: u64, budget_bytes: u64 }`.
- Both structs are Clone/Debug/PartialEq and serialize with serde. Byte counts stay bytes until presentation.
- `snapshot() -> Result<MemorySnapshot>` performs native reads only.
- `assess(model: &str, ctx: i64, snapshot: &MemorySnapshot) -> Result<MemoryAssessment>` is pure.
- `inspect(model: &str, ctx: i64) -> Result<MemoryAssessment>` calls snapshot and assess.
- `MemoryAssessment::fits(&self) -> bool`; `ensure_fit(&self) -> Result<()>`; `message(&self) -> String`.
- `MemoryProbe = Arc<dyn Fn() -> Result<MemorySnapshot> + Send + Sync>` injects snapshots into `LocalHooks` and `MacDeps`. Production defaults to `snapshot`.
- `LocalHooks.memory` holds that probe. Update every affected fixture explicitly; no production environment override.
- Error text uses GiB and model/context values, with closing-apps/Cloud guidance. Unknown measurements never become invented zeros or a pass.

Accounting: used pages = anonymous - purgeable + wired + compressor. Reject invalid counters/overflow. Available = total - used. Leave 4 GiB system reserve. Budget = minimum of remaining physical availability and Metal recommendation. No swap credit or page-count double addition.

Requirement: catalog weights + padded context * 34,816 bytes + 4 GiB runtime reserve. Padding is 256 tokens; supported contexts are 1–262,144. Q6/Q8 only. `required <= budget` passes, one byte less fails.

## App contract

- `Backend::local_memory(&self, model: &str) -> Result<MemoryAssessment>` is async. Production resolves context from current config through the same defaults as local Start. Run blocking native reads off the UI thread.
- Production `Backend::up` performs a fresh local memory assessment before `control::up`. Shared `LocalProvider::rent` independently enforces its own fresh probe before writes/download/spawn.
- App `LocalMemory` is an exported TS type with `model`, `ctx`, `status`, `message` and optional byte fields mirroring the core assessment. `status` is `ready`, `insufficient` or `unavailable`; `PanelState.local_memory: Option<LocalMemory>` uses `None` for checking/no result.
- Start enables only for a current `ready` result for the selected local model. Cloud does not depend on this state. Failed/Retry paths use the same rule.
- Refresh after target/model/config changes and through visible idle polling. Capture a selection generation; discard an assessment if the generation/model/target changed while awaiting it.
- Keep insufficient/unknown messages inline with model/target controls usable. No automatic model or target switch. Suggest Q6 only when its own fresh assessment passes; otherwise closing apps or Cloud is sufficient.
- Test hooks exist only with the app's `e2e` feature and isolated E2E config. They provide deterministic low-memory, unavailable and passing display fixtures. They never grant a production bypass. Any UI test that invokes Start uses a denying snapshot.

## Self-test sanity check

- [x] Existing branch ancestry and worktree ownership inspected. All previous feature branches are ancestors of merged `master`.
- [x] Existing exact backend source passed 434 workspace tests and clippy on Dell; two opt-in checks and three Go interop checks also passed. `d2557c3` changes Docker/docs only after that source validation.
- [ ] Confirm focused local baseline on Dell, using the task-owned test container. Do not alter the image build source tree.
- [x] UI baseline: frozen install, type check (zero errors/warnings) and all nine tests pass.

## Task 1: Snapshot and calculator

Files: new memory module/tests, `local/mod.rs`, native bindings in core Cargo and lockfiles if needed.

- [ ] Add tests for Q6/Q8 requirement, context padding/max/invalid, Metal versus physical limit, exact boundary, malformed stats and overflow.
- [ ] On Dell run `cargo test --locked -p lobo-core local::memory::`. Observe named failures before implementation.
- [ ] Implement checked arithmetic and the locked interfaces. Use Mach statistics and Metal recommendation directly; release native handles correctly.
- [ ] Keep macOS bindings target-specific. Do not use `os_proc_available_memory`, `llama-server`, shell probes, model reads or sysctl writes.
- [ ] Rerun the focused tests; expect nonzero test count and all pass. Check the native adapter compiles through app build/hosted macOS CI.

## Task 2: Enforce before local side effects and inference

Files: `local/{provider,deps}.rs`, their tests and `tests/local_provider.rs`.

- [ ] Add provider tests proving low/unknown memory creates no weights directory, runtime request, log or supervisor.
- [ ] Add MacDeps tests proving rejected GPU checks do not invoke the device probe, and declining memory after preparation prevents the inference process.
- [ ] Run `cargo test --locked -p lobo-core --test local_provider` and `cargo test --locked -p lobo-core local::deps::tests` on Dell, first failing then passing.
- [ ] Replace the fixed-budget decision with fresh `MemoryProbe` checks. Keep Metal-device detection for valid starts and all existing cleanup behavior.
- [ ] Self-review and commit this core batch in the feature worktree; do not push until app wiring uses the same contract.

## Task 3: Native app state and controls

Files: app backend/controller/store/types, frontend start/failure views, generated types/fixtures and focused tests.

- [ ] Add controlled backend/controller cases for insufficient, unavailable, stale result and fresh Start rejection. A displayed pass cannot authorize a later denied Start.
- [ ] Implement `Backend::local_memory`, app state conversion and fresh local `up` guard.
- [ ] Refresh safely without holding store locks across awaits. Invalidate after configuration/model/target changes; preserve existing operation exclusion.
- [ ] Render required/budget values with correct GiB units. Keep local Start disabled until ready. Failed retry cannot bypass the check; Cloud remains selectable.
- [ ] Add frontend presentation tests for all verdicts, missing data and long errors. Regenerate Rust-owned types and UI fixtures, without hand-written drift.
- [ ] Run frontend checks on Mac. App Rust tests run through hosted macOS CI; no local backend suite.

## Task 4: Deterministic native memory E2E

Files: app E2E-only fixture adapter, native driver scripts, Makefile, CI.

- [ ] Add `--memory-only` to `tools/native_app_e2e.py`. It is an allowed UI-only mode on Mac. Preserve existing `--setup-only` behavior.
- [ ] Native E2E covers Q8 insufficient, Q6 selection refresh, measurement unavailable, enough-memory display, context change, stale response rejection and explicit Cloud selection.
- [ ] Invoke the real Start IPC while forced-denied. Assert error, zero task-owned supervisor/model PID/state/log artifacts and no runtime download. Never invoke Start with the passing fixture.
- [ ] Include a final read-only native probe case without invoking Start. Do not depend on this Mac actually being short of memory.
- [ ] Check scroll/client dimensions and visible controls for each case. Keep task-owned cleanup; do not close the user's app or browser sessions.
- [ ] Run `make app-e2e-build`, then `python3 tools/native_app_e2e.py --memory-only` and existing `--setup-only` on this Mac. Both must pass.
- [ ] Add the memory-only E2E command to the hosted macOS app CI job. Run browser layout checks through the managed Playwright CLI skill when useful.

## Task 5: Integrated validation and QA build

Files: affected tests, evidence and targeted fixes only.

- [ ] On Dell run `cargo test --locked --workspace --all-features` and `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` with two build jobs. Do not rerun unchanged optional network checks.
- [ ] Push the feature branch through explicit SSH credentials. Watch normal Rust CI for that exact revision; it includes macOS core/app tests and E2E.
- [ ] Run `pnpm -C app/ui check`, `pnpm -C app/ui test`, production UI build and formatting checks. Inspect new layouts and actual native read-only memory state.
- [ ] Build/install the local standalone app and DMG after smoke checks. Preserve CLI/config/models. Notify QA readiness with exact revision and remaining validation.
- [ ] Finish remaining affected regression cases before declaring acceptance. No inference benchmark is authorized; keep estimate/calibration caveat.

## Task 6: Merge and continue the updater feature

Files: docs/assets/changelog/memory and this feature's planning records.

- [ ] Record actual formulas, behavior, E2E evidence and limits. Update screenshots and README. Remove pending-implementation wording only after implementation.
- [ ] Reconcile spec as-built notes. Record test counts/CI run and install receipt; do not claim manual dragging verified.
- [ ] Complete final diff review and fix substantive findings. Commit required delivery docs with the feature.
- [ ] Merge the tested feature to `master` and push without a PR. Verify the merged revision and CI state; preserve release hold.
- [ ] Mark this plan done and move its directory under `plans/done/`, updating links.
- [ ] Revise the GitHub app-updates plan for current master, newly authorized implementation and normal CI pushes. Then implement it in its separate worktree without another approval prompt.

## Execution record

- 2026-09-30: user explicitly made memory a new feature after the existing-work merge. User then requested E2E, merge, and sequential updater implementation in full auto.
- Public-image full builds remain an independent active task on Dell. Do not sync this worktree over their source or BuildKit cache.
