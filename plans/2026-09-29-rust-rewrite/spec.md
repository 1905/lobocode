# Rust rewrite: one core, CLI, pod agent and Tauri app

**Date:** 2026-09-29
**Scope:** /Users/kass/dev/lobocode
**Status:** approved for implementation after plan correction (user: full auto, 2026-09-29). Earlier review covered v1.0 only.

## TL;DR

**P1 — Rust workspace + shared types.** What: a cargo workspace replaces the Go module on branch `feat/rust`. `lobo-proto` holds every type that crosses a boundary (pod `/api/*` JSON, `up` events, model catalog, release manifest). TypeScript types for the app are generated from it. Why: today Go, Swift and the pod share only hand-copied JSON shapes. You do: nothing. Does NOT do: any behaviour yet.

**P2 — pod agent in Rust.** What: `lobo-agent` (runner, watchdog, download, metrics, `/api`) as a library plus a static Linux binary. The baked image `ghcr.io/1905/lobocode` builds it. Why: you chose "everything in Rust". You do: allow one live pod boot per provider to prove it (a few $). Does NOT do: change the pod API shape or the bootstrap contract.

**P3 — `lobo-core`: config, providers, control.** What: config file (same `config.env` keys), RunPod, Vast, local (Mac supervisor on the same agent library), up/status/down/test/logs, release upload. Why: this is the code the app currently reaches through the CLI. You do: nothing. Does NOT do: any UI.

**P4 — `lobo` CLI in Rust.** What: same commands, flags, output and exit codes as today. That includes the `up` progress view, the live `status` dashboard (ratatui), `lobo config` wizard, `--json` everywhere, and `release`. brew formula from the Rust build. Why: parity before merge (your pick). You do: nothing. Does NOT do: new commands.

**P5 — Tauri app.** What: same shape as today: menu bar item + small panel, Settings window. It calls `lobo-core` directly, with no CLI subprocess. Opening it from Finder also shows the panel, which fixes the item hidden behind the notch. Why: the CLI dependency is the thing you dislike. You do: review PNG renders of every panel state before merge. Does NOT do: Windows/Linux builds.

**P6 — cutover.** What: live checks on a RunPod pod, a Vast instance and local. The e2e suite runs. Then merge `feat/rust` to master, and Go + Swift are gone. Why: you chose a big-bang replace. Master keeps the working Go build until this merge. You do: receive QA and final reports; full-auto execution is authorized. Does NOT do: keep Go around afterwards.

## Problem(s)

1. The mac app has no logic of its own. Every action shells out to the bundled `lobo` and parses its JSON (`macos/Sources/Lobocode/CLI.swift:13` "this is the only door", `Store.swift:170` `upArgs`). So app features need CLI features, errors arrive as stderr text, and the app breaks when the binary is missing or stale.
2. Three languages carry one model. Go types (`internal/agent/status.go`, `internal/control/events.go`, `internal/release/manifest.go`) are copied by hand into Swift (`macos/Sources/Lobocode/Models.swift`). Every field change is edited twice and has already drifted. Example: port rules exist in `config.ParseLocalPort` and again in `Store.swift`/`SettingsView.swift`.
3. The app cannot be found when the menu bar is full. The notch hides the item, and opening the app from Finder shows nothing (seen live 2026-09-29).

## Goals

1. One Rust core workspace and a separate Tauri app workspace. All laptop logic lives in `lobo-core`, which the CLI and the app both link (P1, P3, P5).
2. Boundary types live once in `lobo-proto`. The TypeScript types are generated from it and never hand-written (P1).
3. The pod agent and the local supervisor share one agent library (P2).
4. Full feature parity with the Go/Swift build at the recorded master fork after `fix/app-silent` (`6a72092`) is merged, plus later master changes tracked in the P6 port ledger (P4, P5, P6). The checklist is in "Parity".
5. Every live-learned behaviour listed under "Carry-over rules" survives the rewrite, each with a test (P2–P4).
6. Merge only after live boots on RunPod, Vast and local pass (P6).

