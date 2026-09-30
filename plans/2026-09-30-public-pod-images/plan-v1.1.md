# Public GPU Images Implementation Plan v1.1

**Date:** 2026-09-30
**Status:** in-progress
**Spec:** ./spec.md (approved by the user)
**Goal:** Start the selected model from the latest complete public image without private storage dependencies.
**Architecture:** The shared core resolves a model tag to its current digest before rental. The provider pulls that digest. The agent verifies and uses the model already in the image.
**Tech Stack:** Rust, reqwest, Tokio, Docker/BuildKit, GGUF, Svelte/Tauri, OpenSSH.

> For agentic workers: use superpowers:subagent-driven-development to implement task-by-task. Checkbox syntax tracks completion.

This continues the existing authorized Rust implementation on `feat/rust`.
The user approved the concrete spec after requesting implementation. Preserve
the existing partial changes; do not reset the branch or demand repeated scope
approval. No push, merge, publication or GPU rental is authorized by this plan.
The parent verifies integration and reviews each change. No automatic per-task
model-review loop. Unrelated desktop-updater work is outside this plan.

This revision records implementation details within the approved scope. The release
workflow now calls the image workflow before publishing artifacts. Build-only
source caching avoids repeated weight downloads after packaging fixes; every reuse
still checks the catalog size and hash. Native drag acceptance remains unresolved.

## File map

Create: `crates/lobo-core/src/images.rs`; `crates/lobo-agent/src/image_model.rs`;
`docker/pod/build_model.py`; `docker/pod/start.sh`.

Modify: `crates/lobo-core/src/{lib.rs,bootstrap.rs,config/mod.rs,config/readiness.rs,config/show.rs,config/tests.rs,control/mod.rs,control/up.rs,control/wiring.rs,control/testkit.rs,control/up_tests.rs,provider/mod.rs,provider/runpod.rs,provider/vast.rs,bootstrap/tests.rs}`;
`crates/lobo-core/tests/local_provider.rs`;
`crates/lobo-agent/src/{lib.rs,config.rs,pod.rs,main.rs}`;
`crates/lobo-agent/tests/bin_tests.rs`;
`crates/lobo-cli/src/{cli.rs,wizard/flow.rs,wizard/state.rs}`;
affected `crates/lobo-cli/tests` snapshots/replay expectations;
`docker/pod/Dockerfile`; `.github/workflows/{pod-image,release}.yml`; `.dockerignore`;
`crates/lobo-cli/src/{cmd/up,fakes,tui/up,tui/status}.rs`;
`app/src-tauri/src/backend.rs`;
`app/ui/src/{lib/settings.ts,panel/SetupCard.svelte,settings/Settings.svelte}`;
`app/e2e/app.spec.js`; app fixtures/assets; `README.md`; `CHANGELOG.md`;
`AGENTS.md`; `docs/cloud-without-domain.md`; `docs/implementation-mistakes.md`.

Out of scope: desktop updater source, existing Go fixtures, installed CLI,
provider accounts and public release state.

## Self-test sanity check

- [x] Earlier source snapshot compiled on Dell with `cargo check --locked --workspace --all-targets --all-features`.
- [x] Sync the latest source to `/storage/lobocode-public-images-check`; run focused tests before changing their old assertions. Record failures.

All backend commands below run inside the task-owned Dell container
`lobo-public-image-check`, working directory `/work`, with `CARGO_BUILD_JOBS=3`.
Use `rtk proxy ssh dell` from the Mac. No backend suite or model inference on the Mac.

## Task 1: Resolve the current public image

Files: core `images.rs`, `control/{mod,up,wiring,testkit,up_tests}.rs`, `lib.rs`.

- [x] Keep `ImageResolver::latest(&self, model: &str) -> Result<String>` asynchronous and injectable.
- [x] Test changing Q8/Q6 manifests, invalid digest and registry failure after a previous success.
- [x] Test two new starts use two resolved digests; registry failure makes zero creates.
- [x] Ignore `LOBO_POD_IMAGE` in normal starts. Preserve only explicit CLI `--image` override.
- [x] Reject cloud `--release` and `--source` before renting. Keep local mode independent of the registry.
- [x] Set default startup timeout to 40 minutes and pre-agent image deadline to 30 minutes. Preserve explicit overrides and owned cleanup.
- [x] Run `cargo test --locked -p lobo-core images::` and `cargo test --locked -p lobo-core control::up_tests::`. Expected: all pass.

## Task 2: Remove private-source startup requirements

