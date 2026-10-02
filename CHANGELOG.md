# Changelog

## App 0.2.1 — 2026-10-02

- Make the Mac app cloud-only. Remove local Start, memory/model controls, local settings, local IPC and the local supervisor entry point.
- Preserve existing CLI local settings, model files and runtime state. Old Local preferences open cloud setup.
- Default new cloud selections to Q6. Keep GPU metrics, private SSH and cloud OpenCode configuration.
- Add an independent app-only DMG release with source provenance and checksums. The optional CLI release is unchanged.
- Validation: Rust/core/CLI and app tests pass, including 19 UI tests and native fresh/legacy setup checks. Production app settings fit without scrolling. Published DMG checksums, source receipt, signature, standalone contents and anonymous downloads pass.
- Limits: ad-hoc signature; no notarization. Drag position, global Command-Tab, live OpenCode and broader loading telemetry remain unverified.

## Unreleased

Historical development entries below retain their original validation limits. App 0.2.1 supersedes the app release hold and removes app-local features. CLI release and Q8 publication remain held.

- Clarify the Rust source quick start, Q6-only public cloud image, separate app/CLI installation and CLI bulk-stop behavior. App/CLI releases remain held; loading-memory progression and remaining native checks are not complete.
- Pass the actual hosted Mac app `b5617ae` cloud Start → Ready → direct reply → Stop → Off flow. The request used 25 uncached input tokens and returned the expected `4` in two output tokens. First content took 1.107 seconds; two tokens do not establish sustained output speed. Controls fit without scrolling, and rounded corners were visible. Exact pod and local tunnel cleanup, native Quit and guarded preference restoration passed.
- Record startup variation: TUI Ready took 230.178 seconds; Mac Ready fell between about 21m11s and 22m30s after the Start capture. The provider still reported downloading at 15 minutes. Exact Mac boot duration, download-byte progress and cache reuse were not measured.
- Observe 22.8 / 31.8 GB GPU memory in native Ready. Post-request idle gauges showed `0 / 0`; live rate updates remain unverified. Drag, Dock, Command-Tab, local Metal, OpenCode and broader telemetry checks remain pending. App/CLI releases and Q8 remain held.
- Pass a controlled RunPod Q6 TUI check: 47,000 uncached input tokens, two output tokens and the expected answer `4`. First content took 24.040 seconds; prompt processing reached 2,186.677 tok/s. Reported output speed was 45.271 tok/s over only two tokens, not a sustained benchmark. Exact pod/tunnel cleanup passed. Local Metal, OpenCode and broader native acceptance remain pending; app/CLI releases stay held.
- Correct the diagnostic for Python 3.10 nanosecond expiry timestamps and llama.cpp b11118's flat `generation_settings/n_predict` key. Do not require the unsupported `n_cmpl` response field. Full fake checks passed on Feesh in 1.968 seconds. Preserve the first live validator rejection as failed acceptance; the controlled repeat passed.
- Verify that real TUI Ready and status views fit 80 columns. Some startup detail lines still clip at that width.
- Publish the complete Q6 GPU image from the feesh build through CI. Independent layer, model and runtime verification passed. Anonymous versioned-tag and `latest-q6` checks return the verified digest. Direct RunPod TUI inference and the bounded native Mac cloud lifecycle passed. App/CLI releases and Q8 remain on hold.
- Add independent 512 MiB CI transfer parts with completed-part reuse, isolated upload logging and exact image reconstruction before hosted publication. All 47 fresh artifacts and token checks passed. Feesh uploaded 24.62 GB in 11m03s at 37.1 MB/s; exact temporary artifact and runner cleanup passed. Earlier Dell terminal logs also prove completed-part reuse.
- Correct transfer packing for BuildKit exports containing an empty `ingest/` directory. The first live packing attempt rejected this valid export detail before upload; the image passed verification and task cleanup passed.
- Add a direct cloud diagnostic with RunPod instance/boot checks, private saved-connection validation and a fixed 47,000-input/32-output limit. Initial Dell fake checks passed; the corrected controlled RunPod repeat now passes.
- Before image publication, actual Mac app and 80×24 TUI starts rejected the missing Q6 image (HTTP 404) before rental. Preserve this failure-path result; Q6 publication and the later TUI inference check now pass.
- Prepare an optional CI path to verify and publish a complete Q6 image built elsewhere. Verification is the default; publication and latest-tag promotion require explicit inputs. Dell fixture checks and workflow lint passed. Full Feesh-image verification and publication now pass.
- Reject prepared images whose advertised CPU platform differs from their Linux AMD64 image config. All 23 verifier fixtures pass on Dell.
- Build and independently verify a complete 24.69 GB Q6 GPU image with bundled runtime, agent, SSH and model weights. Verification-only CI passed at `505c255`. That Dell image remains unpublished. The rebuilt Feesh image is public and passed the controlled TUI and bounded native Mac cloud checks; broader acceptance remains pending.

- Prepare a separate one-request Q6 diagnostic with exactly 47,000 synthetic input tokens and at most 32 output tokens. Dell HTTP fake checks pass. Actual Mac startup was blocked by normal memory admission: 26.7 GiB required, 9.4 GiB available after reserves. No model loaded or generation ran; speed and model acceptance remain unverified.
- Hosted app checks and bundle verification pass at `b5617ae`. Its bounded native cloud lifecycle now passes. Drag, Dock, Command-Tab and broader app-flow acceptance remain pending; the installed app is unchanged.

