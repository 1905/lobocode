# Retired model cleanup results
**Date:** 2026-10-31
**Status:** in progress

## Authorization and constraints
- User requested removal of unused Q6/Q8 code after the fullsend delivery.
- Keep only qwen38-q6 on RunPod/Vast.
- No local Rust build, inference, GPU rental, app installation, or optional CLI release.
- Final Telegram notification requested explicitly: send through the notify skill after delivery and checks finish.

## Progress
- Clean baseline: eb88ecd on master; feature branch cleanup/retired-models.
- Baseline tests from the preceding release passed: 548 workspace, 21 UI, 99 app, six native setup cases.
- Spec and implementation plan recorded. Plan language pass delegated once to ClaudeExec Sonnet 5.5.
- Catalog, local runtime/provider/CLI, full-image packaging, local protocol types, and obsolete upload action removed.
- Existing config values migrate without deleting unknown keys or user files. Cloud ownership fields remain readable.
- Cloud diagnostic helpers now have no local runtime entrypoint.
- Plan pass: Codex through T3, one call, 66/66 lines covered, 49 lines changed, two mechanical findings left.
- Feesh focused checks passed: 127 agent tests, 31 image verifier checks, 22 image transfer tests, cloud fake HTTP and ownership checks.
- Air app tests passed: 97 library tests and two fixture tests; app clippy passed. UI: 21 tests, type checks and formatting passed.
- Workspace final run, native checks, final review, image/app build and publication pending.

## Decisions
- Retain historical Go implementation and frozen historical evidence; cleanup targets the active Rust app/CLI/agent.
- Keep migration and rejection fixtures containing retired IDs. Do not change q8_0 KV-cache quantization.
- Preserve persisted cloud state formats and identity checks when removing local inference helpers.
