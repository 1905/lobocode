# Rust rewrite execution record

Date: 2026-09-29.
Authorization: the user said, "when plan is fixed start implementation in full auto mode."

## Active documents

| Phase | Plan | State |
|---|---|---|
| P1 | [Workspace and protocol](plan-p1-v1.1.md) | done: 5dfe9ab, CI 36557109125 |
| P2 | [Pod agent](plan-p2-v1.2.md) | code/CI done at ed65ecc; live E2E pending P6 |
| P3 | [Core](plan-p3-v1.2.md) | paused: implementation complete; phase-close checks/CI pending |
| P4 | [CLI](plan-p4-v1.2.md) | pending P3 |
| P5 | [App](plan-p5-v1.2.md) | pending P4 |
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
