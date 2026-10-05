# Startup diagnostics execution record
**Date:** 2026-10-05
**Status:** in-progress

## Baseline and evidence

- Master c16f6ea, exact CI 36994301207 passed. App 0.2.1 installed from public app-v0.2.1.
- User reported a 20-minute RunPod container-step stall. No durable startup log existed; app stdout/stderr were /dev/null. At 05:36:52 UTC, read-only RunPod and Vast requests each returned HTTP 200 with zero instances. This does not identify the earlier failure.
- Native Computer Use failed to start its pipe; no live UI status was read during diagnosis. Private read-only receipt: /var/folders/85/nvpy_pws0774v6l8wl87_8b80000gn/T/lobo-stall-20261005-zy6asv09/summary.json.

## Implementation checks

- Feesh UI: 20 tests pass and production Vite build passes for the initial freshness/Logs/cleanup-control changes. Svelte type checking waits for hosted Rust-generated bindings.
- Dell core checks passed: 270 library tests, one pre-existing opt-in archive-download test ignored; all-target clippy passed. Five new cases cover pre-agent missing pods, unknown authorization state, repeated heartbeat, hung reads and cancellation. Evidence: /tmp/lobocode-startup-core-20261005-evidence/. This follows the 2026-10-03 Rust-host correction; no containers ran on Dell.
- App Rust and native checks run on hosted macOS. No local Cargo, inference or OpenCode launch.

## Limits

The historical root cause remains unknown. No new paid GPU test is part of this logging/regression release. Release, local install and persistent real launch-log verification remain pending.
