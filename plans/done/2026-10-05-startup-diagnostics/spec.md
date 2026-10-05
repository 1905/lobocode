# Persistent startup diagnostics and bounded boot state
**Date:** 2026-10-05
**Scope:** /Users/kass/dev/lobocode
**Status:** done (approved by direct implementation, re-release and local-update request)

## TL;DR
**What:** Save private, redacted startup logs and expose Logs from the app.
**Why:** A frozen container step currently leaves no durable evidence.
**What:** Show recent startup checks and explicit missing-pod, timeout or worker failures.
**Your action:** Nothing; implementation, DMG release and local installation are authorized.
**Limits:** No local inference, OpenCode launch, CLI release, new image build or paid GPU test.

## Problem(s)
1. App startup details exist only in a six/eight-line memory buffer (`app/src-tauri/src/store.rs:299`). Launch Services routes console output to /dev/null on the installed app. Earlier startup history cannot be reconstructed.
2. Provider absence is checked only after the agent answered once (`crates/lobo-core/src/control/up.rs:689`). A pod that disappears before agent startup can remain at container until the broader timeout. Connection errors and provider checks need visible, timestamped evidence.
3. The app awaits progress events before worker completion and does not supervise an outer controller panic (`app/src-tauri/src/controller.rs:453`). Failure text can favor old progress over the actual error (`store.rs:342`). The visible timer does not establish that backend work is alive (`app/ui/src/panel/BootLog.svelte`).
4. App 0.2.1 has no way to open diagnostic logs. Releasing requires hosted macOS checks and installation of the exact public artifact.

## Goals
1. Persist startup/lifecycle diagnostics across restart with version, session, time, provider, instance/boot identity, phase, selected model, errors and completion. Include status checks while waiting.
2. Keep secrets, credentials, full configuration, request prompts and private SSH keys out of logs. Bound storage and restrict file access.
3. Detect a missing pod before its agent ever connects; distinguish provider state, agent reachability and unknown status. Do not claim desired RUNNING means a started container.
4. Surface real worker failure and a startup deadline. Retain operation ownership while cancellation/cleanup is unresolved; never enable a second rental during uncertain cleanup.
5. Show last startup update age and a Logs action without scrolling. Install released app 0.2.2.

## Diagnostics contract
Use newline-delimited JSON in a private app log directory next to the active config: `config.app-logs/app.jsonl`. Normal path is `~/.config/lobo/config.app-logs/app.jsonl`; fixture configurations remain isolated. Rotate at 2 MiB, keep current plus three archives, bound individual messages, mode 0700 directory/0600 files. Do not follow symlinks. Record one launch/session marker, config load outcome without values, start/admission, ownership changes, progress/heartbeat, terminal outcomes, Stop/Quit and worker panic. Durable writes must occur before rendering corresponding state.

Only allowlisted fields enter records; scrub configured secret values plus common credential/private-key forms from free text. Never serialize complete config, request/response bodies, status structs or environment. Logging failure must be visible in the app; it must not block Stop/cleanup. Logs opens the fixed diagnostic path through the native opener. No in-app scrolling log viewer.

PanelState adds `last_update_ms: Option<i64>`, `log_path: Option<String>` and `logging_error: Option<String>`, and `start_allowed: bool`. Last update means a real backend event or successful snapshot, not a frontend timer. The startup panel shows elapsed time since that update and marks silence beyond 45 seconds as unconfirmed. Start/Retry requires backend start_allowed plus cloud readiness; failed cleanup exposes Retry Stop and disables Dismiss. Logs remains available in all phases.

## Worker and provider behavior
Check the exact provider instance while waiting for the agent, including before first contact. A confirmed 404 ends startup with an explicit failure and preserves guarded cleanup semantics. Network/auth failures are unknown status, not absence. Emit a bounded periodic heartbeat with provider/agent outcome. Existing 30-minute container and 40-minute overall limits remain; the app's wall-clock supervision must cancel stalled startup and report any unconfirmed cleanup without dropping/aborting a possibly successful rental worker. Observe outer startup panics and preserve the actual error over stale progress. Bound read-only snapshots to 45 seconds. Resumed boot state has a 40-minute observation deadline only before the current runtime first reaches Ready. Never infer an unfinished startup from pod age. Reset tracking for a new provider/instance/boot identity. A previously Ready runtime must recover after a temporary agent disconnect. Configuration parse failures must not include source values in logs or UI. Dismiss cannot clear unresolved cleanup through IPC.

