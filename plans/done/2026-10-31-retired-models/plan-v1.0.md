# Plan for removal of retired models v1.0
**Date:** 2026-10-31
**Status:** done — merged, published and verified
**Spec:** ./spec.md
**Goal:** Remove retired models and local inference. Keep supported cloud use.
**Architecture:** Keep one catalog entry. Migrate previously saved model values at the config boundary. Remove the local provider and complete-image loader. Keep shared helpers for cloud state and verified chunk downloads.
**Tech Stack:** Rust, Svelte, Python, Docker.

> For agent workers: use superpowers:subagent-driven-development to do one task at a time. Use checkbox syntax to monitor progress.

## File map
Change: `crates/lobo-proto/catalog.json`, `crates/lobo-proto/src/config.rs`, `crates/lobo-proto/src/catalog.rs`, `crates/lobo-proto/src/tests.rs`.
Change: `crates/lobo-core/src/config/mod.rs`, `crates/lobo-core/src/config/readiness.rs`, `crates/lobo-core/src/config/tests.rs`, `crates/lobo-core/src/control/wiring.rs`, `crates/lobo-core/src/connection.rs`, `crates/lobo-core/src/provider/mod.rs`, `crates/lobo-core/src/lib.rs`, `crates/lobo-core/src/bin/testchild.rs`.
Change: `crates/lobo-cli/src/app.rs`, `crates/lobo-cli/src/cli.rs`, `crates/lobo-cli/src/help.rs`, `crates/lobo-cli/src/cmd/mod.rs`, `crates/lobo-cli/src/cmd/up.rs`, `crates/lobo-cli/src/wizard/flow.rs`, `crates/lobo-cli/src/fakes.rs`.
Change: `app/src-tauri/src/backend.rs`, `app/src-tauri/src/main.rs`, `app/src-tauri/src/controller/tests.rs`, `app/src-tauri/src/opencode/tests.rs`.
Change: `crates/lobo-agent/src/config.rs`, `crates/lobo-agent/src/main.rs`, `crates/lobo-agent/src/lib.rs`, `crates/lobo-agent/src/pod.rs`, `docker/pod/Dockerfile`, `docker/pod/start.sh`.
Remove: `crates/lobo-core/src/local/`. Keep shared helpers moved to cloud-neutral modules. Remove: `crates/lobo-cli/src/cmd/local.rs`, `crates/lobo-cli/src/cmd/models.rs`, `crates/lobo-agent/src/image_model.rs`, `docker/pod/build_model.py`.
Change tests and generated types that refer to these interfaces. Keep explicit test cases for migration and rejection.
Change: `README.md`, `CHANGELOG.md`, `AGENTS.md`. Make: `results.md` in this directory.
Out of scope: historical Go code, personal configuration, installed binaries, model files, existing public releases, and PR #4.

## Self-test sanity check
- [x] Make sure that `master` is clean at `eb88ecd`. Make `cleanup/retired-models`.
- [x] Reuse the baseline from the immediately preceding release. Successful tests: 548 workspace tests, 21 UI tests, 99 app tests, and six native setup cases.
- [x] Keep Rust commands on Feesh or Air. Do not use GitHub Actions.

## Task 1: Remove retired catalog values and migrate saved configs
Files: catalog, protocol defaults/tests, core config files and fixtures that depend on them.
- [x] Before you remove entries, add assertions for one catalog ID and migration of saved `q6`/`q8` values.
- [x] Make `DEFAULT_MODEL` and `DEFAULT_CLOUD_MODEL` agree on `qwen38-q6`.
- [x] Keep explicit invalid selections invalid. Keep config data for unknown keys when you save.
- [x] Do `cargo test --locked -p lobo-proto -p lobo-core` on Feesh. Make sure that all tests are successful.
- [x] After Task 2 removes dependent local paths, examine the changes. Then commit.

## Task 2: Remove local inference entrypoints and code
Files: core code for local/control/connection/provider, CLI commands/help/wizard/fakes/tests, app backend/main/tests.
- [x] Remove the local provider, hidden supervisor command, local model commands, and `--q6` flag.
- [x] Change `Wiring::new` to accept the executable path directly. Keep the behavior of helpers for cloud connections.
- [x] Move each necessary helper for state/process into a cloud-neutral module with the same safety checks.
- [x] Keep previous local ownership records readable. Do not start them again. Do not remove them.
- [x] Replace positive fixtures for retired models with `qwen38-q6`. Keep negative test cases and migration test cases.
- [x] Do `cargo test --locked --workspace` on Feesh. Make sure that all tests are successful.

## Task 3: Remove model packages for complete images
Files: agent config/main/pod/lib/tests, Dockerfile/start script, obsolete tools for model packages and verification.
- [x] Remove `LOBO_IMAGE_MODEL`, `check-image`, the shard loader, and the full Docker target.
- [x] Keep the `thin` target usable. Make it the default build result.
- [x] Keep catalog hashes, CDN retry, verified fallback to Hugging Face, and `q8_0` cache settings unchanged.
- [x] Change the remaining agent tests and references to obsolete active tools.
- [x] Do `cargo test --locked -p lobo-agent` on Feesh. Make sure that all tests are successful.
- [x] Assemble the image on Feesh. Do offline smoke checks for runtime/agent/SSH.

## Task 4: Examine, verify, and supply
Files: README, changelog, project memory, results record, fixture files that the changes affect.
- [x] Describe one cloud model. Remove obsolete active instructions.
- [x] Do frontend tests/lint on Feesh. Do app tests/native setup checks on Air.
- [x] Do one last external code review of `eb88ecd..HEAD`. Correct verified defects.
- [x] Merge the branch with successful tests directly into `master`. Push without GitHub Actions.
- [x] Publish changed app/image artifacts through Air and Feesh as specified by the current fullsend instruction.
- [x] Verify public artifacts and source identity. Keep the installed app and optional CLI.
- [x] Record test counts and untested live inference. Set statuses to done. Move this directory to `plans/done/`.

## Interface consistency
`qwen38-q6` is the only active model. `Wiring::new(PathBuf, PathBuf)` makes only cloud providers.
Saved retired values migrate at config load. Explicit new selections of retired models do not pass validation.
Shared ownership and SSH helpers keep their persisted formats and identity checks.
