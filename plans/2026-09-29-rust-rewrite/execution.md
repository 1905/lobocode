# Rust rewrite execution record

Date: 2026-09-29.
Authorization: the user said, "when plan is fixed start implementation in full auto mode."

## Active documents

| Phase | Plan | State |
|---|---|---|
| P1 | [Workspace and protocol](plan-p1-v1.1.md) | done: 5dfe9ab, CI 36557109125 |
| P2 | [Pod agent](plan-p2-v1.2.md) | code/CI done at ed65ecc; live E2E pending P6 |
| P3 | [Core](plan-p3-v1.2.md) | code/CI done at 2884464; live inference pending P6 |
| P4 | [CLI](plan-p4-v1.2.md) | done; CLI and packaging acceptance at fcb8aff, live checks P6 |
| P5 | [App](plan-p5-v1.2.md) | in progress; baseline saved, app scaffold and shared APIs underway |
| P6 | [Cutover](plan-p6-v1.2.md) | pending P5 |

[contracts.md](contracts.md) v1.2 defines shared APIs. This record defines execution and delivery decisions.
Earlier files and review-bundle-v1.0.md are historical records. Only v1.0 received the earlier external review (6/10).
The current corrections have not received another independent model review. Runtime fixes listed here are planned, not implemented yet.

## Decisions

- Implementation, commits, direct merges, pushes and normal release CI are authorized. Do not create pull requests or repeat approval prompts.
- Test and merge fix/app-silent before branching feat/rust. Record the resulting master SHA as the compatibility baseline.
- Preserve Go's handling of extra positional arguments. Commands with explicit argument rules keep them.
- App readiness uses the same config validation as CLI. A malformed bucket URL selects SETUP before attempting a boot.
- First Rust release is v0.2.0, provided the tag remains unused.
- Add hidden release --no-promote. Upload immutable zip and version metadata only. Test with an explicit release version.
- Record the shared latest manifest hash before and after candidate testing. It must stay unchanged.
- Promote a release built from the merged source only after verifying the tagged release. Candidate testing needs no public pointer rollback.
- GPU rental timing: the user confirmed "you alloed to reng gpu in the end for full e2e test." Defer P2 live rentals to the final P6 run. P3 can start after P2 local/static/CI checks. Keep P2 live acceptance pending until then.
- Keep one final Codex code review in P6. Builds, fixture checks and focused tests run in every phase.
- HTTPS workflow push was rejected for missing workflow scope. The existing ~/ssh/github-kass key authenticates as 1905. Use an explicit SSH command with git@github.com:1905/lobocode.git; do not reuse stale account aliases.
- No Jira issue is identified for lobocode. Do not use unrelated Sputnik issues.
- On 2026-09-30 the user explicitly requested app E2E testing when implementation finishes. Verify the actual app flows in P5/P6, including setup, start/stop, reopen/resume and failure handling. Keep browser fixture checks distinct from native app and final live GPU evidence. Use the local Playwright CLI skill for browser automation.
- Use the installed notify skill for QA and final Telegram messages. Keep temporary public preview links out of session chat.

## Plan corrections

1. UpOperation separates event transport from worker completion. The CLI and app retain ownership through cleanup.
2. At 120 seconds, Stop warns and keeps waiting. Start stays disabled. Down cannot race an unfinished create.
3. Cancellation reaches provider tiers, Vast offers, local runtime download, extraction and supervisor startup.
4. Cleanup errors, panics and uncertain creates remain failures. An unresolved create blocks another up until reconciled.
5. Cancellation tests include a 121-second rent, closed/full event receivers, failed deletes and pre-state-file supervisor cleanup.
6. The app has its own Cargo.lock, explicit dependencies, correct Tauri working directory and bundle output path.
7. CI covers app-only changes, installs UI dependencies and builds the app bundle from a clean checkout.
8. Release candidates do not overwrite latest. Full build inputs invalidate previous smoke evidence, including lockfiles and workflows.
9. P1 numeric fixtures compare integer values exactly. TypeScript checks must explicitly export first, without depending on test order.
10. Spec baseline, phase links, approval language and review status match the current execution instruction.

