# Cloud-only app execution record
**Date:** 2026-10-02
**Status:** in-progress

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
