# Retired model cleanup results
**Date:** 2026-10-31
**Status:** done — merged, published and publicly verified

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
- Full workspace run passed: 452 tests, zero failures; one separate SSH test excluded from the default command.
- The excluded SSH integration passed in a task-owned Feesh container: authentication, streaming, detached reopen, reconnect, 105-second response, crash recovery, key rejection, port conflict and cleanup.
- Six Air native setup/migration cases passed with no cleanup errors. All controls fit without scrolling; fixed model text and rounded corners inspected.
- Final review found two issues: stale local CLI operation intent and outdated example config. Both fixed; two added regression tests and affected core/CLI suites plus clippy pass.
- Initial image/app candidates built from 3f9a2f9; offline image runtime smoke passed. Final artifacts were rebuilt after the review fix.

## Delivery
- Source: `240989574fbc1c9029e229864e2182b74368dbb1`, fast-forwarded into `master` and pushed. The annotated `app-v0.2.4` tag points to that commit.
- [App 0.2.4](https://github.com/1905/lobocode/releases/tag/app-v0.2.4) published at 2026-10-31 05:05:18 UTC. Release ID: `429734447`.
- DMG: 6,107,978 bytes; SHA-256 `9b1907e66878625d140b8b6f857e5f42ee5518f206264fdb86a657822bec5bf8`.
- Anonymous downloads of the DMG, `app-release.json` and `SHA256SUMS` matched the local files and GitHub asset digests.
- Air: arm64 binary, version 0.2.4, valid ad-hoc signature, no bundled CLI or agent. Mounted DMG contains only `lobocode.app` and the Applications shortcut. The verification mount was detached.
- All 575 files in both final build directories matched the Git archive. The app does not embed a source commit; the receipt records the verified source archive and build, not an embedded binary attestation.
- Image tags: `sha-240989574fbc-qwen38-q6` and `latest-qwen38-q6`.
- Public image digest: `sha256:3d64617400c59420f0aea21a0bb6c0ac83376a8ee377dd2710ef63d5fc41fc0a`; linux/amd64, 18 layers, correct source/model labels, anonymous manifest/config access verified.
- Offline image smoke: agent 0.2.4, llama.cpp b11118, cloudflared 2026.9.1 and OpenSSH 9.6 execute. No retired model manifest/directory or baked SSH host keys remain.
- GitHub Actions remains disabled. The optional CLI latest release remains `v0.1.0`. No new PR was created.

## Final test evidence
- Full workspace: 452 passed, zero failures, 22 suites. Its one ignored SSH integration was run separately and passed.
- Review fixes: 291 core/CLI tests passed, zero failures, 16 suites. This includes the two new migration regressions. Workspace clippy passed again.
- Air rerun after review fixes: 97 library tests plus two fixture tests passed. UI and native behavior did not change after their successful checks.
- The SSH integration took 122.13 seconds. It covered a 105-second response, streaming, reconnect, detached reopening, helper crash, authentication, host/client key rejection, port conflict and owned cleanup.
- Both native setup runs returned exit code 0 and `cleanup_error: null`. Their six cases covered fresh and legacy config paths. The task SSH container is absent.
- First DMG verification assumed the app embedded `LOBO_COMMIT` and failed that assertion. Source inspection showed the app does not use it. The corrected check records that limitation and verifies the source archive, signature, version and contents.

## Evidence locations
- Public artifact receipts are saved beside this record: `app-release.json`, `public-image.json`, `public-release-verification.json`, `final-app-artifact.json` and `final-app-source.json`.
- Feesh logs: `/srv/lobo-retired-models-20261031/evidence/`. Relevant files: `workspace-tests.log`, `review-fix-tests.log`, `review-fix-clippy.log`, `ssh-integration.log`, `final-image-build.log`, `final-image-smoke.log`, `final-image-push.log` and `latest-image-push.log`.
- Air logs: `/Users/claw/dev/lobocode-retired-models-20261031/evidence/`. Relevant files: `app-tests.log`, `app-clippy.log`, `native-e2e.log`, `review-fix-app-tests.log` and `final-release-build.log`.

## Limits and preservation
- No fresh GPU allocation, inference or live provider lifecycle check ran for this cleanup. Model chunks and download behavior are unchanged.
- Installed app, optional CLI, personal config and model files remain unchanged. Historical Go source remains outside this cleanup.
- App signing is ad-hoc; Developer ID signing and notarization remain unverified.

## Decisions
- Retain historical Go implementation and frozen historical evidence; cleanup targets the active Rust app/CLI/agent.
- Keep migration and rejection fixtures containing retired IDs. Do not change q8_0 KV-cache quantization.
- Preserve persisted cloud state formats and identity checks when removing local inference helpers.
