# Cloud-only app implementation plan v1.0
**Date:** 2026-10-02
**Status:** in-progress
**Spec:** ./spec.md (approved under full auto)
**Goal:** Remove every local-run entry point from the Mac app and publish its standalone DMG.
**Architecture:** Cloud-only app adapters over unchanged shared core/CLI. Isolate app release from CLI/images.
**Tech Stack:** Rust/Tauri, Svelte, Vitest, Python/WebdriverIO, GitHub Actions.

> For agentic workers: use superpowers:subagent-driven-development to implement task-by-task. Checkbox syntax for tracking.

## File map
Exact allowed files and directories are in spec.md. Backend implementer owns app/src-tauri/src and Rust tests/fixture generator. UI implementer owns app/ui/src. Release implementer owns app-release.yml and app version metadata. Parent owns native harness/CI, docs, integration and publication. Prior app-runtime worktree is out of scope.

## Baseline sanity
- [x] Clean master 4b117c3; fresh feat/cloud-only-app worktree. Prior CI passed builds, units, core/CLI and agent; native smoke failed. Fix the native test gate in Task 3.

## Tasks
- [x] 1. Rust app boundary: remove Target Local, memory/model scanning and local supervisor; migrate old preferences/owner safely; reject non-cloud providers in all mutation/sample paths. Remove models/local_memory/choose_target/local_models/free_bytes/choose_weights IPC. Cloud state keeps model/provider and cloud readiness; no target/is_local/local_memory/models fields. Keep cloud SSH helper. Update Rust fixtures and regression tests. Hosted `make app-lint app-test app-fixtures` must pass; `cargo test --locked -p lobo-core -p lobo-cli --features lobo-cli/test-fakes -j 2` stays green.
- [x] 2. UI: remove LocalStart/MemoryCheck, target picker and Local tab. Tabs are cloud/defaults/clients; default cloud. Start requires cloud_ready. Preserve provider/model/context and OpenCode controls. Remove local render fixtures and adapt tests. Feesh `pnpm -C app/ui test`, `check`, `build`, `format:check` pass. Generate types/JSON fixtures through Rust on hosted macOS; no hand-edited generated API divergence.
- [x] 3. Native harness/CI: retrieve exact failure evidence; replace obsolete memory-only scenario with cloud-only migration/setup assertions. Use isolated fixture config/state, deny real providers. Hosted native smoke must pass without Local controls or scroll. Keep local Mac inference/build/backend-suite restrictions.
- [x] 4. Independent app release: app-v0.2.1 tag and version0.2.1 (verify no existing conflict). Add workflow_dispatch accepting exact release tag; require tag source on master and successful Rust CI; anonymously verify latest-q6; build/validate standalone DMG on hosted macOS, SHA256 and release metadata. Do not dispatch legacy v* workflow, publish CLI or rebuild Q8. Parent owns final publish.
- [x] 5. Parent integration: inspect changes, regenerate fixtures remotely, focused checks and required hosted CI. Inspect actual native production app from hosted artifact with private prefs/config and no real rental. Update README/changelog/assets/memory. Run one final rival-codex review per plan skill, fix material findings and rerun affected checks.
- [ ] 6. Commit/merge/push; verify exact merged CI. Tag and dispatch app release, validate downloaded DMG/checksum/bundle, record evidence, notify. Mark done and move feature directory to plans/done only when delivered.

## Interface consistency
- [x] Rust PanelState and generated TS/JSON match cloud-only UI; Backend mocks have no local models/memory methods.
- [x] Settings tabs cloud/defaults/clients match IPC validator; no app local-supervisor handler.
- [x] Cloud provider allowlist applies before dependency work and mutation; legacy local state remains untouched.
- [ ] Published app-only tag/version/asset identity match; optional CLI and image publication unchanged.

Full-auto authorization supersedes skill handoff prompts. Parent performs task verification, with no automatic per-task model reviews. No implementation runs in primary checkout.
