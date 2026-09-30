# Implementation and validation results

Status: implementation in progress. E2E deferred at the user's request. The Mac currently has no available memory for builds or app testing; continue source edits here and use remote builds/tests.

- Base: master `ddd1d6a`; memory implementation `b9a1966`; docs `597fb1c`; isolated merge `e84e56f`.
- Earlier native memory run passed five cases, then its app exited normally at forced-denied Start. The WebDriver connection failed. The initiating exit cause remains unknown; this is not passing acceptance.
- Memory review found a queued-Start selection race. The approved feature plan includes its fix.
- Baselines pass: UI 11 tests, type check with zero warnings, Vite build; Dell core control module 52 tests. No new inference or provider requests.
- Public release, GPU images and updater remain held or paused. CLI, config and models are preserved.

## Deferred native fixture repair

Read-only diagnosis found that case 6 assigns a replacement to Tauri 2.12.0's non-writable, non-configurable `__TAURI_INTERNALS__.invoke`. The replacement cannot install, so its error variable stays null even after denied Start. Replace this interception with observed Failed state and a direct invoke/try-catch assertion.

The denial notification arrived at 09:54:32.073Z. The task app PID 46560 exited with status 0 at 09:54:35.566Z; WebDriver disconnected at 09:54:35.688Z. No crash report or recorded initiating exit command exists. Add test-only quit/ExitRequested and child-exit tracing before the deferred rerun. Do not report the normal exit as a crash or infer user action.

Cloud recovery limit: provider Instance metadata has no boot identity. After an uncertain create, the app must retain unresolved ownership unless an exact local boot or saved cloud connection proves the candidate. It must not delete an unproven instance or rent again. Live cloud acceptance remains held.

## Hosted baseline fb6e0bf

Run `36702647743` uses `native_e2e=false`. Protocol/Linux checks and static agent build pass. App lint, tests, fixture generation and native bundle pass; the native E2E step is skipped. Core macOS: 226 tests passed, one ignored, one failed. `native_snapshot_reads_without_loading_a_model` unconditionally unwraps the probe; the runner returns `Mac memory measurement unavailable: Metal device unavailable`. Production startup refusal is correct. Commit `fb0489b` independently checks Metal availability and requires that exact error when no device exists. Hosted retest is pending.

## Core ownership batch

Commits `d4b5c6c` and `ee60fbf` implement Tasks 1–4. The core app APIs use immutable runtime identity for discovery, status, sampling, Start and Stop. They preserve foreign pending records and saved connections. Local actions do not call cloud providers. CLI behavior is unchanged.

Dell validation passes: 74 control tests, 19 local-provider tests, one scoped connection fixture and Clippy with warnings denied. Final review corrections pass 24 app-scope tests. The native app build check passes on this Mac; no backend tests ran here.

Parent review found and corrected an unchecked adjacent saved port and unproven cloud discovery through a shared domain. Invalid ports preserve saved bytes; a domain reporting another runtime cannot establish instance ownership. Parent compliance and quality review pass for this batch. App wiring is next; E2E and final acceptance remain pending.

## Hosted ownership checkpoint 9ea8f6a

Run `36704616125` completed. The app, core-macos and static agent jobs pass. The missing-Metal memory-test correction is verified on hosted macOS. Native E2E remains disabled.

Linux lint passes, but the core library suite reports 249 passed, one ignored and one failed. `local::deps::tests::check_gpu_table` fails while executing a generated fixture with `ExecutableFileBusy` / `Text file busy`. This fixture failure is under investigation. It is not an inference run or evidence that the memory guard failed. The full workflow is not green yet.

The test helper now writes its executable through a child shell with literal positional arguments, then waits for exit before execution. This removes writable fixture descriptors from the shared test process. Concurrent fork inheritance is the likely failure mechanism; the hosted run did not capture descriptor traces. No production retries or sleeps were added. Dell `cargo test --locked -p lobo-core --lib` passes 250 tests with one opt-in test ignored. Hosted Linux retest remains pending.