## Non-goals

- New features, new providers, performance tuning. Local runs at 5.4 tok/s today; that is a separate investigation.
- Changing the config file format, the pod `/api/*` JSON or the local state file. The new binaries must read what the old ones wrote.
- Windows or Linux desktop app builds. The CLI still builds for Linux.
- Keeping any Go or Swift code after the merge.

## Workspace

```
Cargo.toml                 (workspace)
crates/
  lobo-proto/   types crossing a boundary: agent Status/Timings/Download, UpEvent/ReadyInfo, Manifest/Resolved,
                model catalog + chunk sha table, local ModelsInfo. serde + ts-rs (TS export).
  lobo-agent/   lib: Runner, watchdog, downloader (HTTP range/parallel/resume/sha, SSH source), metrics
                (llama /metrics, nvidia-smi, /proc), /api server (axum), process helpers,
                CleanEnv, LlamaArgs, pod self-terminate (selfkill: RunPod GraphQL, Vast REST).  bin: lobo-agent (x86_64-unknown-linux-musl, static).
  lobo-core/    config (dotenv read/write keeping comments + Layout), providers (runpod REST, vast,
                local), control (up/status/down/target/test checks), release (zip, secret scan, R2 upload,
                presign), bootstrap script, local supervisor (uses lobo-agent), opencode genkey.
  lobo-cli/     bin: lobo. clap commands, ratatui views (up progress, status dashboard), inquire wizard, --json.
  lobo-e2e/     #[ignore] live suite (today's e2e/ checks), run explicitly against a running endpoint.
app/
  src-tauri/    Tauri 2 shell: tray item, panel window, settings window, commands → lobo-core, events for up.
                Hidden `--lobo-local-run` argv re-execs the supervisor (no CLI needed).
  ui/           TypeScript + Svelte 5 (vite). Types generated by ts-rs from lobo-proto. Renders every state
                from fixtures for review (vitest + a /render route screenshot via playwright-cli).
docker/pod/Dockerfile  rust:musl build stage → same llama.cpp base image.
```

Crate choices (why): tokio + reqwest(rustls) (async HTTP), serde, clap (derive), ratatui + crossterm (TUI), inquire (wizard like huh), tracing (logs like zerolog), axum (agent API), russh (SSH model source), rusty-s3 (R2 presign/upload without the AWS SDK weight), sha2, tar + flate2 (runtime untar), zip, nix (signals, setsid, flock), ts-rs (TS types), insta (TUI/golden snapshots). Exact versions get pinned in the plan after `cargo add`.

## Flows

```
CLI:  lobo up ─► lobo-core::control::up(deps, opts, cancel) ─► provider.rent ─► poll agent /api/status ─► events → ratatui | --json
App:  panel START ─► tauri command up(opts) ─► same lobo-core::control::up(deps, opts, cancel) ─► Svelte UI
Local supervisor: CLI `lobo local run …` | app `lobocode --lobo-local-run local run …` ─► lobo-core::local::supervise ─► lobo-agent Runner (Mac hooks)
Pod:  bootstrap.sh ─► /lobo/lobo-agent (Rust, baked image or release zip) ─► Runner (pod hooks)
```

The app never parses CLI output. Errors are typed (`lobo_core::Error`) and reach the UI as `{kind, message}`. Exception: `up` failures arrive as `UpEvent.err` (a string, same as `lobo up --json`); the app shows them as kind `up`. No wire change. Cancelled `up` uses an owned operation with separate event and completion channels. Successful cleanup ends with phase `cancelled`. A 120 s app deadline only warns; it never detaches a creating worker. Cleanup errors remain visible. Details: contracts.md and execution.md.

## Parity (v1 must have all)

