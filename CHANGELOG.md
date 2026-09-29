# Changelog

## Unreleased

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