Files: core config, wiring, bootstrap, providers and their tests; CLI help/wizard/replays.

- [x] Normal readiness requires provider credentials and app API authentication, without bucket or model-source validation.
- [x] Do not read, initialize or forward private download credentials during normal cloud startup.
- [x] Keep explicit legacy maintainer release commands responsible for their own storage validation.
- [x] Verify both providers invoke `/lobo/start` and send the selected digest with `image_model: true`.
- [x] Update obsolete help and wizard assertions. Keep frozen Go fixtures untouched; document intentional Rust behavior differences.
- [x] Run `cargo test --locked -p lobo-core config::`, `cargo test --locked -p lobo-core bootstrap::`, `cargo test --locked -p lobo-core provider::`, and `cargo test --locked -p lobo-cli`. Expected: all affected checks pass.

## Task 3: Validate bundled models and boot

Files: agent source, agent binary tests, Docker build/start files.

- [x] Keep manifest fields `model`, `source_sha256`, and `shards[{file,size,sha256}]`.
- [x] Reject invalid identity, names, sizes, symlinks, missing/corrupt shards; honor cancellation.
- [x] Verify image mode requires no model URL and never downloads a fallback.
- [x] Load the first native GGUF shard. Retain authentication, watchdog, SSH and provider deletion.
- [x] Validate `/lobo/lobo-agent check-image --model q6|q8` without GPU or network.
- [x] Exercise image entrypoint failure and provider request quoting with fixtures on Dell.
- [x] Run `cargo test --locked -p lobo-agent`. Expected: agent and binary checks pass.

## Task 4: Build complete model images

Files: Docker files, `.dockerignore`, `.github/workflows/pod-image.yml`.

- [x] Verify upstream size/hash before GGUF split. Put each shard below 5 GB into its own layer.
- [x] Include runtime, agent, SSH, legacy tunnel binary, weights, manifest, license and attribution.
- [ ] Build Q6 and Q8 using task-owned BuildKit storage under Dell `/storage`.
- [ ] Do not publish. Record exact image/build verification and disk requirements.
- [x] Make CI build both variants and promote `latest-q6`/`latest-q8` only after both builds succeed. Use immutable model tags for build outputs.
- [x] Ensure CI has adequate disk or an explicit capacity prerequisite; do not claim an untested runner succeeds.
- [ ] Verify both final-stage network-free model checks and each layer size.

## Task 5: Verify integrated backend behavior

Files: affected tests and build evidence only, with targeted fixes where failures expose defects.

- [ ] Run `cargo test --locked --workspace --all-features` on Dell; configure required fixture tools and child-process reaping.
- [x] Run `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` on Dell.
- [x] Keep conditional/live/network acceptance limits explicit. No paid instance creation.
- [ ] Review final code against the approved spec; commit the verified backend change locally.

## Task 6: Finish native setup and local delivery

Files: app settings/setup, UI tests/fixtures/assets, README, cloud docs, mistakes, changelog and memory.

- [x] Remove bucket/image/source controls from normal Settings and setup copy.
- [ ] Run frontend tests and production build. Verify actual native controls, dragging, rounded corners and no scrolling.
- [x] Check TUI layout changes; all non-UI CLI suites remain on Dell.
- [x] Update documentation and assets to reflect verified behavior, including the unpublished-image limitation.
- [ ] Build/install the standalone app and verify local DMG contents. Preserve CLI/config/model files.
- [ ] Commit UI/docs delivery locally after focused checks. Keep release hold active.

## Validation record — 2026-09-30

- Dell focused agent: 109 library and 4 binary tests passed.
- Dell core: 212 tests passed; one network test ignored.
- Dell CLI: 53 tests passed, including replay and seven TUI checks.
- Dell clippy passed for all workspace targets/features. Final full workspace rerun is in progress.
- Both changed workflows pass actionlint 1.7.7. No workflow was dispatched.
- Frontend: 9 tests, type checks and formatting passed. All 20 panels and 3 Settings tabs fit without scrolling.
- Native fresh-config setup/save passed. Rounded corners were visually inspected. Drag automation did not report movement, so movement acceptance remains open.
- Complete Q8/Q6 image builds are running on Dell. No publication or GPU rental.

## Held release acceptance

- [ ] Anonymous pulls, live Q6/Q8 RunPod/Vast checks and published-tag rotation.
- [ ] Public images, DMG and Homebrew release in that order after authorization.

These items remain pending while the release hold applies. They are not replaced
by fixtures or a successful local build. Do not label the whole plan done until
required acceptance is complete or the user explicitly narrows its scope.
