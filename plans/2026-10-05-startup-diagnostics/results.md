# Startup diagnostics execution record
**Date:** 2026-10-05
**Status:** in-progress

## Baseline and evidence

- Master c16f6ea, exact CI 36994301207 passed. App 0.2.1 installed from public app-v0.2.1.
- User reported a 20-minute RunPod container-step stall. No durable startup log existed; app stdout/stderr were /dev/null. At 05:36:52 UTC, read-only RunPod and Vast requests each returned HTTP 200 with zero instances. This does not identify the earlier failure.
- Native Computer Use failed to start its pipe; no live UI status was read during diagnosis. Private read-only receipt: /var/folders/85/nvpy_pws0774v6l8wl87_8b80000gn/T/lobo-stall-20261005-zy6asv09/summary.json.

## Implementation checks

- Feesh UI: 20 tests, Svelte type checks (zero warnings), production Vite build and formatting pass with hosted Rust-generated bindings from CI 37269716115.
- Dell core checks passed: 270 library tests, one pre-existing opt-in archive-download test ignored; all-target clippy passed. Five new cases cover pre-agent missing pods, unknown authorization state, repeated heartbeat, hung reads and cancellation. Evidence: /tmp/lobocode-startup-core-20261005-evidence/. This follows the 2026-10-03 Rust-host correction; no containers ran on Dell.
- App Rust and native checks run on hosted macOS. No local Cargo, inference or OpenCode launch.

## Review and integration

- Rival review of dab5025 found three defects: Stop/Ready cleanup race, resumed deadline falsely failing a previously Ready runtime, and malformed config error exposing opaque credentials. Full output: [review.txt](review.txt). The follow-up changes accept successful completion during Stop, track established readiness per runtime identity, bound resumed observation instead of pod age, and remove config values from parser/app errors. Dell follow-up passes 34 configuration tests and 53 CLI tests with test-fakes, including Go consumers and all TUI snapshots. Feature-enabled core/CLI clippy passes. CI 37270507497 passed all shared checks and app lint. App tests: 94 passed, one existing directory-count assertion failed because diagnostics adds a directory. The test is being changed to compare directory contents before and after cancellation. All new diagnostic/deadline/race regressions passed.
- First hosted CI compiled the app and generated bindings. It found a one-character clippy fix and CLI/TUI golden expectation changes for the intentional initial agent-unreachable detail. These failures block release until corrected.

## Limits

The historical root cause remains unknown. No new paid GPU test is part of this logging/regression release. Release, local install and persistent real launch-log verification remain pending.
