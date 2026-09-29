# Rust rewrite execution record

Date: 2026-09-29.
Authorization: the user said, "when plan is fixed start implementation in full auto mode."

## Active documents

| Phase | Plan | State |
|---|---|---|
| P1 | [Workspace and protocol](plan-p1-v1.1.md) | done: 5dfe9ab, CI 36557109125 |
| P2 | [Pod agent](plan-p2-v1.2.md) | in progress |
| P3 | [Core](plan-p3-v1.2.md) | pending P2 |
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

The P2 live budget is approximately $1. P6 live spending is capped at $5.
Recheck actual provider prices and account availability before renting. Record actual costs separately from estimates.
Use cleanup on every live exit path. Only task-owned instances and processes may be stopped.

## Progress

- Plan corrections written. Active plan references and cancellation signatures checked. Historical review files retain their original whitespace.
- Baseline validation passed: Go build, 361 Go tests across 18 packages, Swift tests and `make mac` (bundle plus renders).
- Baseline merged/pushed as 3117f9b; pod-image CI 36556262206 passed. Finder reopen was not manually exercised in this pass.
- P1 complete on feat/rust at 5dfe9ab: 35 tests, fmt, clippy, fixture/TS drift and CI 36557109125 pass. Actionlint v1.7.7 passes.
- P2 starts next. Post-delivery Go regression check remains pending.
- Full P1–P6 acceptance, live tests, final code review and release remain pending.
