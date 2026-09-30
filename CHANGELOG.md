# Changelog

## Unreleased

- Refresh native UI assets and Rust branch instructions. Release remains on hold for manual testing.
- Fit the TUI status values and quit hint into 80×20 and 80×24 terminals. Show a resize hint in smaller terminals. Six layout tests and five fixture-only terminal checks pass.
- Accept the initial streaming chat chunk with null content. Reject malformed numeric content. Focused regression checks passed before local backend testing was stopped.
- Restrict the Mac native smoke target to UI setup. Further backend and lifecycle tests must run on Dell or authorized cloud hosts.

- Fix native app startup by keeping tray changes on the main thread. Add standard macOS title bars, rounded panel corners and Settings tabs without scrolling.
- Size windows for their actual webview content, including the measured macOS title-bar inset. Long config paths wrap without forcing horizontal overflow.
- Enable native dragging from the panel and Settings headers. Native movement verification remains in progress.
- Native E2E passes setup/save, start/stop, cancellation and failure/retry with a real supervisor and isolated fake runtime. All 37 app Rust tests and 8 UI tests pass; 20 panel renders and 3 Settings tabs have no clipped controls. Live inference, reopen/resume and release acceptance remain pending.

- Rust P5 app candidate: shared-core backend, owned Start/Stop/Quit, tray, native windows, local supervisor, Svelte panel and settings. The app does not launch or bundle the CLI.
- App checks pass: 36 Rust tests, 8 UI tests, lint, generated UI fixtures and production frontend build. Failed Quit can retry cleanup without disabling polling.
- CI now builds and verifies the macOS app bundle with frozen frontend dependencies. Native app E2E, final render acceptance and clean-runner app CI remain pending; Swift source is retained meanwhile.

- Rust P5 shared app APIs: config readiness, settings validation, explicit startup overrides and free-space checks for unsaved folders. The desktop app is still in progress.
- Shared protocol/core checks pass: 255 tests, one network test ignored; clippy passes. Native app E2E remains pending.

- Rust CLI packaging now builds macOS and static Linux archives for Intel and ARM. Release CI pins Rust, Zig, cargo-zigbuild and GoReleaser. Snapshot builds do not need tap credentials.
- All four archives, a clean snapshot and temporary Homebrew installation/test pass. App E2E remains pending. All 51 CLI tests pass, including completion output and candidate publication without changing `latest.json`.
- Rust and pod-image CI passed at fcb8aff, including the HTTP pool and process-fixture fixes.

- Serialize local process fixtures so a concurrent fork cannot temporarily inherit another test’s port probes. The earlier port-range change alone did not prevent Mac CI collisions. All 13 process tests pass ten consecutive local runs.

- Fix a cross-runtime HTTP connection pool that could stall or fail downloads. Repeated model requests keep their own client for connection reuse.
- Regression test reproduced the old hang with two runtimes. All 399 workspace tests and clippy pass after the fix; one network test remains ignored. Candidate CI remains pending.

- Rust CLI adds exact log output, streamed chat/tool checks and agent release publishing. Hidden `release --no-promote` preserves the shared latest manifest.
- Local HTTP tests cover authentication, model selection, cancellation, tool failures and UTF-8 output. Release command construction and metadata tests pass; live publishing, packaging and app E2E remain pending.

- Rust P4 control commands: start, status and stop call the shared core. Inline dashboards preserve Go output; boot timings go to stderr and the local timing log.
- Ctrl-C during startup and broken JSON output wait for owned cleanup. Cleanup failures stay errors. Stop keeps issued deletes awaited after Ctrl-C.
- Focused checks: eight Go dashboard goldens, output replay, delayed-create cancellation, failed deletion and broken-output cleanup pass. Logs, live API checks, release packaging and app implementation remain pending.

- Rust CI repair: normalize version-dependent Go JSON decoder messages in CLI fixtures. Local process tests use unique ports outside the usual outgoing-connection range.
- Focused checks: all 53 CLI replay cases and repeated local process tests pass. Ubuntu and Apple Silicon CI passed at 8cf8515.

- Rust P4 partial CLI: config commands and terminal wizard, help, API-key generation, model listing and the shared local supervisor entry. The Go CLI remains the installed default.
- Checks: 27 Rust test functions pass, covering 53 Go CLI replay cases, help metadata, key rotation and local flags. Isolated terminal Save/Escape smoke tests pass. Live control commands, dashboards, release packaging and candidate CI remain pending.