| Area | Today (Go/Swift) | Rust home |
|---|---|---|
| Providers | RunPod (community-only default, `--cloud secure`, net tiers), Vast (cheapest, inet floor, no-credit stop), local | lobo-core::provider::{runpod,vast,local} |
| up | release resolve / `--image` / local defaults, model sources r2/feesh/ssh/public, bad-host replace ×4, container timeout, progress events | lobo-core::control |
| status / down / test / logs / models / gen-api-key / config (show/set/get/path + wizard) / release / version | cmd/lobo | lobo-cli (+ lobo-core) |
| TUI | up progress, live status dashboard, goldens | lobo-cli (ratatui + insta) |
| Pod agent | tunnel, GPU check, download (parallel, resume, chunk sha, slow-host drop, fallback), llama start, watchdog idle/expiry, self-terminate, /api | lobo-agent |
| Local | runtime fetch b11118, models listing + markers, supervisor, state claim/flock, group kill, active endpoints | lobo-core::local + lobo-agent |
| Image + CI | pod-image.yml, release.yml (goreleaser → brew tap via deploy key) | Dockerfile rust stage; release via cargo-dist or goreleaser rust builder (plan picks after a test) |
| App | tray square + status word, panel off/boot/ready/fail/setup (cloud + local), settings (all keys, weights picker), notifications, renders | app/ |

## Carry-over rules (live-learned; each gets a test)

- RunPod REST refuses a default HTTP User-Agent → always send a UA. Pod self-terminate uses GraphQL `podTerminate` with the pod-scoped key (REST 403s it).
- Vast: `runtype: "ssh"`, or onstart never runs. DELETE 404 = already gone. `insufficient_credit` stops at once.
- Bootstrap: time-bounded steps. Any failure → terminate retried 30× then sleep. `execfail` for a missing agent. The baked image skips apt + zip.
- Agent: CUDA init retries 3 min. VRAM free ≥ model + 2560 MiB, else bad host. Slow download (< `LOBO_MIN_MBPS` after 20 s) drops the host. R2 slow → model-server fallback. SSH source ≤ 8 streams (sshd MaxStartups). PID-1 exit makes RunPod restart the container, so fatal paths self-terminate.
- Secrets never enter pod env, releases or children (`CleanEnv` union list). Config is read only from the config file, never the OS env.
- `up` refuses while anything runs. `down` needs only provider keys. `release` needs R2 + bucket.
- Local: supervisor identity = `local run` + exact `--boot-id`. SIGKILL the process group. State removed only by pid+boot id. Marker `<sha> <size> <mtime_ns>`. Endpoints from the running state's ports.
- Home network DNS-hijacks `*.r2.dev`. This is a test-environment fact, not code, but the live checks need the Dell pinned-IP workaround.

## File-level changes

| Path | Change |
|---|---|
| `Cargo.toml`, `crates/*`, `app/*` | New workspace per "Workspace". |
| `macos/` | Removed in P5 after render verification and QA notification (moved, then `git add -A`). |
| `cmd/`, `internal/`, `e2e/`, `go.mod`, `go.sum` | Removed at P6, not earlier: the Go fixture dumpers and drift checks (P1, P3, P4) and `git merge master` need the Go module until cutover. |
| `tools/protofixtures/`, `tools/corefixtures/`, `tools/clifixtures/`, `cmd/lobo/capture_test.go` | Go/Python fixture generators added in P1/P3/P4. Removed at P6; their generated fixtures stay as frozen golden files. |
| `app/src-tauri/` | Its own cargo workspace (path deps on `crates/`), so the pod image and ubuntu CI never build Tauri. |
| `docker/pod/Dockerfile` | Rust musl build stage for `lobo-agent`. Same base image and `/lobo` layout. |
| `.github/workflows/pod-image.yml`, `release.yml`, `.goreleaser.yaml` | Build Rust. Brew formula still `lobo`. App bundle built by `cargo tauri build` (unsigned, dev). |
| `Makefile` | Targets map to cargo/pnpm: build, test, lint (clippy -D warnings), install, install-mac, e2e, release. |
| `README.md`, `.env.example`, `opencode.json.example` | Same content, Rust build/install commands. |

## Tests