## File-level changes
| Files | Change |
| --- | --- |
| app/src-tauri/src/diagnostics.rs; diagnostics/tests.rs; controller.rs; controller/tests.rs; store.rs; store/tests.rs; types.rs; backend.rs; commands.rs; lib.rs; Cargo.toml; Cargo.lock; examples/generate_ui.rs; tests/fixtures.rs | Private bounded logger, lifecycle supervision, error/freshness state, Logs IPC and regressions. |
| crates/lobo-core/src/control/up.rs; up_tests.rs; testkit.rs; provider/runpod.rs; provider/runpod/tests.rs | Provider checks/heartbeats during pre-agent wait; preserve ownership and cancellation; change only files required by evidence. |
| app/ui/src/{panel,lib,gen,fixtures}; app/e2e/app.spec.js; fixture.py; tools/native_app_e2e.py | Freshness/Logs affordance, visible logging errors and native fixture coverage; regenerate bindings remotely. |
| app/src-tauri/tauri.conf.json; app/ui/package.json; .github/workflows/{rust,app-release}.yml; Makefile | App 0.2.2 metadata and required check/release adaptations only. |
| README.md; CHANGELOG.md; AGENTS.md; docs/implementation-mistakes.md; docs/img; plans/done/2026-10-05-startup-diagnostics | Log location, failure behavior, release/install evidence and unresolved historical diagnosis. |

## Tests
- Logger: write/reopen/rotation/permissions, symlink rejection, secret and PEM scrubbing, write failure surfaced, bounded line length.
- Core: pod missing before first agent response; transient provider error does not delete a live pod; periodic heartbeat and existing timeout/cancel/owned-cleanup regressions.
- App: worker panic and terminal error end boot visibly; no stale-progress replacement of error; deadline/cancel retains cleanup ownership and prevents duplicate Start; diagnostic records survive restart.
- UI/native: Logs visible, freshness and logging failure clear, failure plus Retry/Abort honest and no scrolling; fresh/legacy setup stays green.
- Hosted macOS builds, app tests/generated fixtures, full Rust CI. Rust/core tests may run on Dell; UI/container checks on Feesh. No compilation on this Mac.
- Download exact public DMG, verify checksum/source/signature, install app only, retain rollback, check process and new log marker. Native visual check depends on available automation; report any blockage.

## Failure modes & decisions
| Failure | Behavior |
| Log directory unavailable | Visible logging error; cleanup remains available. |
| Provider 404 before agent | Explicit missing-pod failure; no endless container state. |
| Provider timeout/auth error | Status unknown with error; do not invent absence or successful cleanup. |
| No worker completion by deadline | Cancel cooperatively; show timeout/cleanup status; retain worker and ownership. |
| Worker panic | Observe join/catch failure, log it and end misleading boot state. |
| Native tool unavailable | Hosted native checks plus installed artifact/process/log checks; no invented visual acceptance. |

## Out of scope
Historical root-cause certainty without logs; remote log export/cloud telemetry; local inference; CLI release; Q8/image rebuild; automatic retries beyond existing limits.

## Rollout
P1: Implement diagnostics and failure handling in an isolated worktree; focused tests and hosted smoke.
P2: Independent whole-branch review, fix findings, merge/push exact passing revision.
P3: App-only tag/release 0.2.2, verify anonymous artifact, install locally, notify, archive evidence.

## Research
- [Tokio JoinHandle](https://docs.rs/tokio/latest/tokio/task/struct.JoinHandle.html): dropped handles detach tasks; await join errors and retain cleanup workers.
- [tracing-appender rotation](https://docs.rs/tracing-appender/latest/tracing_appender/rolling/struct.Builder.html): retention is not bounded by default. Use explicit size/retention limits for this small structured diagnostic stream.