- Rust P3 start control: owned worker completion, cancellation during cloud/local startup, persistent recovery of uncertain creates, shared boot progress and bad-host replacement.
- Workspace tests pass, including 24 start-loop tests, 13 real local-process tests and the public API contract test. These cover a 121-second create, full/closed event queues, failed cleanup and worker panic. The Go interop checks were not enabled in this run; one network test was ignored. Ubuntu, Apple Silicon and pod-image CI passed at 2884464. Explicit Go interop and Go regression checks also passed. Live inference remains pending.

- Rust P3 partial start/stop ownership: persistent pending-operation records, process locks, conservative reconciliation and verified cleanup. Start integration remains pending.
- Tests cover record reopen, lock cancellation, corrupt files, ambiguous listings and cleanup that preserves pre-existing instances. No live provider request was made.

- Rust P3 shared control: agent HTTP client, status/stop/target operations, launch defaults and config-to-provider wiring for CLI and app.
- Seventeen focused control tests pass. Stop continues across provider errors; local endpoints use running state. The owned start operation remains unfinished.

- Fix Rust CI checkout: track the local process test helper under `src/bin`; ignore only the root build-output directory.

- Rust P3 local provider: checks disk/ports, starts detached CLI or app supervisors, and waits for owned process groups during stop.
- Eleven real-process tests pass. They cover stuck supervisors, surviving children, changed identities, startup failure and cancellation before state publication. Local model inference remains pending.

- Rust P3 Mac runner and supervisor: shared agent lifecycle, loopback API, config forwarding, scoped logs and cancellation cleanup. CLI/app entry points remain pending.
- Focused checks cover process exit, child environment filtering, API responses, occupied ports and duplicate supervisors. The actual pinned runtime archive passed download/hash/extraction validation; full local inference remains pending.

- Rust P3 model/runtime cache: compatible verification markers, model listing and pinned llama.cpp download/extraction with cancellation cleanup.
- Runtime checks: 29 focused local tests and clippy pass. Extraction rejects unsafe paths and symlinks. Pinned archive network verification passed in the following batch. Full local inference remains pending.

- Rust P3 local foundation: Apple Silicon memory checks, atomic state claims, Go-compatible process locks and supervisor identity checks.
- Local checks: 14 focused tests and 3 Go/Rust interop tests pass. Local runtime and supervisor execution are still unfinished.

- Rust P3 release/checks batch: build and scan agent zips, sign R2 requests, publish isolated candidates, validate chat/tool calls and generate API keys/OpenCode config.
- P3 checks: 105 core tests pass. Candidate publishing and failed uploads leave latest unchanged in HTTP tests; live R2 validation remains pending.

- Rust P3 providers: Go-compatible pod bootstrap, RunPod tier selection and Vast offer recovery. Cancellation prevents later creates and preserves in-flight results for cleanup.
- P3 provider checks: 80 core tests pass, including real Bash failure cleanup and cancelled Vast reconciliation. Final GPU E2E remains pending.

- Rust P3 foundation: shared file-only config loading, masked display, atomic config writes, errors and clock.
- P3 checks: 36 core tests pass; 77 Go fixture files reproduce exactly. Providers and lifecycle remain unfinished.

- Rust rewrite P2 candidate: add the pod agent, HTTP/SSH downloads, watchdog, metrics, API and provider self-deletion.
- P2 checks: 136 workspace tests and Rust CI pass. Static Linux agent: 8.02 MB. Image CI passed. Final live E2E remains pending.

- Rust rewrite P1: add the workspace, shared protocol types, Go compatibility fixtures and generated TypeScript types.
- P1 validation: 35 protocol tests pass. The Rust CLI and app remain pending.
- Show the macOS panel on launch and reopen. Add a DMG release asset and local Mac memory in status.
- Validation: Go build, 361 Go tests, Swift tests and macOS bundle/render build pass. Finder reopen was not exercised in this pass.
- Planning: correct Rust rewrite cancellation ownership, app workspace builds and release candidate isolation.
- Planning: preserve CLI argument behavior and record full-auto delivery authorization. Runtime implementation and validation remain pending.
