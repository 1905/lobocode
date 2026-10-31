# Final code review
**Date:** 2026-10-31
**Status:** done — both findings confirmed, fixed, and covered by passing regression checks

Scope: `eb88ecd..3f9a2f9`. Reviewer: Claude Opus 5.5, medium, through T3.

## Parent assessment
- Finding 1 confirmed. Only a stale CLI `local` operation intent is cleared. Local process/model/ownership files remain untouched. Unknown providers and app-scoped intents stay blocked. Two regression tests cover persistence and provider-call absence.
- Finding 2 confirmed. `.env.example` now describes the supported cloud model and connection.

## Reviewer output

Two low-severity findings, no blockers. Model/config migration, cloud Start/status/Stop ownership checks, the thin image, `start.sh`, chunk downloads and client setup all look correct. I did not run any builds or tests on this Mac; this is a code read only.

**1. 🐛 Low/Medium — a leftover `local` pending create blocks the Rust CLI's `lobo up` and `lobo down` for good**
- Where: `crates/lobo-core/src/control/cleanup.rs:135-137`. The same guard is in `adopt` at `cleanup.rs:13-15`.
- Before this branch, the CLI had a `local` provider, so `cleanup::pending` could reconcile and clear a `PendingCreate{provider:"local"}` in `~/.local/state/lobo/operation.json`.
- Now it returns `UnresolvedCreate "provider is no longer supported"` and never calls `guard.clear()`.
- How it happens: an older CLI crashes during a local start and leaves that pending record.
  - `lobo up` → `up.rs:122` `Self::Cli => cleanup::pending` → error on every start.
  - `lobo down` → `status.rs:52` → same error on every run.
- The only fix is to delete the state file by hand, and the error text gives no hint.
- No test covers this case.
- This Mac: the state dir has only `operation.json.lock`, no `operation.json`, so nobody is stuck right now.
- The app is unchanged here: it already refused this case before the branch, at `up.rs:124-133`.
- Fix: for a non-cloud provider in the CLI scope, clear the record. Nothing can delete a cloud instance from it, so clearing is safe. Add a test: a local pending record, then `up`/`down` proceed with zero provider calls.

**2. Low — `.env.example` still documents retired settings**
- Where: `.env.example:30-44`, not touched by this branch.
- It still says `# q8 | q6 (default q8)`, documents local mode, and lists `LOBO_WEIGHTS_DIR` / `LOBO_LOCAL_PORT`.
- A user who copies it and writes `LOBO_MODEL=q8` gets a silent migration to `qwen38-q6`.
- The local keys do nothing now.
- The spec's documentation row expects active docs to describe only what is still supported.
- Fix: drop the local block and change the model line to `qwen38-q6`.

What I checked and found clean:
- **Config:** `saved_model` and the `local` provider migration. `validate_selection` rejects `q6`/`q8`/`local` before any write. `save()` writes the migrated model on a normal save and keeps unknown keys. `show()` hides the legacy keys.
- **CLI setup and app selection:** the wizard always writes `LOBO_MODEL`. The app store falls back to `DEFAULT_CLOUD_MODEL`.
- **App ownership:** `load_owner` still hides a legacy local owner.
- **Cloud lifecycle:** `delete_owned` now accepts only runpod/vast. The state path is unchanged (`state_dir()` = old `local.json` parent).
- **Pod image:** `thin` is the last Dockerfile stage. `start.sh` requires `LOBO_MODEL_BASE_URL`, and bootstrap always sets it (`image_model: true`, non-empty default URL). The agent's `LOBO_IMAGE_MODEL` bypass is gone and tested.
- **Image tools:** the verifier rejects `LOBO_IMAGE_MODEL` and models other than `qwen38-q6`.
- **Clients and probes:** the OpenCode binding uses `lobo` only. The renamed probe tools have no references left to removed names.