Research: Tokio documents that dropping a JoinHandle detaches its task; awaiting a mutable handle is cancellation-safe.
Sources: [JoinHandle](https://docs.rs/tokio/latest/tokio/task/struct.JoinHandle.html),
[CancellationToken](https://docs.rs/tokio-util/latest/tokio_util/sync/struct.CancellationToken.html),
[Cargo workspaces](https://doc.rust-lang.org/cargo/reference/workspaces.html).
These establish API behavior. They do not prove the planned implementation works.

## Delivery and checks

Implement small usable batches. Run focused smoke and required CI before delivering each candidate.
Verify the exact pushed revision, notify QA readiness, then finish broader suites and failure/recovery checks.
P1 is protocol infrastructure; it is not a runnable replacement. P2 first delivers a testable Rust pod agent.
Keep Go on master until P6 cutover. Intermediate Rust candidate builds use feat/rust and normal repository CI.
Update CHANGELOG.md in the same batch commit. Label partial work and validation limits explicitly.

P2 live checks now run during final P6 E2E, not during P2. Keep the final live spending cap at $5.
Retain the baseline Go CLI binary to check agent compatibility during that same final rental where practical.
Recheck actual provider prices and account availability before renting. Record actual costs separately from estimates.
Use cleanup on every live exit path. Only task-owned instances and processes may be stopped.

## Progress

- Plan corrections written. Active plan references and cancellation signatures checked. Historical review files retain their original whitespace.
- Baseline validation passed: Go build, 361 Go tests across 18 packages, Swift tests and `make mac` (bundle plus renders).
- Baseline merged/pushed as 3117f9b; pod-image CI 36556262206 passed. Finder reopen was not manually exercised in this pass.
- P1 complete on feat/rust at 5dfe9ab: 35 tests, fmt, clippy, fixture/TS drift and CI 36557109125 pass. Actionlint v1.7.7 passes.
- P2 library, pod wiring and binary are implemented. Local workspace: 136 tests pass. Static build, candidate CI and live providers remain pending.
- Post-P1 Go regression check: 361 tests passed across 19 packages.
- P2 runner keeps boot/watchdog futures owned by run. SSH bodies close their socket because russh Handle drop detaches its worker.
- P2 `pod::run` config-reader reference needs `+ Sync` for the spawned fatal-path wrapper. This is an implementation-driven contract correction.
- Full P1–P6 acceptance, live tests, final code review and release remain pending.

- P2 candidate ed65ecc pushed. Rust CI 36561539180 passed; local static musl build passed (8,015,704 bytes). Pod image CI 36561539178 is still running.
- Runner tests passed five consecutive runs. P3 config preparation starts while image publishing finishes; it depends on the tested P2 library, not a rented pod.

- P2 pod image CI 36561539178 passed. Candidate image: `ghcr.io/1905/lobocode@sha256:7e0d6c8e5c42b06152eea991def5af83df69572d8436767728cdc248c7077f8d`. No GPU rented.

- P3 Tasks 0–13 foundation/config batch implemented. 36 core tests and clippy pass. All 77 generated fixture files reproduce exactly; CI now checks drift. Provider and lifecycle work remains pending.

- P3 foundation a658c35 passed Rust CI 36563716290 and image CI 36563716069.
- P3 Tasks 14–26 implemented. 80 core tests pass. Submitted creates stay awaited on cancel; Vast adoption continues on cancel. No live provider request was made. Cross-operation blocking after an unresolved create remains part of the upcoming control work.

- Release implementation research: rusty-s3 0.10.2 source confirms path-style signing, paginated ListObjectsV2 parsing and custom signed headers. R2 documents conditional PutObject support (https://developers.cloudflare.com/r2/api/s3/api/). Task 31 now requires signed If-None-Match:* on immutable uploads, since HEAD alone races. Mock and final live verification remain pending.

- P3 providers 3bc03fc passed Rust CI 36565014524 and pod image CI 36565014449.
- P3 Tasks 27–35 implemented. 105 core tests and clippy pass. Mock tests cover isolated candidate upload, immutable-write conflict and failed uploads without latest promotion. No live R2 write was made.

## Paused by user

The user said "pause it for now". Stop implementation until the user resumes.
Last pushed commit: 292505a (release/checks batch). Rust CI 36566162351 and image CI 36566162343 passed.
Uncommitted work: local platform, state/locking, supervisor identity helpers, state tests and Go interop tests.
Resume by inspecting this working tree and the last local test result. Finish and verify Tasks 36–38, then continue models/runtime and the remaining P3 work. No GPU or R2 write was made.

## Resumed

The user said "great, continue". Resume the authorized full-auto workflow.
The paused local tests had passed (14 focused tests). Go interop and lint checks are now running.

- P3 Tasks 36–38 plus identity/state-URL helpers from Task 47 are implemented. 14 focused local tests and 3 real Go/Rust interop tests pass. CI now runs interop explicitly; P6 removes this Go dependency. Runtime/model/supervisor/control work remains pending.

- P3 Tasks 39–41 implemented. 29 focused local tests and clippy pass. Extraction owns staging files through cancellation, defers symlinks and rejects targets outside the staged tree. An explicit ignored test verifies the actual pinned archive; not run yet.

- Runtime/model batch 2b98c0c passed Rust CI 36575917465 and image CI 36575917816. The explicit pinned-archive test passed: real download, size/SHA check and extraction. No model weights were downloaded.
- P3 Tasks 42–47 implemented: Mac hooks, shared supervisor options, CLI/app Spawner prefixes, loopback API and scoped logging. The seven Mac dependency tests and seven supervisor/argument tests pass. Local provider lifecycle remains pending.
- Supervisor logging uses tracing's per-future WithSubscriber, not a global subscriber (https://docs.rs/tracing/latest/tracing/instrument/trait.WithSubscriber.html). Startup prepares fallible version data before claiming state. Cleanup failures return errors instead of reporting a successful stop.

- Supervisor batch 7226675 passed Rust CI 36577709755; image CI 36577710152 was still running at the next batch.
- P3 Tasks 48–49 and 51–52 implemented. All 11 real-process local-provider tests pass; clippy passes. Startup cancellation/timeout kills and reaps the owned group even without state. Identity is rechecked only for a living leader, preserving dead-leader child cleanup. Test fixtures now reserve unique port pairs and allow two seconds for the no-state timeout case.
- LocalHooks now owns its ps closure and explicit child-only environment overrides. This removes global test mutation; contracts and Task 49 record the change. Task 50 control traits and Tasks 53 onward remain pending.

- CI caught a packaging mistake in 40b51e7: the unanchored `bin/` ignore hid `src/bin/testchild.rs`. Local tests passed with the untracked file; CI had no helper. Scope the ignore to `/bin/`, add the source and rerun CI.

- P3 Task 50, Task 53, Task 54 fakes (events helper pending up), and Tasks 60–63 implemented. Seventeen control tests pass, including defaults precedence, provider errors during down, stale listings, local URLs and config-path forwarding. UpOperation and its cancellation/reconciliation tests remain pending.

- Clean-checkout fix 9489b21 passed Rust CI 36579110545, including all local process tests on Linux. The control batch's new-agent closure triggered clippy type_complexity; an equivalent AgentFactory alias fixes it.
- Before implementing up, filled the persistence gap in the v1.2 contract: Deps owns an OperationState. CLI/app share an OS-locked pending-operation file, while tests use isolated memory or temporary files. Uncertain creates survive a process restart and block later creates until reconciled.

- Operation storage and cleanup implemented ahead of up: pending records use 0600 files, atomic replacement, fsync and an OS lock. Empty/ambiguous listings retain uncertainty; a single new instance can be adopted and deleted. Down now uses the same operation lock. Up integration remains pending.
- Provider errors distinguish explicit RunPod create rejection from an uncertain response. RunPod/Vast uncertain creates include provider and boot ID. RunPod v1 create/list documentation and Vast create documentation were checked before this work. They do not establish retry idempotency or prove absence after one list response: https://docs.runpod.io/api-reference/pods/POST/pods ; https://docs.runpod.io/api-reference/pods/GET/pods ; https://docs.vast.ai/api-reference/instances/create-instance . No live provider request was made.

- P3 Tasks 55–59 implemented with the owned UpOperation. 24 start tests pass, including delayed/uncertain creates, failed cleanup, stale statuses, source selection, bad-host retries and panic cleanup. Thirteen real local process tests pass, including cancellation through the core worker before state publication and during runtime preparation.
- Full event queues can drop intermediate progress; an owned reserved channel permit guarantees room for the terminal event. A closed receiver requests cancellation, then the worker stays awaited. Successful ready delivery hands ownership back to normal running-state management; dropping a completed operation does not delete the instance.
- Task 64 API compile check passes. Missing planned re-exports were added. Contracts now include the actual operation-state/error fields rather than conflicting appended notes alone.
- Task 65 adds core-macos on macos-14 with an explicit arm64 check. GitHub's current runner reference lists macos-14 as Apple Silicon: https://docs.github.com/en/actions/reference/runners/github-hosted-runners . The workflow result still needs verification.
- Previous control/lint batch 1bdbbe7 passed Rust CI 36579862212 and image CI 36579862281. Current P3 close checks remain in progress; no GPU rental or live R2 write.


## Paused again — 2026-09-29

The user said "pause work for now. commit everything,". Save all current changes locally, then stop. Do not push or continue implementation until resumed.

- P3 owned start operation, persistent recovery, cancellation cleanup and API exports are implemented. The new Apple Silicon CI job is written but has not run.
- Latest local workspace tests passed. This run did not enable the three Go interop checks and ignored the pinned runtime network test. The real archive test passed in an earlier batch.
- Workspace clippy, documentation checks and workflow actionlint passed. Fixture regeneration completed; inspect the fixture diff when resuming.
- Pending phase-close work: explicit Go interop, Go regression checks, final formatting/drift checks, push and Ubuntu/macOS/image CI. No P3 completion notification has been sent.
- P4–P6 remain pending. Before P4 implementation, correct its stale TUI interruption text claiming that a pod keeps booting; cancellation now waits for cleanup.
- Estimated progress remains about 45% by planned tasks. This is not a time estimate or full acceptance.
- No GPU rental, live R2 write or model-weight download was made.


## Resumed — 2026-09-30

The user said "continue". Resume the authorized full-auto implementation and delivery workflow from 488d447. Finish the remaining P3 checks and candidate CI, then implement P4. Corrected the stale P4 interruption inventory to match owned cancellation and cleanup.

- Resume checks: explicit Go/Rust interop passed (3 tests); workspace formatting and tracked fixture/TypeScript drift checks passed.
- CLI research: clap supports optional boolean values with require_equals/default_missing_value, and ValueSource::CommandLine preserves explicit false flags (https://docs.rs/clap/latest/clap/struct.Arg.html, https://docs.rs/clap/latest/clap/struct.ArgMatches.html). Ratatui supplies terminal restoration hooks (https://ratatui.rs/examples/apps/panic/); inquire distinguishes cancellation and interruption (https://docs.rs/inquire/latest/inquire/error/enum.InquireError.html). Local CLI verification remains pending.

- P3 complete for frontend integration at 2884464: Rust CI 36654674144 passed on Ubuntu and Apple Silicon; static agent and image CI 36654674125 passed. Full Go regression passed locally. Live local/cloud inference remains P6.
- P4 baseline remains master 3117f9b. Go CLI/TUI/config source diff is empty. Captured 53 isolated Go CLI cases and 19 help outputs; replay implementation is next.

- P4 first CLI batch: 27 Rust test functions pass, including all 53 Go replay cases and 17 help paths. Real terminal Save/Escape checks passed; build-info override printed lobo 9.9.9. Captured fixtures regenerate without drift. CLI lint and Go capture vet passed. Control/dashboard/release handlers remain pending.

- First CLI candidate befb354 passed pod-image CI 36657068951, but Rust CI 36657068815 failed. Ubuntu exposed version-dependent Go JSON decoder wording; the fixture now records the stable CLI-owned prefix. Apple Silicon exposed an ephemeral-port collision in a local process test; fixtures now allocate unique low port pairs and never re-probe a pair already assigned to another test. Focused replay and repeated process tests pass locally; corrected CI remains pending.

- Corrected candidate 8cf8515 passed Rust CI 36658079574: Ubuntu, Apple Silicon and the static agent. All 53 CLI fixtures reproduce exactly; 13 real-process tests passed three consecutive local runs. P4 setup candidate QA notification sent; live command implementation continues.
- TUI implementation research: Ratatui inline viewports reserve rows from the current cursor and clamp to terminal size (https://docs.rs/ratatui/latest/ratatui/enum.Viewport.html). Local 0.30.2 source shows `resize` preserves the configured inline height, so content-height changes need terminal recreation at the old origin. Crossterm disallows mixing EventStream with synchronous input reads (https://docs.rs/crossterm/latest/crossterm/event/index.html); use nonblocking poll/read ticks alongside Ratatui cursor queries. Terminal restoration and native terminal smoke remain required.

- P4 control batch implements Tasks 30–41 and the fake-binary part of 46: eight Go dashboard goldens, terminal-buffer snapshots, boot report, start/status/stop handlers and executable JSON replay. All 43 CLI tests pass with test-fakes enabled. Cancellation waits for a 121-second fake create; failed cleanup never reports completion. Broken JSON output waits for deletion. An issued down remains owned through Ctrl-C. App E2E is still pending implementation; no live provider call was made.

- Five isolated PTY checks passed for the compiled fake-provider CLI: ready, q, Ctrl-C, status quit and terminal resize. Each restores canonical input, echo, signals and cursor visibility; no alternate screen is used. Both cancelled starts exit 1 after cleanup. These checks use fake providers and verify terminal behavior, not live inference. All-target/all-feature CLI clippy and workflow actionlint pass.

- Control candidate ab1313c passed Rust CI 36659723042 and pod-image CI 36659723082. QA notification sent. The user reported the earlier befb354 failure; verified that all three jobs pass on the current pushed revision. Every later delivery still requires green CI.
- P4 logs/test/release handlers implemented. Four real loopback HTTP tests and two release construction/metadata tests pass. Publication and its build child stay awaited once started. No live R2 write or GPU request was made. Cross-platform CLI packaging is next.

- CLI handler candidate 958ff4a passed core-macos and agent-musl, but Ubuntu failed in `ensure_fetches_once` with `error sending request`. Investigated the process-global HTTP client. A two-runtime regression test reproduces a hang before the fix: the pool reuses a connection whose current-thread runtime is no longer polled. HTTP clients now belong to callers; HttpSource retains its client across model chunks. Corrected CI remains pending. Upstream evidence: https://github.com/seanmonstar/reqwest/issues/2501 and Tokio Runtime shutdown docs (https://docs.rs/tokio/latest/tokio/runtime/struct.Runtime.html).

- HTTP ownership fix passed its regression, all 399 workspace tests (one pinned-runtime network test ignored), formatting and all-target/all-feature clippy. P3 Go interop is not enabled in this run. The release spike has built Intel macOS; the other three targets remain in progress.

- Candidate d4ed8c4 fixed the HTTP regression, but Mac CI exposed another local-process port collision at 10022. The test helper forks via pre_exec while other fixtures probe ports. Child processes can inherit those descriptors until exec, so distinct port ranges alone do not isolate concurrent fixtures. Added a fixture-lifetime async mutex around all real-process tests, keeping their startup/cancellation/cleanup assertions intact. This is an inferred race mechanism from the code and Unix fork behavior; repeated local runs and corrected CI are the verification.

- Mac process fixture fix: all 13 lifecycle tests passed ten consecutive local runs; focused clippy passed. Ubuntu and agent-musl succeeded on d4ed8c4. The next commit reruns all jobs with process isolation. Official pre_exec documentation confirms child descriptor duplication: https://doc.rust-lang.org/std/os/unix/process/trait.CommandExt.html#tymethod.pre_exec.

- CI repair verified at 57caf1a: Rust run 36661673477 passed protocol, core-macos and agent-musl. No retries or test removal were used to get this result. Pod-image publishing is still running.

- P5 preparation while the four-target P4 packaging spike runs: the Swift source is unchanged from baseline 3117f9b. Rebuilding and saving its fixed-state renders before replacement. This does not claim P4 packaging or P5 acceptance.

- P5 baseline preparation complete: rebuilt unchanged Swift source, saved 10 panel renders, 10 tray renders, settings and icon under p5-baseline/. All 17 Swift tests pass. Inspected ready/settings references. The Rust app is not implemented yet.

- P4 packaging spike: all four Rust CLI targets and archives built. GoReleaser 2.13.3 requires `--package=lobo-cli` for package detection. The first archive run then failed at formula generation because `HOMEBREW_TAP_KEY` was absent. Snapshot target now supplies an empty key and limits builds to one at a time. No release was published. A clean-revision snapshot and local formula install remain required.

- Pod image CI 36661673515 also passed at 57caf1a. Sent the CI-repair QA notification with the green Rust run and remaining app/live-test limits.

- P4 complete at runtime/package revision fcb8aff. All six packaging criteria, temporary Homebrew install/test, default-feature seam check and 51 CLI tests pass. Rust CI 36662757471 and pod image 36662757454 are green. Final live checks remain P6. P5 shared config/protocol and separate app workspace implementation started.

- P5 shared API batch: UpRequest/Readiness and generated TS, readiness/validation, resolve_up and free_bytes_nearest implemented. 255 proto/core tests pass (one ignored network check), clippy passes. Separate app scaffold, reducer and controller are uncommitted work in progress.

- Shared P5 API candidate 914ac19 passed Rust CI 36664236686 and image CI 36664236708. All three Rust jobs are green.
- P5 native backend/UI candidate implemented. 36 Rust and 8 UI tests pass, including failed Quit retry, 121-second cleanup ownership and running-state endpoint selection. UI check has zero errors/warnings. App CI now installs frozen dependencies, checks generated files and builds/verifies a native bundle. Local bundle build is running; clean-runner app CI and actual app E2E remain pending.
- All 41 browser fixtures were inspected in Chromium. Selected colors, the OFF icon preview and settings header/actions needed fixes; recapture is in progress. This is renderer validation, not native app E2E.

- Renderer recapture passes: 41 screenshots, 14 computed-color checks, zero console errors/warnings and zero horizontal overflow. Settings differs from Swift by one CSS pixel in height. Self-contained review page generated in bin/app-renders/review/index.html. Native acceptance remains pending.

- App candidate 8668216 passed all four Rust CI jobs in run 36666850050, including the native bundle, code-signature check and artifact upload. Local release bundle also built (13.24 MiB, ad-hoc signature). QA notification sent with native/live acceptance limits.
- Native startup smoke found a UI deadlock that build/unit/render checks did not catch. A worker held the tray cache mutex while a native setter waited for the main thread; setup on the main thread tried to acquire that same cache. The owned app's process sample showed the main-thread mutex wait. Tray state changes now run together on the main thread. The corrected native launch/E2E is pending; do not call the candidate runtime accepted yet.
- Native tests use the documented WDIO embedded driver with a non-default e2e feature and separate bundle identifier. Dependencies are pinned to WDIO 9.30.1 / Tauri service and plugins 1.4.0; newer WDIO packages had conflicting expectation-library peer requirements. Tests create a sparse q6 fixture and fake local HTTP runtime. This verifies the actual core/supervisor lifecycle, not model inference.

## Session recovery — 2026-09-30

The user lost the implementation session, then said "do finish it." Continue the recorded full-auto workflow through P5, P6 and v0.2.0 release. Recovered branch: `feat/rust` at `8668216`, with the native startup correction and E2E harness uncommitted. The public release is still v0.1.0. The external model drive and Dell runner are available. Do not claim the pending startup fix or release is accepted until verified.

- Testing override: the user said "dont test it on this mac no mem yet" and "only UI". Stop Mac model inference and backend/lifecycle suites. UI-only native/browser checks and UI builds may continue here. Move further non-UI checks to Dell, normal CI or the authorized final GPU runs. This overrides P5/P6 local inference acceptance steps; do not rerun them on the Mac without a new user instruction.
- Release hold: the user said "good. dont release yet i will test in later", then "just update assets and readme" and "check tui too". Do not merge to master, tag, publish, rent GPUs or continue the P6 cutover. Finish the requested assets/README and UI-only TUI check, preserve current work, and leave the release pending the user's later testing.

- User corrections during native testing: the window could not move, both scrollbars appeared and content was clipped. Never use scrolling; the app must feel native and have rounded corners. This overrides the planned overlay title bar and single-page Settings layout. Use native title bars, content-sized windows, compact Settings tabs and rounded transparent tray-panel corners. Verify actual window movement and every native view's bounds.
- First native E2E reached SETUP and proved startup no longer deadlocks, but its text selector assumed a space between `sys:` and `SETUP`. No lifecycle test passed. Replace that selector with the existing semantic `data-phase` attribute and add actual bounds checks. Test-owned processes were cleaned up.

- Native fix verified locally: four E2E tests pass (setup/save validation, real supervisor start/copy/stop, loading cancellation, runtime failure/retry). The harness confirms every tested view fits, and checks supervisor/runtime cleanup. App Rust tests: 37 pass; UI tests: 8 pass; clippy/check clean. Browser QA also passes all 20 panels and three Settings tabs. Reopen/resume, actual notification delivery and live model inference remain pending.
- Window sizing evidence: Tauri reported 269 logical pixels of content height while WKWebView had 241, a 28-pixel native title-bar inset. Measuring the inset before content sizing fixes clipping. Native title bars retain OS movement; transparent tray windows and 12-pixel surface clipping provide rounded corners. Research: https://v2.tauri.app/learn/window-customization/ ; local pinned Tao 0.37.1 window sizing implementation. The native build, not the documentation alone, established the fix.

## Domain-free cloud — 2026-09-30

The user requested a free connection without buying a domain, then said
"plan it first. then implement. fuck domains". This authorizes the implementation
below. It does not lift the release hold or the Mac testing limit.

- Wrote [the connection plan](../2026-09-30-domain-free-cloud.md) before implementation.
- Added shared automatic SSH forwarding with per-instance keys, pinned server
  identity, provider TCP mappings, a detached reconnect helper and owned cleanup.
  New cloud setups use localhost port 8933 and need no domain or Cloudflare token.
  GPU rental, bandwidth and model storage remain separate costs.
- Updated native Settings, CLI wizard, OpenCode configuration, README, screenshots
  and changelog. Saved the earlier UI/TUI/documentation batch in local commit
  `376b38c`. Neither this feature nor that batch has been pushed.
- Backend suites and real SSH fixtures ran on Dell. Streaming, tool calls,
  authentication, a 105-second first response, reconnect, helper crash recovery,
  key rejection, occupied ports and cleanup pass. Workspace clippy passes.
  Detailed run boundaries and the initial fixture failures are in the plan.
- Mac checks were limited to UI and builds: native setup/save, frontend checks,
  view-store tests and fixture generation pass. All 20 panel and 3 Settings
  browser layouts fit without scrolling. TUI layout and terminal smoke checks pass.
- Release, push, merge, GPU rental and agent publication remain on hold. The new
  SSH mode requires the matching agent, which has not been published. Live
  RunPod/Vast provisioning, GPU inference, real sleep/wake and native window
  movement still need acceptance. The installed app and old DMG were not replaced
  with this domain-free feature.
