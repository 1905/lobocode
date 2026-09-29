# Changelog

## Unreleased

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
