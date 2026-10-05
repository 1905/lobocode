# Startup diagnostics execution record
**Date:** 2026-10-05
**Status:** done

## Baseline and evidence

- Master c16f6ea, exact CI 36994301207 passed. App 0.2.1 installed from public app-v0.2.1.
- User reported a 20-minute RunPod container-step stall. No durable startup log existed; app stdout/stderr were /dev/null. At 05:36:52 UTC, read-only RunPod and Vast requests each returned HTTP 200 with zero instances. This does not identify the earlier failure.
- Native Computer Use failed to start its pipe; no live UI status was read during diagnosis. Private read-only receipt: /var/folders/85/nvpy_pws0774v6l8wl87_8b80000gn/T/lobo-stall-20261005-zy6asv09/summary.json.

## Implementation checks

- Feesh UI: 20 tests, Svelte type checks (zero warnings), production Vite build and formatting pass with hosted Rust-generated bindings from CI 37269716115.
- Dell core checks passed: 270 library tests, one pre-existing opt-in archive-download test ignored; all-target clippy passed. Five new cases cover pre-agent missing pods, unknown authorization state, repeated heartbeat, hung reads and cancellation. Evidence: /tmp/lobocode-startup-core-20261005-evidence/. This follows the 2026-10-03 Rust-host correction; no containers ran on Dell.
- App Rust and native checks run on hosted macOS. No local Cargo, inference or OpenCode launch.

## Review and integration

- Rival review of dab5025 found three defects: Stop/Ready cleanup race, resumed deadline falsely failing a previously Ready runtime, and malformed config error exposing opaque credentials. Full output: [review.txt](review.txt). The follow-up changes accept successful completion during Stop, track established readiness per runtime identity, bound resumed observation instead of pod age, and remove config values from parser/app errors. Dell follow-up passes 34 configuration tests and 53 CLI tests with test-fakes, including Go consumers and all TUI snapshots. Feature-enabled core/CLI clippy passes. CI 37270507497 passed all shared checks and app lint. App tests: 94 passed, one existing directory-count assertion failed because diagnostics adds a directory. The test now compares directory contents before and after cancellation. All new diagnostic/deadline/race regressions passed.
- First hosted CI compiled the app and generated bindings. It found a one-character clippy fix and CLI/TUI golden expectation changes for the intentional initial agent-unreachable detail. These failures block release until corrected.

## Exact source acceptance

- Source `24b30284063a49a3492e029b38c8a34cf32ae6a5` passed branch CI [37271018351](https://github.com/1905/lobocode/actions/runs/37271018351) and exact master CI [37271794244](https://github.com/1905/lobocode/actions/runs/37271794244). Direct merge and push are complete.
- App: 95 Rust library tests plus two binary tests, 20 UI tests, lint/type/format/generated-file checks, production bundle verification and six native fresh/legacy setup cases passed. Native checks cover Logs visibility, JSONL launch/config records, 0600 files, 0700 directory, credential absence and controls fitting without scrolling.
- Shared core library: 271 Linux and 272 macOS tests pass, with the pre-existing opt-in archive test ignored. Required CLI/TUI, protocol, Go compatibility and static agent build checks pass. No new cloud rental, local inference or OpenCode launch occurred.
- Review fixes are verified: Stop/Ready cleanup race, old-pod disconnect/reconnect, per-runtime observation reset and malformed opaque credentials.
- Hosted native screenshot: `docs/img/panel_setup_logs.png`. Evidence is retained in `/tmp/lobo-diagnostics-native-37271018351/`; full CI log is `/tmp/lobo-diagnostics-ci-37271018351.log`.
- Tag `app-v0.2.2` points to the exact source. App release [37272648758](https://github.com/1905/lobocode/actions/runs/37272648758) passed. All three public assets matched anonymous downloads.

## Release and local installation

- [App 0.2.2](https://github.com/1905/lobocode/releases/tag/app-v0.2.2) is public. DMG: 6,356,829 bytes; SHA256 `c080fa94f04dfb9e389836a067606b273a91ae48598a8ae4815d47a3ec7b3b32`. Source receipt, mounted contents, arm64 executable, app signature and no bundled CLI passed independent checks.
- Installed the exact public DMG at `/Applications/lobocode.app`; process 93435 is running. Verification completed at `2026-10-05T06:38:11.251956+00:00`. Old app retained at `/var/folders/85/nvpy_pws0774v6l8wl87_8b80000gn/T/lobo-diagnostics-install-20261005-modqiqbf/previous-lobocode.app`.
- Real private log `/Users/kass/.config/lobo/config.app-logs/app.jsonl` contains `launch`, `ownership_loaded`, `config_loaded` and `snapshot` events from version 0.2.2. File 0600, directory 0700. Configured credentials were absent.
- Config, preferences and CLI file hashes match the pre-install values. Read-only RunPod and Vast checks returned zero instances immediately before installation. No pending operation or app ownership existed.
- Private installation receipt: `/var/folders/85/nvpy_pws0774v6l8wl87_8b80000gn/T/lobo-diagnostics-install-20261005-modqiqbf/receipt.json`.
- QA-ready notification sent through the notify skill. Final delivery records are included with this documentation update.
- Task-owned Dell build/format and Feesh UI scratch directories were removed. Evidence and the previous installed app are retained. No local Rust artifacts were created.

## Limits

The historical root cause remains unknown. No new paid GPU test is part of this logging/regression release. Local native Computer Use failed to start its pipe; this Mac's visual acceptance is not claimed. Hosted native checks passed. The app remains ad-hoc signed and is not notarized.