- Port every Go test's behaviour, table-driven with `#[test]`/`#[tokio::test]`. The plan maps Go test file → Rust test module, so nothing is silently dropped. Rough count: ~6.1k lines of Go tests.
- Fakes: `wiremock` for RunPod/Vast/HF/GitHub HTTP. Fake llama-server/nvidia-smi scripts in a temp dir. Helper child processes for supervisor/kill tests.
- TUI + help output: `insta` snapshots, from today's goldens.
- `lobo-proto`: JSON round-trip tests against fixtures captured from the Go build (status, up events, models, manifest), so the wire format is proven unchanged.
- App: vitest for UI state mapping. The render route + playwright-cli screenshots of every state go to the user for review.
- Live (orchestrator only, P6): one RunPod boot, one Vast boot, one local boot, `lobo test` on each, and the e2e suite on one of them.

## Failure modes & decisions

| Failure / risk | Behaviour / decision |
|---|---|
| Big-bang leaves the branch unusable for weeks | Master keeps the working Go build. The branch merges only at P6. `git merge master` into it every few days (merge-hygiene rule) |
| Wire format drift breaks old pods / old state files | Fixture round-trip tests from Go output (P1). Unknown JSON fields are ignored, not rejected |
| Rust agent differs on a live pod | P2 ends with a real pod boot per provider before P3 builds on it |
| musl static binary needs glibc-only bits | Nothing in the agent needs glibc. It is a build error if something does, never a runtime surprise |
| Tauri tray panel positioning on a notched Mac | `tauri-plugin-positioner` TrayCenter. Also opens as a window on app launch/reopen |
| App needs the supervisor without a CLI | The app binary re-execs itself with `--lobo-local-run`. Same code as `lobo local run` |
| brew/goreleaser Rust support | Plan step: try the goreleaser rust builder first, fall back to cargo-dist. Same tap and formula name |

## Out of scope

- Local performance (5.4 tok/s) and Metal tuning.
- Code signing / notarization of the app.
- Windows/Linux desktop app.
- Any new user-facing feature. Changes beyond parity need their own spec.

## Rollout

- **P1** workspace, `lobo-proto` + fixtures, CI builds Rust (fmt, clippy, test). Plan: `plan-p1…`.
- **P2** `lobo-agent` lib + bin, Dockerfile, image CI. Live: one RunPod + one Vast boot with the Rust agent (+ `lobo test` from the Go CLI, since the API is unchanged).
- **P3** `lobo-core` (config, providers, control, local supervisor, release).
- **P4** `lobo-cli` parity incl. TUI + wizard + release + brew build.
- **P5** Tauri app (renders reviewed by you).
- **P6** live cutover checks (RunPod, Vast, local, e2e), then merge `feat/rust` → master, Go/Swift gone.

Each phase gets its own plan file in this dir (one spec, several plans, because a single plan for all of it would be too big to review).

## Execution update — 2026-09-29

The user authorized implementation, commit, merge, push and release in full auto after the plan is corrected. `execution.md` identifies the active plans, decisions and remaining checks. This supersedes the parked status and old per-phase approval waits.

Keep Go extra-argument behavior. Use shared CLI validation for app readiness (invalid bucket URL shows SETUP). First Rust release: v0.2.0. Add hidden release --no-promote for immutable candidate uploads; shared latest stays unchanged until cutover release verification.

Parity exceptions: parser/help layout, OS error wording, JSON escapes and terminal log colors may differ as listed in P4. Wizard uses sequential prompts and has no Shift+Tab back navigation. Preserve existing command outcomes. Cancellation cleanup is an intentional fix.

Full-auto delivery uses required CI and smoke tests first, then QA notification, then full acceptance tests and final reporting. Keep the one final Codex code review; no extra per-phase model review.

## As-built P1

Completed on feat/rust at 5dfe9ab. Rust CI 36557109125 passed. The protocol library has 35 passing tests, 15 JSON fixtures, a Go-generated model catalog and 22 generated TypeScript types. No Rust agent, CLI or desktop application ships in P1. Full rewrite acceptance remains pending.
