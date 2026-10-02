# Cloud-only Mac app
**Date:** 2026-10-02
**Scope:** /Users/kass/dev/lobocode
**Status:** delivered (explicit full-auto authorization)

## TL;DR
**What:** Remove local execution, local settings and local memory controls from the Mac app, then release a standalone DMG.
**Why:** The app must only manage cloud GPUs.
**Your action:** Nothing; full auto covers implementation, testing, merge and app release.
**Limits:** Keep CLI local execution, model files and existing local processes unchanged. No CLI release or Q8 image build.

## Problem(s)
1. Start exposes Local and Cloud, and Settings exposes local weights and ports (`app/ui/src/panel/StartCard.svelte:22`, `app/ui/src/settings/Settings.svelte:286`). Hiding a button alone leaves local execution reachable.
2. App startup registers local IPC, probes memory and constructs a local provider (`app/src-tauri/src/lib.rs:33`, `backend.rs:89`, `controller.rs:439`). The executable also accepts a local-supervisor mode (`main.rs:18`).
3. Old preferences and ownership can select/adopt a local runtime (`store.rs:232`, `backend.rs:240`). Cloud migration must preserve that runtime and its files.
4. The existing release workflow couples DMG publication to CLI and both model images (`.github/workflows/release.yml:13`). Only Q6 has verified public availability.
5. The merged revision's hosted native smoke failed despite passing unit/build checks (run 36836204672). Release requires a passing cloud-only native smoke.

## Goals
1. No app UI or IPC path starts, downloads, probes or manages a local model.
2. Old local preferences/configuration open cloud setup; they do not trigger a local runtime or silently rewrite CLI settings.
3. Cloud identity-scoped Start, status, Stop, SSH and OpenCode setup continue to work.
4. New app sessions default to Q6 when no explicit cloud model choice exists. Explain unavailable Q8 without publishing another image.
5. Ship a verified standalone Apple Silicon macOS DMG independently of Homebrew and model-image builds.

## Non-goals
- Remove local support from shared core or CLI.
- Delete models, terminate existing local processes, or edit personal config during implementation.
- Finish unrelated telemetry/updater features or include the dirty prior Task11 worktree.

## Cloud-only boundary
The app exposes cloud providers RunPod/Vast and cloud model/context/connection settings. Remove the target picker, local settings tab, disk/model listing, local memory display and local IPC commands. App state and generated fixtures become cloud-only; remove unused local types/components rather than retain dead execution branches.

Backend rejects non-cloud provider input at Start, configuration writes and scoped runtime actions. Construct no LocalProvider: disable local capability in the app wiring only. The old local-supervisor argument exits with an explanatory error before app launch. The cloud SSH helper remains.

Old preferences selecting Local normalize to Cloud. A legacy local app-ownership record is ignored for app operations without killing the process or removing model/config/runtime files. A later cloud ownership write may replace app-only ownership metadata after comparing the captured record; it must not weaken concurrency checks. Cloud discovery never falls back to local. Old CLI LOBO_PROVIDER=local remains unchanged on disk; the app chooses an available cloud provider in memory.

App settings hide local-only keys and reject their writes. Shared config and unrelated CLI values survive cloud setting saves. Copy endpoint/key and Configure OpenCode use the app-owned cloud runtime only.

## File-level changes
| Files | Change |
| --- | --- |
| app/src-tauri/src/{backend,controller,store,types,prefs,commands,lib,main,supervisor,e2e_memory,tray,windows,opencode}.rs; controller/tests.rs; store/tests.rs; opencode/tests.rs; tests/fixtures.rs; examples/generate_ui.rs | Remove local app behavior, migrate legacy state safely, add boundary regression tests, remove obsolete local modules. |
| app/ui/src/{panel,settings,lib,render}; App.svelte; gen; fixtures | Remove local views/API calls and regenerate cloud-only state/types/fixtures. Update cloud selection and UI tests. |
| app/e2e/{app.spec.js,fixture.py,fake_llama.py,memory.spec.js,wdio.conf.js}; tools/native_app_e2e.py; Makefile; .github/workflows/rust.yml | Replace obsolete local-memory native smoke with cloud-only setup/migration checks. Diagnose baseline native harness failure; no real inference in fixtures. |
| .github/workflows/app-release.yml; app/src-tauri/{Cargo.toml,Cargo.lock,tauri.conf.json}; scripts/macos; app/ui/package.json | Independent versioned app release using tested source, public Q6 preflight, standalone DMG, checksum and release notes. Only change build scripts/package versions if required. |
| README.md; CHANGELOG.md; AGENTS.md; docs/implementation-mistakes.md; docs/img; plans/done/2026-10-02-cloud-only-app | Cloud-only app documentation, regenerated relevant screenshots, execution receipts and release links. |
| crates/**; CLI release/Homebrew; prior dirty worktree | Preserve. Shared interfaces may be reused but do not remove CLI local support. |

## Tests
- Backend: reject explicit local start/save/stop/sampling and old executable mode before dependency work; preserve legacy local files/processes; cloud ownership races remain covered.
- Store/preferences: Local preference and local CLI defaults become cloud UI; configured cloud provider/model selection remains valid.
- UI: no Local picker/tab/memory/disk controls; only cloud providers; cloud readiness drives Start; Settings/Clients fit without scrolling.
- Hosted CI: Rust/core/CLI unchanged suites, app lint/unit/type/fixtures/build, cloud-only native setup/migration. No model rental in CI.
- Actual native app: hosted production artifact opens cloud-only, settings fit, old Local prefs do not restore Local, normal corners/window behavior. No local model or OpenCode process.
- Release: validated exact commit passes CI, public Q6 digest exists, DMG has signed app and no bundled CLI, published asset download/checksum/bundle match.

## Failure modes & decisions
| Failure | Behavior |
| Old Local prefs or CLI provider | Cloud setup in memory; preserve original config. |
| Legacy local owner | Never poll/stop/adopt local runtime; preserve unrelated state. |
| No cloud key | Setup with Start unavailable; never use Local. |
| Missing selected image | Fail before rental; no cached fallback. |
| CI/native smoke/release asset check fails | Fix and rerun affected checks before publishing. |
| Old release workflow would build Q8/publish CLI | Use independent app-only workflow and app-prefixed tag. |

## Out of scope
CLI feature changes, Q8 publication, personal local runtime cleanup, updater, sustained generation benchmarks and new paid services.

## Rollout
P1: Cloud-only app and focused tests; push branch and verify hosted build/native smoke.
P2: Docs/assets plus complete required CI and whole-branch review; merge directly to master.
P3: Verify merged source, publish app-only DMG, download/verify release and notify user.

## As-built notes

- App 0.2.1 ships from `358b9cb`. The shared core and optional CLI retain local support.
- In-memory cloud configuration also resets the hidden legacy local port, so an invalid CLI-local value cannot block cloud Start. Provider selection falls back to a configured cloud key when the previous provider key is removed. Original file values remain intact.
- Removed-command native checks handle only the expected unknown-command/invalid-operation errors at both page and driver boundaries. Unexpected transport errors still fail.
- The release workflow uses numeric draft IDs after the initial draft tag-lookup failure. Its path is included in Rust CI triggers. Published assets passed independent download and mounted bundle validation; see results.md.
- Native settings fit without scrolling and show rounded corners. Actual drag position and global app-switcher behavior were not established by these checks. Live cloud inference was not repeated for this release.
- Relevant Settings assets were replaced; no new model image or CLI release was needed. The complete feature record is archived under plans/done.