- Add a compact OpenCode Clients tab and Ready shortcut. Show the selected file and running model before repair; preserve default choice, prevent duplicate writes and retain safe setup errors. Dell: 19 UI tests, Svelte checks, build and formatting pass. Native visual acceptance is deferred.
- Use regular macOS activation and remove the agent-app bundle setting. Preserve tray, background launch and hide/reopen behavior. Hosted native build passes; Dock/Cmd+Tab and actual window acceptance remain pending.

- Prepare an explicit bounded runtime-check script with two fixed short requests and private numeric evidence. It never starts a runtime or downloads models. Dell fake-only execution passes. Real generation, speed measurements and E2E acceptance remain pending.

- Add app-owned OpenCode setup commands. Authenticate the exact Ready runtime with bounded model discovery; reject stale ownership, settings and key revisions before repair. Preserve provider restrictions and keep keys out of app metadata. Hosted app validation passes; native E2E remains pending.

- Add the core OpenCode repair writer. Preserve JSONC comments and unrelated settings, use private key files and backups, reject concurrent changes, and avoid file churn on unchanged repairs. App controls are in progress. Dell: 15 repair fixtures, six unchanged CLI fixtures and Clippy pass.
- Wire the app to its recorded runtime for Start, status and Stop. Capture Start before queuing memory admission; reject changed settings after the check. Guard discovered ownership against stale replies and clear old runtime status after Stop. Hosted app lint, units and generated-file checks pass; E2E is deferred.
- Update the denied-Start regression fixture for the asynchronous worker refresh. Verify that changed memory appears as insufficient and that neither Start nor Retry launches a runtime. Hosted validation passes.

- Add app-scoped runtime operations in the shared core. Match provider, instance and boot identity before cleanup; preserve unrelated pending operations and connections. App integration is in progress.
- Core checks pass on Dell: 74 control tests, 19 local-provider tests and the scoped connection fixture. Final discovery fixes pass 24 scoped tests. Native E2E remains deferred; no inference or provider requests ran.
- Correct the native memory probe test for macOS runners without Metal. Require the exact unavailable error on those runners; keep startup admission unchanged. Hosted macOS checks pass.
- Write executable core-test fixtures in a separate process to avoid inherited writable handles. Dell core library checks pass: 250 tests, one opt-in test ignored. Hosted Linux checks pass after the `Text file busy` failure.

- Fix app fixtures rejected by macOS CI: remove a redundant default initializer and give the ready fake GPU its agent/API addresses.

- Remove package-generated SSH host keys from the GPU image during the install layer. Each cloud instance keeps its own pinned key. Final image verification is still running on Dell.
- Bundle the model license in the Docker build context. A license download timeout previously discarded completed model packaging; rebuilding now reuses the verified source cache.

- Check current Mac memory before local Start. Show required memory and the usable budget; block Start and Retry while checking, when insufficient, or when measurement fails. Keep model and Cloud choices available.
- Include exact model weights, context cache, a runtime reserve and a separate system reserve. Repeat admission before runtime preparation and model load; stale displayed results cannot authorize startup.
- Memory validation in progress: 449 workspace tests and Clippy pass on Dell; 11 frontend tests and native UI fixture compilation pass. Native E2E passed five cases, then disconnected during denied Start; the final case was blocked. Cleanup passed. E2E is now deferred at the user's request; hosted app checks remain pending. No inference or peak-memory calibration ran on this Mac.

- Resolve the current public Q6/Q8 image digest on every new cloud start. Ignore old bucket/image settings and fail before rental if the registry cannot resolve the image.
- Package the agent, CUDA runtime, SSH server and selected GGUF shards together. Verify model hashes offline; remove agent/model downloads from image startup.
- Remove bucket and image fields from setup. Show SHA-256 verification in the app and TUI. Update help, README, cloud docs and screenshots.
- Extend startup limits to 40 minutes, with a 30-minute image deadline and maximum-lifetime cleanup during startup. Require both complete image jobs before app/CLI release jobs.
- Dell workspace: 434 tests pass; clippy passes. Both opt-in checks (SSH integration and runtime archive) and all three explicit Go interoperability checks also pass. Nine frontend tests, 23 browser layouts and native fresh-config setup pass. Full image builds are still running. Native drag automation failed to observe movement; manual drag acceptance remains open.
- Install local app build `2bd9538` and verify the matching standalone DMG. Preserve CLI, config and model files. No public release, image publication or GPU rental; live provider acceptance remains pending.

- Add automatic private SSH connections for cloud instances. New setups need no domain, Cloudflare account or tunnel subscription. OpenCode uses `http://127.0.0.1:8933/v1`; GPU and provider charges still apply.
- Pin a fresh server key for each instance, restrict forwarding to the inference/control APIs, and reconnect through a detached helper. Stop closes the owned connection and removes its keys. Preserve existing complete Cloudflare configurations.
- Update the CLI wizard, native Cloud Settings, OpenCode configuration, README and screenshots for domain-free setup.
- Dell checks pass for provider requests, authentication, progressive streaming, tool calls, a 105-second first response, reconnect, helper crash recovery, rejected keys, occupied ports and cleanup. Native UI-only setup, 9 frontend tests and all 23 browser layouts pass without scrolling.
- Release remains on hold. No GPU was rented, and no new agent or image was published. Live RunPod/Vast provisioning, inference and sleep/wake acceptance remain pending.

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
