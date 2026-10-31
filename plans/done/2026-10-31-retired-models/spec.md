# Retired model cleanup
**Date:** 2026-10-31
**Scope:** /Users/kass/dev/lobocode
**Status:** done — approved cleanup delivered under the existing fullsend instruction

## TL;DR
Remove retired `q6`/`q8` models, local inference code, and the old complete-image build.
Keep `qwen38-q6` on RunPod and Vast, including verified chunk downloads and Hugging Face fallback.
Old config files must still load. Existing user data and installed software stay untouched.
User action: nothing. This does not rent GPUs, run inference, or publish the optional CLI.

## Problem(s)
1. `crates/lobo-proto/catalog.json:1` still contains retired models. Removing them without migration breaks config defaults in `crates/lobo-core/src/config/mod.rs:362`.
2. `crates/lobo-core/src/control/wiring.rs:22` and `crates/lobo-cli/src/cli.rs:25` retain local providers and commands. These paths contradict the cloud-only supported scope.
3. `docker/pod/Dockerfile:5` retains a complete-image build for retired models. `crates/lobo-agent/src/pod.rs:417` retains its separate loader.

## Goals
1. Expose and run only `qwen38-q6` on RunPod or Vast.
2. Remove unused local inference and retired model image implementations.
3. Preserve config migration, cloud ownership checks, authenticated connections, and verified downloads.

## Non-goals
- Delete user files, cloud resources, existing public images, releases, or the installed app.
- Change the supported model, chunk hashes, CDN, fallback, or cache quantization.
- Release the optional CLI or replace historical Go parity reference code.

## Config and runtime compatibility
Saved `q6` or `q8` model values become the supported default when loaded.
New explicit model selections must accept only `qwen38-q6`.
Saved local provider values must not start a local runtime.
Old local ownership records remain safely readable and cannot authorize cloud deletion.
Shared process and state helpers move out of the removed local inference module when still required.
The GPU image contains the runtime, agent, and SSH server. The agent downloads and verifies model chunks at startup.

## File-level changes
| Files | Change |
|---|---|
| `crates/lobo-proto/catalog.json`, `crates/lobo-proto/src/` | Remove retired catalog entries and active defaults; retain required compatibility types. |
| `crates/lobo-core/src/local/`, `crates/lobo-core/src/control/`, `crates/lobo-core/src/config/`, `crates/lobo-core/src/connection.rs`, `crates/lobo-core/src/provider/mod.rs`, `crates/lobo-core/src/bin/testchild.rs` | Remove local provider/runtime wiring and move shared helpers; migrate saved config values. |
| `crates/lobo-cli/src/`, `crates/lobo-cli/tests/` | Remove local commands and retired model flags; adapt meaningful regression fixtures. |
| `app/src-tauri/src/`, `app/src/lib/`, `app/src/Render.svelte` | Remove local supervisor entrypoint and retired model assumptions; preserve migration and ownership tests. |
| `crates/lobo-agent/src/`, `docker/pod/`, `tools/verify_cloud_candidate.py`, `tools/test_cloud_candidate_transfer.py` | Remove the complete-image path and its obsolete packaging/validation code. Keep thin-image and chunk validation. |
| `tools/bounded_runtime_e2e.py`, `tools/same_prompt_runtime_e2e.py`, `tools/cloud_runtime_e2e.py`, `Makefile`, `scripts/` | Remove local-only tooling and adjust active build/test references if they depend on removed code. |
| `README.md`, `CHANGELOG.md`, `AGENTS.md`, this plan directory | Describe the actual remaining support and record validation limits. |

## Tests
- Catalog: one supported model; retired IDs fail lookup.
- Config: saved old model values migrate; explicit retired values fail; local cannot start.
- Ownership: legacy records remain safe; cloud Start/status/Stop retain exact identity checks.
- Agent: chunk checksum, retry, CDN failure, and Hugging Face fallback tests pass.
- Feesh: Rust workspace and frontend checks; offline image smoke when image code changes.
- Air: app library/binary tests, native setup/config migration checks, and app build.

## Failure modes & decisions
| Failure | Behavior |
|---|---|
| Saved retired model | Read as `qwen38-q6`; persist on normal save. |
| Explicit retired model or local start | Reject before allocation or process launch. |
| Old local ownership record | Preserve it; never use it to stop a cloud instance. |
| CDN unavailable | Keep verified Hugging Face byte-range fallback. |
| Build or review fails | Fix and repeat affected checks before delivery. |

## Out of scope
- Historical Go implementation and frozen historical evidence.
- GPU inference, live provider lifecycle acceptance, app installation, CLI publication, and unrelated PR #4.

## Rollout
P1: Remove retired catalog and local paths; commit after focused checks.
P2: Remove complete-image code and update documentation; commit after focused checks.
P3: Review the whole branch, run required checks, merge and push directly; publish changed app/image through authorized hosts.

## As-built notes
- Remove the unused local protocol types and bindings after confirming no runtime consumers remain.
- Move only the shared operation-directory helper into `crates/lobo-core/src/state.rs`.
- Remove the retired GitHub Actions upload action and local diagnostic entrypoints. Cloud probe helpers retain their fake coverage.
- Final review found a stale local CLI intent could block cloud operations. Clear only that retired intent; preserve local ownership files and processes.
- Unknown operation providers and app-scoped local intents remain blocked. `.env.example` now documents the supported cloud setup.
