# Implementation and validation results

Status: implementation in progress. Native E2E remains deferred. The separately authorized 47,000-token diagnostic was blocked by actual memory admission before model launch. Builds and unit checks remain remote.

Task20 source is prepared: two sequential fixed prompts, exact tokenizer arrays, hard token/request limits, owned existing-runtime checks and numeric-only private evidence. It starts nothing. Parent source review corrected a FIFO-read risk, fake identity-change case and a fragile privacy assertion; the child process receives a minimal environment. The original and same-size script syntax checks and HTTP fake self-tests now pass on Dell. No real generation request has run. An EOS-only result can validate protocol/counts while `content_seen=false`; it does not prove a visible reply. Acceptance remains pending in Task21.

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

## App ownership checkpoint 5d3817e

Task 5 source and regression tests are committed. Start captures the request and reserves its worker before memory admission. The app persists runtime identity privately, guards discovery before adoption, and uses the same owned target through Stop. Parent source review corrected stale discovery persistence and stale Ready status after Stop.

Rust formatting and whitespace checks pass. The new app tests have not run yet; compilation and tests will run on hosted macOS. No local build or test started after the user reported unavailable Mac memory. OpenCode repair, Dock behavior and telemetry remain in progress. No install, E2E or public release occurred.

Run `36706410556` at `70855c3` was cancelled after a fixture correction. Replacement run `36706552790` at `b79c973` passes Linux protocol/workspace checks, core-macos and static agent. App compilation fails with E0282 in the admission result. Commit `d26d5c2` adds the missing explicit result type; its hosted retest is pending. It also preserves legacy snapshot IDs and boot metadata while keeping the new ownership fields private. E2E remains disabled.

Run `36707086020` at `ff959e8` found app lint failures: public helpers exposed private StartSubmission, and the test preparation gate needed a type alias. Commit `aad828b` corrects both. Source formatting/whitespace pass; hosted app tests still await a passing build. No local compilation ran.

Run `36707493061` at `aad828b` passes Linux/core-macos/static-agent and app lint. App units: 52 pass, one fails. The old denied-Start assertion expected no display sample; the asynchronous worker now refreshes after admission failure. Its fake still returned sufficient memory, independently of its forced admission denial. The fixture now changes the displayed sample to insufficient before denial and waits for that fresh sample. It still requires zero runtime launches/rentals and two independent rejected admission attempts. Production admission logic is unchanged; hosted retest is pending.

Hosted run `36708630935` at `ca76f55`: app lint, Rust/UI unit tests and generated fixture/TypeScript checks pass. The corrected denied-Start fixture passes. Core macOS, Linux protocol/workspace and static-agent jobs pass. Native bundle is still building; native E2E remains disabled. Task 5 source review and unit acceptance are complete.

## Core OpenCode repair checkpoint

Commit `739ca5b` completes Tasks 6–7. Parent source review passes strict parsing, leaf edits, unrelated-byte preservation, private key/backup files, no-op validation, rotation, and final fingerprint checks. Dell: 15 OpenCode fixtures, six unchanged CLI export fixtures and Clippy pass. Both lockfiles add only jsonc-parser 0.33.2. The final fingerprint check is optimistic; it cannot synchronize an external writer after that check. App authentication and controls remain in progress. No personal config was read or changed.

Run `36708630935` completed successfully at `ca76f55`, including the native bundle and bundle verification. The native E2E step was skipped. The core OpenCode checkpoint `86bdb68` is pushed for its own hosted checks; app setup wiring is in progress.

Run `36709440347` completed successfully at `86bdb68`. The new core OpenCode writer passes both Linux and macOS CI, and the app bundle passes. Native E2E was disabled. This verifies the core checkpoint, not the app setup commands still being implemented.

## App OpenCode setup checkpoint

Commit `bab6a73` adds Task8 commands and regression fixtures. Parent source review covers captured runtime/key validation, bounded no-redirect model discovery, final commit guards and selected-file metadata. Corrections keep commit guards alive across panic handling and parse the bounded captured config bytes instead of reopening the file. Owner reads are also bounded. App compilation and new units are pending hosted macOS. Task9 frontend and Task10 native activation are in progress; no personal config, installed app or runtime was changed.

Run `36712619930` at `18c010e`: Linux protocol/workspace, core-macos and static-agent jobs pass. App Rust Clippy and Svelte checks pass, with zero Svelte errors/warnings. Prettier rejects `app/ui/src/lib/api.ts`; the app unit/build steps did not run. The Clients delivery will include the formatting correction and a hosted rerun. Native E2E remains disabled.

## Clients and native activation checkpoint

Commit `fe54faf` implements Tasks9–10. Parent source review passes path/checkbox retention, runtime-change invalidation, duplicate prevention, safe errors and durable Settings-tab delivery. Review corrections retain useful sanitized authentication errors and preserve pending success when the user changes tabs. The native app uses Regular activation and no longer declares LSUIElement. Actual Dock, Command-Tab, drag, corner and picker acceptance remains deferred.

Dell UI checks pass: 19 tests, zero Svelte errors/warnings, Vite build with 189 modules, and the full UI formatting check. These checks include the Task8 API formatting correction. No Node process, build, app launch or test ran on the memory-constrained Mac. Hosted native compilation, app tests and bundle verification are next. README now describes the app-owned setup flow and marks the old screenshots and pending native acceptance.

## Hosted app artifact b5617ae

Run `36713757006` passed all jobs at `b5617ae6e4a44f7deada77dbeeff4f60f68c4aa1`: protocol/Linux, core-macos, agent-musl and app. App lint/tests, generated TypeScript/fixtures, bundle and signature checks pass. Native E2E was disabled. Artifact `11095757309` was downloaded into a private temporary directory; local `codesign --verify --deep --strict` passes. The actual bundle declares neither `LSUIElement` nor `LSBackgroundOnly`. The installed app remains unchanged.

## Direct same-size diagnostic — blocked before inference

The user requested a direct comparison without OpenCode. Plan v1.1 fixes one Q6 request at exactly 47,000 synthetic input tokens, context 65,536 and at most 32 output tokens. The script uses the model template and native completion endpoint, with a 30-minute deadline and no retry. This is a size comparison, not a replay of the private original request.

Dell validation: both Python files compile; the original HTTP fake suite and 12 new diagnostic scenarios pass. These cover exact input construction, large request bounds, changed model/context/identity, missing content, wrong counts, cached input, truncation, authentication, answer-match reporting and no retries. Parent source review passes.

The actual hosted app binary was run only as its normal headless supervisor, using isolated private config/state, a task-only key, existing Q6 weights and existing llama.cpp b11118. Production memory admission remained active. No GUI was launched or installed.

| Measurement | Result |
|---|---:|
| Physical Mac memory | 64 GiB |
| Available before startup | 13.36 GiB |
| Guard-required model/context budget | 26.7 GiB |
| Guard-available budget after reserves | 9.4 GiB |
| Model loads | 0 |
| Generation requests | 0 |
| Observed stages | tunnel → gpu → failed |
| Supervisor exit | 0 |
| Stop errors | 0 |
| Task processes remaining | 0 |
| Task state file removed | yes |
| Available after cleanup | 13.47 GiB |

The memory guard blocked startup correctly on the real Mac. This does **not** verify local generation, throughput or peak memory. The model-versus-OpenCode comparison remains pending, so the conditional OpenCode investigation has not resumed. At this snapshot, approximately 17.3 GiB more available memory is needed for the same configuration. Personal settings, installed app, Homebrew CLI and weights are unchanged.
