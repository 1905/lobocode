# Cloud-only app execution record
**Date:** 2026-10-02
**Status:** delivered

Baseline: master4b117c3. Prior hosted run36836204672 passed units/builds/core/CLI/static agent; native startup/settings smoke failed. New clean worktree /Users/kass/dev/lobocode-cloud-only preserves prior dirty Task11 preparation.

## Implementation checks

- Feesh: Svelte check reports zero errors/warnings; all 19 UI tests pass; Vite production build and Prettier pass after hosted type/fixture regeneration.
- Feesh release preflight: anonymous public latest-q6 index/manifest/config validation passed at digest `sha256:aa590424f7862b5664c943cc5753bb72d447cbdaec5ed03a66642e138e3857f4`. Workflow YAML and embedded Python/JS/shell syntax passed.
- Hosted run36988818924 generated cloud-only types/fixtures. Its app lint stopped on Rust formatting. Run36988946856 had the same known formatting failure. Core/CLI, protocol and static agent jobs passed in both runs.
- Generation artifact11219335094 was downloaded and applied to the branch. Obsolete Local state fixtures and Target/LocalMemory types are deleted.
- Latest validation source: `1f0d463`. Backend/provider/legacy ownership guards, native cloud-only harness and standalone release workflow are implemented. Native acceptance and publication are pending.
- No local inference, OpenCode launch, new GPU rental, CLI publication or Q8 image build occurred.

- Run36989404394 at `1f0d463`: protocol, static agent and core/CLI pass. App lint/unit/UI checks and generated fixture/type comparison pass. Production/native bundle checks are still running.
- Whole-branch Rival Codex review completed: one high provider-selection finding and one medium hidden-local-port finding against `e3d57b6`. Both are fixed and covered in `1f0d463`; see review.md for the full output. No other material findings were reported.

## Native validation

- Hosted production artifact at `1f0d463`: ad-hoc signature, no bundled CLI, and removed local-supervisor mode exit2 passed.
- Actual production bundle on this Mac: isolated legacy local config with invalid local port opened cloud Setup. No Local control. Settings had Cloud, Defaults and Clients only; Q6 selected; all controls visible and corners rounded. Clients correctly showed no active endpoint and disabled Configure. No scrolling used.
- Native Quit stopped only task PID78171. Installed app PID51172 remained running. Isolated config bytes stayed unchanged; no local runtime state was created. Evidence: `/tmp/lobo-cloud-only-native-1f0d463` and task state receipt under `/var/folders/85/nvpy_pws0774v6l8wl87_8b80000gn/T/lobo-cloud-only-native-state-d3r2ztx9`.
- Title-bar drag input was issued through Computer Use. The tool does not expose window position, so actual movement is not claimed. Dock/global Command-Tab behavior remains outside this feature's verified scope.
- Run36989404394 native setup/settings save/layout passed. Removed-command test failed because the embedded driver surfaced the expected `Command choose_target not found` outside the page catch. `8bf47a7` handles only the expected rejection messages at the driver boundary; transport/unexpected errors still fail. Full native rerun is pending.
- README Cloud/Defaults/Clients screenshots now come from that hosted native fixture run. The obsolete Local settings image is removed.

- Run36990463015 at `8bf47a7`: all build/lint/unit/type/fixture checks and native fresh/legacy smoke passed. Native checks exercise real IPC and real Settings saves with no providers or model runtime. Both test modes preserve the legacy preferences, CLI data and ownership sentinel. Only the expected removed/invalid-operation errors are accepted by the harness.
- Code is ready for direct master merge. App-only release remains pending exact merged-revision CI and the release workflow.

## Published release

- App source/tag: `358b9cbd331902722c61ac4ffde0acc0949c1497`, `app-v0.2.1`. Direct master merge and push completed.
- [Exact master Rust CI](https://github.com/1905/lobocode/actions/runs/36991557535) passed all four jobs: protocol/workspace, static agent, core/CLI on macOS, and app. App checks passed 79 library tests, two fixture tests, 19 UI tests, generated-file comparison and production bundle validation. Six native cases passed across fresh and legacy setup, with clean task cleanup.
- [App release workflow](https://github.com/1905/lobocode/actions/runs/36992628194) attempt 1 built the DMG and verified draft assets, then failed publication on a draft tag-lookup 404. Independent validation passed before publishing owned draft `401708978` by ID. Attempt 2 passed the full published-asset retry path without rebuilding.
- [Public release](https://github.com/1905/lobocode/releases/tag/app-v0.2.1) contains only `lobocode.dmg`, `app-release.json` and `SHA256SUMS`. Legacy CLI latest remains unchanged; no CLI or GPU image publication ran.
- DMG: 5,767,384 bytes; SHA256 `50af54392577b6f51b5e78a7280b76ec69a7c2b5a92e7a12d4c17884dd52704b`. Anonymous downloads matched all three validated files byte for byte.
- Independent Mac artifact checks: mounted layout is the app plus Applications symlink; bundle version 0.2.1 and source match the receipt; arm64 executable; strict/deep signature validation; no bundled CLI/agent. This checks the released artifact, not a local rebuild.
- Local receipts: `/tmp/lobo-app-release-0.2.1/independent-validation.json` and `anonymous-validation.json`. Exact master native evidence: `/tmp/lobo-cloud-only-native-master-358b9cb`.
- Future release correction: authenticated exact-tag listing for drafts, duplicate rejection and numeric release ID publication. Existing source/ownership/hash gates stay in place. Feesh YAML and 14 embedded syntax checks passed; its scratch was cleaned.
- QA-ready Telegram notification returned `SENT: ok`. Installed app, CLI, personal config/model/runtime files and prior dirty Task11 work remain untouched. No new rental, local inference or OpenCode launch occurred.

## Remaining limits

The released app is ad-hoc signed and is not notarized. Drag position, Dock/global
Command-Tab, live OpenCode and loading-memory progression remain unverified.
Cloud lifecycle evidence from `b5617ae` remains historical; this release ran
isolated native setup/migration checks, not another paid inference run.
