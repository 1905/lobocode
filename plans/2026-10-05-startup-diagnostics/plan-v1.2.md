# Startup diagnostics implementation plan v1.2
**Date:** 2026-10-05
**Status:** in-progress
**Spec:** ./spec.md (approved by direct implement/re-release/install instruction)
**Goal:** Deliver app 0.2.2 with durable private logs and explicit stalled-start outcomes.
**Architecture:** App-owned bounded JSONL diagnostics plus supervised startup; exact provider liveness in shared startup loop.
**Tech Stack:** Rust/Tauri, Svelte, Python native fixtures, GitHub Actions.

> For agentic workers: use superpowers:subagent-driven-development to implement task-by-task. Checkbox syntax for tracking.

## File map
Use the exact spec map. App backend worker owns Rust app code except version metadata. Core worker owns shared core startup checks. UI worker owns panel/lib changes. Parent owns integration, generated bindings, native harness, version/release, docs and delivery.

## Baseline sanity
- [x] Clean master c16f6ea; last exact CI 36994301207 passed. Fresh fix/app-startup-diagnostics worktree. No Mac Cargo.

## Tasks
- [ ] 1. App diagnostics and supervision: implement the contract, expose Logs IPC, redact/bound/rotate, add regressions for privacy, worker failure and deadlines. Verify hosted `make app-lint app-test app-fixtures`.
- [x] 2. Core startup: regression for pre-agent missing pod and heartbeat/unknown status; preserve guarded cleanup/retries. Dell focused core tests and clippy, then normal CI.
- [ ] 3. UI: Logs action, last-update age and visible logging errors; retain fixed layout. Feesh UI tests/check/build/format; remotely generated types must match.
- [ ] 4. Parent integration: native diagnostics checks, version 0.2.2, docs/changelog, one whole-branch rival-codex review, fix material findings and run required CI.
- [ ] 5. Direct merge/push; exact master CI, app-v0.2.2 app release, anonymous DMG validation, local app-only install and new durable log proof. Notify QA readiness then complete remaining suites/records; archive only after delivery.

## Interface consistency
- [ ] PanelState last_update_ms/log_path/logging_error/start_allowed match Rust-generated TS and fixtures.
- [ ] Logs command name `open_logs`; UI uses existing native invoke route.
- [ ] Provider absence and unknown state remain distinct; timeout never abandons an in-flight rental.
- [ ] App version/tag/source/install match; CLI and model images unchanged.

User authorization supersedes skill approval/handoff pauses. Parent owns all E2E, publication and installation. No live GPU rental needed for this regression.

## Revision v1.1
Expose backend Start permission so unresolved cleanup cannot display an actionable Retry. Set silence warning after 45 seconds to allow the normal 30-second provider heartbeat. Parent implements UI because the agent thread limit prevented a third implementer.

## Revision v1.2
Bound resumed startup snapshots and restored Booting state; enforce unresolved-cleanup dismissal guard in IPC as well as the UI. Keep panic payloads out of diagnostics. These close the same indefinite-boot failure modes after restart.
