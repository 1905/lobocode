# Rust rewrite — full plan bundle for review (v1.0)

This file concatenates: spec.md, contracts.md, plan-p1 … plan-p6 (all v1.0). Review the WHOLE set as one plan: a Go+Swift → Rust (+Tauri) big-bang rewrite on branch `feat/rust`, merged at P6. Repo: /Users/kass/dev/lobocode — read the Go/Swift source to check claims.

Focus: crit/high gaps — wrong behaviour ports, wire/file-format breaks, phase-order breaks (a phase needing something a later phase builds), cross-phase name/signature mismatches, missing tests for live-learned rules, tasks that cannot pass as written, CI that breaks, secret leaks, money-bearing steps given to implementers.

Known open items (already on the fix list — do not spend findings on these, but DO say if the proposed direction is wrong):
1. `macos/` removal: P5 removes it after render approval; P6 also lists it. Spec file table says "removed in the phase that replaces each part", P4/P6 keep Go until P6.
2. contracts.md v1.0 is behind the plans. Additions proposed in P2 (`api::router` takes `bytes::Bytes` version, selfkill module in lobo-agent), P3 (`deps_from_config(cfg, &Wiring)`, `Spawner::app` prefix `[--lobo-local-run, local, run]`, pre-checks + testkit in core), P4 (Error messages byte-identical, `UpOpts: Default`), P5 (`UpRequest`, `Readiness`, `resolve_up`, `is_supervisor` accepts app argv, state-event API) will be folded into contracts v1.1.
3. P1 `rust.yml` runs `cargo test --workspace` on ubuntu; P5 Tauri crate breaks it (webkit2gtk). P5 proposes exclude + macOS job.
4. `UpEvent.err` is a string; typed up-error kind for the app is undecided.
5. Spec parity baseline `5443667` is stale (6a72092 on branch fix/app-silent changed status TUI + app).

---



---
<!-- end of review-header.md -->

# Rust rewrite: one core, CLI, pod agent and Tauri app

**Date:** 2026-09-29
**Scope:** /Users/kass/dev/lobocode
**Status:** parked (user, 2026-09-29: "park spec, first fix what we have now")

## TL;DR

**P1 — Rust workspace + shared types.** What: a cargo workspace replaces the Go module on branch `feat/rust`. `lobo-proto` holds every type that crosses a boundary (pod `/api/*` JSON, `up` events, model catalog, release manifest). TypeScript types for the app are generated from it. Why: today Go, Swift and the pod share only hand-copied JSON shapes. You do: nothing. Does NOT do: any behaviour yet.

**P2 — pod agent in Rust.** What: `lobo-agent` (runner, watchdog, download, metrics, `/api`) as a library plus a static Linux binary. The baked image `ghcr.io/1905/lobocode` builds it. Why: you chose "everything in Rust". You do: allow one live pod boot per provider to prove it (a few $). Does NOT do: change the pod API shape or the bootstrap contract.

**P3 — `lobo-core`: config, providers, control.** What: config file (same `config.env` keys), RunPod, Vast, local (Mac supervisor on the same agent library), up/status/down/test/logs, release upload. Why: this is the code the app currently reaches through the CLI. You do: nothing. Does NOT do: any UI.

**P4 — `lobo` CLI in Rust.** What: same commands, flags, output and exit codes as today. That includes the `up` progress view, the live `status` dashboard (ratatui), `lobo config` wizard, `--json` everywhere, and `release`. brew formula from the Rust build. Why: parity before merge (your pick). You do: nothing. Does NOT do: new commands.

**P5 — Tauri app.** What: same shape as today: menu bar item + small panel, Settings window. It calls `lobo-core` directly, with no CLI subprocess. Opening it from Finder also shows the panel, which fixes the item hidden behind the notch. Why: the CLI dependency is the thing you dislike. You do: review PNG renders of every panel state before merge. Does NOT do: Windows/Linux builds.

**P6 — cutover.** What: live checks on a RunPod pod, a Vast instance and local. The e2e suite runs. Then merge `feat/rust` to master, and Go + Swift are gone. Why: you chose a big-bang replace. Master keeps the working Go build until this merge. You do: approve the merge after the live report. Does NOT do: keep Go around afterwards.

## Problem(s)

1. The mac app has no logic of its own. Every action shells out to the bundled `lobo` and parses its JSON (`macos/Sources/Lobocode/CLI.swift:13` "this is the only door", `Store.swift:170` `upArgs`). So app features need CLI features, errors arrive as stderr text, and the app breaks when the binary is missing or stale.
2. Three languages carry one model. Go types (`internal/agent/status.go`, `internal/control/events.go`, `internal/release/manifest.go`) are copied by hand into Swift (`macos/Sources/Lobocode/Models.swift`). Every field change is edited twice and has already drifted. Example: port rules exist in `config.ParseLocalPort` and again in `Store.swift`/`SettingsView.swift`.
3. The app cannot be found when the menu bar is full. The notch hides the item, and opening the app from Finder shows nothing (seen live 2026-09-29).

## Goals

1. One Rust workspace. All laptop logic lives in `lobo-core`, which the CLI and the app both link (P1, P3, P5).
2. Boundary types live once in `lobo-proto`. The TypeScript types are generated from it and never hand-written (P1).
3. The pod agent and the local supervisor share one agent library (P2).
4. Full feature parity with the Go/Swift build as of `5443667` (P4, P5, P6). The checklist is in "Parity".
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
                (llama /metrics, nvidia-smi, /proc, macOS sysctl/ps), /api server (axum), process helpers,
                CleanEnv, LlamaArgs.  bin: lobo-agent (x86_64-unknown-linux-musl, static).
  lobo-core/    config (dotenv read/write keeping comments + Layout), providers (runpod REST+GraphQL, vast,
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
CLI:  lobo up ─► lobo-core::control::up(opts, sink) ─► provider.rent ─► poll agent /api/status ─► events → ratatui | --json
App:  panel START ─► tauri command up(opts) ─► same lobo-core::control::up(opts, sink=tauri event channel) ─► Svelte UI
Local supervisor: CLI `lobo local run …` | app `lobocode --lobo-local-run …` ─► lobo-core::local::supervise ─► lobo-agent Runner (Mac hooks)
Pod:  bootstrap.sh ─► /lobo/lobo-agent (Rust, baked image or release zip) ─► Runner (pod hooks)
```

The app never parses CLI output. Errors are typed (`lobo_core::Error`) and reach the UI as `{kind, message}`.

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
| `cmd/`, `internal/`, `e2e/`, `go.mod`, `go.sum`, `macos/` | Removed on the branch (moved, then `git add -A`), in the phase that replaces each part. |
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


---
<!-- end of spec.md -->

# Rust rewrite — cross-phase contracts v1.0

**Date:** 2026-09-29
**Status:** draft
**Spec:** ./spec.md

Public API one phase exposes and a later phase calls. Every phase plan uses these names and signatures exactly. A plan that needs more public API lists it under "Contract additions" at its end; the orchestrator folds it in here as a new version.

Conventions (all crates):
- Async: tokio. Cancellation: `tokio_util::sync::CancellationToken`. Logging: `tracing`.
- Errors: each crate has `pub enum Error` (thiserror) + `pub type Result<T> = std::result::Result<T, Error>`. No `anyhow` in libraries. `lobo-cli` may use `anyhow` at the top level.
- Wire types come from `lobo-proto` (P1). No crate redeclares one.
- Clock and HTTP are injected for tests: `Arc<dyn Clock>` (`fn now(&self) -> DateTime<Utc>`), base URLs as fields, `reqwest::Client` passed in.

## lobo-agent (P2) — used by the pod binary (P2) and the local supervisor (P3)

```rust
pub struct Deps {                                   // mirrors internal/agent/runner.go Deps
    pub tunnel:  Arc<dyn Tunnel>,                   // start(boot, life) -> Result<oneshot::Receiver<Result<()>>>
    pub gpu_check: Arc<dyn GpuCheck>,               // check(cancel) -> Result<()>
    pub download: Arc<dyn Download>,                // run(cancel, on_progress: &(dyn Fn(DownloadProgress) + Sync)) -> Result<()>
    pub llama:   Arc<dyn Llama>,                    // start() -> Result<oneshot::Receiver<Result<()>>>; wait_healthy(cancel) -> Result<()>
    pub metrics: Arc<dyn Metrics>,                  // llama(), gpu(), host() -> Result<lobo_proto::{Llama,Gpu,Host}>
    pub killer:  Arc<dyn Killer>,                   // kill_self(cancel) -> Result<()>
}
pub struct RunnerConfig { pub boot_id: String, pub timings: Timings, pub model: String, pub ctx: i64,
    pub idle: Duration, pub expires_at: DateTime<Utc>, pub boot_timeout: Duration, pub tick: Duration, pub fail_grace: Duration }
pub struct Runner;                                  // Arc-shared
impl Runner { pub fn new(d: Deps, cfg: RunnerConfig) -> Arc<Runner>;
              pub fn status(&self) -> lobo_proto::Status;
              pub async fn run(self: Arc<Self>, cancel: CancellationToken) -> Result<()>; }
pub mod api    { pub fn router(runner: Arc<Runner>, api_key: String, logs: LogSource, version: Manifest) -> axum::Router; }
pub mod source { pub trait Source: Send + Sync { async fn open(&self, offset: i64, len: i64) -> Result<(BoxRead, i64)>; }
                 pub struct HttpSource { pub url: String }  pub struct SshSource { user, addr, file, key, host_key }
                 pub fn redact_url(raw: &str) -> String; }
pub mod download { pub async fn download(cancel, src: &dyn Source, dst: &Path, sha: &str, on_progress) -> Result<()>;
                   pub async fn download_parallel(cancel, src, dst, size: i64, sha: &str, chunk_sha: &[String], conns: usize, on_progress) -> Result<()>;
                   pub async fn hash_file(cancel, path, total: i64, on_progress) -> Result<String>;
                   pub async fn bench(cancel, src, size: i64, conns: usize, dur: Duration) -> Result<(i64, f64)>; }
pub mod watchdog { pub struct State; pub struct Config; pub enum Decision; }   // port of internal/watchdog
pub mod metrics  { pub struct Collector { pub llama_url: String, .. } }        // nvidia-smi, /proc, macOS sysctl/ps
pub mod process  { pub struct CleanEnv; pub fn clean_env() -> Vec<(String,String)>;   // secret-stripping env for children
                   pub struct LlamaArgs { .. } impl LlamaArgs { pub fn to_args(&self) -> Vec<String>; } }
```

## lobo-core (P3) — used by lobo-cli (P4) and the Tauri app (P5)

```rust
pub mod config {
    pub struct Laptop { /* one field per internal/config/laptop.go key, same env names */ pub r2: R2Creds }
    pub fn default_path() -> PathBuf;                              // ~/.config/lobo/config.env
    pub fn load_laptop(path: &Path) -> Result<Laptop>;             // file only, never the OS env
    pub fn values(path: &Path) -> Result<BTreeMap<String,String>>;
    pub fn save(path: &Path, set: &BTreeMap<String,String>) -> Result<()>;   // keeps comments + layout
    pub fn show(path: &Path) -> Result<lobo_proto::ConfigShow>;
    pub fn parse_local_port(v: &str) -> Result<u16>;
    impl Laptop { pub fn require_cloud(&self) -> Result<()>; pub fn require_provider_key(&self) -> Result<()>;
                  pub fn require_bucket(&self) -> Result<()>; pub fn require_r2(&self) -> Result<()>;
                  pub fn secret_values(&self) -> BTreeMap<String,String>; pub fn defaults(&self) -> Result<Defaults>;
                  pub fn weights(&self) -> PathBuf; pub fn port(&self) -> u16;
                  pub fn providers(&self) -> Vec<String>; pub fn default_provider(&self) -> String; }
}
pub mod provider {
    pub struct CreateOpts { /* internal/provider/provider.go CreateOpts, snake_case */ }
    #[async_trait] pub trait Provider: Send + Sync {
        fn name(&self) -> &'static str; fn replaceable(&self) -> bool;
        async fn rent(&self, o: &CreateOpts, note: &(dyn Fn(String) + Sync)) -> Result<Instance>;
        async fn list(&self) -> Result<Vec<Instance>>; async fn get(&self, id: &str) -> Result<Instance>;
        async fn delete(&self, id: &str) -> Result<()>; }
    pub mod runpod; pub mod vast; pub mod local;                   // Error::NoCapacity, Error::NotFound
}
pub mod control {
    pub struct UpOpts { /* internal/control/events.go UpOpts, snake_case, Duration fields */ }
    pub struct Deps { pub providers: BTreeMap<String, Arc<dyn Provider>>, pub releases: Arc<dyn ReleaseResolver>,
                      pub presign: Option<Arc<dyn Presigner>>, pub new_agent: Arc<dyn Fn(&str) -> Arc<dyn AgentApi> + Send + Sync>,
                      pub cfg: Laptop, pub clock: Arc<dyn Clock>, pub poll: Duration }
    pub fn deps_from_config(cfg: Laptop) -> Result<Deps>;           // the real wiring; CLI and app both call it
    pub fn up(d: Deps, o: UpOpts, cancel: CancellationToken) -> tokio::sync::mpsc::Receiver<lobo_proto::UpEvent>;
    pub async fn snapshot(d: &Deps) -> Result<lobo_proto::Snap>;
    pub async fn down(d: &Deps) -> Result<f64>;                     // spent USD
    pub async fn target(d: &Deps) -> Result<(Arc<dyn AgentApi>, String)>;
    #[async_trait] pub trait AgentApi: Send + Sync { async fn status(&self) -> Result<Status>;
        async fn version(&self) -> Result<Manifest>; async fn logs(&self, n: usize) -> Result<String>; }
    pub struct HttpAgent;  impl HttpAgent { pub fn new(base: &str, key: &str) -> Self; }
}
pub mod checks  { pub async fn chat(..) -> Result<String>; pub async fn tool_call(..) -> Result<Vec<u8>>; pub fn validate_tool_call(body: &[u8]) -> Result<()>; }
pub mod release { pub fn next_version(existing: &[String], today: DateTime<Utc>) -> String; pub fn zip_key(v: &str) -> String;
                  pub fn meta_key(v: &str) -> String; pub const LATEST_KEY: &str;
                  pub fn build_zip(agent_bin: &Path, m: &Manifest, out: &Path) -> Result<String>;
                  pub fn scan_for_secrets(zip: &Path, secrets: &BTreeMap<String,String>) -> Result<()>;
                  pub struct Store; impl Store { pub fn new(r2: &R2Creds) -> Result<Self>; publish, list_release_keys, presign_get }
                  pub async fn resolve(hc: &reqwest::Client, bucket_url: &str, version: &str) -> Result<Resolved>; }
pub mod bootstrap { pub fn script(provider: &str) -> String; pub fn env(o: &CreateOpts, provider: &str) -> BTreeMap<String,String>; }
pub mod local {
    pub fn supported() -> Result<()>; pub fn usable_mib() -> Result<i64>;
    pub fn list(weights: &Path) -> Result<lobo_proto::Listing>;
    pub async fn ensure_runtime(weights: &Path, note: &(dyn Fn(String)+Sync)) -> Result<PathBuf>;
    pub fn state_path() -> PathBuf; pub fn read_state() -> Result<Option<LocalState>>;
    pub fn claim_state(s: &LocalState) -> Result<()>; pub fn remove_state_if(pid: i32, boot_id: &str) -> Result<()>;
    pub struct RunConfig { .. }                                    // = today's `lobo local run` flags
    pub async fn supervise(cfg: RunConfig, cancel: CancellationToken) -> Result<()>;   // uses lobo_agent::Runner
    pub const SUPERVISOR_ARG: &str = "--lobo-local-run";           // app re-exec marker (P5)
    pub struct Spawner { pub exe: PathBuf, pub args_prefix: Vec<String> } // how local::Provider starts the supervisor:
                                                                   // CLI = (lobo, ["local","run"]); app = (self, [SUPERVISOR_ARG])
}
pub mod genkey { pub fn opencode_config(..) -> Result<String>; }
pub mod error  { impl Error { pub fn kind(&self) -> &'static str; } }   // app maps to {kind, message}
```

## lobo-cli (P4)

Binary `lobo`. Commands, flags, output and exit codes = Go `cmd/lobo` at `5443667`. No public API.

## app (P5)

Tauri commands (Rust → TS via ts-rs types from lobo-proto + `AppError { kind, message }`):
`config_show() -> ConfigShow`, `config_save(set: BTreeMap<String,String>)`, `snapshot() -> Snap`, `up(opts: UpRequest)` (streams `UpEvent` on event `lobo://up`), `cancel_up()`, `down() -> f64`, `local_models() -> Listing`, `catalog() -> Vec<Model>`.
`UpRequest { provider, model, ctx, source, cloud }` lives in `lobo-proto` (added by P5 via "Contract additions").


---
<!-- end of contracts.md -->

# Rust rewrite P1 — workspace + `lobo-proto` Implementation Plan v1.0

**Date:** 2026-09-29
**Status:** draft
**Spec:** ./spec.md (spec status: parked — this plan is written on request; exec still needs spec + plan approval)
**Phase:** P1 of 6. P2–P6 get their own plan files (`plan-p2-v1.0.md` …) when their phase starts.

**Goal:** a cargo workspace on `feat/rust` whose `lobo-proto` crate holds every type that crosses a boundary, proven byte-compatible with the Go build by fixture round-trip tests, with TypeScript types generated from it.

**Architecture:** Go stays untouched and keeps building. A small Go program dumps fixed sample values of every wire type to JSON fixtures. `lobo-proto` decodes each fixture, re-encodes it, and must produce the same JSON (numbers compared numerically, times compared as exact strings). `ts-rs` exports the same types to `app/ui/src/proto/`. CI runs both the Rust checks and a "Go dumper output == committed fixtures" check.

**Tech Stack:** Rust 1.98 (edition 2024), serde + serde_json, chrono, ts-rs (chrono-impl), thiserror. Versions pinned by `cargo add` in Task 1 and written into this file's "Pinned versions" line.

> For agentic workers: use superpowers:subagent-driven-development to implement task-by-task. Checkbox syntax for tracking.

> Implementer scope (verbatim, every dispatch): writes only the code and unit tests its task names and runs that task's focused unit test. Never runs e2e / integration / live / smoke tests, never rents a GPU or pod, never calls a provider API, never publishes, deploys or touches infra, never runs anything money-bearing. Never sets `TEST_DATABASE_URL` or any test-DB env var.

---

## File map

**Create**
- `Cargo.toml` — workspace: `members = ["crates/*"]`, `[workspace.package]` (edition 2024, rust-version 1.98), `[workspace.dependencies]`, `[workspace.lints]` (clippy `all = warn`, promoted to errors by `-D warnings` in CI).
- `rust-toolchain.toml` — channel `1.98`, components rustfmt + clippy.
- `.cargo/config.toml` — `[env] TS_RS_EXPORT_DIR = { value = "app/ui/src/proto", relative = true }`.
- `crates/lobo-proto/Cargo.toml`
- `crates/lobo-proto/src/lib.rs` — module list + re-exports.
- `crates/lobo-proto/src/gotime.rs` — `GoTime`.
- `crates/lobo-proto/src/agent.rs` — `Stage`, `DownloadProgress`, `Gpu`, `Host`, `Llama`, `Timings`, `Status`.
- `crates/lobo-proto/src/release.rs` — `ModelRef`, `Defaults`, `Manifest`, `Resolved`, pins.
- `crates/lobo-proto/src/control.rs` — `Instance`, `ReadyInfo`, `UpEvent`, `Snap`.
- `crates/lobo-proto/src/local.rs` — `ModelState`, `RuntimeInfo`, `Listing`, `LocalState`.
- `crates/lobo-proto/src/config.rs` — `ConfigShow`.
- `crates/lobo-proto/src/catalog.rs` — `Model`, `CHUNK_SIZE`, `get`, `all`, `min_free_mib`.
- `crates/lobo-proto/src/testutil.rs` — `#[cfg(test)]` `assert_json_eq`, `fixture`.
- `crates/lobo-proto/fixtures/*.json` — generated by the Go dumper (list in Task 2) + `boots_ready_legacy.json` (hand-copied, redacted).
- `crates/lobo-proto/catalog.json` — generated by the Go dumper; embedded by `catalog.rs`.
- `app/ui/src/proto/*.ts` — generated by ts-rs; committed.
- `tools/protofixtures/main.go` — Go fixture dumper (lives in the Go module; removed with Go at P6).
- `.github/workflows/rust.yml`

**Modify**
- `Makefile` — add `rust-build`, `rust-test`, `rust-lint`, `proto-fixtures`, `proto-ts`. Go targets unchanged.
- `.gitignore` — add `target/`.
- `plans/2026-09-29-rust-rewrite/spec.md` — file table: add `tools/protofixtures/` row; status line.

**Out of scope for P1**
- `lobo-agent`, `lobo-core`, `lobo-cli`, `lobo-e2e`, `app/src-tauri`, the Svelte app itself (only its `src/proto/` folder is created).
- Any change to Go code outside `tools/protofixtures/`, to `docker/`, `pod-image.yml`, `release.yml`, `.goreleaser.yaml`.
- `release::NextVersion`, `ZipKey`, `MetaKey` — logic, not wire types → P3 (`lobo-core::release`).

---

## Wire rules (locked; every type task obeys these)

1. Field names come from the Go `json:"…"` tags, via `#[serde(rename = …)]` or `rename_all = "snake_case"` where it matches. Rust field names are snake_case of the Go field (`cost_per_hr`, `elapsed_ns`, `usd_per_h` → field `cost_per_hr` with rename where the tag differs).
2. Go `omitempty` → `#[serde(default, skip_serializing_if = …)]` (`String::is_empty`, `Option::is_none`, `std::ops::Not::not` for bool). A field WITHOUT `omitempty` is always written, even when zero/empty.
3. Go pointer (`*T`) → `Option<T>`, serialised as `null` when `None` unless the Go tag has `omitempty`.
4. Every struct gets `#[serde(default)]` on decode, so a missing field is zero, and unknown fields are ignored (no `deny_unknown_fields` anywhere).
5. Go `int`/`int64` → `i64`; Go `uint64` → `u64`; Go `float64` → `f64`. Every `i64`/`u64` field carries `#[ts(type = "number")]` (ts-rs would emit `bigint`, which JSON.parse never produces).
6. Go `time.Time` → `GoTime` (Task 3). Go zero time ↔ `"0001-01-01T00:00:00Z"`. `time.Duration` with tag `elapsed_ns` → `i64` nanoseconds.
7. Go `map[string]T` → `BTreeMap<String, T>` (Go sorts map keys on encode).
8. Go slices that can be `nil` → `Vec<T>` with `#[serde(default, deserialize_with = "crate::null_as_empty")]`. Encode writes `[]`. `assert_json_eq` treats `null` and `[]` as equal for arrays (documented in the helper).
9. Go `Stage` string enum → Rust enum with `#[serde(rename_all = "lowercase")]` plus `#[serde(other)] Unknown`, so a newer agent's stage never fails a decode.
10. Derives on every wire type: `Debug, Clone, PartialEq, Default, Serialize, Deserialize, TS` + `#[ts(export)]`.

## Pinned versions

_(Task 1 fills this line from `Cargo.lock`: serde, serde_json, chrono, ts-rs, thiserror.)_

---

## Task 0 — Preconditions + branch (orchestrator, no implementer)

- [ ] ⚠ `fix/app-silent` (6a72092) is not on master. It must merge first (merge-hygiene rule), or `feat/rust` forks from a master that lacks it. User decides.
- [ ] `git status` clean, on `master`, `git pull`.
- [ ] `git checkout -b feat/rust master`.
- [ ] Baseline green: `go build ./... && go test ./...` → `ok` for every package. Record the package count.
- [ ] `rustc --version` → `1.98.x`. `cargo tauri --version` present (not used in P1, checked for P5).
- [ ] Update `spec.md`: status line `approved (P1 in progress)` (only after the user approves), add file-table row `tools/protofixtures/ | Go fixture dumper for lobo-proto round-trip tests. Removed with Go at P6.`
- [ ] Commit: `plans: rust P1 plan; spec file table adds tools/protofixtures`.

## Task 1 — Workspace skeleton

**Files:** Create `Cargo.toml`, `rust-toolchain.toml`, `.cargo/config.toml`, `crates/lobo-proto/Cargo.toml`, `crates/lobo-proto/src/lib.rs`. Modify `.gitignore`, `Makefile`.

- [ ] `cargo new --lib crates/lobo-proto --vcs none`, then convert to workspace member (`edition.workspace = true`, `rust-version.workspace = true`, `[lints] workspace = true`).
- [ ] `cargo add -p lobo-proto serde --features derive`, `serde_json`, `chrono --no-default-features --features std,serde`, `ts-rs --features chrono-impl,serde-json-impl`, `thiserror`. Move the versions into `[workspace.dependencies]`; the crate uses `x.workspace = true`.
- [ ] `lib.rs`: `//! Types that cross a process boundary: pod /api JSON, `lobo up --json` lines, release manifests, local state files, the model catalog. Wire format = the Go build's, proven by fixtures/.` and nothing else yet.
- [ ] Makefile targets (Go targets untouched):
  - `rust-build: cargo build --workspace`
  - `rust-test: cargo test --workspace`
  - `rust-lint: cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings`
- [ ] `.gitignore`: `target/`.
- [ ] Verify: `make rust-lint rust-test` → exit 0, `running 0 tests`.
- [ ] Fill "Pinned versions" above from `Cargo.lock`.
- [ ] Commit: `rust: cargo workspace + empty lobo-proto crate`.

## Task 2 — Go fixture dumper

**Files:** Create `tools/protofixtures/main.go`, `crates/lobo-proto/fixtures/*.json`, `crates/lobo-proto/catalog.json`, `crates/lobo-proto/fixtures/boots_ready_legacy.json`. Modify `Makefile`.

Interface: `go run ./tools/protofixtures <outdir> <catalog.json path>`. It builds fixed values in Go (no clock, no randomness), encodes each with `json.MarshalIndent(v, "", "  ")` + `\n`, and writes one file per fixture. Two runs → identical bytes.

Fixtures (file → Go value; field values chosen so every field is non-zero unless the case is about zero):

| File | Go type | Case |
|---|---|---|
| `status_ready.json` | `agent.Status` | stage ready, every section set: GPU, Host, Llama, timings all non-zero, `boot_id` set, `expires_at` with fractional seconds `.5` (Go trims to `.5Z`) |
| `status_booting.json` | `agent.Status` | stage download, `GPU/Host/Llama` nil → `null`, `boot_id` "" (omitted), `download.verifying` true, `download.source` set, `timings` all zero → zero times `0001-01-01T00:00:00Z` |
| `status_offset_time.json` | `agent.Status` | `expires_at` in a `+03:00` zone with nanosecond precision `.123456789` |
| `up_event_progress.json` | CLI `--json` line (anonymous struct from `cmd/lobo/main.go:293`) | phase download, detail, download set, no ready, no err |
| `up_event_ready.json` | same | phase ready, `ready` with `timings` set, `done` true, `elapsed_ns` 123456789012 |
| `up_event_error.json` | same | phase failed, `err` "no host" , `done` true |
| `snap_running.json` | `control.Snap` | pod (Instance with APIURL/AgentURL set — must NOT appear), version, status set |
| `snap_down.json` | `control.Snap` | pod nil, version nil, status nil, `down` true |
| `manifest.json` | `release.Manifest` | all fields, `git_dirty` true |
| `resolved.json` | `release.Resolved` | wraps the manifest above |
| `listing.json` | `local.Listing` | two models (one verified), runtime present |
| `listing_empty.json` | `local.Listing` | `Models` nil → `null`, runtime absent |
| `local_state.json` | `local.State` | all fields, `started_at` in local `+02:00` zone |
| `config_show.json` | anonymous struct from `cmd/lobo/config.go:156` (copy it into the dumper as a named struct with identical tags) | 3 keys in values, 3 in set, unsorted insert order |

`catalog.json` (separate path, not a fixture): `[{"id","file","sha256","alias","size","chunk_sha":[…]}]` for `model.All()` in ID order, `chunk_sha` from `m.ChunkSHA()`. This is the single source the Rust catalog embeds.

`boots_ready_legacy.json`: hand-copy the `ready` object of the FIRST line of the local `boots.jsonl` (gitignored, has the legacy fields `cloud`, no `provider`). Replace the R2 host in `download_source` with `https://example.r2.cloudflarestorage.com` and the `url` host with `https://lobo.example.com`. Purpose: prove old records with unknown/missing fields still decode.

- [ ] Write the dumper. It imports `internal/agent`, `internal/control`, `internal/metrics`, `internal/release`, `internal/local`, `internal/model`, `internal/provider`. For the two CLI-local anonymous structs, the dumper redeclares them with the same tags and a comment `// mirror of cmd/lobo/main.go:293 — keep tags identical`.
- [ ] Makefile: `proto-fixtures: go run ./tools/protofixtures crates/lobo-proto/fixtures crates/lobo-proto/catalog.json`.
- [ ] Verify: `make proto-fixtures && make proto-fixtures && git status --porcelain crates/` → only new files, second run changes nothing. `go vet ./tools/...` clean. `go test ./...` still green.
- [ ] Eyeball checks (orchestrator): `status_booting.json` has `"gpu": null` and no `boot_id` key; `snap_running.json` has no `APIURL`/`api_url`; `status_ready.json` has `"expires_at": "…T…:…:….5Z"`.
- [ ] Commit: `rust: Go fixture dumper + lobo-proto fixtures`.

## Task 3 — `GoTime` + test helpers

**Files:** Create `crates/lobo-proto/src/gotime.rs`, `crates/lobo-proto/src/testutil.rs`. Modify `lib.rs`.

Locked interface:
```rust
pub struct GoTime(pub Option<chrono::DateTime<chrono::FixedOffset>>); // None = Go zero time
impl GoTime { pub const ZERO: GoTime; pub fn is_zero(&self) -> bool; pub fn from_utc(t: DateTime<Utc>) -> GoTime; }
// Serialize: None → "0001-01-01T00:00:00Z"; Some → RFC 3339, "Z" for +00:00, fractional seconds with trailing zeros trimmed (Go RFC3339Nano).
// Deserialize: any RFC 3339 string; "0001-01-01T00:00:00Z" → None. TS type: string.
pub(crate) fn null_as_empty<'de, D, T>(d: D) -> Result<Vec<T>, D::Error>; // in lib.rs
```
`testutil` (cfg(test)):
```rust
pub fn fixture(name: &str) -> serde_json::Value;          // reads fixtures/<name>
pub fn assert_json_eq(got: &Value, want: &Value);         // numbers compared as f64; null == [] for arrays; objects by key; panics with the JSON path of the first difference
pub fn round_trip<T: Serialize + DeserializeOwned>(name: &str) -> T; // decode fixture → T → encode → assert_json_eq with fixture; returns T
```

- [ ] Failing tests first (`gotime.rs` `mod tests`, table-driven): `"0001-01-01T00:00:00Z"` → zero and back; `"2026-09-29T10:00:00.5Z"` round-trips exactly; `.123456789+03:00` round-trips exactly; `.100Z` input re-encodes as `.1Z`; whole seconds encode with no fraction; garbage string → decode error.
- [ ] `testutil` self-tests: `3` vs `3.0` equal; `null` vs `[]` equal; `{"a":1}` vs `{"a":2}` panics with path `a`.
- [ ] Implement. `cargo test -p lobo-proto gotime testutil` → all pass.
- [ ] Commit: `lobo-proto: GoTime (Go zero time + RFC3339Nano) and fixture helpers`.

## Task 4 — agent types

**Files:** Create `crates/lobo-proto/src/agent.rs`. Modify `lib.rs`.

Types (fields = Go source, rules above): `Stage { Boot, Tunnel, Gpu, Verify, Download, Load, Ready, Failed, Terminating, Unknown }` (`Gpu` renames to `"gpu"`), `DownloadProgress` (`internal/agent/download.go:14`), `Gpu`, `Host`, `Llama` (`internal/metrics`), `Timings`, `Status` (`internal/agent/status.go`). `Stage::default()` = `Boot`.

- [ ] Failing tests: `round_trip::<Status>` for `status_ready.json`, `status_booting.json`, `status_offset_time.json`. Plus: `{"stage":"warp"}` decodes to `Stage::Unknown`; `{}` decodes to `Status::default()`; `status_booting` re-encoded has no `boot_id` key and has `"gpu": null`.
- [ ] Implement. `cargo test -p lobo-proto agent` → pass.
- [ ] Commit: `lobo-proto: agent /api/status types`.

## Task 5 — release types

**Files:** Create `crates/lobo-proto/src/release.rs`. Modify `lib.rs`.

Types: `ModelRef`, `Defaults`, `Manifest`, `Resolved` (`internal/release/manifest.go`). Consts: `DEFAULT_LLAMA_IMAGE = "ghcr.io/ggml-org/llama.cpp:server-cuda-b11118"`, `DEFAULT_MODEL = "q8"`, `DEFAULT_DEFAULTS: Defaults = { ctx: 65536, idle_min: 30, max_hours: 12 }`.

- [ ] Failing tests: `round_trip::<Manifest>("manifest.json")`, `round_trip::<Resolved>("resolved.json")`; a test that reads `internal/release/manifest.go` (`include_str!("../../../internal/release/manifest.go")`) and asserts it contains `DefaultLlamaImage = "<DEFAULT_LLAMA_IMAGE>"` — a drift guard, deleted at P6 with the Go file.
- [ ] Implement. `cargo test -p lobo-proto release` → pass.
- [ ] Commit: `lobo-proto: release manifest types and pins`.

## Task 6 — control types

**Files:** Create `crates/lobo-proto/src/control.rs`. Modify `lib.rs`.

Types:
- `Instance` (`internal/provider/provider.go:19`) — wire fields + `#[serde(skip)] #[ts(skip)] pub api_url: String, pub agent_url: String`.
- `ReadyInfo` (`internal/control/events.go`) — `timings: Option<Timings>` omitempty, `elapsed_ns: i64`, `cost_per_hr` renamed `"usd_per_h"`, `rent_s` renamed `"rent_to_container_s"`.
- `UpEvent` — the `lobo up --json` line: `phase`, `detail` (omitempty), `download: Option<DownloadProgress>` (omitempty), `ready: Option<ReadyInfo>` (omitempty), `done: bool` (omitempty), `err: Option<String>` (omitempty). Doc comment: "One `lobo up --json` line; also the event the app receives. `err` set = this event is a failure."
- `Snap` (`internal/control/status.go:17`) — `pod`, `version`, `status` as `Option` (null when None), `down`, `at: GoTime`.

- [ ] Failing tests: `round_trip` on `up_event_progress/ready/error.json`, `snap_running.json`, `snap_down.json`. `snap_running` decode: `api_url` is empty (never on the wire). `boots_ready_legacy.json` decodes into `ReadyInfo` without error, `provider` empty, `timings` present (not round-tripped — legacy shape).
- [ ] Implement. `cargo test -p lobo-proto control` → pass.
- [ ] Commit: `lobo-proto: up events, ready info, status snapshot`.

## Task 7 — local + config types

**Files:** Create `crates/lobo-proto/src/local.rs`, `crates/lobo-proto/src/config.rs`. Modify `lib.rs`.

Types: `ModelState`, `RuntimeInfo { version, present }`, `Listing { weights, free_bytes: u64, models: Vec<ModelState> (null_as_empty), runtime: RuntimeInfo }` (`internal/local/models.go:17-33`), `LocalState` (`internal/local/state.go:15`), `ConfigShow { path, exists, values: BTreeMap<String,String>, set: BTreeMap<String,bool> }` (`cmd/lobo/config.go:156`).

- [ ] Failing tests: `round_trip` on `listing.json`, `listing_empty.json`, `local_state.json`, `config_show.json`. `local_state` re-encode keeps the `+02:00` offset string exactly.
- [ ] Implement. `cargo test -p lobo-proto local config` → pass.
- [ ] Commit: `lobo-proto: local listing/state and config show`.

## Task 8 — model catalog

**Files:** Create `crates/lobo-proto/src/catalog.rs`. Modify `lib.rs`.

Locked interface:
```rust
pub const CHUNK_SIZE: i64 = 256 << 20;
pub struct Model { pub id: String, pub file: String, pub sha256: String, pub alias: String, pub size: i64, pub chunk_sha: Vec<String> }
pub fn all() -> &'static [Model];                 // sorted by id; parsed once from include_str!("../catalog.json") via LazyLock
pub fn get(id: &str) -> Result<&'static Model, UnknownModel>;
#[derive(thiserror::Error)] #[error("unknown model {id:?}, valid: {valid}")] pub struct UnknownModel { pub id: String, pub valid: String } // valid = "q6, q8"
impl Model { pub fn url(&self, bucket_url: &str) -> String; }   // trims trailing '/' + "/models/" + file
pub fn min_free_mib(model_bytes: i64) -> i64;     // (bytes >> 20) + 2560
```
`Model` derives `Serialize, Deserialize, TS` too (the app's weights picker lists it).

- [ ] Failing tests — port `internal/model/catalog_test.go` one-to-one: `get` table for q8/q6 (file, alias, size, sha len 64, url with trailing slash bucket); `get("x")` error contains `"q6, q8"`; chunk count = ceil(size / CHUNK_SIZE) for both; `all()` ids `["q6","q8"]`; `min_free_mib(28595762272)` in `(29274, 31602)`.
- [ ] Implement. `cargo test -p lobo-proto catalog` → pass.
- [ ] Commit: `lobo-proto: model catalog from the Go-generated catalog.json`.

## Task 9 — TypeScript export

**Files:** Create `app/ui/src/proto/*.ts` (generated). Modify `Makefile`, every type module (only if a `#[ts]` attribute is missing).

- [ ] Makefile: `proto-ts: cargo test -p lobo-proto export_bindings` (ts-rs generates one `export_bindings_<type>` test per `#[ts(export)]`; the `.cargo/config.toml` env points them at `app/ui/src/proto`).
- [ ] Failing test in `lib.rs`: `ts_exports_have_no_bigint` — reads every `app/ui/src/proto/*.ts` after export and asserts none contains `bigint`; asserts `GoTime.ts` (or the inlined type) is `string`; asserts the file set equals the expected list: `Stage, DownloadProgress, Gpu, Host, Llama, Timings, Status, ModelRef, Defaults, Manifest, Resolved, Instance, ReadyInfo, UpEvent, Snap, ModelState, RuntimeInfo, Listing, LocalState, ConfigShow, Model`. (Order: run `make proto-ts` first, then this test; the test documents that dependency in its doc comment.)
- [ ] Verify: `make proto-ts && cargo test -p lobo-proto ts_exports` → pass. `grep -c api_url app/ui/src/proto/Instance.ts` → `0`.
- [ ] Commit: `lobo-proto: TypeScript types for the app (ts-rs)`.

## Task 10 — CI

**Files:** Create `.github/workflows/rust.yml`.

Jobs (ubuntu-latest, `actions/checkout@v4`, `dtolnay/rust-toolchain` reading `rust-toolchain.toml`, `Swatinem/rust-cache@v2`), on push to `master`, `feat/**` and on PRs, path filter `crates/**`, `Cargo.*`, `app/ui/src/proto/**`, `tools/protofixtures/**`, `internal/**`, `cmd/**`, `.github/workflows/rust.yml`:
1. `make rust-lint`
2. `make rust-test`
3. `make proto-ts && git diff --exit-code app/ui/src/proto` — generated TS is committed and current.
4. `actions/setup-go@v5` (go-version-file `go.mod`), `make proto-fixtures && git diff --exit-code crates/lobo-proto/fixtures crates/lobo-proto/catalog.json` — the Go build and the committed fixtures agree. This is the drift alarm while Go and Rust live side by side.

- [ ] Write the workflow. Validate locally: `actionlint .github/workflows/rust.yml` if installed, else say it was not run.
- [ ] Commit. Push `feat/rust`. `gh run watch` → green. Red → `gh run view --log-failed`, fix, push, repeat.

## Task 11 — Phase close (orchestrator)

- [ ] Full local run: `make rust-lint rust-test proto-ts && git diff --exit-code && go test ./...` → all green, tree clean.
- [ ] Type-consistency check: names in the File map, Wire rules, Tasks 4–9 and the Task 9 expected list are identical (`Gpu` not `GPU`, `UpEvent` not `Event`, `LocalState` not `State`).
- [ ] Reconcile spec: P1 "As-built notes" — fixtures dir, `tools/protofixtures`, `GoTime`, the `null == []` rule.
- [ ] Plan status → `done` (P1). `/notify`: "Rust P1 done on feat/rust: lobo-proto + fixtures + TS types, CI green. Approve P2 plan next."
- [ ] Merge hygiene: `feat/rust` stays open until P6 by design. `git merge master` into it every few days; a Go change to any wire type means re-run `make proto-fixtures` and fix the Rust type in the same merge.

---

## Go test → Rust home (whole rewrite; later phases own their rows)

| Go test file | Lines | Rust home | Phase |
|---|---|---|---|
| internal/model/catalog_test.go | 58 | lobo-proto::catalog | P1 |
| internal/agent/{api,download,env,hang,killer,llama,parallel,proc,runner,source_ssh}_test.go | 1,142 | lobo-agent | P2 |
| internal/metrics/metrics_test.go | 130 | lobo-agent::metrics | P2 |
| internal/watchdog/watchdog_test.go | 74 | lobo-agent::watchdog | P2 |
| cmd/lobo-agent/main_test.go | 62 | lobo-agent bin | P2 |
| internal/bootstrap/bootstrap_test.go | 147 | lobo-core::bootstrap | P3 |
| internal/config/config_test.go | 389 | lobo-core::config | P3 |
| internal/control/control_test.go (+ controltest/fakes.go) | 703 | lobo-core::control | P3 |
| internal/checks/checks_test.go | 50 | lobo-core::checks | P3 |
| internal/release/release_test.go | 117 | lobo-core::release | P3 |
| internal/runpod/runpod_test.go | 174 | lobo-core::provider::runpod | P3 |
| internal/vast/vast_test.go | 276 | lobo-core::provider::vast | P3 |
| internal/local/{deps,models,platform,provider,runtime,state}_test.go | 1,235 | lobo-core::local | P3 |
| cmd/lobo/{defaults,help,local,target}_test.go | 450 | lobo-cli | P4 |
| internal/tui/tui_test.go | 144 | lobo-cli::tui (insta) | P4 |
| internal/configtui/configtui_test.go | 187 | lobo-cli::wizard | P4 |
| e2e/e2e_test.go | 522 | lobo-e2e (#[ignore]) | P6 |

Total 5,860 lines (measured `wc -l`, 2026-09-29). The spec's "~6.1k" was an estimate.

## Later phases (outline only — each gets its own plan)

- **P2** `lobo-agent`: lib + musl bin, Dockerfile rust stage, `pod-image.yml` paths + `DefaultLlamaImage` source moves to `crates/lobo-proto/src/release.rs`. Live: one RunPod + one Vast boot with the Rust agent image, driven by the Go CLI (`lobo up --image …`, `lobo test`).
- **P3** `lobo-core`: config, providers, control, local supervisor, release, bootstrap.
- **P4** `lobo-cli`: clap, ratatui, inquire wizard, `--json`, brew (goreleaser rust builder tried first, cargo-dist fallback).
- **P5** Tauri app + Svelte UI on the P1 TS types; render review.
- **P6** live cutover checks, e2e, the one `/rival-codex review` of the whole branch + `/simplify`, merge, Go/Swift removed.


---
<!-- end of plan-p1-v1.0.md -->

# Rust rewrite P2 — `lobo-agent` Implementation Plan v1.0

**Date:** 2026-09-29
**Status:** draft
**Spec:** ./spec.md (spec status: parked — this plan is written on request; exec still needs spec + plan approval)
**Contracts:** ./contracts.md v1.0 (this plan asks for one contract change and several additions, listed at the end)
**Phase:** P2 of 6. Needs P1 done on `feat/rust` (workspace, `lobo-proto`, `rust.yml`).

**Goal:** a Rust `lobo-agent` (library + static `x86_64-unknown-linux-musl` binary) that does everything the Go pod agent does today, with the same `/api/*` JSON, baked into `ghcr.io/1905/lobocode` by the pod image workflow, and proven on one real RunPod pod and one real Vast instance driven by the unchanged Go CLI.

**Architecture:** one crate `crates/lobo-agent`. The library holds the parts the local supervisor (P3) will reuse: `Runner` + its dependency traits, watchdog, downloader (HTTP + SSH sources), metrics, `/api` router, process helpers, self-terminate clients. The pod wiring (cloudflared, CUDA check, model download policy, llama-server start, env config) lives in `lobo_agent::pod`, so it is unit-tested too. `src/main.rs` is thin: clap, logging, `pod::main_flow`. The Go agent keeps building on the branch until P6. The live check uses `lobo up --image <digest>`, so no Go code changes in P2.

**Tech Stack:** Rust 1.98 (edition 2024), tokio, tokio-util (`CancellationToken`), async-trait, reqwest (rustls, bundled webpki roots, ring, http2, stream), axum, russh (client + in-process test server), sha2, hex, base64, url, bytes, futures-util, regex-lite, thiserror, tracing + tracing-subscriber (json), clap (derive), chrono, serde_json, `lobo-proto`. Dev: wiremock, tempfile, tokio test-util. Local musl build: cargo-zigbuild + zig.

> For agentic workers: use superpowers:subagent-driven-development to implement task-by-task. Checkbox syntax for tracking.

> Implementer scope (verbatim, every dispatch): writes only the code and unit tests its task names and runs that task's focused unit test. Never runs e2e / integration / live / smoke tests, never rents a GPU or pod, never calls a provider API, never publishes, deploys or touches infra, never runs anything money-bearing. Never sets `TEST_DATABASE_URL` or any test-DB env var.

> Extra P2 rules for implementers: never `docker pull` / `docker run` / `docker build` on this Mac (project rule: the laptop is control-only). Never set real `RUNPOD_*`, `CONTAINER_*`, `VASTAI_*` or `LOBO_*` values in the shell; tests build config from maps, not from the process env.

---

## Decisions (locked)

| Question | Decision | Why (one line) |
|---|---|---|
| Go files removed in P2? | **No.** Nothing under `cmd/` or `internal/` is moved or deleted. | `cmd/lobo` still imports `internal/agent` (`agent.Status`, `Stage*`, `Runner`, `Download`, `HashFile`, `FetchFile`, `CleanEnv`, `LlamaArgs`, `StartProcessPID`, `WaitHealthy`), `internal/metrics`, `internal/watchdog`, `internal/config`, `internal/runpod`, `internal/vast`; `lobo release` (Go) still builds `./cmd/lobo-agent` (`cmd/lobo/main.go:141`). All removed at P6. |
| Self-terminate code home | `lobo_agent::selfkill` (RunPod GraphQL + Vast REST, instance-scoped keys) | It only runs on the pod. The laptop provider clients in P3 never need it. |
| SSH tests | In-process `russh` server (`testutil::fake_feesh`) | The Go tests prove host-key-algorithm pinning against a server that offers ECDSA + ed25519 (`source_ssh_test.go:30-40`); a mock `Source` can't test that. |
| HTTP fakes | `wiremock` for JSON APIs (RunPod GraphQL, Vast, llama `/metrics`, `/health`); a small axum `testutil::RangeServer` for byte ranges | wiremock can't cut a body mid-stream or go silent after 100 bytes, which 6 Go download tests need. |
| Process fakes | Shell scripts written into a `tempfile::TempDir` (fake `llama-server --list-devices`, fake `nvidia-smi`, env-dump child) | Same as the spec. No real CUDA on the Mac or CI. |
| Local musl build | `cargo zigbuild` (brew `zig` + `cargo install cargo-zigbuild`) | No Docker on the laptop (project rule), and zig links musl + ring C code without a cross sysroot. |
| CI musl build | `rust.yml` job on ubuntu with `musl-tools`; `pod-image.yml` builds inside `rust:1.98-alpine` | Alpine's native target is musl, static by default; no cross tooling in the image build. |
| TLS / crypto | rustls + `ring` + bundled webpki roots everywhere; `aws-lc-sys` and `openssl-sys` must not be in the tree | No cmake/perl/nasm in the musl build; the binary doesn't depend on the image's CA bundle. |
| HTTP version | reqwest with `http2` on (ALPN) | Go's `http.DefaultClient` negotiates h2 on TLS. Keeps the same wire behaviour. Not verified which one R2 picked for Go. |
| `/api/version` body | Served as the raw bytes of `/lobo/release.json` (fallback `{"version":"unknown"}`) | Go serves raw bytes (`api.go:13-16`, `main.go:80-83`). Re-encoding a typed `Manifest` would add zero fields like `"built_at":"0001-…"`. Needs a contract change (see end). |
| Release zip for the live test | **Image-only:** `lobo up --image ghcr.io/1905/lobocode@sha256:…`. No `lobo release` change. | `control/up.go:84-85` skips the bucket manifest when `--image` is set, and the bootstrap skips the zip (`bootstrap.go:45-47`). Zero Go edits. Rust zips come with P3 `lobo_core::release::build_zip` + P4 `lobo release`. |
| Test-only tuning (stall timeout, chunk size) | `download::Tuning { stall, chunk }` in a `tokio::task_local!`; defaults 30 s and `lobo_proto::catalog::CHUNK_SIZE` | Go tests mutate a package var (`hang_test.go:23-27`); that races under cargo's parallel tests. 256 MiB chunks × ~6 parallel tests would also allocate > 1.5 GB. |
| Download workers | Futures in the caller's task (`FuturesUnordered`), not `tokio::spawn` | So the task-local `Tuning` reaches them; the work is IO-bound. |
| PID-1 panic | `main_flow` runs `run()` in `tokio::spawn`; a panic (`JoinError::is_panic`) is a fatal path like `Err` → self-terminate | Go only covers `Err` (`main.go:49-57`); a Rust panic would exit PID 1 and RunPod would restart the container, billing on. `panic = "unwind"` stays. |
| SSH stream cap | Agent clamps `ssh://` sources to ≤ 8 streams, on top of the laptop clamp | Go clamps only on the laptop (`control/up.go:150-152`). Same result today; guards a hand-set `LOBO_DL_CONNS`. |
| SIGTERM as PID 1 | `main` handles SIGTERM/SIGINT: log, exit 143, no self-terminate | Go's runtime exits on SIGTERM; a Rust PID 1 without a handler ignores it. No test (signals to PID 1 need a container). |

---

## File map

**Create**
- `crates/lobo-agent/Cargo.toml` — lib `lobo_agent` + `[[bin]] name = "lobo-agent", path = "src/main.rs"`.
- `crates/lobo-agent/src/lib.rs` — module list, `pub const VERSION`, `pub const LLAMA_ADDR = "127.0.0.1:8080"`, `pub const AGENT_ADDR = "127.0.0.1:8081"`.
- `crates/lobo-agent/src/error.rs` — `Error`, `Result`.
- `crates/lobo-agent/src/http.rs` — `USER_AGENT`, `client()`.
- `crates/lobo-agent/src/logring.rs` — `LogRing`, `LogSource`.
- `crates/lobo-agent/src/process.rs` — `CleanEnv`, `clean_env`, `LlamaArgs`, `start_process`, `Proc`, `last_line`.
- `crates/lobo-agent/src/health.rs` — `wait_healthy`.
- `crates/lobo-agent/src/fetch.rs` — `fetch_file`.
- `crates/lobo-agent/src/source.rs` — `Source`, `BoxRead`, `HttpSource`, `SshSource`, `redact_url`, `range_total`, `model_source`.
- `crates/lobo-agent/src/download.rs` — `download`, `download_parallel`, `hash_file`, `bench`, `Tuning`, `with_tuning`, `MAX_RESUMES`.
- `crates/lobo-agent/src/watchdog.rs` — `State`, `Config`, `Decision`, `Reason`, `Sample`.
- `crates/lobo-agent/src/metrics.rs` — `Collector`, `SMI_ARGS`, `parse_llama`, `parse_nvidia_smi`, `parse_host`.
- `crates/lobo-agent/src/runner.rs` — `Deps` + traits `Tunnel`, `GpuCheck`, `Download`, `Llama`, `Metrics`, `Killer`; `RunnerConfig`; `Runner`.
- `crates/lobo-agent/src/api.rs` — `router`.
- `crates/lobo-agent/src/selfkill.rs` — `PodApi`, `RetryKiller`, `RunPodSelf`, `VastSelf`, `self_api`.
- `crates/lobo-agent/src/config.rs` — `AgentConfig::from_vars`.
- `crates/lobo-agent/src/pod.rs` — pod hooks + policy: `PodTunnel`, `PodGpuCheck`, `GpuCheckCfg`, `free_mib`, `ModelDownload`, `SlowGate`, `download_any`, `PodLlama`, `boot_timings`, `bench_line`, `init_logging`, `main_flow`, `run`.
- `crates/lobo-agent/src/main.rs` — clap: default = run, `version`, `bench`.
- `crates/lobo-agent/src/testutil/mod.rs` (`#[cfg(test)]`) — `RangeServer`, `fake_feesh`, `script`, `fake_deps`, `data(n)`, `chunk_table`, `within`.
- `crates/lobo-agent/testdata/llama_metrics.txt` — copy (`cp`, not `mv`) of `internal/metrics/testdata/llama_metrics.txt`.

**Modify**
- `Cargo.toml` (workspace) — new `[workspace.dependencies]`; `[profile.release] strip = true, lto = "thin", codegen-units = 1` (panic stays `unwind`).
- `crates/lobo-proto/src/agent.rs` — add `impl Stage { pub fn as_str(&self) -> &'static str }` (contract addition; needed for `"<stage>: <err>"` details).
- `docker/pod/Dockerfile` — Rust musl build stage replaces the Go stage. Same base `${LLAMA_IMAGE}`, same `/lobo/lobo-agent` + `/lobo/release.json`.
- `.dockerignore` — add `target`.
- `.github/workflows/pod-image.yml` — path filters → Rust; llama pin read from `crates/lobo-proto/src/release.rs`.
- `.github/workflows/rust.yml` — new job `agent-musl` (static check + size).
- `Makefile` — add `rust-agent` (zigbuild + static check + size). Go targets unchanged.
- `plans/2026-09-29-rust-rewrite/plan-p1-v1.0.md` — "Go test → Rust home" table: move `TestLoadAgent*` (config), `TestSelf` (runpod), `TestSelfTerminateAndGone` (vast) rows to P2.

**Out of scope for P2**
- Any Go change (`cmd/`, `internal/`, `e2e/`, `go.mod`). `lobo release` keeps zipping the Go agent.
- Bootstrap script, `execfail`, container env building (P3 `lobo_core::bootstrap`). P2 only keeps the `/lobo/lobo-agent` path the script execs (drift test in Task 28).
- macOS host metrics (`internal/local/sysctl_darwin.go`), local supervisor, local runtime fetch (P3).
- `release.yml`, `.goreleaser.yaml`, brew (P4).
- Laptop-side bad-host replace ×4 and `retriable()` (P3). P2 only keeps the stage-detail text they match on.

---

## Wire and text rules (locked)

1. `/api/status` body = `serde_json` of `lobo_proto::Status` + `"\n"` (Go `json.Encoder` adds it). Key set when `boot_id` is empty: `ctx download expires_at gpu host idle_s kill_in_s kill_reason llama metrics_failures model stage stage_detail timings uptime_s`. Unavailable sections are `null`.
2. `/api/version` = raw `release.json` bytes, `Content-Type: application/json`.
3. `/api/logs`: `Authorization: Bearer <LOBO_API_KEY>` exact match, else `401` body `unauthorized\n`. `n` missing/bad/≤0 → 200; >1000 → 1000. Body = lines joined `\n` + `\n`, `text/plain; charset=utf-8`.
4. Stage detail on failure = `"<stage>: <cause>"` with stage from `Stage::as_str()` (`runner.go:118`). GPU failure = `"gpu: <cause>"` (`runner.go:141`). The laptop re-rents on `detail.starts_with("gpu: ") || detail.contains("host: ")` (`control/up.go:230`). Every bad-host cause the agent raises must contain `host: ` or start with `gpu: `.
5. Exact cause texts kept (substring-tested): `cloudflared exited: …`, `llama-server exited while loading: …`, `llama-server exited: …`, `boot timeout <d> during <stage>: …`, `llama-server sees no CUDA device after <n> tries / <d>: <last line>`, `only <free> MiB VRAM free, <id> needs <need> MiB`, `host: download too slow: <x.x> MB/s after <n>s from <redacted> (min <floor>)`, `download <src>: sha256 <got>, want <sha>`, `server ignored Range at offset <n>, can't resume`, `no response headers after <d>`, `chunk <a>-<b>: gave up after 8 resumes`, `chunk table has <n> entries for <size> bytes`.
6. Child exit error text: `exit status <code>` or `signal: <n>` (Go's `ExitError` text; shows in `stage_detail`).
7. Kill reasons: `idle`, `expired`, `failed`.
8. Every outgoing HTTP request carries `User-Agent: lobo-agent/<VERSION>` (`http::client()`). reqwest sends none by default; Go sent `Go-http-client/1.1`. RunPod refuses some default UAs (spec carry-over).
9. Secret URLs never appear in errors: errors use `redact_url()` (`scheme://host`) and reqwest errors go through `without_url()`.

## Pinned versions

_(Task 1 fills this line from `Cargo.lock`: tokio, tokio-util, async-trait, reqwest, axum, russh, sha2, hex, base64, url, bytes, futures-util, regex-lite, thiserror, tracing, tracing-subscriber, clap, chrono, serde_json, wiremock, tempfile. Plus `zig --version` and `cargo zigbuild --version` from Task 27.)_

---

## Carry-over rules landing in P2 → named test

| Rule (spec "Carry-over rules") | Go source today | Rust test |
|---|---|---|
| CUDA init retries for 3 min, 10 s apart, 1 min per try | `cmd/lobo-agent/main.go:103-136` | `pod::tests::gpu_check_retries_until_cuda0`, `gpu_check_gives_up_after_window`, `gpu_check_hung_try_is_bounded`, `gpu_check_defaults_are_3min_10s_1min` |
| VRAM free ≥ model + 2560 MiB, else bad host | `main.go:118-122`, `catalog.MinFreeMiB` | `pod::tests::gpu_check_short_vram_is_bad_host` (+ `runner::tests::stage_detail_bad_host_markers`) |
| Slow download (< `LOBO_MIN_MBPS`, default 100, after 20 s) drops the host | `main.go:39-41,358-387`, `config/agent.go:70` | `pod::tests::slow_gate_decides_once_after_20s`, `slow_source_fails_with_host_marker`, `config::tests::min_mbps_default_100` |
| R2 slow → model-server fallback on the same pod | `main.go:141-150,389-402` | `pod::tests::download_any_falls_back_on_any_error`, `r2_slow_switches_to_fallback_source` |
| SSH source ≤ 8 streams | `control/up.go:150-152` (laptop) | `pod::tests::ssh_source_conns_capped_at_8` (new agent-side clamp) |
| SSH: pinned host key, negotiate only its type, handshake failures permanent (fail2ban) | `source.go:141-195` | `source::ssh_tests::ssh_rejects_wrong_host_key`, `ssh_picks_pinned_ed25519_over_ecdsa` |
| PID-1 fatal paths self-terminate (config error, missing model, panic) | `main.go:49-57,233-245` | `pod::tests::fatal_error_self_terminates`, `panic_in_run_self_terminates`, `bin_tests::bin_fatal_without_creds_exits_1` |
| RunPod self-terminate = GraphQL `podTerminate` with the pod-scoped key + a UA | `runpod/self.go:21-75` | `selfkill::tests::runpod_self`, `runpod_self_sends_user_agent` |
| Vast self-terminate = REST DELETE with `CONTAINER_API_KEY`, 404 = gone | `vast/self.go`, `vast/client.go:174-193` | `selfkill::tests::vast_self_terminate_and_gone`, `vast_self_sends_user_agent` |
| Terminate retried until confirmed gone (2 s doubling, cap 60 s, 10 s per try) | `agent/killer.go:28-55` | `selfkill::tests::kill_self_retries`, `kill_self_waits_for_gone`, `kill_self_cancel` |
| Secrets never reach children (`LLAMA_ARG_*`, `LOBO_*`, `LD_LIBRARY_PATH`, `*_API_KEY`, `*_TOKEN`) | `agent/env.go:8-19`, `main.go:86,100,112,154` | `process::tests::clean_env`, `start_process_does_not_inherit_parent_env`, `pod::tests::pod_children_env_has_only_their_own_secret` |
| Secret URLs never in errors / `/api/status` | `source.go:31-42,69-71` | `source::tests::http_source_errors_hide_secret_url` |
| `execfail` for a missing agent | `bootstrap.go:56-61` | **P3** (bootstrap). P2: `docker_tests::dockerfile_keeps_lobo_layout` guards the exec path. |
| Home network DNS-hijacks `*.r2.dev` | test environment | Task 30 note (live only) |

---

## Task 0 — Preconditions (orchestrator, no implementer)

- [ ] P1 is `done` on `feat/rust`: `make rust-lint rust-test` green, `lobo-proto` has `Status`, `Timings`, `DownloadProgress`, `Gpu`, `Host`, `Llama`, `Stage`, `GoTime`, `Manifest`, `catalog::{get, CHUNK_SIZE, min_free_mib}`, `DEFAULT_LLAMA_IMAGE`.
- [ ] `git status` clean on `feat/rust`. `git merge master` first if master moved (merge-hygiene rule).
- [ ] Baseline: `go test ./...` → every package `ok`. Record the count.
- [ ] `rustc --version` → `1.98.x`.
- [ ] User approved this plan. Plan status → `approved (P2 in progress)`.
- [ ] Commit: `plans: rust P2 plan`.

## Task 1 — Crate skeleton, deps, error, HTTP client

**Files:** Create `crates/lobo-agent/{Cargo.toml,src/lib.rs,src/error.rs,src/http.rs,src/main.rs}`. Modify root `Cargo.toml`.

- [ ] `cargo new --lib crates/lobo-agent --vcs none`; workspace fields (`edition.workspace`, `rust-version.workspace`, `[lints] workspace = true`); add `[[bin]]`.
- [ ] `cargo add` (then move versions into `[workspace.dependencies]`, crate uses `x.workspace = true`):
  - `-p lobo-agent lobo-proto --path crates/lobo-proto`
  - `tokio --features rt-multi-thread,macros,time,process,io-util,net,fs,signal,sync`, `tokio-util`, `async-trait`, `futures-util`, `bytes`
  - `reqwest --no-default-features` + features for rustls with bundled webpki roots, `http2`, `stream` (names per the installed reqwest version; the goal is: rustls, ring, webpki roots, h2, streaming body)
  - `axum --no-default-features --features http1,tokio,query`, `russh` (ring backend if the crate offers it), `sha2`, `hex`, `base64`, `url`, `regex-lite`, `thiserror`, `tracing`, `tracing-subscriber --features json`, `clap --features derive`, `chrono`, `serde_json`
  - dev: `wiremock`, `tempfile`, `tokio --features test-util`
- [ ] `lib.rs`: modules, `pub const VERSION: &str` = `option_env!("LOBO_VERSION")` or `"dev"`, `LLAMA_ADDR`, `AGENT_ADDR`.
- [ ] `error.rs` (locked):
  ```rust
  #[derive(Debug, thiserror::Error)]
  pub enum Error {
      #[error("{0}")] Permanent(String),   // source will not get better: stop resuming
      #[error("context canceled")] Cancelled,
      #[error("{0}")] Msg(String),
      #[error(transparent)] Io(#[from] std::io::Error),
  }
  impl Error { pub fn is_permanent(&self) -> bool; pub fn msg(s: impl Into<String>) -> Self; }
  pub type Result<T> = std::result::Result<T, Error>;
  ```
- [ ] `http.rs`: `pub const USER_AGENT: &str` = `"lobo-agent/" + VERSION` (const via `concat!` over a const macro or a `LazyLock<String>`; implementer picks); `pub fn client() -> reqwest::Client` (process-wide `LazyLock`, UA set, no overall timeout — callers set their own).
- [ ] `main.rs`: `fn main() {}` placeholder.
- [ ] Failing test `http::tests::client_sends_user_agent`: wiremock matcher `header("user-agent", USER_AGENT)` → 200; any other → 400. GET via `client()` → 200.
- [ ] Verify: `cargo test -p lobo-agent http` → 1 passed. `cargo tree -p lobo-agent -i aws-lc-sys` and `-i openssl-sys` → both print "did not match any packages". If a crate forces aws-lc, stop and report to the orchestrator (Dockerfile then needs `cmake perl`).
- [ ] Fill "Pinned versions".
- [ ] Commit: `lobo-agent: crate skeleton, error type, UA http client`.

## Task 2 — Watchdog

**Files:** Create `src/watchdog.rs`.

Locked interface (port of `internal/watchdog/watchdog.go`):
```rust
pub struct Sample { pub at: DateTime<Utc>, pub ok: bool, pub processing: i64, pub deferred: i64, pub prompt_tokens: i64, pub gen_tokens: i64 }
pub struct Config { pub idle: Duration, pub expires_at: DateTime<Utc> }
#[derive(Clone, Copy, PartialEq, Eq, Debug)] pub enum Reason { Idle, Expired }   // as_str(): "idle" | "expired"
#[derive(Clone, Copy, PartialEq, Eq, Debug)] pub enum Decision { Kill { reason: Reason }, Wait { reason: Reason, kill_in: Duration } }
impl Decision { pub fn reason(&self) -> Reason; pub fn kill_in(&self) -> Duration /* 0 for Kill */; pub fn is_kill(&self) -> bool; }
pub struct State { .. }
impl State { pub fn new(start: DateTime<Utc>) -> Self; pub fn set_ready(&mut self, at: DateTime<Utc>);
             pub fn observe(&mut self, s: Sample); pub fn idle_for(&self, now: DateTime<Utc>) -> Duration;
             pub fn failed_samples(&self) -> i64; pub fn decide(&self, now: DateTime<Utc>, cfg: &Config) -> Decision; }
```
Semantics = Go line for line: expiry first (`<= 0` → Kill Expired); not ready → Wait Expired; idle `<= 0` → Kill Idle; smaller of idle/expiry wins; activity = processing>0 || deferred>0 || token totals moved since the previous OK sample, and only if `at > last_active`; failed samples only bump the counter.

- [ ] Failing tests: `decide` (the 8 Go table rows, same names, same expected kill/reason/kill_in), `expired_at_start`, `failed_samples`.
- [ ] Implement. `cargo test -p lobo-agent watchdog` → 3 passed.
- [ ] Commit: `lobo-agent: watchdog`.

## Task 3 — Metrics parsers

**Files:** Create `src/metrics.rs` (parsers only), `testdata/llama_metrics.txt` (cp).

Locked: `pub const SMI_ARGS: [&str; 2] = ["--query-gpu=name,memory.used,memory.total,utilization.gpu", "--format=csv,noheader,nounits"];`
`pub fn parse_llama(text: &str) -> Result<lobo_proto::Llama>`, `pub fn parse_nvidia_smi(csv: &str) -> Result<Gpu>`, `pub fn parse_host(loadavg: &str, meminfo: &str) -> Result<Host>`. Rules = `internal/metrics/{llama,gpu,host}.go`: `llamacpp:` prefix trimmed; `requests_processing` + `prompt_tokens_total` required; `#`/blank skipped; a line with < 2 fields or a bad float is an error; used = MemTotal − MemAvailable, kB/1024.

- [ ] `cp internal/metrics/testdata/llama_metrics.txt crates/lobo-agent/testdata/`.
- [ ] Failing tests: `parse_llama` (sample + 3 error cases), `parse_llama_fixture` (fixture must parse; no skip), `parse_nvidia_smi`, `parse_host` — values as in `metrics_test.go`.
- [ ] Implement. `cargo test -p lobo-agent metrics::tests::parse` → 4 passed.
- [ ] Commit: `lobo-agent: metrics parsers`.

## Task 4 — Metrics collector

**Files:** Modify `src/metrics.rs`. Create `src/testutil/mod.rs` with `script(dir, name, body) -> PathBuf` (writes `#!/bin/sh\n<body>`, mode 0755).

Locked:
```rust
pub struct Collector { pub llama_url: String, pub api_key: String, pub http: reqwest::Client, pub nvidia_smi: PathBuf /* "nvidia-smi" */, pub proc_dir: PathBuf /* "/proc" */ }
#[async_trait] impl crate::runner::Metrics for Collector { .. }   // trait lands in Task 19; until then plain async fns llama()/gpu()/host()
```
Each read has a 3 s timeout (`collect.go:20`). `llama()` sends `Authorization: Bearer <api_key>` to `<llama_url>/metrics`; non-200 → error. `gpu()` runs `nvidia_smi` with `SMI_ARGS` and a cleaned env (`CleanEnv`, Task 5 — until then an empty env), `kill_on_drop(true)`.

- [ ] Failing tests: `collector` (wiremock `/metrics` needs `Bearer k`; fake `nvidia-smi` script echoes `NVIDIA GeForce RTX 5090, 1, 2, 3`; temp proc dir with `loadavg` + `meminfo`; wrong key → error; UA header present), `collector_timeout` (wiremock delay 10 s → error in < 5 s).
- [ ] Implement. `cargo test -p lobo-agent metrics::tests::collector` → 2 passed.
- [ ] Commit: `lobo-agent: metrics collector (llama /metrics, nvidia-smi, /proc)`.

## Task 5 — LogRing, CleanEnv, LlamaArgs, last_line

**Files:** Create `src/logring.rs`, `src/process.rs` (these parts).

Locked:
```rust
pub struct LogRing;  impl LogRing { pub fn new(max: usize) -> Self; pub fn write(&self, b: &[u8]); pub fn tail(&self, n: usize) -> Vec<String>; }
pub type LogSource = Arc<LogRing>;
impl std::io::Write for &LogRing { .. }            // for the tracing writer
pub struct CleanEnv;
impl CleanEnv { pub fn is_stripped(key: &str) -> bool;
                pub fn filter<I: IntoIterator<Item = (String, String)>>(env: I) -> Vec<(String, String)>; }
pub fn clean_env() -> Vec<(String, String)>;        // CleanEnv::filter(std::env::vars())
pub struct LlamaArgs { pub model_path: String, pub alias: String, pub host: String, pub port: u16, pub ctx: i64 }
impl LlamaArgs { pub fn to_args(&self) -> Vec<String>; }   // "-m" <path> then llama.go:12-18 in order
pub fn last_line(s: &str) -> String;
```
`LogRing` keeps a partial last line (`logring.go:18-28`), `tail(n)` returns up to n complete lines. `is_stripped`: prefix `LLAMA_ARG_` or `LOBO_`, exact `LD_LIBRARY_PATH`, suffix `_API_KEY` or `_TOKEN`.

- [ ] Failing tests: `logring::tests::log_ring` (Go `TestLogRing`: `"b,cd,e"`, tail(1)=`e`, 8 threads × 100 writes no panic), `process::tests::clean_env` (Go input → exactly `PATH=/bin HOME=/root` in order), `process::tests::llama_args` (exact Go `want` vector for q8, `/models`, `127.0.0.1`, 8080, 65536), `process::tests::last_line` (3 Go cases).
- [ ] Implement. `cargo test -p lobo-agent logring process` → 4 passed.
- [ ] Commit: `lobo-agent: log ring, CleanEnv, llama args`.

## Task 6 — start_process

**Files:** Modify `src/process.rs`.

Locked:
```rust
pub struct Proc { pub pid: u32, pub exited: tokio::sync::oneshot::Receiver<Result<()>> }
pub fn start_process(program: &Path, args: &[String], env: &[(String, String)], logs: LogSource,
                     tag: Option<&'static str>, kill: CancellationToken) -> Result<Proc>;
```
Always `env_clear()` then `envs(env)` (Go inherits the whole env when `cmd.Env` is nil — Rust never does). stdout + stderr piped and split into lines. `tag: Some(t)` → each line `"[t] line"` into the ring and stdout (`main.go:196-219`); `None` → raw line into the ring only. `kill` cancelled → SIGKILL. `exited` gets `Ok(())` on 0, else `Error::Msg("exit status N")` / `Error::Msg("signal: N")`, once.

- [ ] Failing tests: `start_process` (`/bin/sh -c "echo hi; exit 3"` → `exit status 3`, `tail(1) == ["hi"]`), `start_process_tags_lines` (tag `llama` → `[llama] hi`), `start_process_does_not_inherit_parent_env` (env `[("A","1")]`, child runs `env` → output has `A=1` and no `HOME=`/`PATH=` from the parent), `start_process_kill_token_kills` (`exec sleep 30`, cancel → `exited` resolves `signal: 9` within 2 s).
- [ ] Implement. `cargo test -p lobo-agent process::tests::start_process` → 4 passed.
- [ ] Commit: `lobo-agent: child processes with clean env and tagged logs`.

## Task 7 — wait_healthy, fetch_file

**Files:** Create `src/health.rs`, `src/fetch.rs`.

Locked:
```rust
pub async fn wait_healthy(base: &str, poll: Duration, cancel: CancellationToken) -> Result<()>;   // GET base/health until 200
pub async fn fetch_file(url: &str, dst: &Path, mode: u32, size: i64, sha: &str) -> Result<()>;      // fetch.go:18-64
```
`fetch_file`: whole fetch bounded to 5 min; `dst` created (truncate, `mode`) only after a 200; `size >= 0` caps at size+1 bytes and checks length; `sha != ""` checks sha256. Error texts `fetch <url>: HTTP N`, `fetch <url>: size N, want M`, `fetch <url>: sha256 X, want Y`.

- [ ] Failing tests (new, no Go equivalent — Go covered these via local tests in P3): `wait_healthy_polls_until_200` (wiremock 503 ×2 then 200, poll 10 ms), `wait_healthy_cancel`, `fetch_file_ok_sets_mode` (0755), `fetch_file_404_creates_nothing`, `fetch_file_size_and_sha_mismatch`.
- [ ] Implement. `cargo test -p lobo-agent health fetch` → 5 passed.
- [ ] Commit: `lobo-agent: health wait and file fetch`.

## Task 8 — Test range server

**Files:** Modify `src/testutil/mod.rs`.

Locked (`#[cfg(test)]`):
```rust
pub enum FirstReq { Normal, Drop /* close at once, no response */, HalfThenClose /* 200 + full Content-Length, half body, close */,
                    StallAfter(usize) /* 206 + Content-Range for chunk 0, n bytes, then silent */, Silent /* no headers ever */ }
pub struct RangeServer { pub url: String, pub calls: Arc<AtomicUsize>, pub ranges: Arc<Mutex<Vec<String>>>, pub served: Arc<AtomicU64> }
impl RangeServer { pub async fn start(data: Arc<Vec<u8>>, first: FirstReq, status_for: Option<(String /*path*/, u16)>) -> Self; }
pub fn data(n: usize) -> Arc<Vec<u8>>;                // deterministic pseudo-random bytes
pub fn chunk_table(data: &[u8], chunk: i64) -> Vec<String>;
pub async fn within<T>(d: Duration, f: impl Future<Output = T>) -> T;   // panics "hung"
```
axum on `127.0.0.1:0`. `Normal` honours `Range: bytes=a-b` / `a-` → 206 + `Content-Range: bytes a-b/N`; no Range → 200 + `Content-Length`. Body via `Body::from_stream`; `HalfThenClose` ends the stream with an error so hyper drops the connection.

- [ ] Self-tests: `range_server_serves_ranges` (reqwest GET `bytes=5-9` → 206, 5 bytes, `Content-Range: bytes 5-9/N`), `range_server_half_then_close` (body read errors).
- [ ] `cargo test -p lobo-agent testutil` → 2 passed.
- [ ] Commit: `lobo-agent: test range server`.

## Task 9 — Source trait + HttpSource

**Files:** Create `src/source.rs` (HTTP parts), `src/download.rs` (`Tuning` only).

Locked:
```rust
pub type BoxRead = Box<dyn tokio::io::AsyncRead + Send + Unpin>;
#[async_trait] pub trait Source: Send + Sync + std::fmt::Display {
    /// len < 0 = to the end. Returns (body, total) — total = full size when known, else -1.
    async fn open(&self, offset: i64, len: i64) -> Result<(BoxRead, i64)>;
}
pub struct HttpSource { pub url: String }             // Display = redact_url(url)
pub fn redact_url(raw: &str) -> String;               // "scheme://host", "?" if no host
pub fn range_total(content_range: &str) -> i64;
// download.rs
#[derive(Clone, Copy)] pub struct Tuning { pub stall: Duration, pub chunk: i64 }
impl Default for Tuning { /* 30 s, lobo_proto::catalog::CHUNK_SIZE */ }
pub async fn with_tuning<F: Future>(t: Tuning, f: F) -> F::Output;   // task_local scope; doc(hidden), tests only
pub(crate) fn tuning() -> Tuning;                                    // task-local or default
```
`HttpSource::open` = `source.go:44-89`: Range header only when offset>0 or len>=0; header wait bounded by `tuning().stall` → `no response headers after <d>`; 200 unranged → (body, content_length or -1); 206 ranged → (body, range_total); ranged 200 → Permanent `server ignored Range…`; ≥500 or 429 → Msg `HTTP N` (retryable); other → Permanent `download <redacted>: HTTP N`. Uses `http::client()`; errors via `without_url()`.

- [ ] Failing tests: `range_total` (4 Go cases), `http_source_errors_hide_secret_url` (403 server + refused `127.0.0.1:1`, both with `/tok3n/…?X-Amz-Signature=s3cret` → error has neither `s3cret` nor `tok3n`), `http_source_status_mapping` (new: 404 permanent, 503 retryable, ranged 200 permanent), `default_tuning` (30 s, `CHUNK_SIZE`).
- [ ] Implement. `cargo test -p lobo-agent source::tests download::tests::default_tuning` → 4 passed.
- [ ] Commit: `lobo-agent: Source trait and HTTP source`.

## Task 10 — download (single stream): happy path, resume

**Files:** Modify `src/download.rs`.

Locked:
```rust
pub const MAX_RESUMES: usize = 8;
pub async fn download(cancel: CancellationToken, src: &dyn Source, dst: &Path, sha: &str,
                      on_progress: Option<&(dyn Fn(DownloadProgress) + Sync)>) -> Result<()>;
```
Contract says `on_progress` without `Option`; this plan uses `Option<&dyn Fn>` because Go passes `nil` in 8 places (contract clarification, listed at the end). Semantics = `download.go:29-128`: open rw/create (no truncate); partial resume via a 1-byte probe `open(0,1)` (unknown total or file longer than total → truncate and start over; else hash existing bytes, progress base = file size); up to 8 resumes with `attempt*2 s` backoff; short body → retry; unknown total → Permanent; progress throttled to 500 ms, MB/s excludes resumed bytes, forced final report; sha mismatch → rename to `dst.bad` + `download <src>: sha256 …`; exhausted → `download <src>: <last> (after 8 resumes)`.

- [ ] Failing tests: `download_basic` (Go `TestDownload`: good, bad sha → `.bad` exists and dst gone, 404 → error has `404`), `download_resumes_after_drop` (`HalfThenClose`; 2 calls, 2nd has `Range`), `download_resumes_partial_file` (3 Go cases: probe range `bytes=0-0`, main range `bytes=300000-`, served ≤ want, final progress bytes=total=len).
- [ ] Implement. `cargo test -p lobo-agent download::tests::download_` → 3 passed.
- [ ] Commit: `lobo-agent: single-stream download with resume and sha check`.

## Task 11 — download: stalls and cancel

**Files:** Modify `src/download.rs`.

A read that returns no bytes for `tuning().stall` drops the stream and resumes. `cancel` ends everything with `Error::Cancelled`.

- [ ] Failing tests: `download_stall_respects_cancel` (Go `TestDownloadStallRespectsDeadline`: server sends 3 of 100 bytes then goes silent; cancel after 50 ms → `Error::Cancelled` within 1 s), `single_stalled_stream` (Go `TestDownloadSingleStalledStream`, via `download_parallel(conns=1)` once Task 13 lands — write it here against `download` directly; stall 200 ms; ≥ 2 calls; within 20 s).
- [ ] Implement. `cargo test -p lobo-agent download::tests::stall download::tests::single_stalled` → 2 passed.
- [ ] Commit: `lobo-agent: stall timeout and cancellation for downloads`.

## Task 12 — hash_file

**Files:** Modify `src/download.rs`.

Locked: `pub async fn hash_file(cancel: CancellationToken, path: &Path, total: i64, on_progress: Option<&(dyn Fn(DownloadProgress) + Sync)>) -> Result<String>;` Verifying progress at start, at most every 500 ms, at the end (`download.go:158-172`). 4 MiB reads. Cancel → `Error::Cancelled`.

- [ ] Failing tests (new; Go covered it in `internal/local` tests): `hash_file_matches_sha256` (first and last progress have `verifying: true`, last bytes = total), `hash_file_cancel`.
- [ ] Implement. `cargo test -p lobo-agent download::tests::hash_file` → 2 passed.
- [ ] Commit: `lobo-agent: hash_file with verifying progress`.

## Task 13 — download_parallel (HTTP)

**Files:** Modify `src/download.rs`.

Locked:
```rust
pub async fn download_parallel(cancel: CancellationToken, src: &dyn Source, dst: &Path, size: i64, sha: &str,
                               chunk_sha: &[String], conns: usize,
                               on_progress: Option<&(dyn Fn(DownloadProgress) + Sync)>) -> Result<()>;
```
`conns <= 1` → `download`. Empty `chunk_sha` = no table (Go `nil`). Semantics = `parallel.go:28-206`: table length checked before anything else (`chunk table has N entries for S bytes`); dst created + `set_len(size)`; chunks of `tuning().chunk` queued to `conns` workers (futures in this task); per chunk ≤ 8 attempts with `attempt*1 s` backoff (`gave up after 8 resumes`); with a table, a chunk restarts from its start on drop or mismatch and its counted bytes are taken back; without a table, a drop resumes mid-chunk; Permanent or write error cancels all workers; progress ticker 500 ms, never concurrent with itself, never after return; no table → one full sha pass with a `verifying: true` report, mismatch → `.bad`; last report `verifying: false`, bytes = total = size.

Tests use `with_tuning(Tuning { stall: 200 ms or 1 s, chunk: 64 KiB })`.

- [ ] Failing tests: `parallel_http` (3 chunks + short tail, `Drop` first; equal, saw verify, last not verifying, ≥ 4 calls), `parallel_bad_sha` (`.bad` exists), `parallel_chunk_sha` (`Drop` first; table; last not verifying), `parallel_chunk_sha_mismatch` (all-zero table entry → `gave up`).
- [ ] Implement. `cargo test -p lobo-agent download::tests::parallel_` → 4 passed.
- [ ] Commit: `lobo-agent: parallel ranged download with per-chunk sha`.

## Task 14 — download_parallel hangs + bench

**Files:** Modify `src/download.rs`.

Locked: `pub async fn bench(cancel: CancellationToken, src: &dyn Source, size: i64, conns: usize, dur: Duration) -> Result<(i64, f64)>;` — ranges into a discard sink for up to `dur`; hitting `dur` is not an error (`parallel.go:213-224`).

- [ ] Failing tests: `parallel_no_headers` (`Silent` first; stall 200 ms; table; within 20 s → Ok), `parallel_stalled_stream` (`StallAfter(100)` first; stall 1 s — same reason as the Go comment; ≥ 3 calls; within 60 s), `parallel_bad_table_no_ticker_leak` (table of 2 for 10 bytes → `chunk table` error; sleep 700 ms; progress calls == 0), `bench` (4 MiB, 2 conns, 5 s → n = len, mbps > 0).
- [ ] Implement. `cargo test -p lobo-agent download::tests` → all download tests pass.
- [ ] Commit: `lobo-agent: download hang guards and bench`.

## Task 15 — Fake model server (russh)

**Files:** Modify `src/testutil/mod.rs`.

Locked (`#[cfg(test)]`): `pub async fn fake_feesh(data: Arc<Vec<u8>>, client_pub: russh::keys::PublicKey) -> (String /*addr*/, russh::keys::PublicKey /*ed25519 host key*/, Arc<AtomicUsize> /*exec sessions*/);` and `pub async fn silent_tcp() -> String` (accepts TCP, never speaks). Contract = `source_ssh_test.go:25-93`: offers an ECDSA P-256 **and** an ed25519 host key; accepts only `client_pub`; `exec "<file> <offset>[ <len>]"`; session 1 sends half its range then drops the TCP connection; later sessions send the range, exit-status 0, close.

- [ ] Self-test `fake_feesh_serves_range` using a raw russh client: exec `m 5 5` on session 2 (after a throwaway session 1) → 5 bytes.
- [ ] `cargo test -p lobo-agent testutil::fake_feesh` → 1 passed.
- [ ] Commit: `lobo-agent: in-process fake model server for SSH tests`.

## Task 16 — SshSource + model_source

**Files:** Modify `src/source.rs`.

Locked:
```rust
pub struct SshSource { pub user: String, pub addr: String /* host:port */, pub file: String, pub size: i64,
                       pub key: Arc<russh::keys::PrivateKey>, pub host_key: russh::keys::PublicKey }
// Display: "ssh://user@addr/file"
pub fn model_source(url: &str, ssh_key_b64: &str, host_key_line: &str, file: &str, size: i64) -> Result<Arc<dyn Source>>;
```
`open` = `source.go:141-188`: TCP dial error → retryable; handshake + auth + exec bounded by `tuning().stall`; handshake/auth/host-key failure → Permanent `ssh <addr>: …` (text for a key mismatch contains `host key`); preferred host-key algorithms = only the pinned key's type (RSA → `rsa-sha2-512`, `rsa-sha2-256`, `ssh-rsa`); exec `"<file> <offset>"` + `" <len>"` when len ≥ 0; body = channel stdout as `AsyncRead`; dropping the body closes channel + connection; total = `size`. `model_source`: non-`ssh` scheme → `HttpSource`; `ssh://user@host[:port]` (port default 22), key = base64 of an OpenSSH PEM (`LOBO_MODEL_SSH_KEY: …` on error), host key = authorized_keys line (`LOBO_MODEL_SSH_HOSTKEY: …` on error).

- [ ] Failing tests (`source::ssh_tests`): `ssh_resume` (Go `TestSSHSourceResume` via `download`; sessions == 2), `ssh_rejects_wrong_host_key` (error contains `host key`, `is_permanent()`), `ssh_picks_pinned_ed25519_over_ecdsa` (pin = the ed25519 key; server offers ECDSA first; `open` succeeds), `ssh_silent_handshake` (`silent_tcp`; stall 200 ms; error within 10 s), `parallel_ssh` (Go `TestDownloadParallelSSH`; chunk 64 KiB; sessions ≥ 3), `model_source_parses` (https → Display `https://host`; `ssh://lobo@h` → addr `h:22`; bad base64 → error names `LOBO_MODEL_SSH_KEY`; bad host key → `LOBO_MODEL_SSH_HOSTKEY`).
- [ ] Implement. `cargo test -p lobo-agent source::ssh_tests` → 6 passed.
- [ ] Commit: `lobo-agent: SSH model source with pinned host key`.

## Task 17 — Self-terminate

**Files:** Create `src/selfkill.rs`.

Locked:
```rust
#[async_trait] pub trait PodApi: Send + Sync { async fn terminate(&self) -> Result<()>; async fn gone(&self) -> Result<bool>; }
pub struct RetryKiller { pub api: Arc<dyn PodApi> }            // impl runner::Killer (Task 19); until then an inherent kill_self
pub struct RunPodSelf { pub pod_id: String, pub key: String, pub url: String /* https://api.runpod.io/graphql */ }
pub struct VastSelf { pub id: i64, pub key: String, pub base_url: String /* https://console.vast.ai/api/v0 */ }
impl RunPodSelf { pub fn new(pod_id: &str, key: &str) -> Self; }
impl VastSelf { pub fn new(id: &str, key: &str) -> Result<Self>; }   // id must parse as i64
pub fn self_api(provider: &str, rp_id: &str, rp_key: &str, vast_id: &str, vast_key: &str) -> Option<Arc<dyn PodApi>>;
```
`RetryKiller::kill_self(cancel)` = `killer.go:28-55`: loop { cancel → `Cancelled`; per try 10 s: terminate, then gone; gone → Ok; else sleep backoff (2 s, ×2, cap 60 s) }. Uses `tokio::time::sleep` (tests pause time instead of injecting sleep). `RunPodSelf`: POST `{"query": …}` with `Authorization: Bearer <key>`, `Content-Type: application/json`, UA; 10 s timeout; terminate = `mutation { podTerminate(input:{podId:"<id>"}) }`; gone = `{ pod(input:{podId:"<id>"}) { desiredStatus } }` → `pod == null || desiredStatus == "TERMINATED"`; non-200 → `runpod graphql: HTTP N`; `errors[0].message` → `runpod graphql: <msg>`. `VastSelf`: 30 s timeout; terminate = DELETE `/instances/<id>/`, 404 → Ok; gone = GET `/instances/<id>/` → 404 or `"instances": null` → true; 401/403 → error. `self_api`: `"vast"` needs both vast values; anything else is RunPod and needs both RunPod values; never crosses.

- [ ] Failing tests: `kill_self_retries` (paused time; terminate fails twice; 3 terms; elapsed exactly 6 s = 2 s + 4 s), `kill_self_waits_for_gone` (gone false, false, true → 3 terms), `kill_self_cancel` (cancel during backoff → `Cancelled`), `runpod_self` (Go `TestSelf` on wiremock: query text, RUNNING → not gone, `pod:null` → gone, `errors` → Err), `runpod_self_sends_user_agent` (wiremock requires UA + `Bearer podkey`), `vast_self_terminate_and_gone` (wiremock: GET 200 instance → not gone; DELETE 200; GET 200 `instances:null` → gone; DELETE 404 → Ok), `vast_self_sends_user_agent`, `self_api_per_provider` (Go `TestSelfAPIPerProvider`, including its `CleanEnv` assertion).
- [ ] Implement. `cargo test -p lobo-agent selfkill` → 8 passed.
- [ ] Commit: `lobo-agent: self-terminate via RunPod GraphQL and Vast REST`.

## Task 18 — Agent config

**Files:** Create `src/config.rs`.

Locked:
```rust
pub struct AgentConfig { pub lobo_api_key: String, pub cf_tunnel_token: String, pub model: String, pub model_url: String,
    pub model_fallback: String, pub provider: String, pub runpod_pod_id: String, pub runpod_api_key: String,
    pub vast_id: String, pub vast_api_key: String, pub model_ssh_key: String, pub model_host_key: String, pub boot_id: String,
    pub ctx: i64, pub idle_min: i64, pub dl_conns: usize, pub min_mbps: i64, pub expires_at: DateTime<Utc>, pub boot_timeout: Duration }
impl AgentConfig { pub fn from_vars(get: &dyn Fn(&str) -> Option<String>) -> Result<Self>; }   // main passes std::env::var(..).ok()
```
Rules = `config/agent.go:35-99`: required `LOBO_API_KEY`, `CF_TUNNEL_TOKEN`, `LOBO_MODEL`, `LOBO_MODEL_URL` (must parse as a URL) → `config: <ENV>: required` style (text names the env var); provider default `runpod`; `runpod` needs `RUNPOD_POD_ID` + `RUNPOD_API_KEY`; `vast` needs `CONTAINER_ID` + `CONTAINER_API_KEY`; other → error naming `LOBO_PROVIDER`; `ssh://` URL needs `LOBO_MODEL_SSH_KEY` + `LOBO_MODEL_SSH_HOSTKEY`; ints `LOBO_CTX` 8192, `LOBO_IDLE_MIN` 30, `LOBO_MIN_MBPS` 100, `LOBO_DL_CONNS` 1 — positive only, error names the var; `LOBO_EXPIRES_AT` RFC 3339 required; `LOBO_BOOT_TIMEOUT` default 40 min, Go duration syntax (`40m`, `1h30m`, `90s`) — implement a small parser for `h`/`m`/`s`/`ms` units.

- [ ] Failing tests (ported from `internal/config/config_test.go`): `load_agent` (Go env map → ctx 8192, idle 30, boot 40 min, expires hour 20), `load_agent_errors` (4 Go rows), `load_agent_per_provider` (Go sequence), plus `min_mbps_default_100`, `boot_timeout_go_syntax` (`40m`, `1h30m`, `bad` → error).
- [ ] Implement. `cargo test -p lobo-agent config` → 5 passed.
- [ ] Commit: `lobo-agent: pod env config`.

## Task 19 — Runner: traits, fakes, happy path

**Files:** Create `src/runner.rs`. Modify `src/testutil/mod.rs` (`fake_deps`). Wire `impl Metrics for Collector`, `impl Killer for RetryKiller`.

Locked (contract, with async-trait):
```rust
#[async_trait] pub trait Tunnel: Send + Sync { async fn start(&self, boot: CancellationToken, life: CancellationToken) -> Result<oneshot::Receiver<Result<()>>>; }
#[async_trait] pub trait GpuCheck: Send + Sync { async fn check(&self, cancel: CancellationToken) -> Result<()>; }
#[async_trait] pub trait Download: Send + Sync { async fn run(&self, cancel: CancellationToken, on_progress: &(dyn Fn(DownloadProgress) + Sync)) -> Result<()>; }
#[async_trait] pub trait Llama: Send + Sync { async fn start(&self) -> Result<oneshot::Receiver<Result<()>>>; async fn wait_healthy(&self, cancel: CancellationToken) -> Result<()>; }
#[async_trait] pub trait Metrics: Send + Sync { async fn llama(&self) -> Result<lobo_proto::Llama>; async fn gpu(&self) -> Result<Gpu>; async fn host(&self) -> Result<Host>; }
#[async_trait] pub trait Killer: Send + Sync { async fn kill_self(&self, cancel: CancellationToken) -> Result<()>; }
pub struct Deps { pub tunnel, pub gpu_check, pub download, pub llama, pub metrics, pub killer }   // Arc<dyn …> each
pub struct RunnerConfig { pub boot_id: String, pub timings: Timings, pub model: String, pub ctx: i64, pub idle: Duration,
    pub expires_at: DateTime<Utc>, pub boot_timeout: Duration, pub tick: Duration, pub fail_grace: Duration }
impl Runner { pub fn new(d: Deps, cfg: RunnerConfig) -> Arc<Runner>; pub fn status(&self) -> Status;
              pub async fn run(self: Arc<Self>, cancel: CancellationToken) -> Result<()>; }
```
Semantics = `runner.go`: initial status (boot_id, stage Boot, model, ctx, expires_at, timings); `status()` fills `uptime_s`; stage change records the previous stage's seconds into timings (tunnel/gpu/download/verify/load), `ready_at` on Ready; boot order tunnel → gpu → download (progress: `verifying` flips Download→Verify once, sets `timings.download_source` when `source` set, `download_mbps` when bytes == total and mbps > 0) → llama start → wait healthy **raced against the llama exit** → watchdog ready → Ready → wait for llama exit / cancel / killed. Boot token = child of `cancel` cancelled at `boot_timeout` (flag records the timeout). `fail()` first-wins (AtomicBool CAS), stage Failed with detail, then after `fail_grace` → `kill("failed")`. `kill()` once: set kill_reason, stage Terminating (detail kept), `killer.kill_self(fresh token)`, then signal killed. Tunnel exit watcher: `fail("cloudflared exited: …")` **before** cancelling the boot token. Watch loop: tick at once, then every `tick`. Tick: llama metrics only when Ready (sample ok on success); GPU metrics only after the GPU check passed (`runner.go:258`); host always; `Some` on success else `None`; watchdog observe (when Ready) + decide; `metrics_failures`, `idle_s`, `kill_in_s`; `kill_reason` = next reason unless Terminating; kill decision → spawn `kill(reason)`. `run()` → `Ok(())` when killed, `Err(Cancelled)` on cancel.

`testutil::fake_deps(killer: Arc<FakeKiller>) -> Deps` = Go `okDeps`: tunnel never exits, gpu ok, download reports `{bytes 5, total 10}`, llama never exits, healthy at once, llama metrics zero, gpu `RTX 5090`, host error. Each fake is a struct with closure fields so tests override one piece. `cfg()` = Go `cfg()` (tick 5 ms, grace 20 ms, idle 1 h, expires +1 h, boot 1 min). `wait_stage(r, stage) -> Status` (2 s), `wait_done(join)` (3 s).

- [ ] Failing tests: `happy_path` (download bytes 5, model q8, llama Some, gpu Some, host None, no kill), `status_timings_record_stage_seconds` (new: after Ready, `ready_at` non-zero, `gpu_check_s >= 0`).
- [ ] Implement. `cargo test -p lobo-agent runner::tests::happy_path runner::tests::status_timings` → 2 passed.
- [ ] Commit: `lobo-agent: Runner boot + status`.

## Task 20 — Runner failure paths

**Files:** Modify `src/runner.rs`.

- [ ] Failing tests (Go names → snake_case, same assertions): `download_fails` (detail has `sha mismatch`, 1 kill, reason `failed`), `download_hangs_hits_boot_timeout` (boot 30 ms → `boot timeout`), `never_healthy` (1 kill), `llama_exits_after_ready` (`llama-server exited`), `no_gpu_fails_before_download` (detail starts `gpu: `, download not called), `llama_exits_while_loading` (`exited while loading`), `tunnel_exit_during_download` (`cloudflared exited`), `tunnel_exit_is_the_reported_cause` (30 iterations), plus `stage_detail_bad_host_markers` (new: gpu check error → detail starts `gpu: `; download error `host: download too slow: …` → detail `download: host: …`, contains `host: `).
- [ ] Implement. `cargo test -p lobo-agent runner` → all pass. Run `runner` tests 5× in a row (`for i in 1 2 3 4 5; do cargo test -p lobo-agent runner -q || break; done`) → no flake.
- [ ] Commit: `lobo-agent: Runner failure handling (first cause wins, grace, self-kill)`.

## Task 21 — Runner watchdog paths

**Files:** Modify `src/runner.rs`.

- [ ] Failing tests: `expires_during_download` (expires +30 ms → reason `expired`, 1 kill), `expired_at_start` (1 kill), `idle_kill_once` (idle 40 ms → 1 kill, reason `idle`), `busy_not_killed` (prompt tokens grow each read; idle 40 ms; 150 ms → 0 kills).
- [ ] Implement. `cargo test -p lobo-agent runner` → all 15 runner tests pass.
- [ ] Commit: `lobo-agent: Runner watchdog kills (idle, expiry)`.

## Task 22 — /api router

**Files:** Create `src/api.rs`.

Locked: `pub fn router(runner: Arc<Runner>, api_key: String, logs: LogSource, version: bytes::Bytes) -> axum::Router;` (contract change: `version` is the raw `release.json`, see end). Routes and bodies = "Wire and text rules" 1–3.

- [ ] Failing test `api_routes` (Go `TestAPI`, via `tower::ServiceExt::oneshot` or a bound listener): version body byte-equal; status key set exact (runner built from `fake_deps`, not running → `gpu`/`llama` null); logs without key / wrong key → 401; `n=5` → 5 lines; `n=5000` → 1000 lines (ring of 2000 with 1500 lines).
- [ ] Implement. `cargo test -p lobo-agent api` → 1 passed.
- [ ] Commit: `lobo-agent: /api/version, /api/status, /api/logs`.

## Task 23 — Pod GPU check

**Files:** Create `src/pod.rs` (GPU parts).

Locked:
```rust
pub const LLAMA_BIN: &str = "/app/llama-server";  pub const CUDA_LD_PATH: &str = "/app:/usr/local/cuda/lib64";
pub fn free_mib(list_devices_out: &str) -> Option<i64>;   // regex-lite `CUDA0: .*\((\d+) MiB, (\d+) MiB free\)` → group 2
pub struct GpuCheckCfg { pub llama_bin: PathBuf, pub model_id: String, pub min_free_mib: i64,
                         pub give_up_after: Duration, pub retry_every: Duration, pub try_timeout: Duration, pub env: Vec<(String,String)>, pub logs: LogSource }
impl GpuCheckCfg { pub fn pod(model: &lobo_proto::catalog::Model, env: Vec<(String,String)>, logs: LogSource) -> Self; } // 3 min, 10 s, 1 min, min_free_mib(size)
pub struct PodGpuCheck(pub GpuCheckCfg);   // impl GpuCheck
```
= `main.go:103-136`: run `<llama_bin> --list-devices` with `env` + `LD_LIBRARY_PATH=CUDA_LD_PATH`, `kill_on_drop(true)`, each try under `try_timeout` (dropping the output future kills the child); output has `CUDA0:` → write it to logs tagged `gpu-check`; `free_mib` missing or < min → `only <free> MiB VRAM free, <id> needs <min> MiB`; else Ok. No `CUDA0:` → if elapsed > `give_up_after` → `llama-server sees no CUDA device after <n> tries / <d>: <last_line>`; else wait `retry_every` (cancel → `Cancelled`).

Fake `llama-server` scripts keep a counter in a temp file; the hung one uses `exec sleep 30` so the kill reaches the sleeping process.

- [ ] Failing tests: `free_mib_reads_cuda0_free` (Go `TestFreeMiB`), `gpu_check_retries_until_cuda0` (fails 2×, then `CUDA0: NVIDIA GeForce RTX 5090 (32109 MiB, 31000 MiB free)`, retry 10 ms → Ok, counter 3), `gpu_check_gives_up_after_window` (never CUDA0, give-up 50 ms → `sees no CUDA device`), `gpu_check_hung_try_is_bounded` (1st try hangs, try timeout 100 ms, then ok → Ok within 5 s), `gpu_check_short_vram_is_bad_host` (q8, 20000 MiB free → `only 20000 MiB VRAM free, q8 needs 29831 MiB`; 29831 = (28595762272 >> 20) + 2560 = 27271 + 2560), `gpu_check_defaults_are_3min_10s_1min`.
- [ ] Implement. `cargo test -p lobo-agent pod::tests::gpu pod::tests::free_mib` → 6 passed.
- [ ] Commit: `lobo-agent: pod CUDA check with retries and VRAM floor`.

## Task 24 — Pod model download policy

**Files:** Modify `src/pod.rs`.

Locked:
```rust
pub const MODEL_DIR: &str = "/models";  pub const SLOW_CHECK_AFTER: Duration = Duration::from_secs(20);  pub const SSH_MAX_CONNS: usize = 8;
pub struct SlowGate { .. }
impl SlowGate { pub fn new(after: Duration, floor_mbps: f64, redacted_src: String) -> Self;
                pub fn observe(&mut self, elapsed: Duration, mbps: f64) -> Option<String>; } // decides once, after `after`
pub async fn download_any<F, Fut>(cancel: &CancellationToken, urls: &[String], try_one: F, on_switch: impl Fn(&Error, &str)) -> Result<()>
    where F: Fn(usize, String) -> Fut, Fut: Future<Output = Result<()>>;
pub type MakeSource = Arc<dyn Fn(&str) -> Result<Arc<dyn Source>> + Send + Sync>;
pub struct ModelDownload { pub urls: Vec<String>, pub dst: PathBuf, pub size: i64, pub sha: String, pub chunk_sha: Vec<String>,
                           pub conns: usize, pub min_mbps: f64, pub slow_check_after: Duration, pub make_source: MakeSource }
impl ModelDownload { pub fn conns_for(&self, url: &str) -> usize; }   // ssh:// → min(conns, 8)
// impl runner::Download for ModelDownload
```
= `main.go:137-150,358-402`: urls = `[LOBO_MODEL_URL, LOBO_MODEL_URL_FALLBACK?]`; `download_any` tries in order, returns the first Ok, stops on cancel, switches on **any** error (logs `source failed, switching` with the redacted next URL), returns the last error. Per source: child token; `SlowGate` checked in the progress callback; a slow verdict stores the cause `host: download too slow: <x.x> MB/s after <n>s from <redacted> (min <floor>)` and cancels the child; after return, a stored cause wins over the `Cancelled` error unless the parent was cancelled. After a switch, every progress report carries `source = redact_url(url)`.

- [ ] Failing tests: `download_any_falls_back_on_any_error` (Go test, 3 parts), `slow_gate_decides_once_after_20s` (19 s @ 10 → None; 21 s @ 99.9 → Some(exact text above with `https://h` and `min 100`); later calls → None; 21 s @ 150 → None and never fires later), `slow_source_fails_with_host_marker` (fake `Source` that trickles 1 KiB / 50 ms; `slow_check_after` 100 ms; floor 1000 → error contains `host: download too slow`), `r2_slow_switches_to_fallback_source` (first URL = trickle source, second = `RangeServer`; Ok; file equal; a progress report after the switch has `source == redact_url(second)`), `ssh_source_conns_capped_at_8` (`conns 32`: `ssh://…` → 8, `https://…` → 32).
- [ ] Implement. `cargo test -p lobo-agent pod::tests` → all pass.
- [ ] Commit: `lobo-agent: model download policy (slow host, fallback source, ssh cap)`.

## Task 25 — Pod tunnel, llama, timings, child env

**Files:** Modify `src/pod.rs`.

Locked:
```rust
pub const CLOUDFLARED_URL: &str = "https://github.com/cloudflare/cloudflared/releases/download/2026.9.1/cloudflared-linux-amd64";
pub const BIN_DIR: &str = "/lobo/bin";  pub const RELEASE_JSON: &str = "/lobo/release.json";
pub struct PodTunnel { pub bin: PathBuf, pub url: String, pub token: String, pub env: Vec<(String,String)>, pub logs: LogSource }  // impl Tunnel
pub struct PodLlama { pub bin: PathBuf, pub args: LlamaArgs, pub api_key: String, pub env: Vec<(String,String)>,
                      pub logs: LogSource, pub life: CancellationToken, pub health_base: String, pub poll: Duration }   // impl Llama
pub fn tunnel_env(clean: &[(String,String)], token: &str) -> Vec<(String,String)>;   // clean + TUNNEL_TOKEN
pub fn llama_env(clean: &[(String,String)], api_key: &str) -> Vec<(String,String)>;  // clean + LD_LIBRARY_PATH=CUDA_LD_PATH + LLAMA_API_KEY
pub fn boot_timings(get: &dyn Fn(&str) -> Option<String>) -> Timings;               // LOBO_T_BOOT0/APT/ZIP, main.go:296-310
```
`PodTunnel::start`: `fetch_file(url, bin, 0o755, -1, "")` under the boot token, then `start_process(bin, ["tunnel","--no-autoupdate","run"], tunnel_env, logs, Some("cloudflared"), life)`. `PodLlama::start`: `start_process(bin, args.to_args(), llama_env, logs, Some("llama"), life)`; `wait_healthy(health_base, 2 s)`.

- [ ] Failing tests: `pod_children_env_has_only_their_own_secret` (clean = `CleanEnv::filter` of a map with `LOBO_API_KEY`, `CF_TUNNEL_TOKEN`, `RUNPOD_API_KEY`, `CONTAINER_API_KEY`, `LLAMA_ARG_HOST`, `PATH`; a fake child that dumps `env` → tunnel child has `TUNNEL_TOKEN` and none of the others; llama child has `LLAMA_API_KEY` + `LD_LIBRARY_PATH=/app:/usr/local/cuda/lib64` and none of the others), `boot_timings` (t0 100.5, apt 110.5, zip 112 → started `1970-01-01T00:01:40.5Z`, apt 10, zip 1.5; zip without apt → zip 0; no t0 → zero), `pod_tunnel_fetches_then_starts` (wiremock serves a shell script as "cloudflared" that prints its args; log ring gets `[cloudflared] tunnel --no-autoupdate run`).
- [ ] Implement. `cargo test -p lobo-agent pod::tests` → all pass.
- [ ] Commit: `lobo-agent: pod tunnel and llama hooks, boot timings`.

## Task 26 — Binary: run, version, bench, fatal paths

**Files:** Modify `src/pod.rs`, `src/main.rs`.

Locked:
```rust
pub fn init_logging(logs: LogSource);            // tracing json → stdout + ring, field service="lobo-agent"
pub async fn run(get: &dyn Fn(&str) -> Option<String>, logs: LogSource) -> Result<()>;  // = main.go run(): config, catalog, release.json, Collector, Deps, Runner, api on AGENT_ADDR
pub async fn main_flow<F>(run: F, api: Option<Arc<dyn PodApi>>, kill_budget: Duration) -> std::process::ExitCode
    where F: Future<Output = Result<()>> + Send + 'static;
pub fn bench_line(source: &str, conns: usize, bytes: i64, mbps: f64, err: Option<&str>) -> String;  // one JSON object; source cut to 40 chars
```
`main_flow`: spawn `run`; `Ok` → exit 0; `Err(e)` → stderr `lobo-agent fatal: <e>`; `JoinError::is_panic` → stderr `lobo-agent fatal: panic: <msg>`; both → `api` present ? `RetryKiller.kill_self` with a `kill_budget` deadline (10 min in main) : stderr `lobo-agent: no instance id/key for provider "<p>", cannot self-terminate`; exit 1. `main.rs`: clap `lobo-agent [version|bench]`; no subcommand → build the runtime, `init_logging`, SIGTERM/SIGINT → exit 143, `main_flow(pod::run(env), self_api(env LOBO_PROVIDER, RUNPOD_POD_ID, RUNPOD_API_KEY, CONTAINER_ID, CONTAINER_API_KEY), 10 min)`. `bench --conns 1,4,8 --seconds 45 --url URL --model q8`: no `--url` → `model_source` from `LOBO_MODEL_URL` + key vars; one `bench_line` per conns value. `run()` details from `main.go:67-184`: model from catalog (unknown → fatal), `release.json` missing → `{"version":"unknown"}`, `timings.download_conns` = `dl_conns`, `timings.download_source` = `scheme://host` of `LOBO_MODEL_URL`, Runner tick 30 s, fail grace 2 min, API server on `AGENT_ADDR` (bind error logged, not fatal).

- [ ] Failing tests: `fatal_error_self_terminates` (wiremock GraphQL `RunPodSelf`: run returns `Err` → 1 `podTerminate` POST with UA + `Bearer podkey`, then `pod:null` → exit code 1), `panic_in_run_self_terminates` (same with `panic!`), `bench_line_shape` (keys `source conns bytes mbps` + `err` only when set; 60-char source cut to 40), `bin_tests::bin_fatal_without_creds_exits_1` (spawn `env!("CARGO_BIN_EXE_lobo-agent")` with `env_clear()` and only `PATH` → exit 1, stderr has `lobo-agent fatal: config:` and `cannot self-terminate`; no network), `bin_tests::bin_version` (`version` → `dev\n`).
- [ ] Implement. `cargo test -p lobo-agent` → everything green. `cargo clippy -p lobo-agent --all-targets -- -D warnings` → clean.
- [ ] Commit: `lobo-agent: binary with fatal-path self-terminate`.

## Task 27 — musl static build (local)

**Files:** Modify `Makefile`.

- [ ] One-time tools (orchestrator runs; implementer may if missing): `brew install zig`, `cargo install cargo-zigbuild --locked`, `rustup target add x86_64-unknown-linux-musl`.
- [ ] Makefile:
  ```make
  rust-agent:
  	cargo zigbuild --release --locked -p lobo-agent --target x86_64-unknown-linux-musl
  	file target/x86_64-unknown-linux-musl/release/lobo-agent | grep -Eq 'statically linked|static-pie linked'
  	ls -l target/x86_64-unknown-linux-musl/release/lobo-agent
  ```
- [ ] Verify: `make rust-agent` → exit 0. `file` says `ELF 64-bit LSB … x86-64 … static(-pie) linked … stripped`. Record the size next to Go's 12.3 MB (informational only; no gate).
- [ ] Record `zig version`, `cargo zigbuild --version` in "Pinned versions".
- [ ] Commit: `make rust-agent: static musl lobo-agent via cargo-zigbuild`.

## Task 28 — Dockerfile + layout drift test

**Files:** Modify `docker/pod/Dockerfile`, `.dockerignore`. Create test module `docker_tests` in `src/lib.rs` (`#[cfg(test)]`).

Dockerfile (locked shape; not built on the laptop):
```dockerfile
# Pod image: llama.cpp server + lobo-agent (Rust, static musl) baked in. `lobo up --image` (or LOBO_POD_IMAGE)
# rents this instead of the plain llama image; the bootstrap skips apt + the release zip.
# Built and pushed by .github/workflows/pod-image.yml only; LLAMA_IMAGE = lobo_proto DEFAULT_LLAMA_IMAGE.
ARG LLAMA_IMAGE

FROM rust:1.98-alpine AS build
ARG VERSION=dev
ARG GIT_SHA=none
ARG LLAMA_IMAGE
RUN apk add --no-cache musl-dev
WORKDIR /src
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY .cargo ./.cargo
COPY crates ./crates
RUN --mount=type=cache,target=/usr/local/cargo/registry --mount=type=cache,target=/src/target \
    LOBO_VERSION="$VERSION" cargo build --release --locked -p lobo-agent --target x86_64-unknown-linux-musl && \
    mkdir -p /out && cp target/x86_64-unknown-linux-musl/release/lobo-agent /out/lobo-agent
RUN printf '{"version":"%s","git_sha":"%s","built_by":"github-actions","llama_image":"%s"}\n' \
      "$VERSION" "$GIT_SHA" "$LLAMA_IMAGE" > /out/release.json

FROM ${LLAMA_IMAGE}
COPY --from=build /out/lobo-agent /lobo/lobo-agent
COPY --from=build /out/release.json /lobo/release.json
```
The `cp` sits in the same `RUN` because a cache mount is not part of the layer. `.dockerignore`: add `target`.

- [ ] Failing test `docker_tests::dockerfile_keeps_lobo_layout`: reads `../../docker/pod/Dockerfile` (`include_str!`), asserts it contains `COPY --from=build /out/lobo-agent /lobo/lobo-agent`, `COPY --from=build /out/release.json /lobo/release.json`, `FROM ${LLAMA_IMAGE}`, `--target x86_64-unknown-linux-musl`. Doc comment: the bootstrap execs `/lobo/lobo-agent` (`internal/bootstrap/bootstrap.go:45,60`).
- [ ] Implement. `cargo test -p lobo-agent docker_tests` → 1 passed. `hadolint docker/pod/Dockerfile` if installed, else say it was not run.
- [ ] Commit: `pod image: build lobo-agent in Rust (musl, static)`.

## Task 29 — CI: pod-image.yml + rust.yml musl job (orchestrator pushes)

**Files:** Modify `.github/workflows/pod-image.yml`, `.github/workflows/rust.yml`.

pod-image.yml changes:
- `paths:` → `crates/**`, `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`, `.cargo/**`, `docker/pod/**`, `.github/workflows/pod-image.yml`. Go paths removed (on this branch the image no longer contains Go).
- Llama pin step: `llama=$(sed -n 's/.*DEFAULT_LLAMA_IMAGE: &str = "\(.*\)";/\1/p' crates/lobo-proto/src/release.rs)`; the `[ -n "$llama" ] || exit 1` guard stays. The P1 drift test (`lobo-proto` release test) keeps it equal to Go's `DefaultLlamaImage` until P6.
- Header comment: "lobo-agent (Rust)".

rust.yml new job `agent-musl` (ubuntu-latest, same checkout/toolchain/cache steps as P1):
1. `sudo apt-get install -y musl-tools && rustup target add x86_64-unknown-linux-musl`
2. `cargo build --release --locked -p lobo-agent --target x86_64-unknown-linux-musl`
3. `file target/x86_64-unknown-linux-musl/release/lobo-agent | tee /dev/stderr | grep -Eq 'statically linked|static-pie linked'`
4. `ls -l …/lobo-agent >> "$GITHUB_STEP_SUMMARY"` (size, informational)

- [ ] Write both. `actionlint` if installed, else say it was not run.
- [ ] Confirm the pin line in `crates/lobo-proto/src/release.rs` is exactly `pub const DEFAULT_LLAMA_IMAGE: &str = "…";` — the sed depends on it. Local check: `sed -n 's/.*DEFAULT_LLAMA_IMAGE: &str = "\(.*\)";/\1/p' crates/lobo-proto/src/release.rs` → `ghcr.io/ggml-org/llama.cpp:server-cuda-b11118`.
- [ ] Commit. Orchestrator pushes `feat/rust`; `gh run watch` on both workflows → green. Red → `gh run view --log-failed`, fix, push, repeat.
- [ ] Record the image digest from the pod-image run summary (`ghcr.io/1905/lobocode@sha256:…`) in this plan's "As-built" line.

## Task 30 — Live check (ORCHESTRATOR ONLY — costs money)

⚠ Rents real GPUs (RunPod ~$0.69/h, Vast similar; expected total ≈ $1). Implementers never run any step here. The Go CLI drives it; the pod API is unchanged, so the Go CLI works as is.

Preconditions:
- [ ] Task 29 green; digest recorded. `lobo status --json` → `"down": true` (nothing running; `up` refuses otherwise).
- [ ] Go CLI built from this branch (`make build-lobo`, Go code unchanged) and the user's normal config.
- [ ] r2.dev note: `--image` skips the bucket manifest on the laptop (`control/up.go:84-85`), the default source `r2` is a presigned `*.r2.cloudflarestorage.com` URL fetched by the pod, and `lobo test` goes through the tunnel domain. So the home `*.r2.dev` DNS hijack should not matter. Not verified. If any laptop call resolves `*.r2.dev`, use the Dell pinned-IP workaround (it is not written down in the repo; the orchestrator knows it from earlier sessions or asks the user).

RunPod:
- [ ] `bin/lobo up --provider runpod --image ghcr.io/1905/lobocode@sha256:<digest> --idle-min 2 --plain` → ready. The `ready` line shows `release` = the image ref.
- [ ] `bin/lobo status --json` → `status.stage == "ready"`, `status.timings.gpu_check_s > 0`, `download_mbps > 0`, `gpu` not null. Compare `download_mbps` with the last Go boots in `boots.jsonl` (same source) and note the numbers.
- [ ] `bin/lobo test` → every check passes.
- [ ] `bin/lobo logs` → has `[llama]` and `[cloudflared]` lines and JSON agent lines with `"service":"lobo-agent"`.
- [ ] Self-terminate path: stop sending requests; wait ≤ 3 min (idle 2 min + 30 s tick). `bin/lobo status` → pod gone. This proves `RetryKiller` + GraphQL `podTerminate` + the custom UA on RunPod.
- [ ] Safety net: `bin/lobo down` (no-op if gone) → provider list empty.

Vast:
- [ ] Same five steps with `--provider vast`. The idle kill proves the Vast DELETE path with `CONTAINER_API_KEY`.

Failure handling: any step red → `bin/lobo down` at once, capture `lobo logs` + `lobo status --json`, fix on the branch, rebuild image (Task 29), repeat. Do not leave an instance running.

- [ ] Write the numbers (boot seconds per stage, MB/s, cost per provider) into "As-built".
- [ ] `/notify`: "Rust agent live check done: RunPod <ok/fail>, Vast <ok/fail>. Digest <…>. Approve P3 plan next."

## Task 31 — Phase close (orchestrator)

- [ ] `make rust-lint rust-test rust-agent && go test ./...` → all green; `git status` clean.
- [ ] Every Go test row in the mapping table below has a passing Rust test (`cargo test -p lobo-agent -- --list | wc -l` ≥ 70).
- [ ] Update `plan-p1-v1.0.md` Go-test table (moved rows, see File map → Modify). Update `contracts.md` with the accepted additions (new version v1.1).
- [ ] Plan status → `done` (P2). Spec status line unchanged except "P2 done".
- [ ] Merge hygiene: `feat/rust` stays open until P6 by design. `git merge master` into it every few days. A master change under `internal/agent`, `internal/metrics`, `internal/watchdog`, `cmd/lobo-agent` or `docker/pod`/`pod-image.yml` must be ported to `lobo-agent` in the same merge.

---

## Go test → Rust test (every Go test in the P2 packages)

53 Go test functions in `cmd/lobo-agent`, `internal/agent`, `internal/metrics`, `internal/watchdog` (measured `grep -c '^func Test'`), plus 5 moved from P3 packages because their code moves to `lobo-agent`. 58 total, none dropped.

| Go file | Go test | Rust test | Task |
|---|---|---|---|
| cmd/lobo-agent/main_test.go | TestFreeMiB | `pod::tests::free_mib_reads_cuda0_free` | 23 |
| | TestDownloadAnyFallsBackOnAnyError | `pod::tests::download_any_falls_back_on_any_error` | 24 |
| | TestSelfAPIPerProvider | `selfkill::tests::self_api_per_provider` | 17 |
| internal/agent/api_test.go | TestAPI | `api::tests::api_routes` | 22 |
| internal/agent/download_test.go | TestDownload | `download::tests::download_basic` | 10 |
| | TestDownloadResumesAfterDrop | `download::tests::download_resumes_after_drop` | 10 |
| | TestDownloadStallRespectsDeadline | `download::tests::download_stall_respects_cancel` | 11 |
| | TestLogRing | `logring::tests::log_ring` | 5 |
| | TestHTTPSourceErrorsHideSecretURL | `source::tests::http_source_errors_hide_secret_url` | 9 |
| | TestDownloadResumesPartialFile | `download::tests::download_resumes_partial_file` | 10 |
| | TestRangeTotal | `source::tests::range_total` | 9 |
| internal/agent/env_test.go | TestCleanEnv | `process::tests::clean_env` | 5 |
| internal/agent/hang_test.go | TestDownloadParallelNoHeaders | `download::tests::parallel_no_headers` | 14 |
| | TestDownloadSingleStalledStream | `download::tests::single_stalled_stream` | 11 |
| | TestSSHSourceSilentHandshake | `source::ssh_tests::ssh_silent_handshake` | 16 |
| | TestDownloadParallelBadTableNoTickerLeak | `download::tests::parallel_bad_table_no_ticker_leak` | 14 |
| internal/agent/killer_test.go | TestKillSelfRetries | `selfkill::tests::kill_self_retries` | 17 |
| | TestKillSelfWaitsForGone | `selfkill::tests::kill_self_waits_for_gone` | 17 |
| | TestKillSelfCtx | `selfkill::tests::kill_self_cancel` | 17 |
| internal/agent/llama_test.go | TestLlamaArgs | `process::tests::llama_args` | 5 |
| internal/agent/parallel_test.go | TestDownloadParallelHTTP | `download::tests::parallel_http` | 13 |
| | TestDownloadParallelBadSHA | `download::tests::parallel_bad_sha` | 13 |
| | TestDownloadParallelSSH | `source::ssh_tests::parallel_ssh` | 16 |
| | TestBench | `download::tests::bench` | 14 |
| | TestDownloadParallelChunkSHA | `download::tests::parallel_chunk_sha` | 13 |
| | TestDownloadParallelChunkSHAMismatch | `download::tests::parallel_chunk_sha_mismatch` | 13 |
| | TestDownloadParallelStalledStream | `download::tests::parallel_stalled_stream` | 14 |
| internal/agent/proc_test.go | TestStartProcess | `process::tests::start_process` | 6 |
| | TestLastLine | `process::tests::last_line` | 5 |
| internal/agent/runner_test.go | TestHappyPath | `runner::tests::happy_path` | 19 |
| | TestDownloadFails | `runner::tests::download_fails` | 20 |
| | TestDownloadHangsHitsBootTimeout | `runner::tests::download_hangs_hits_boot_timeout` | 20 |
| | TestNeverHealthy | `runner::tests::never_healthy` | 20 |
| | TestExpiresDuringDownload | `runner::tests::expires_during_download` | 21 |
| | TestExpiredAtStart | `runner::tests::expired_at_start` | 21 |
| | TestIdleKillOnce | `runner::tests::idle_kill_once` | 21 |
| | TestBusyNotKilled | `runner::tests::busy_not_killed` | 21 |
| | TestLlamaExitsAfterReady | `runner::tests::llama_exits_after_ready` | 20 |
| | TestNoGPUFailsBeforeDownload | `runner::tests::no_gpu_fails_before_download` | 20 |
| | TestLlamaExitsWhileLoading | `runner::tests::llama_exits_while_loading` | 20 |
| | TestTunnelExitDuringDownload | `runner::tests::tunnel_exit_during_download` | 20 |
| | TestTunnelExitIsTheReportedCause | `runner::tests::tunnel_exit_is_the_reported_cause` | 20 |
| internal/agent/source_ssh_test.go | TestSSHSourceResume | `source::ssh_tests::ssh_resume` | 16 |
| | TestSSHSourceRejectsWrongHostKey | `source::ssh_tests::ssh_rejects_wrong_host_key` | 16 |
| internal/metrics/metrics_test.go | TestParseLlama | `metrics::tests::parse_llama` | 3 |
| | TestParseLlamaFixture | `metrics::tests::parse_llama_fixture` (no skip: fixture is committed) | 3 |
| | TestParseNvidiaSMI | `metrics::tests::parse_nvidia_smi` | 3 |
| | TestParseHost | `metrics::tests::parse_host` | 3 |
| | TestCollector | `metrics::tests::collector` | 4 |
| | TestCollectorTimeout | `metrics::tests::collector_timeout` | 4 |
| internal/watchdog/watchdog_test.go | TestDecide (8 rows) | `watchdog::tests::decide` (same 8 rows) | 2 |
| | TestExpiredAtStart | `watchdog::tests::expired_at_start` | 2 |
| | TestFailedSamples | `watchdog::tests::failed_samples` | 2 |
| internal/config/config_test.go (moved from P3) | TestLoadAgent | `config::tests::load_agent` | 18 |
| | TestLoadAgentErrors | `config::tests::load_agent_errors` | 18 |
| | TestLoadAgentPerProvider | `config::tests::load_agent_per_provider` | 18 |
| internal/runpod/runpod_test.go (moved from P3) | TestSelf | `selfkill::tests::runpod_self` | 17 |
| internal/vast/vast_test.go (moved from P3) | TestSelfTerminateAndGone | `selfkill::tests::vast_self_terminate_and_gone` | 17 |

New Rust tests with no Go twin (carry-over rules, UA, env isolation, helpers): `http::client_sends_user_agent`, `http_source_status_mapping`, `default_tuning`, `start_process_tags_lines`, `start_process_does_not_inherit_parent_env`, `start_process_kill_token_kills`, `wait_healthy_*` (2), `fetch_file_*` (3), `hash_file_*` (2), `range_server_*` (2), `fake_feesh_serves_range`, `ssh_picks_pinned_ed25519_over_ecdsa`, `model_source_parses`, `runpod_self_sends_user_agent`, `vast_self_sends_user_agent`, `min_mbps_default_100`, `boot_timeout_go_syntax`, `status_timings_record_stage_seconds`, `stage_detail_bad_host_markers`, `gpu_check_*` (4), `slow_gate_decides_once_after_20s`, `slow_source_fails_with_host_marker`, `r2_slow_switches_to_fallback_source`, `ssh_source_conns_capped_at_8`, `pod_children_env_has_only_their_own_secret`, `boot_timings`, `pod_tunnel_fetches_then_starts`, `fatal_error_self_terminates`, `panic_in_run_self_terminates`, `bench_line_shape`, `bin_fatal_without_creds_exits_1`, `bin_version`, `dockerfile_keeps_lobo_layout`.

## As-built

_(Task 29: image digest. Task 27: binary size vs Go 12.3 MB. Task 30: per-provider stage seconds, MB/s, cost.)_

---

## Self-review

**Spec P2 requirements → task**
- `lobo-agent` lib: runner (19–21), watchdog (2), download (9–14, 16), metrics (3–4), `/api` (22), process helpers (5–6), `CleanEnv` (5), `LlamaArgs` (5) ✓
- Static Linux binary `x86_64-unknown-linux-musl` (26–27, CI 29) ✓
- Baked image builds it (28–29) ✓
- One live boot per provider + `lobo test` from the Go CLI (30) ✓
- Does NOT change the pod API shape (Wire rules 1–3, `api_routes` exact key set, raw `/api/version`) ✓
- Does NOT change the bootstrap contract (`/lobo/lobo-agent`, `/lobo/release.json`, same env names in `AgentConfig`; drift test 28) ✓
- Carry-over rules for the agent: all rows in the carry-over table have a named test; `execfail` explicitly routed to P3 ✓
- Tests: every Go test mapped (58/58) ✓. Fakes: temp-dir scripts, wiremock, in-process SSH ✓
- Rollout P2 line (Dockerfile, image CI, live RunPod + Vast) ✓

**Names vs contracts.md v1.0:** `Deps` fields `tunnel gpu_check download llama metrics killer` ✓; `RunnerConfig` fields ✓; `Runner::{new,status,run}` ✓; `api::router` ✗ one param type changed (see below); `source::{Source, HttpSource, SshSource, redact_url}` ✓ (+ `size` field); `download::{download, download_parallel, hash_file, bench}` ✓ (progress param `Option`); `watchdog::{State, Config, Decision}` ✓ (`Decision` is an enum as the contract says); `metrics::Collector { llama_url, .. }` ✓; `process::{CleanEnv, clean_env, LlamaArgs::to_args}` ✓.

**Placeholders:** only the "Pinned versions" and "As-built" lines, filled by Tasks 1, 27, 29, 30 by design. No TODOs.

## Contract changes and additions (for contracts.md v1.1)

**Change (needs orchestrator OK):**
- `api::router(runner, api_key, logs, version: bytes::Bytes)` instead of `version: Manifest`. Reason: Go serves `release.json` bytes verbatim; re-encoding a typed `Manifest` changes the `/api/version` body.

**Clarifications of existing entries:**
- `download` / `download_parallel` / `hash_file` take `on_progress: Option<&(dyn Fn(DownloadProgress) + Sync)>`; `chunk_sha` empty = no table.
- `SshSource` gains `pub size: i64`; `key: Arc<russh::keys::PrivateKey>`, `host_key: russh::keys::PublicKey`.
- `Source: Display` (redacted description, used in errors); `pub type BoxRead = Box<dyn AsyncRead + Send + Unpin>`.
- `LogSource = Arc<LogRing>`; `LogRing::{new, write, tail}`.
- `watchdog`: `Sample`, `Reason { Idle, Expired }`, `Decision::{Kill{reason}, Wait{reason, kill_in}}`, `State::{new, set_ready, observe, idle_for, failed_samples, decide}`.
- Traits `Tunnel`, `GpuCheck`, `Download`, `Llama`, `Metrics`, `Killer` are `#[async_trait]` with the signatures in Task 19.

**Additions (P3 local supervisor will use most of them):**
- `lobo_agent::{VERSION, LLAMA_ADDR, AGENT_ADDR}`, `Error::{Permanent, Cancelled, Msg, Io}`.
- `http::{USER_AGENT, client()}`.
- `process::{CleanEnv::{is_stripped, filter}, start_process, Proc, last_line}`, `LlamaArgs { model_path, alias, host, port, ctx }`.
- `health::wait_healthy`, `fetch::fetch_file`.
- `source::{range_total, model_source}`, `download::{MAX_RESUMES, Tuning, with_tuning}` (`with_tuning` is `#[doc(hidden)]`, tests only).
- `metrics::{SMI_ARGS, parse_llama, parse_nvidia_smi, parse_host}`, `Collector { llama_url, api_key, http, nvidia_smi, proc_dir }`.
- `selfkill::{PodApi, RetryKiller, RunPodSelf, VastSelf, self_api}`.
- `config::AgentConfig::from_vars`.
- `pod::*` (bin-only wiring; public so the bin crate can call it, not for other crates).
- `lobo_proto::Stage::as_str()` (small P1 crate edit, Task 0/19).

## Spec issues found

1. "SSH source ≤ 8 streams" is enforced on the laptop (`internal/control/up.go:150-152`), not in the agent. P2 adds an agent-side clamp as a second guard; P3 must keep the laptop clamp.
2. `execfail` is in the bootstrap script (`internal/bootstrap/bootstrap.go:56-61`), which is P3. P2 can only keep the exec path stable.
3. Go never sets a User-Agent (`internal/runpod/self.go:31-32`, `internal/vast/client.go:77-78`); it sends `Go-http-client/1.1` by default. reqwest sends none, so Rust must set one on every request. The spec rule is right, but it is new code, not a port. Whether RunPod GraphQL accepts `lobo-agent/<ver>` is unverified until Task 30.
4. `contracts.md` types `/api/version` as `Manifest`; Go serves raw bytes. Contract change requested above.
5. `contracts.md` lists "macOS sysctl/ps" under `lobo_agent::metrics`; in Go these live in `internal/local/sysctl_*.go` with local tests. P2 does not port them; the P3 plan decides the home.
6. The Dell pinned-IP workaround for the `*.r2.dev` hijack is not written down anywhere in the repo (only `spec.md:101`).
7. `Dockerfile` copies only `crates/`. If P5 adds `app/src-tauri` as a workspace member, the image build breaks (missing member). P5 must keep it out of `members` or the Dockerfile must copy it.
8. `CleanEnv` does not strip `R2_*` (`internal/agent/env.go:12-13`). The pod never has R2 keys; the local llama-server on the laptop could inherit exported `R2_*` shell vars. Unchanged in P2 (parity); flag for P3.
9. Go's API server has `ReadHeaderTimeout: 10s` (`cmd/lobo-agent/main.go:177`); `axum::serve` has no such knob. The agent binds `127.0.0.1` only and cloudflared is the only client, so P2 accepts the gap.


---
<!-- end of plan-p2-v1.0.md -->

# Rust rewrite P3 — `lobo-core` Implementation Plan v1.0

**Date:** 2026-09-29
**Status:** draft
**Spec:** ./spec.md (spec status: parked — this plan is written on request; exec still needs spec + plan approval)
**Contracts:** ./contracts.md v1.0 (names and signatures used exactly; extras listed under "Contract additions")
**Phase:** P3 of 6. Needs P1 (`lobo-proto`) and P2 (`lobo-agent`) done on `feat/rust`.

**Goal:** `crates/lobo-core` holds all laptop logic: config file, RunPod/Vast/local providers, up/status/down/target, `lobo test` checks, release (zip, secret scan, R2), bootstrap script, local supervisor, opencode genkey. Behaviour = Go at `5443667`, proven by a Rust port of every Go test in these packages plus golden files written by the Go code.

**Architecture:** Go stays untouched and keeps building until P6 (the Go CLI and the Swift app still use it). `lobo-core` depends on `lobo-proto` (wire types) and `lobo-agent` (Runner, API router, downloader, process helpers). A second Go dumper, `tools/corefixtures`, writes golden files from the real Go functions (`config.Save`, `godotenv.Read`, `bootstrap.Script/Env`, `runpod.BuildCreatePayload`, `vast.CreateBody`, the `gen-api-key` output). Rust tests compare bytes against them. CI re-runs the dumper, so any Go change shows up as a diff. No live calls in P3: every HTTP peer is `wiremock`, every child process is a fake script or a test helper binary.

**Tech Stack:** Rust 1.98 (edition 2024). tokio, tokio-util (CancellationToken), reqwest (rustls, json, stream), async-trait, serde + serde_json, chrono, thiserror, tracing, url, rand, hex, sha2, base64, rusty-s3, zip, tar + flate2, nix (signal, process, fs), libc (sysctlbyname). Dev: wiremock, tempfile. No russh in P3: the SSH model source lives in `lobo-agent` (P2).

> For agentic workers: use superpowers:subagent-driven-development to implement task-by-task. Checkbox syntax for tracking.

> Implementer scope (verbatim, every dispatch): writes only the code and unit tests its task names and runs that task's focused unit test. Never runs e2e / integration / live / smoke tests, never rents a GPU or pod, never calls a provider API, never publishes, deploys or touches infra, never runs anything money-bearing. Never sets `TEST_DATABASE_URL` or any test-DB env var.

Note on the scope quote: the `tests/*.rs` files in this plan are cargo integration-test *files* with local fakes only (wiremock, temp dirs, a helper binary). They are unit-level by the rule above: no network, no provider, no money. Implementers run them.

---

## File map

**Create**
- `crates/lobo-core/Cargo.toml` — deps on `lobo-proto`, `lobo-agent` (path, workspace). Feature `testkit` (exposes `control::testkit` to P4). `[[bin]] name = "lobo-core-testchild"`, `path = "src/bin/testchild.rs"`, `test = false`, `doc = false`.
- `crates/lobo-core/src/lib.rs` — module list + re-exports. Doc: "Laptop logic shared by the lobo CLI and the app. Go-compatible: same config file, same state file, same pod env."
- `crates/lobo-core/src/error.rs` — `Error`, `Result`, `Error::kind`.
- `crates/lobo-core/src/clock.rs` — `Clock`, `SystemClock`, `FixedClock`, `StepClock`.
- `crates/lobo-core/src/http.rs` — `client(timeout)`, `USER_AGENT`.
- `crates/lobo-core/src/config/mod.rs` — `Laptop`, `R2Creds`, `load_laptop`, `require_*`, `secret_values`, `Defaults`, `defaults`, `weights`, `port`, `providers`, `default_provider`, `parse_local_port`, `DEFAULT_LOCAL_PORT`, `default_path`, `loose_mode`.
- `crates/lobo-core/src/config/dotenv.rs` — port of godotenv v1.5.1 `parseBytes` (read side).
- `crates/lobo-core/src/config/envfile.rs` — `LAYOUT`, `HEADER`, `save`, `set_env_value`, `values`, `quote`, `sort_by_layout`.
- `crates/lobo-core/src/config/show.rs` — `PLAIN_KEYS`, `mask`, `masked`, `show`.
- `crates/lobo-core/src/provider/mod.rs` — `Provider`, `CreateOpts`, `POD_NAME`, `on_domain`; `pub mod local { pub use crate::local::provider::*; }`.
- `crates/lobo-core/src/provider/runpod.rs` — `RunPodTime`, `Pod`, `Machine`, `Client`, `RunPodApi`, `RunPodProvider`, `build_create_payload`, `NET_TIERS`, `GPU_TYPE`, `is_no_capacity`.
- `crates/lobo-core/src/provider/vast.rs` — `Offer`, `Inst`, `Client`, `search_query`, `create_body`, `VastProvider`, `DISK_GB`, `DEFAULT_BASE_URL`.
- `crates/lobo-core/src/bootstrap.rs` — `script`, `env`.
- `crates/lobo-core/src/control/mod.rs` — `UpOpts`, `Deps`, `AgentApi`, `ReleaseResolver`, `Presigner`, `list_all`, consts.
- `crates/lobo-core/src/control/agent_http.rs` — `HttpAgent`.
- `crates/lobo-core/src/control/up.rs` — `up`, `cloud_opts`, `boot`, `retriable`, `new_boot_id`.
- `crates/lobo-core/src/control/status.rs` — `snapshot`, `down`, `target`.
- `crates/lobo-core/src/control/precheck.rs` — `check_target`, `check_providers`, `check_release`, `apply_defaults`.
- `crates/lobo-core/src/control/wiring.rs` — `Wiring`, `providers_from_config`, `local_provider_from_config`, `deps_from_config`.
- `crates/lobo-core/src/control/testkit.rs` — `#[cfg(any(test, feature = "testkit"))]` port of `internal/control/controltest/fakes.go`.
- `crates/lobo-core/src/control/tests.rs` — `#[cfg(test)]` port of `internal/control/control_test.go`.
- `crates/lobo-core/src/checks.rs` — `validate_tool_call`, `read_stream`, `chat`, `tool_call`.
- `crates/lobo-core/src/release/mod.rs` — `next_version`, `zip_key`, `meta_key`, `LATEST_KEY`, `BUCKET`, `zip_url`, `resolve`, `BucketReleases`.
- `crates/lobo-core/src/release/zip.rs` — `build_zip`, `scan_for_secrets`.
- `crates/lobo-core/src/release/store.rs` — `Store` (rusty-s3 + reqwest).
- `crates/lobo-core/src/local/mod.rs` — re-exports + contract free functions (`state_path`, `read_state`, …).
- `crates/lobo-core/src/local/platform.rs` — `supported`, `usable_mib`, `mem_bytes`, `wired_limit_mib`, `sysctl_string`.
- `crates/lobo-core/src/local/state.rs` — `StateFile`, `alive`.
- `crates/lobo-core/src/local/models.rs` — `HF_BASE`, `marker_path`, `marker_line`, `marker_valid`, `write_marker`, `list`, `free_space`.
- `crates/lobo-core/src/local/runtime.rs` — `RUNTIME_VERSION`, `RuntimePin`, `runtime_dir`, `runtime_note`, `ensure_runtime`, `untar`, `find_server`.
- `crates/lobo-core/src/local/deps.rs` — `MacConfig`, `MacDeps`, `new_deps`, `parse_vm_stat`, `host_gpu`.
- `crates/lobo-core/src/local/provider.rs` — `Spawner`, `LocalProvider`, `LocalHooks`, `is_supervisor`, `command_of`, `log_path`, `instance`.
- `crates/lobo-core/src/local/supervise.rs` — `RunConfig`, `RunConfig::from_args`, `supervise`, `SUPERVISOR_ARG`.
- `crates/lobo-core/src/genkey.rs` — `new_api_key`, `ensure_api_key`, `opencode_config`, `write_opencode`, `OPENCODE_OUT`.
- `crates/lobo-core/src/bin/testchild.rs` — stand-in for `lobo local run` (port of `internal/local/provider_test.go` `TestMain`/`helperChild`).
- `crates/lobo-core/tests/config_os_env.rs` — one test, own process: OS env never read.
- `crates/lobo-core/tests/local_provider.rs` — spawn/delete tests against the helper binary.
- `crates/lobo-core/tests/go_interop.rs` — flock + state file against a Go process; runs only when `LOBO_GO_INTEROP=1`.
- `crates/lobo-core/tests/contracts_api.rs` — compile-only check that every contracts.md P3 name exists with its signature.
- `crates/lobo-core/fixtures/**` — written by `tools/corefixtures` (list in Task 4) + `runpod/pod.json` (copied from `internal/runpod/testdata/pod.json`).
- `tools/corefixtures/main.go` — Go golden dumper for lobo-core (removed with Go at P6).

**Modify**
- `Cargo.toml` (workspace) — `[workspace.dependencies]` gains the P3 crates.
- `Makefile` — `core-fixtures` target.
- `.github/workflows/rust.yml` — core fixtures drift check + `LOBO_GO_INTEROP=1` test run; add `tools/corefixtures/**` to the path filter.
- `plans/2026-09-29-rust-rewrite/spec.md` — file table row for `tools/corefixtures/`; status line.
- `plans/2026-09-29-rust-rewrite/contracts.md` — v1.1 with "Contract additions" folded in (orchestrator, Task 0).

**Out of scope for P3**
- `lobo-cli` (clap, ratatui, wizard, `config set` parsing, `models` text output, `lobo test` printing), the app, `lobo-e2e`.
- Any code in `lobo-agent` or `lobo-proto`. If P3 needs something P2/P1 did not ship, Task 0 stops and the orchestrator adds it in that crate first.
- Pod-side code: `config.LoadAgent`, `runpod.Self`, `vast.Self` (the pod binary is `lobo-agent`, which cannot depend on `lobo-core`). See "Spec issues" 1.
- Any Go change outside `tools/corefixtures/`. `docker/`, `pod-image.yml`, `release.yml`, `.goreleaser.yaml`.

---

## Rules (locked)

**Wire rules:** P1 plan "Wire rules" 1–10 apply to every serde type P3 adds. Provider JSON (`Pod`, `Offer`, `Inst`) uses the provider's field names via `#[serde(rename)]`, `#[serde(default)]` on decode, unknown fields ignored.

**Go int mapping:** Go `int` in a struct → `i64`, except ports → `u16`. Durations → `std::time::Duration`. Times → `chrono::DateTime<Utc>` inside lobo-core; `lobo_proto::GoTime` only on wire types.

**Env and time in tests:** no test calls `std::env::set_var` except `tests/config_os_env.rs` (its own process, one test). Every function that reads `HOME`, `XDG_*` or the process env has a pure twin that takes the value as an argument (`default_path_from`, `StateFile::at`, `MacConfig.base_env`). Tests use the twin.

**HTTP:** every reqwest client in lobo-core comes from `http::client(timeout)`. It sets `User-Agent: lobo/<CARGO_PKG_VERSION>` and uses rustls. reqwest sends no User-Agent by default, and RunPod refuses requests without a real one (plans/done/2026-09-25-vast-provider/results.md:36).

**Errors:** `Error` Display text keeps the Go wording where a Go test or a user reads it (each task names the string). `Error::kind()` is stable snake_case for the app.

## Assumed `lobo-agent` API (P2)

From contracts.md: `Deps` + traits `Tunnel`, `GpuCheck`, `Download`, `Llama`, `Metrics`, `Killer`; `Runner`, `RunnerConfig`; `api::router(runner, api_key, logs: LogSource, version: Manifest)`; `source::HttpSource`; `download::{download, hash_file}`; `process::{clean_env, LlamaArgs}`; `metrics::Collector`.

Also used here, as locked in `plan-p2-v1.0.md` (Task 0 re-checks them against the built crate; if P2 shipped a different name, Task 0 records the mapping and every task uses P2's name):
- `logring::{LogRing, LogSource}` — `LogRing::new(n)`, `write`, `tail(n)`; `LogSource = Arc<LogRing>`; `impl std::io::Write for &LogRing`.
- `process::{clean_env, LlamaArgs { model_path, alias, host, port, ctx }, start_process, Proc { pid, exited }, last_line}` — `start_process(program, args, env: &[(String,String)], logs: LogSource, tag: Option<&'static str>, kill: CancellationToken) -> Result<Proc>`; it always `env_clear()`s, so the caller passes the full child env.
- `health::wait_healthy(base: &str, poll: Duration, cancel: CancellationToken) -> Result<()>`.
- `fetch::fetch_file(url: &str, dst: &Path, mode: u32, size: i64, sha: &str) -> Result<()>` (5 min bound, size + sha check).
- `download::download(cancel, src: &dyn Source, dst, sha, on_progress: Option<&dyn Fn(DownloadProgress)>)` renames a sha mismatch to `<dst>.bad` (Go `agent.Download`, used by `local/deps.go:148`). `download::hash_file` likewise takes `Option<&dyn Fn>`.
- `selfkill::{RunPodSelf, VastSelf}` — pod self-terminate lives in P2, not here.

`clean_env`, `LlamaArgs`, `download`, `hash_file`, `Runner`, `router` are never copied into lobo-core: goal 3 of the spec is one shared agent library. A missing item → Task 0 stops and it is added to `lobo-agent` first.

## Pinned versions

_(Task 1 fills this line from `Cargo.lock`: tokio, tokio-util, reqwest, async-trait, axum, rusty-s3, zip, tar, flate2, nix, libc, url, rand, hex, sha2, base64, wiremock, tempfile.)_

---

## Task 0 — Preconditions (orchestrator, no implementer)

- [ ] P1 and P2 are `done` on `feat/rust`. `git merge master` into `feat/rust` first (merge-hygiene rule); fix any fixture drift.
- [ ] Tree clean on `feat/rust`. `cargo test --workspace` and `go test ./...` green. Record both counts.
- [ ] Check "Assumed `lobo-agent` API" against the real crate: `cargo doc -p lobo-agent --no-deps` and read the public items. Write the name mapping (or "all match") into this file under the section. A missing must-share item (not one of the three small ones) → stop, add it to P2's crate first, then continue.
- [ ] Fold "Contract additions" (end of this file) into `contracts.md` v1.1 after the user approves this plan.
- [ ] Update `spec.md`: status line; file-table row `tools/corefixtures/ | Go golden dumper for lobo-core tests. Removed with Go at P6.`
- [ ] Commit: `plans: rust P3 plan; contracts v1.1; spec file table adds tools/corefixtures`.

---

## A. Crate skeleton

## Task 1 — `lobo-core` crate + pinned versions

**Files:** Create `crates/lobo-core/Cargo.toml`, `src/lib.rs`. Modify workspace `Cargo.toml`.

- [ ] `cargo new --lib crates/lobo-core --vcs none`; workspace member conventions as P1 (`edition.workspace`, `rust-version.workspace`, `[lints] workspace = true`).
- [ ] `cargo add -p lobo-core`: `tokio --features rt-multi-thread,macros,time,process,sync,fs,io-util,net,signal`, `tokio-util`, `reqwest --no-default-features --features rustls-tls,json,stream`, `async-trait`, `axum` (serves the agent router in `supervise`), `serde --features derive`, `serde_json`, `chrono --no-default-features --features std,clock,serde`, `thiserror`, `tracing`, `url`, `rand`, `hex`, `sha2`, `base64`, `rusty-s3`, `zip --no-default-features --features deflate`, `tar`, `flate2`, `nix --features signal,process,fs`, `libc`, path deps `lobo-proto`, `lobo-agent`. Dev: `wiremock`, `tempfile`, `tokio --features test-util`.
- [ ] Move versions into `[workspace.dependencies]`; crate uses `x.workspace = true`.
- [ ] `[features] testkit = []`.
- [ ] `lib.rs`: module declarations only (`error, clock, http, config, provider, bootstrap, control, checks, release, local, genkey`), each module file an empty stub with its doc line.
- [ ] Verify: `cargo build -p lobo-core` → exit 0. `cargo clippy -p lobo-core -- -D warnings` → exit 0.
- [ ] Fill "Pinned versions" from `Cargo.lock`.
- [ ] Commit: `lobo-core: crate skeleton + pinned deps`.

## Task 2 — `Error`

**Files:** `src/error.rs`.

Locked interface:
```rust
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")] Config(String),                       // kind "config"; messages start "config: " like Go
    #[error("config: {}", fmt_bad(.0))] Defaults(BTreeMap<String, String>), // kind "config"; "K: msg; K2: msg" sorted
    #[error("provider: no gpu capacity: {0}")] NoCapacity(String),         // kind "no_capacity"
    #[error("provider: instance not found")] NotFound,                     // kind "not_found"
    #[error("vast: account has no credit (top up at https://cloud.vast.ai/billing/)")] NoCredit, // "no_credit"
    #[error("vast: offer rejected: {0}")] Rejected(String),               // kind "rejected"
    #[error("{0}")] AlreadyRunning(String),                               // kind "already_running"
    #[error("{0}")] Api(String),                                          // kind "provider_api" (non-2xx, message = Go text)
    #[error(transparent)] Http(#[from] reqwest::Error),                   // kind "network"
    #[error(transparent)] Io(#[from] std::io::Error),                     // kind "io"
    #[error(transparent)] Json(#[from] serde_json::Error),                // kind "json"
    #[error(transparent)] Agent(#[from] lobo_agent::Error),               // kind "agent"
    #[error("{0}")] Local(String),                                        // kind "local"
    #[error("{0}")] Release(String),                                      // kind "release"
    #[error("cancelled")] Cancelled,                                      // kind "cancelled"
    #[error("{}", join_lines(.0))] Multi(Vec<Error>),                     // kind "multi"; Go errors.Join = "\n"
    #[error("{0}")] Other(String),                                        // kind "other"
}
pub type Result<T> = std::result::Result<T, Error>;
impl Error { pub fn kind(&self) -> &'static str; pub fn is_not_found(&self) -> bool; pub fn is_no_capacity(&self) -> bool; }
```
- [ ] Failing tests (`error::tests`): `kind_table` (one row per variant); `defaults_display_sorted` (`{"LOBO_MODEL": "x", "LOBO_CTX": "y"}` → `config: LOBO_CTX: y; LOBO_MODEL: x`); `multi_joins_with_newline`; `no_capacity_display_contains_go_text` (`"no gpu capacity"`).
- [ ] Implement. `cargo test -p lobo-core error` → pass (4 tests).
- [ ] Commit: `lobo-core: typed Error with stable kinds`.

## Task 3 — `Clock` + HTTP client with User-Agent

**Files:** `src/clock.rs`, `src/http.rs`.

Locked interface:
```rust
pub trait Clock: Send + Sync { fn now(&self) -> DateTime<Utc>; }
pub struct SystemClock;                       // Utc::now()
pub struct FixedClock(pub DateTime<Utc>);     // tests: always the same instant
pub struct StepClock { /* Mutex<(t0, n)>, step */ } impl StepClock { pub fn new(t0: DateTime<Utc>, step: Duration) -> Self; } // n-th call = t0 + n*step (Go test clocks)
pub const USER_AGENT: &str = concat!("lobo/", env!("CARGO_PKG_VERSION"));
pub fn client(timeout: Duration) -> reqwest::Client;   // rustls, UA, timeout; panics only if the TLS backend fails to init
```
- [ ] Failing tests: `step_clock_advances_per_call` (t0+1s, t0+2s); `client_sends_user_agent` (wiremock: `GET /` matched with `header_regex("user-agent", "^lobo/")` → 200; unmatched → 404 fails the test). Carry-over rule "RunPod needs a UA" — cite `plans/done/2026-09-25-vast-provider/results.md:36`.
- [ ] Implement. `cargo test -p lobo-core clock http` → pass (2).
- [ ] Commit: `lobo-core: clock + reqwest client that always sends a User-Agent`.

## Task 4 — Go golden dumper `tools/corefixtures`

**Files:** Create `tools/corefixtures/main.go`, `crates/lobo-core/fixtures/**`. Copy `internal/runpod/testdata/pod.json` → `crates/lobo-core/fixtures/runpod/pod.json`. Modify `Makefile`.

**Choice: a Go dumper, not captured files.** Why: the Go code stays live until P6 and `master` keeps changing it. A dumper re-runs in CI and turns any Go change into a `git diff`. Captured files would go stale without a signal. Same pattern as P1's `tools/protofixtures`. A separate program (not a P1 extension) because it needs other packages and a built `lobo` binary, and P1's tool stays about wire types only.

Interface: `go run ./tools/corefixtures dump <outdir>` writes everything below, deterministic (fixed times, fixed keys). Two extra modes for `tests/go_interop.rs`: `holdlock <path> <ms>` (open, `syscall.Flock LOCK_EX`, print `locked\n`, sleep, exit 0) and `trylock <path>` (`LOCK_EX|LOCK_NB`, print `free` or `busy`), `readstate <xdg_state_home>` (sets `XDG_STATE_HOME`, calls `local.ReadState()`, prints `{"ok":…,"pid":…,"boot_id":…}`).

Fixtures:

| Path under `fixtures/` | Go source | Cases |
|---|---|---|
| `dotenv/<case>.env` + `<case>.json` | `godotenv.Read` | `plain`, `export_prefix`, `comments_and_blank`, `inline_comment`, `double_quoted_escapes` (`\n \r \" \\ \$`), `single_quoted`, `expand_earlier_key` (`A=x`, `B=$A-${A}`), `expand_os_home_is_empty` (`H=$HOME`; dumper runs with HOME set — output must be `""`), `escaped_dollar`, `yaml_colon` (`K: v`), `crlf`, `no_trailing_newline`, `empty_value`, `unterminated_quote` (json `{"error": "<go message>"}`), `bad_key_char` (error) |
| `config_save/<case>.before.env` (absent = no file) + `<case>.set.json` + `<case>.after.env` | `config.Save` | `new_file` (set of TestSaveNewFileLayout), `new_file_all_keys` (every Layout key + `ZZ_EXTRA`, `AA_EXTRA`), `keeps_hand_edits` (TestSaveKeepsHandEdits), `set_env_value` (TestSetEnvValue, two calls → two steps `…step1`, `…step2`), `special_values` (TestSaveRoundTripSpecialValues + `"a\tb"`, `"x\ry"`, `"it's"`), `export_and_spaces` (` export  LOBO_DOMAIN = a`), `duplicate_key` (key twice: first replaced, second dropped), `empty_removes`, `commented_key_kept` (`#RUNPOD_API_KEY=x` stays, key appended), `crlf_file`, `empty_file_gets_layout` (0 bytes), `whitespace_file_is_existing` (`"\n\n"`) |
| `bootstrap/script_runpod.sh`, `script_vast.sh` | `bootstrap.Script` | bytes as returned (no trailing newline added) |
| `bootstrap/env_full.json`, `env_baked.json` | `bootstrap.Env` | CreateOpts of TestEnv / TestEnvBakedOmitsRelease + `BootID`, `ModelSSHKey`, `ModelHostKey` in `env_full` |
| `runpod/payload_community.json`, `payload_secure_5000_ssh.json` | `runpod.BuildCreatePayload` → `json.Marshal` | opts of `runpod_test.go:18`; second with `SSHPubKey` set |
| `vast/create_body.json`, `create_body_baked.json` | `vast.CreateBody` → `json.Marshal` | opts of `vast_test.go:130` with fixed `ExpiresAt`; baked per TestCreateBodyBaked |
| `opencode/cloud.json`, `opencode/local_only.json` | `lobo gen-api-key` output file | dumper `go build -o <tmp>/lobo ./cmd/lobo`, writes `<tmp>/config.env` with `LOBO_API_KEY=sk-fixed` (+ `LOBO_DOMAIN=lobo.example.com` for `cloud`, `LOBO_LOCAL_PORT=9000` for `local_only`), runs `<tmp>/lobo --config <tmp>/config.env gen-api-key` with `cmd.Dir = <tmp>`, copies `opencode.lobo.json` |

- [ ] Write the dumper. Makefile: `core-fixtures: go run ./tools/corefixtures dump crates/lobo-core/fixtures`.
- [ ] Verify: `make core-fixtures && make core-fixtures && git status --porcelain crates/lobo-core/fixtures` → only new files; the second run changes nothing. `go vet ./tools/...` clean. `go test ./...` still green.
- [ ] Eyeball (orchestrator): `dotenv/expand_os_home_is_empty.json` is `{"H":""}`; `config_save/keeps_hand_edits.after.env` equals the string in `config_test.go:173`; `bootstrap/script_vast.sh` contains `-X DELETE`.
- [ ] Commit: `rust: Go golden dumper for lobo-core`.

---

## B. Config

## Task 5 — dotenv reader (godotenv v1.5.1 port)

**Files:** `src/config/dotenv.rs`.

Locked interface: `pub fn parse(src: &str) -> Result<BTreeMap<String, String>>`. Port of `parseBytes`/`locateKeyName`/`extractVarValue`/`expandEscapes`/`expandVariables` (godotenv v1.5.1 `parser.go`). Variable expansion reads only keys parsed earlier in the same file. It never reads the process env. Errors: `Error::Config` with Go's text (`unterminated quoted value …`, `unexpected character … in variable name near …`).

Why a port and not `dotenvy`: dotenvy's substitution falls back to the process env. That breaks the carry-over rule "config is read only from the file" (`internal/config/laptop.go:53-55`).

- [ ] Failing test `dotenv::tests::golden_cases`: for every `fixtures/dotenv/*.env`, `parse` equals the `.json` map, or errors when the json is `{"error": …}` (error text compared with `contains` on the first 20 chars of the Go message).
- [ ] Failing test `dotenv::tests::never_reads_process_env`: `parse("H=$HOME\nP=${PATH}\n")` → `{"H": "", "P": ""}` (cites `laptop.go:53-55`).
- [ ] Implement. `cargo test -p lobo-core config::dotenv` → pass (2).
- [ ] Commit: `lobo-core: dotenv reader, godotenv-compatible, file-only expansion`.

## Task 6 — `quote`, `LAYOUT`, `HEADER`, `sort_by_layout`

**Files:** `src/config/envfile.rs`.

Locked interface:
```rust
pub struct LayoutGroup { pub title: &'static str, pub keys: &'static [&'static str] }
pub const LAYOUT: &[LayoutGroup];   // same three groups, titles and key order as internal/config/envfile.go:12-21
pub const HEADER: &str;             // byte-identical to envfile.go:23-28
pub(crate) fn quote(v: &str) -> String;                 // envfile.go:105-110
pub(crate) fn sort_by_layout(keys: &mut Vec<String>);    // envfile.go:112-133
```
- [ ] Failing tests: `quote_table` (`plain-123` unquoted; each of space, tab, `#`, `"`, `'`, `\`, `$`, `\n`, `\r` → double-quoted with `\\ \" \$ \n \r` escapes); `sort_by_layout_then_alpha` (`["ZZ","LOBO_CTX","RUNPOD_API_KEY","AA"]` → `RUNPOD_API_KEY, LOBO_CTX, AA, ZZ`); `header_matches_go` (HEADER equals the first 5 lines of `fixtures/config_save/new_file.after.env` + `\n`).
- [ ] Implement. `cargo test -p lobo-core config::envfile` → pass (3).
- [ ] Commit: `lobo-core: config layout, header and quoting`.

## Task 7 — `save` into an existing file

**Files:** `src/config/envfile.rs`.

Locked interface:
```rust
pub fn save(path: &Path, set: &BTreeMap<String, String>) -> Result<()>;  // envfile.go:30-98
pub fn set_env_value(path: &Path, key: &str, value: &str) -> Result<()>;
pub fn values(path: &Path) -> Result<BTreeMap<String, String>>;         // missing file = empty map (path.go:27-37)
```
Rules: line-by-line as Go (`TrimSpace`, strip `export ` prefix, cut at first `=`, comment lines untouched). First matching line replaced (or dropped if the value is empty), later duplicates dropped. Unmatched keys appended in `sort_by_layout` order. Output = lines joined by `\n` + `\n`. Atomic: temp file `.lobo-config-*` in the same dir, mode 0600, rename. Temp file removed on every error path.

- [ ] Failing tests (Go ports): `set_env_value_replaces_and_appends` (TestSetEnvValue: exact bytes `"# comment\nA=1\nLOBO_API_KEY=new\nB=2\nC=3\n"`, mode 0600); `save_keeps_hand_edits` (TestSaveKeepsHandEdits: exact bytes of `config_test.go:173`, `values` reads back `ssh-ed25519 AAAA#x`, `LOBO_DOMAIN` gone, mode 0600).
- [ ] Implement. `cargo test -p lobo-core config::envfile` → pass (5).
- [ ] Commit: `lobo-core: config save keeps comments, order and unknown keys`.

## Task 8 — `save` new file + golden byte-compat

**Files:** `src/config/envfile.rs`.

Rules: a missing or 0-byte file gets `HEADER` + each `LAYOUT` group (`""`, `"# <title>"`, set keys in group order), then the rest. Parent dir created 0700.

- [ ] Failing tests: `save_new_file_layout` (TestSaveNewFileLayout: starts `# lobo config`, `VASTAI_API_KEY=vk` before `LOBO_PROVIDER=vast`, ends `R2_ENDPOINT=https://e\n`, no `RUNPOD_API_KEY=`, dir mode 0700, `values(missing)` empty); `save_round_trip_special_values` (TestSaveRoundTripSpecialValues: A–F round-trip through `values`); `save_matches_go_bytes` (for every `fixtures/config_save/<case>`: copy `.before.env` to a temp path if present, apply each `.set.json` step with `save`, compare bytes to `.after.env` — must be identical).
- [ ] Implement. `cargo test -p lobo-core config::envfile` → pass (8).
- [ ] Commit: `lobo-core: config save byte-identical to Go config.Save`.

## Task 9 — `Laptop` + `load_laptop`

**Files:** `src/config/mod.rs`. Create `tests/config_os_env.rs`.

Locked interface:
```rust
#[derive(Debug, Clone, Default, PartialEq)]
pub struct R2Creds { pub account_id: String, pub access_key: String, pub secret_key: String, pub endpoint: String }
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Laptop {  // one field per env key, internal/config/laptop.go:17-43
    pub runpod_api_key: String, pub lobo_api_key: String, pub cf_tunnel_token: String, pub domain: String,
    pub bucket_url: String, pub model_source: String, pub model_ssh_key_file: String, pub model_ssh_host_key: String,
    pub min_mbps: String, pub feesh_http_url: String, pub vast_api_key: String, pub vast_max_dph: String,
    pub pod_image: String, pub provider: String, pub model: String, pub cloud: String, pub ctx: String,
    pub idle_min: String, pub max_hours: String, pub weights_dir: String, pub local_port: String, pub r2: R2Creds }
impl Laptop { pub fn from_values(m: &BTreeMap<String, String>) -> Laptop; }   // field ← env name, missing = ""
pub fn load_laptop(path: &Path) -> Result<Laptop>;
```
Rules (`laptop.go:56-76`): read the file with `dotenv::parse`. `LOBO_API_KEY` required → `config: LOBO_API_KEY: required`. `LOBO_BUCKET_URL` if set must be a URL (`url::Url::parse` ok and scheme non-empty) → `config: LOBO_BUCKET_URL: url`. `ssh://` model source needs key file + host key (Go text). Model source must be empty, `r2` or `ssh://…` (Go text). R2 fields not checked here. Missing file → `Error::Config("read <path>: …")`.

- [ ] Failing tests (Go ports, `config::tests`): `load_laptop_full` (TestLoadLaptop: fields, `require_r2` ok, `secret_values` has the 5 keys — both methods land in Task 10/9; mark this test `#[ignore = "Task 10"]` until then, remove the ignore in Task 10); `load_laptop_vast_only`; `load_laptop_no_r2_then_require_r2_fails` (same ignore note); `load_laptop_ignores_bad_defaults`; `load_laptop_local_only` (first half; the `require_cloud` half joins in Task 10).
- [ ] Failing test `tests/config_os_env.rs::load_laptop_ignores_os_env` (TestLoadLaptopMissing): `unsafe { set_var("RUNPOD_API_KEY", "from-os") }` first line, file without the key → `runpod_api_key == ""`, `require_cloud()` error names `RUNPOD_API_KEY`. Carry-over "config only from the file" — cite `internal/config/config_test.go:49-60`, `laptop.go:53-55`.
- [ ] Implement. `cargo test -p lobo-core config:: --test config_os_env` → the non-ignored tests pass.
- [ ] Commit: `lobo-core: Laptop config loaded from the file only`.

## Task 10 — `require_*` + `secret_values`

**Files:** `src/config/mod.rs`.

Locked (contracts.md): `require_cloud`, `require_provider_key`, `require_bucket`, `require_r2`, `secret_values -> BTreeMap<String,String>`. Messages = Go (`laptop.go:78-134`): `config: cloud needs CF_TUNNEL_TOKEN, LOBO_DOMAIN`, `config: set RUNPOD_API_KEY or VASTAI_API_KEY`, `config: set LOBO_BUCKET_URL`, `config: LOBO_BUCKET_URL: want a URL, got "…"`, `config: R2_ACCOUNT_ID: required; R2_ACCESS_KEY: required; …` (Go field order). `secret_values` reads `model_ssh_key_file` and adds `LOBO_MODEL_SSH_KEY_FILE` + `LOBO_MODEL_SSH_KEY_FILE (base64)` when readable.

- [ ] Failing tests: `require_cloud` (TestRequireCloud); `require_parts_table` (TestRequireParts, 9 rows); `secret_values_includes_ssh_key_file` (new: temp key file → both entries, base64 standard). Remove the Task 9 `#[ignore]`s and finish `load_laptop_local_only`.
- [ ] Implement. `cargo test -p lobo-core config::` → pass (all config tests so far, none ignored).
- [ ] Commit: `lobo-core: config require checks and secret list`.

## Task 11 — `defaults()` + `parse_local_port`

**Files:** `src/config/mod.rs`.

Locked interface:
```rust
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Defaults { pub provider: String, pub model: String, pub cloud: String, pub ctx: i64, pub idle_min: i64,
                      pub max_hours: i64, pub min_mbps: i64, pub vast_max_dph: f64 }
impl Laptop { pub fn defaults(&self) -> Result<Defaults>; }       // bad keys → Err(Error::Defaults(bad)); good fields still parsed
pub fn defaults_partial(l: &Laptop) -> (Defaults, BTreeMap<String, String>); // both halves; `defaults()` wraps it
pub const DEFAULT_LOCAL_PORT: u16 = 8931;
pub fn parse_local_port(v: &str) -> Result<u16>;  // "" or "0" → 8931; else 1024..=65534; error text "want a port 1024-65534 (or empty), got \"v\""
```
Rules = `path.go:73-121` (one-of lists, `num` minimums 512/1/1/1, `≥`, `$/h > 0`).

- [ ] Failing tests: `defaults_parse_and_reject` (TestDefaults); `defaults_local_port` (TestDefaultsLocal); `defaults_partial_keeps_good_fields` (new: bad `LOBO_CTX` + good `LOBO_MODEL=q6` → model `q6`, bad map has only `LOBO_CTX`).
- [ ] Implement. `cargo test -p lobo-core config::` → pass.
- [ ] Commit: `lobo-core: launch defaults from the config`.

## Task 12 — paths, weights, port, providers

**Files:** `src/config/mod.rs`.

Locked interface:
```rust
pub fn default_path() -> PathBuf;                                                 // contracts.md
pub fn default_path_from(xdg_config_home: Option<&str>, home: Option<&Path>) -> PathBuf; // path.go:16-25
pub fn loose_mode(path: &Path) -> bool;                                          // perm & 0o077 != 0
impl Laptop {
  pub fn weights(&self) -> PathBuf; pub fn weights_with_home(&self, home: &Path) -> PathBuf;  // path.go:127-137
  pub fn port(&self) -> u16;                                                     // bad/empty/0 → 8931
  pub fn providers(&self) -> Vec<String>;                                         // ["runpod","vast"] by key presence
  pub fn default_provider(&self) -> String; }                                     // path.go:173-187
```
- [ ] Failing tests: `default_path_xdg_and_home` (TestDefaultPath via `default_path_from`); `weights_and_port` (TestWeightsPort via `weights_with_home`); `default_provider_table` (TestDefaultProvider, 7 rows); `loose_mode_bits` (new: 0600 false, 0644 true).
- [ ] Implement. `cargo test -p lobo-core config::` → pass.
- [ ] Commit: `lobo-core: config paths, weights folder, port, provider choice`.

## Task 13 — `show` + masking

**Files:** `src/config/show.rs`.

Locked interface:
```rust
pub const PLAIN_KEYS: &[&str];                 // cmd/lobo/config.go:204-209, same 18 keys
pub fn mask(s: &str) -> String;                // internal/configtui/configtui.go:22-30: "" → "(not set)", <12 bytes → "••••", else first4…last4
pub fn masked(k: &str, v: &str) -> String;     // plain key → v, else mask(v)
pub fn show(path: &Path) -> Result<lobo_proto::ConfigShow>;  // path, exists, values (masked), set (v != "")  — cmd/lobo/config.go:154-167
```
`mask` slices by bytes like Go when both cut points are char boundaries, else by chars (a Go panic-free equivalent; never splits UTF-8).

- [ ] Failing tests: `masked_unknown_keys` (TestMaskedUnknownKeys, moved from P4); `show_masks_secrets` (TestShowConfigJSONMasks, moved from P4: file with `RUNPOD_API_KEY=rpa_SECRETSECRETSECRET`, `LOBO_DOMAIN=lobo.x.cc`, `LOBO_CTX=` → JSON of `show` has no `SECRETSECRET`, `"LOBO_DOMAIN":"lobo.x.cc"`, `"LOBO_CTX":false`; missing file → `"exists":false`); `mask_non_ascii_no_panic` (new).
- [ ] Implement. `cargo test -p lobo-core config::show` → pass (3).
- [ ] Commit: `lobo-core: config show with secrets masked`.

---

## C. Providers + bootstrap

## Task 14 — provider core types

**Files:** `src/provider/mod.rs`.

Locked interface:
```rust
pub const POD_NAME: &str = "lobo";
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CreateOpts { pub image: String, pub release_url: String, pub release_sha256: String, pub model_url: String,
    pub model_fallback: String, pub lobo_api_key: String, pub cf_tunnel_token: String, pub model: String, pub ctx: i64,
    pub idle_min: i64, pub dl_conns: i64, pub min_mbps: i64, pub expires_at: DateTime<Utc>, pub cloud: String,
    pub ssh_pub_key: String, pub model_ssh_key: String, pub model_host_key: String, pub boot_id: String }
#[async_trait] pub trait Provider: Send + Sync {        // contracts.md, exactly
    fn name(&self) -> &'static str; fn replaceable(&self) -> bool;
    async fn rent(&self, o: &CreateOpts, note: &(dyn Fn(String) + Sync)) -> Result<Instance>;
    async fn list(&self) -> Result<Vec<Instance>>; async fn get(&self, id: &str) -> Result<Instance>;
    async fn delete(&self, id: &str) -> Result<()>; }
pub fn on_domain(i: Instance, domain: &str) -> Instance;  // api_url "https://<d>/v1", agent_url "https://<d>"
```
`Instance` = `lobo_proto::control::Instance`.
- [ ] Failing test `on_domain_sets_urls`.
- [ ] Implement. `cargo test -p lobo-core provider::tests` → pass (1).
- [ ] Commit: `lobo-core: provider trait and create options`.

## Task 15 — bootstrap script

**Files:** `src/bootstrap.rs`.

Locked (contracts.md): `pub fn script(provider: &str) -> String`. Built from Rust string constants (the Go template + the per-provider `terminate` body). Unknown provider → the Go behaviour (empty terminate body).
- [ ] Failing tests: `script_per_provider` (TestScriptPerProvider); `script_matches_go_bytes` (equals `fixtures/bootstrap/script_{runpod,vast}.sh`); `script_steps_time_bounded` (carry-over: `timeout 300 bash -c 'apt-get`, `timeout 300 curl`, terminate `curl -s -m 15` in both providers — cites `internal/bootstrap/bootstrap.go:16,20,49,51`); `script_execfail_then_die` (carry-over: contains `shopt -s execfail`, `set +e`, `exec /lobo/lobo-agent\ndie` in that order — `bootstrap.go:56-61`); `script_terminate_retries_30_then_sleeps` (contains `seq 1 30` and `sleep 600` after the loop — `bootstrap.go:32-42`).
- [ ] Implement. `cargo test -p lobo-core bootstrap` → pass (5).
- [ ] Commit: `lobo-core: bootstrap script, byte-identical to Go`.

## Task 16 — bootstrap env

**Files:** `src/bootstrap.rs`.

Locked (contracts.md): `pub fn env(o: &CreateOpts, provider: &str) -> BTreeMap<String, String>`. `LOBO_EXPIRES_AT` = UTC RFC 3339 seconds (`2026-09-25T22:00:00Z`).
- [ ] Failing tests: `env_keys` (TestEnv); `env_baked_omits_release` (TestEnvBakedOmitsRelease); `env_matches_go` (equals `fixtures/bootstrap/env_{full,baked}.json`); `env_never_has_account_secrets` (carry-over: CreateOpts has only pod-safe fields; assert no key starts with `R2_`, and no `RUNPOD_API_KEY`, `VASTAI_API_KEY` key — `bootstrap.go:64`).
- [ ] Implement. `cargo test -p lobo-core bootstrap` → pass (9).
- [ ] Commit: `lobo-core: pod env, never R2 or account keys`.

## Task 17 — bootstrap under real bash

**Files:** `src/bootstrap.rs` (tests only).

Test helper `run_script(prov, release_fails, term_answer) -> (calls, stderr)`: temp dir with fake `apt-get`, `timeout`, `sleep`, `sha256sum`, `unzip`, `curl` (same bodies as `bootstrap_test.go:93-103`), runs `bash -c script(prov)` with `PATH=<dir>:/usr/bin:/bin` and the env of `bootstrap_test.go:114-115` (via `Command::env_clear().envs(..)`, no `set_var`). Skips when `bash` is missing.
- [ ] Failing tests: `script_baked_skips_apt_and_zip` (TestScriptBaked; carry-over "baked image skips apt + zip" — `bootstrap.go:45-47`); `script_terminates_on_failure` (TestScriptTerminatesOnFailure: 1 accepted runpod terminate; 30 on GraphQL error; vast 500 → 30 DELETEs; vast 404 accepted; exec failure → 1 DELETE — carry-over "terminate retried 30×", "execfail").
- [ ] Verify: `cargo test -p lobo-core bootstrap` → pass (11), under 10 s (fake `sleep`).
- [ ] Commit: `lobo-core: bootstrap behaviour tests under bash`.

## Task 18 — RunPod `Pod` + time format

**Files:** `src/provider/runpod.rs`.

Locked interface:
```rust
#[derive(Debug, Clone, Default, PartialEq)] pub struct RunPodTime(pub Option<DateTime<Utc>>);
// Deserialize: "" or non-string → None; "2006-01-02 15:04:05.999999999 -0700 MST" first, then RFC 3339 nano; else error "runpod: time …"
// Serialize: RFC 3339 nano (Go time.MarshalJSON), None → "0001-01-01T00:00:00Z"
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Machine { pub max_download_speed_mbps: i64 /* maxDownloadSpeedMbps */, pub max_upload_speed_mbps: i64, pub disk_throughput_mbps: i64 }
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Pod { pub id: String, pub name: String, pub desired_status: String, pub image_name: String, pub cost_per_hr: f64,
    pub last_started_at: RunPodTime, pub created_at: RunPodTime, pub gpu_count: i64,
    pub port_mappings: BTreeMap<String, i64>, pub machine: Option<Machine> }   // camelCase renames per client.go:44-59
```
- [ ] Failing tests: `pod_fixture_decodes` (TestPodFixture on `fixtures/runpod/pod.json`: cost 0.69, RUNNING, hour 8, port 22 → 18180; `machine: {"redacted": true}` decodes with zeros); `runpod_time_round_trip` (TestTimeRoundTrip).
- [ ] Implement. `cargo test -p lobo-core provider::runpod` → pass (2).
- [ ] Commit: `lobo-core: RunPod pod JSON and its time format`.

## Task 19 — RunPod create payload

**Files:** `src/provider/runpod.rs`.

Locked interface:
```rust
pub const GPU_TYPE: &str = "NVIDIA GeForce RTX 5090";
pub const NET_TIERS: [f64; 5] = [10000.0, 5000.0, 2500.0, 1000.0, 0.0];
pub fn build_create_payload(o: &CreateOpts, cloud: &str, min_download_mbps: f64) -> serde_json::Value;  // payload.go:19-46
```
`cloud == ""` → `SECURE`. SSH prefix per `payload.go:14-16`. `minDownloadMbps` only when > 0, as a JSON float.
- [ ] Failing tests: `build_create_payload` (TestBuildCreatePayload: volume 0, no ports, COMMUNITY, name lobo, GPU id, 11 env keys, `LOBO_EXPIRES_AT`, no `R2_` and no `"RUNPOD_API_KEY":` in JSON, start cmd has the 5 strings); `payload_min_download` (TestPayloadMinDownload); `payload_matches_go` (`fixtures/runpod/payload_*.json`, compared as `serde_json::Value`).
- [ ] Implement. `cargo test -p lobo-core provider::runpod` → pass (5).
- [ ] Commit: `lobo-core: RunPod create payload`.

## Task 20 — RunPod REST client

**Files:** `src/provider/runpod.rs`.

Locked interface:
```rust
#[async_trait] pub trait RunPodApi: Send + Sync {                 // Go runpod.API (provider.go:17-22)
    async fn create(&self, o: &CreateOpts, cloud: &str, min_download_mbps: f64) -> Result<Pod>;
    async fn list(&self) -> Result<Vec<Pod>>; async fn get(&self, id: &str) -> Result<Pod>; async fn delete(&self, id: &str) -> Result<()>; }
pub struct Client { pub base_url: String, key: String, hc: reqwest::Client }
impl Client { pub fn new(key: &str) -> Self; pub fn with_base(key: &str, base_url: &str) -> Self; }  // base "https://rest.runpod.io/v1", 30 s, http::client
impl RunPodApi for Client { .. }
pub fn is_no_capacity(msg: &str) -> bool;   // client.go:110-115
```
Rules (`client.go:72-107`): Bearer key, JSON content type, 404 → `Error::NotFound`, ≥300 with no-capacity text → `Error::NoCapacity(msg)`, other ≥300 → `Error::Api("runpod <M> <path>: HTTP <code>: <body>")`. `delete` of a gone pod → Ok.
- [ ] Failing tests (wiremock): `client_rest_calls` (TestClient: COMMUNITY create → NoCapacity; SECURE → pod; list 1; get gone → NotFound; delete gone → Ok; delete x → Ok; unauthorized when the bearer is wrong); `client_sends_user_agent` (every request carries `user-agent: lobo/…`; carry-over — cite `plans/done/2026-09-25-vast-provider/results.md:36`); `is_no_capacity_table`.
- [ ] Implement. `cargo test -p lobo-core provider::runpod` → pass (8).
- [ ] Commit: `lobo-core: RunPod REST client with User-Agent`.

## Task 21 — RunPod provider

**Files:** `src/provider/runpod.rs`.

Locked interface: `pub struct RunPodProvider { pub api: Arc<dyn RunPodApi>, pub domain: String }`, `impl Provider` (`name` "runpod", `replaceable` true). Rules = `provider.go:36-101`: clouds `[COMMUNITY]`, or `[SECURE, COMMUNITY]` when `o.cloud == "secure"`; each tier of `NET_TIERS`; non-capacity error stops; note `"no 5090 in <CLOUD> at any network speed"` after a cloud is exhausted; `detail` `"<CLOUD>"` + `", host ≥<n> Mbps"` when n > 0. `list` keeps `name == "lobo"`. `get` of `TERMINATED` (case-insensitive) → `NotFound`. `to_instance`: `started_at` = last started, else created; `host_download_mbps` from machine.
- [ ] Failing tests (with a tiny in-test fake `RunPodApi`): `rent_walks_tiers_and_notes`; `get_terminated_is_not_found`; `list_filters_by_name`; `started_at_falls_back_to_created`. (Tier order across clouds is also covered by control tests in Task 55.)
- [ ] Implement. `cargo test -p lobo-core provider::runpod` → pass (12).
- [ ] Commit: `lobo-core: RunPod provider (cloud + network tiers)`.

## Task 22 — Vast client: types, search, errors

**Files:** `src/provider/vast.rs`.

Locked interface:
```rust
pub const DEFAULT_BASE_URL: &str = "https://console.vast.ai/api/v0";
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
pub struct Offer { pub id: i64, pub gpu_name: String, pub dph: f64 /*dph_total*/, pub inet_down: f64, pub geo: String /*geolocation*/, pub reliability: f64 /*reliability2*/ }
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
pub struct Inst { pub id: i64, pub label: String, pub status: String /*actual_status*/, pub dph: f64 /*dph_total*/, pub inet_down: f64, pub geo: String, pub start: f64 /*start_date*/ }
pub struct Client { pub base_url: String, key: String, hc: reqwest::Client }
impl Client { pub fn new(key: &str) -> Self; pub fn with_base(key: &str, base: &str) -> Self;
  pub async fn search_offers(&self, max_dph: f64, min_mbps: i64) -> Result<Vec<Offer>>; }
pub fn search_query(max_dph: f64, min_mbps: i64) -> serde_json::Value;   // client.go:110-124, inet_down gte = min_mbps*8
```
Internal `do_req` returns `(status, Result)`: 404 → `NotFound`; 401/403 → `Error::Api("vast <M> <path>: HTTP <c> (check VASTAI_API_KEY)")`; other ≥300 → a private `ApiError { code /*"error" field*/, status, text }` wrapped as `Error::Api`, with `code` kept for `create`.
- [ ] Failing tests (wiremock): `bad_key_names_env` (TestBadKey); `search_query_shape` (order `[["dph_total","asc"]]`, `verified`, `inet_down.gte == 800` for 100 MB/s, limit 10); `client_sends_user_agent`.
- [ ] Implement. `cargo test -p lobo-core provider::vast` → pass (3).
- [ ] Commit: `lobo-core: Vast client basics`.

## Task 23 — Vast client: create, list, get, destroy

**Files:** `src/provider/vast.rs`.

Locked interface:
```rust
impl Client {
  pub async fn create(&self, offer_id: i64, body: &serde_json::Value) -> Result<i64>;   // PUT /asks/<id>/ ; client.go:141-162
  pub async fn list(&self) -> Result<Vec<Inst>>;                                        // GET /instances/
  pub async fn get(&self, id: i64) -> Result<Inst>;                                     // 404 or {"instances": null} → NotFound
  pub async fn destroy(&self, id: i64) -> Result<()>; }                                 // DELETE; 404 → Ok
```
`create`: 4xx with code `insufficient_credit` → `Error::NoCredit`; other 4xx → `Error::Rejected(text)`; 5xx / transport / bad body → the raw error (the instance may exist); 200 without `success` or `new_contract` → `Rejected`.
- [ ] Failing tests (wiremock): `create_no_credit` (carry-over "insufficient_credit stops at once" — `client.go:150-153`); `create_4xx_is_rejected`; `create_5xx_is_uncertain` (error is not `Rejected`/`NoCredit`); `get_null_instances_is_not_found`; `destroy_404_is_gone` (carry-over "DELETE 404 = gone" — `client.go:187-194`).
- [ ] Implement. `cargo test -p lobo-core provider::vast` → pass (8).
- [ ] Commit: `lobo-core: Vast create/list/get/destroy with Go error semantics`.

## Task 24 — Vast create body

**Files:** `src/provider/vast.rs`.

Locked interface: `pub const DISK_GB: i64 = 80; pub fn create_body(o: &CreateOpts) -> serde_json::Value;` (`provider.go:32-42`: `client_id` "me", image, disk, label "lobo", `runtype` "ssh", `onstart` `"#!/bin/bash\n" + script("vast")`, env).
- [ ] Failing tests: `create_body_runtype_ssh` (carry-over "runtype ssh, or onstart never runs" — `vast/provider.go:31`); `create_body_baked` (TestCreateBodyBaked); `create_body_matches_go` (`fixtures/vast/create_body*.json`).
- [ ] Implement. `cargo test -p lobo-core provider::vast` → pass (11).
- [ ] Commit: `lobo-core: Vast create body`.

## Task 25 — Vast provider: rent

**Files:** `src/provider/vast.rs`.

Locked interface:
```rust
pub struct VastProvider { pub client: Client, pub max_dph: f64, pub domain: String, pub adopt_wait: Duration /* 3 s; tests 1 ms */,
                          tried: Mutex<HashSet<i64>> }
impl VastProvider { pub fn new(client: Client, max_dph: f64, domain: &str) -> Self; }
impl Provider for VastProvider { .. }   // name "vast", replaceable true
```
Rules (`provider.go:44-97`): `max_dph <= 0` → 1.20. Search, snapshot current lobo ids, walk offers skipping `tried` (mark each tried before the PUT). `NoCredit` → return at once. `Rejected` → note `"offer <id> unavailable: <err>"`, next offer. Detail `"offer <id>, <inet> Mbps down, <geo>"`. Nothing left → `NoCapacity("no untried 1× RTX 5090 offer (verified, reliability ≥0.98, ≤$<x>/h)")`.

Test fake: a wiremock-backed `FakeVast` port of `vast_test.go:19-106` (shared state behind a `Mutex`, `respond_with` closures; records PUT ids and last PUT body).
- [ ] Failing tests: `rent_skips_taken_and_tried` (TestRentFastestOfferSkipsTakenAndTried: one note, PUTs `1,2,3`, third rent → NoCapacity); `create_body_via_rent` (TestCreateBody: `runtype` ssh, label lobo, disk 80, env `LOBO_PROVIDER` vast + `LOBO_CTX` "65536", onstart has `CONTAINER_API_KEY` and no `RUNPOD`, no `R2_`/`VASTAI_API_KEY` in JSON); `rent_no_credit_stops_at_once` (TestRentNoCredit: exactly 1 PUT).
- [ ] Implement. `cargo test -p lobo-core provider::vast` → pass (14).
- [ ] Commit: `lobo-core: Vast rent, cheapest untried offer first`.

## Task 26 — Vast provider: uncertain create, list/get/delete

**Files:** `src/provider/vast.rs`.

Rules: uncertain create error → `adopt` (3 list tries, `adopt_wait` apart) a lobo instance not in the snapshot; found → return it with the offer detail; else error `"vast create offer <id>: <err> (not retrying another offer: it may have been rented — check \`lobo status\`)"`. `list` keeps label `lobo`. `get`/`delete` parse the id as i64 (bad id → `Error::Other`).
- [ ] Failing tests: `uncertain_create_never_rents_twice` (TestRentUncertainCreateNeverRentsTwice: lost reply adopts id `52607650` with 1 PUT; 5xx without instance → error, not NoCapacity, 1 PUT); `list_get_delete` (TestListGetDelete).
- [ ] Implement. `cargo test -p lobo-core provider::vast` → pass (16).
- [ ] Commit: `lobo-core: Vast never rents twice on an uncertain create`.

---

## D. Release + checks + genkey

## Task 27 — release keys, next version, zip URL

**Files:** `src/release/mod.rs`.

Locked (contracts.md): `next_version(existing: &[String], today: DateTime<Utc>) -> String`, `zip_key`, `meta_key`, `LATEST_KEY = "releases/latest.json"`. Plus `pub const BUCKET: &str = "lobo"; pub fn zip_url(r: &Resolved, bucket_url: &str) -> String` ("" when `zip_key` is empty).
- [ ] Failing tests: `next_version_table` (TestNextVersion, 4 rows); `zip_url_trims_slash_and_empty_for_baked`.
- [ ] Implement. `cargo test -p lobo-core release::tests` → pass (2).
- [ ] Commit: `lobo-core: release version and keys`.

## Task 28 — release zip + secret scan

**Files:** `src/release/zip.rs`.

Locked (contracts.md): `build_zip(agent_bin: &Path, m: &Manifest, out: &Path) -> Result<String /*sha256 hex*/>`, `scan_for_secrets(zip: &Path, secrets: &BTreeMap<String,String>) -> Result<()>`. Entries `lobo-agent` (0755, deflate) then `release.json` (0644, `serde_json::to_vec_pretty` of the manifest). Scan: values shorter than 8 bytes skipped, names in sorted order, error `release: <NAME> found in <file>, refusing to publish` (value never printed).
- [ ] Failing tests: `build_zip_and_scan` (TestBuildZipAndScan); `scan_catches_laptop_secret_values` (carry-over "secrets never in releases": a zip whose binary contains `Laptop.secret_values()["R2_SECRET_KEY"]` → error naming `R2_SECRET_KEY` — cites `internal/release/zip.go:52-82`).
- [ ] Implement. `cargo test -p lobo-core release::zip` → pass (2).
- [ ] Commit: `lobo-core: release zip + secret scan`.

## Task 29 — release resolve

**Files:** `src/release/mod.rs`.

Locked interface:
```rust
pub async fn resolve(hc: &reqwest::Client, bucket_url: &str, version: &str) -> Result<Resolved>;  // contracts.md; store.go:71-98
pub struct BucketReleases { pub bucket_url: String, pub hc: reqwest::Client }                    // 15 s client
impl control::ReleaseResolver for BucketReleases { .. }
```
URL = `<bucket>/<key>?t=<unix nanos>`. Non-200 → `Error::Release("release <name>: HTTP <code> from <key>")`, name `latest` for `""`.
- [ ] Failing test `resolve_latest_and_version` (TestResolve, wiremock; the `?t=` query is present).
- [ ] Implement (the `ReleaseResolver` trait lands in Task 50; until then implement an inherent `BucketReleases::resolve` and add the trait impl in Task 50). `cargo test -p lobo-core release::tests` → pass (3).
- [ ] Commit: `lobo-core: resolve releases from the public bucket`.

## Task 30 — R2 store: presign

**Files:** `src/release/store.rs`.

Locked interface:
```rust
pub struct Store { bucket: rusty_s3::Bucket, creds: rusty_s3::Credentials, hc: reqwest::Client }
impl Store {
  pub fn new(r2: &R2Creds) -> Result<Self>;             // endpoint URL, path style, bucket "lobo", region "auto" (minio default for R2: path style)
  pub fn presign_get(&self, key: &str, ttl: Duration) -> String;   // rusty-s3 GetObject::sign
}
impl control::Presigner for Store { .. }                  // Task 50
```
- [ ] Failing test `presign_get_shape` (parse the URL: host = endpoint host; path `/lobo/models/x.gguf`; query has `X-Amz-Algorithm=AWS4-HMAC-SHA256`, `X-Amz-Credential=<ak>/<yyyymmdd>/auto/s3/aws4_request`, `X-Amz-Expires=43200` for 12 h, `X-Amz-SignedHeaders=host`, a 64-hex `X-Amz-Signature`; the secret key never appears in the URL).
- [ ] Failing test `presign_differs_per_key`.
- [ ] Implement. `cargo test -p lobo-core release::store` → pass (2).
- [ ] Commit: `lobo-core: R2 presigned GET (rusty-s3)`.

## Task 31 — R2 store: list + publish

**Files:** `src/release/store.rs`.

Locked interface:
```rust
impl Store {
  pub fn with_endpoint_for_test(r2: &R2Creds, hc: reqwest::Client) -> Result<Self>;  // allows http:// wiremock endpoints
  pub async fn list_release_keys(&self) -> Result<Vec<String>>;   // ListObjectsV2 prefix "releases/", follows continuation tokens
  pub async fn publish(&self, zip: &Path, r: &Resolved) -> Result<()>;  // store.go:47-68
}
```
Publish: HEAD zip key and meta key first (200 → `Error::Release("release <v> already exists (<key>); run make release again")`, 404 → continue, other → error). Then PUT zip (`Content-Type: application/zip`), then meta json, then `latest.json` (`application/json`, `Cache-Control: no-cache`, pretty JSON). Every request is a presigned URL (1 h) sent with reqwest.
- [ ] Failing tests (wiremock, requests recorded): `publish_order_zip_meta_latest` (HEAD, HEAD, PUT zip, PUT meta, PUT latest; each URL query has `X-Amz-Signature`); `publish_refuses_existing_release` (HEAD 200 → error, no PUT); `list_follows_continuation` (two XML pages → all keys).
- [ ] Implement. `cargo test -p lobo-core release::store` → pass (5).
- [ ] Commit: `lobo-core: R2 publish (zip → meta → latest) and listing`.

## Task 32 — checks: tool call validation

**Files:** `src/checks.rs`.

Locked (contracts.md): `pub fn validate_tool_call(body: &[u8]) -> Result<()>`; messages = `checks.go:17-54`.
- [ ] Failing test `validate_tool_call_table` (TestValidateToolCall, 6 rows).
- [ ] Implement. `cargo test -p lobo-core checks` → pass (1).
- [ ] Commit: `lobo-core: tool-call response check`.

## Task 33 — checks: streamed chat + tool call request

**Files:** `src/checks.rs`.

Locked interface:
```rust
pub async fn read_stream<R: tokio::io::AsyncBufRead + Unpin>(r: R) -> Result<String>;   // checks.go:91-139
pub async fn chat(hc: &reqwest::Client, base_url: &str, key: &str, model: &str) -> Result<String>;   // stream, temp 0.2, max_tokens 200, same prompt
pub async fn tool_call(hc: &reqwest::Client, base_url: &str, key: &str, model: &str) -> Result<Vec<u8>>; // get_weather body of checks.go:142-156
```
Non-200 → `Error::Other("HTTP <code>: <body>")`.
- [ ] Failing tests: `read_stream_table` (TestReadStream, 5 rows); `chat_request_shape` (wiremock checks `stream: true`, bearer, path `/chat/completions`, returns an SSE body → `"hello"`); `tool_call_request_shape` (`tool_choice: "auto"`, tool name `get_weather`).
- [ ] Implement. `cargo test -p lobo-core checks` → pass (4).
- [ ] Commit: `lobo-core: lobo test checks (chat stream + tool call)`.

## Task 34 — genkey: opencode config

**Files:** `src/genkey.rs`.

Locked interface:
```rust
pub const OPENCODE_OUT: &str = "opencode.lobo.json";
pub fn new_api_key() -> String;                                        // "sk-" + 48 lowercase hex (24 random bytes)
pub fn opencode_config(domain: &str, key: &str, port: u16) -> Result<String>;   // pretty JSON, keys sorted like Go map encoding, trailing "\n"
pub fn write_opencode(path: &Path, domain: &str, key: &str, port: u16) -> Result<()>;   // mode 0600
```
Content = `cmd/lobo/genkey.go:64-111` (q8 alias, name, limits, `lobo-local` always, `lobo` only with a domain, agent tools map). Build with `BTreeMap`/`serde_json::Map` (sorted) so the bytes match Go.
- [ ] Failing tests: `write_opencode_table` (TestWriteOpencode, moved from P4); `opencode_matches_go_bytes` (`fixtures/opencode/{cloud,local_only}.json` with key `sk-fixed`); `new_api_key_shape`.
- [ ] Implement. `cargo test -p lobo-core genkey` → pass (3).
- [ ] Commit: `lobo-core: opencode config, byte-identical to Go`.

## Task 35 — genkey: ensure the key in the config

**Files:** `src/genkey.rs`.

Locked interface: `pub fn ensure_api_key(config_path: &Path, rotate: bool) -> Result<(String /*key*/, bool /*written*/)>`. Reads with `values` semantics but a missing file is an error `read <path>: …` (Go `godotenv.Read`). Empty key or `rotate` → `new_api_key` + `set_env_value`.
- [ ] Failing tests: `ensure_keeps_existing`; `ensure_creates_when_empty`; `ensure_rotate_replaces`; `ensure_missing_file_errors`.
- [ ] Implement. `cargo test -p lobo-core genkey` → pass (7).
- [ ] Commit: `lobo-core: gen-api-key logic shared by CLI and app`.

---

## E. Local (this Mac)

## Task 36 — platform

**Files:** `src/local/platform.rs`.

Locked (contracts.md): `supported() -> Result<()>` (`"local mode needs macOS on Apple Silicon, this is <os>/<arch>"`), `usable_mib() -> Result<i64>` (`iogpu.wired_limit_mb` if > 0, else `hw.memsize*3/4 >> 20`). Plus `pub(crate) fn mem_bytes() -> Result<u64>`, `wired_limit_mib() -> Result<i64>`, `sysctl_string(name) -> Result<String>` via `libc::sysctlbyname` on macOS; `Error::Local("sysctl: darwin only")` elsewhere.
- [ ] Failing tests: `supported_matches_target` (TestSupported, `cfg!(all(target_os = "macos", target_arch = "aarch64"))`); `usable_mib_on_apple_silicon` (TestUsableMiB, skipped elsewhere, ≥ 1024).
- [ ] Implement. `cargo test -p lobo-core local::platform` → pass (2).
- [ ] Commit: `lobo-core: Apple Silicon check and GPU-usable memory`.

## Task 37 — state file: read, claim, remove

**Files:** `src/local/state.rs`, `src/local/mod.rs`.

Locked interface:
```rust
pub struct StateFile { pub path: PathBuf }                   // lock file = "<path>.lock"
impl StateFile {
  pub fn default_path() -> PathBuf;                          // = local::state_path()
  pub fn path_from(xdg_state_home: Option<&str>, home: Option<&Path>) -> PathBuf;   // state.go:26-36
  pub fn at(path: PathBuf) -> Self;
  pub fn read(&self) -> Result<Option<LocalState>>;          // dead pid → remove_if(pid, boot) and None (state.go:40-52)
  pub fn claim(&self, s: &LocalState, is_supervisor: &dyn Fn(i32, &str) -> bool) -> Result<()>;  // state.go:70-108
  pub fn remove_if(&self, pid: i32, boot_id: &str) -> Result<()>; }                            // state.go:112-129
pub fn alive(pid: i32) -> bool;                               // kill(pid, 0): Ok or EPERM
// contracts.md free functions, default path, real is_supervisor:
pub fn state_path() -> PathBuf; pub fn read_state() -> Result<Option<LocalState>>;
pub fn claim_state(s: &LocalState) -> Result<()>; pub fn remove_state_if(pid: i32, boot_id: &str) -> Result<()>;
```
Byte compat with Go: write = `serde_json::to_vec_pretty` + `\n` (Go `MarshalIndent("", "  ")`; field order of `lobo_proto::LocalState` = Go order). Claim = temp `.local-*.json` 0600 fully written, then under the lock `link(tmp, path)`; `EEXIST` → read old, fail `local already running (pid N)` if alive and supervisor, else remove + link. Temp name always removed. Lock = `flock(2)` `LOCK_EX` on `<path>.lock` (0600, dir 0700) via `nix::fcntl::Flock` — the same syscall and path as Go `syscall.Flock` (`state.go:133-147`), so a Go process and a Rust process exclude each other.
- [ ] Failing tests: `state_path_xdg_and_home` (TestStatePath); `state_round_trip` (TestStateRoundTrip: 0600, equal after read, remove twice ok, no temp left); `dead_pid_state_removed` (TestStateDeadPID); `corrupt_state_errors` (TestStateCorrupt); `reads_go_written_state` (reads `crates/lobo-proto/fixtures/local_state.json` copied into a temp state path; fields equal the P1 fixture values; pid replaced by our own pid so it is alive); `written_bytes_match_go_layout` (claim a fixed state → file bytes = pretty JSON + `\n`, keys in order `pid, port, api_port, model, weights, started_at, boot_id`).
- [ ] Implement. `cargo test -p lobo-core local::state` → pass (6).
- [ ] Commit: `lobo-core: local state file, Go-compatible bytes and lock`.

## Task 38 — state file: contention

**Files:** `src/local/state.rs`. Create `tests/go_interop.rs`.

- [ ] Failing tests: `claim_state_table` (TestClaimState, 4 rows; `is_supervisor` passed as a closure — the Go `psSupervisor` swap); `claim_state_concurrent_one_winner` (TestClaimStateConcurrent: 16 threads, one winner, file is the winner's); `remove_state_if_table` (TestRemoveStateIf, 4 rows — carry-over "state removed only by pid + boot id", `state.go:110-129`); `claim_waits_for_lock_holder` (new: a helper thread holds `Flock` on `<path>.lock` for 300 ms; `claim` returns after ≥ 250 ms).
- [ ] `tests/go_interop.rs` (skips unless `LOBO_GO_INTEROP=1`; runs `go run ./tools/corefixtures …` from the repo root): `go_lock_blocks_rust_claim` (`holdlock` 500 ms → Rust claim waits ≥ 400 ms); `rust_lock_seen_by_go` (Rust holds the lock → Go `trylock` prints `busy`); `go_reads_rust_state` (Rust claims with our pid → Go `readstate` prints `ok:true` and the pid + boot id).
- [ ] Verify: `cargo test -p lobo-core local::state` → pass (10). `LOBO_GO_INTEROP=1 cargo test -p lobo-core --test go_interop` → pass (3). (Local `go run`, no network beyond the Go module cache.)
- [ ] Commit: `lobo-core: state claim races and Go lock interop`.

## Task 39 — models: markers + listing

**Files:** `src/local/models.rs`.

Locked interface:
```rust
pub const HF_BASE: &str = "https://huggingface.co/HauhauCS/Qwen3.5-27B-Uncensored-HauhauCS-Aggressive/resolve/main/";
pub fn marker_path(weights: &Path, file: &str) -> PathBuf;                 // "<file>.sha256-ok"
pub(crate) fn marker_line(sha: &str, meta: &std::fs::Metadata) -> String;  // "<sha> <size> <mtime_unix_nanos>\n"
pub(crate) fn marker_valid(weights: &Path, m: &Model, meta: Option<&std::fs::Metadata>) -> bool;
pub(crate) fn write_marker(weights: &Path, m: &Model) -> Result<()>;       // temp ".sha256-ok-*", 0644, rename
pub fn list(weights: &Path) -> Result<Listing>;                            // contracts.md; models.go:82-102
pub(crate) fn free_space(dir: &Path) -> Result<u64>;                       // statvfs bavail*frsize; missing dir → 0
```
mtime nanos = `modified()` since UNIX_EPOCH as i128 nanos, formatted as decimal — same number Go's `ModTime().UnixNano()` prints for the same file.
- [ ] Failing tests: `marker_path` (TestMarkerPath); `marker_valid_table` (TestMarkerValid, 10 rows — carry-over "marker `<sha> <size> <mtime_ns>`", `models.go:41-55`); `list_states` (TestList); `list_missing_weights` (TestListMissingWeights); `list_json_shape` (TestListJSON: exact JSON string via `lobo_proto::Listing`); `marker_written_by_go_is_valid` (new: a marker line built with the Go format for a temp file → valid; guards the transition where Go wrote markers).
- [ ] Implement. `cargo test -p lobo-core local::models` → pass (6).
- [ ] Commit: `lobo-core: weights listing and sha markers`.

## Task 40 — runtime: untar + find server

**Files:** `src/local/runtime.rs`.

Locked interface: `pub(crate) fn untar(src: &Path, dst: &Path) -> Result<()>` (`runtime.go:102-168`: local paths only, relative symlinks inside, dirs `mode|0700`, files `O_EXCL` then exact mode, global headers skipped, other types refused with `unsupported tar entry`); `pub(crate) fn find_server(dir: &Path) -> Result<PathBuf>` (walk without following symlinks, regular file `llama-server` with any exec bit; else `no llama-server in <dir>`).
- [ ] Failing tests (tar built in-test with the `tar` crate, same entries as `runtime_test.go:78-84`): `untar_keeps_modes_and_symlinks`; `untar_refuses_unsafe_paths` (traversal, absolute, symlink out, symlink abs); `find_server_needs_exec_bit`.
- [ ] Implement. `cargo test -p lobo-core local::runtime` → pass (3).
- [ ] Commit: `lobo-core: safe untar for the llama.cpp runtime`.

## Task 41 — runtime: ensure

**Files:** `src/local/runtime.rs`.

Locked interface:
```rust
pub const RUNTIME_VERSION: &str = "b11118";
pub struct RuntimePin { pub url: String, pub size: i64, pub sha256: String }
impl RuntimePin { pub fn pinned() -> Self; }   // runtime.go:21-25 values
pub fn runtime_dir(weights: &Path) -> PathBuf;  // <weights>/runtime/llama-b11118
pub fn runtime_note(size: i64) -> String;       // "llama.cpp b11118 11 MB"
pub async fn ensure_runtime(weights: &Path, note: &(dyn Fn(String) + Sync)) -> Result<PathBuf>;   // contracts.md
pub async fn ensure_runtime_with(weights: &Path, pin: &RuntimePin, note: &(dyn Fn(String) + Sync)) -> Result<PathBuf>;
```
Flow = `runtime.go:41-79` (present → return, no note; else note, fetch to temp `.llama-*.tar.gz` with size + sha check, untar into temp `.llama-*`, check server, chmod 0755, rename; lost rename race → use the winner). Temp files removed on every path.
- [ ] Failing tests (wiremock serves the tarball): `ensure_fetches_once` (TestRuntimeEnsure); `runtime_note` (TestRuntimeNote); `ensure_rejects_table` (TestRuntimeRejects, 6 rows, `runtime/` left empty); `ensure_short_body_fails` (TestRuntimeShort: error contains `size`).
- [ ] Implement. `cargo test -p lobo-core local::runtime` → pass (7).
- [ ] Commit: `lobo-core: fetch the pinned llama.cpp runtime`.

## Task 42 — Mac deps: GPU check + metrics

**Files:** `src/local/deps.rs`.

Locked interface:
```rust
pub struct MacConfig { pub weights: PathBuf, pub llama_server: PathBuf, pub api_key: String, pub port: u16, pub ctx: i64,
                       pub model: lobo_proto::catalog::Model, pub base_env: Vec<(String, String)> /* parent env; tests pass their own */ }
pub struct MacDeps { /* cfg, logs: LogSource, hf_base, llama_url, poll, usable_mib hook, vm_stat hook, host: Result<Gpu, String>, pid, llama_done */ }
pub(crate) fn host_gpu(sysctl: &dyn Fn(&str) -> Result<String>, mem: &dyn Fn() -> Result<u64>) -> Result<lobo_proto::Gpu>;
pub(crate) fn parse_vm_stat(out: &str) -> Result<i64>;   // deps.go:228-256
impl MacDeps { pub(crate) async fn check_gpu(&self, cancel: &CancellationToken) -> Result<()>;   // deps.go:99-122
               pub(crate) async fn gpu(&self) -> Result<lobo_proto::Gpu>; }                       // deps.go:208-225
```
`check_gpu`: `<llama_server> --list-devices`, env = `clean_env(base_env)`, 1 min bound, log `[gpu-check] <out>`; needs `MTL0`; then `min_free_mib(model.size) <= usable`, else `<id> needs <x.x> GB, this Mac allows ~<y> GB to the GPU`.
- [ ] Failing tests: `check_gpu_table` (TestCheckGPU, 4 rows, fake script server); `gpu_metrics_vm_stat` (TestGPUMetrics: 23656 MiB used, 65536 total, util 0, two bad inputs fail).
- [ ] Implement. `cargo test -p lobo-core local::deps` → pass (2).
- [ ] Commit: `lobo-core: Metal check and Mac memory metrics`.

## Task 43 — Mac deps: download

**Files:** `src/local/deps.rs`.

Locked: `pub(crate) async fn download(&self, cancel, on_progress: &(dyn Fn(DownloadProgress) + Sync)) -> Result<()>` (`deps.go:127-167`): full size + valid marker → done; full size, no valid marker → `hash_file` then marker (mismatch → quarantine); short/missing → remove marker, `lobo_agent::download::download(HttpSource{hf_base+file})` then marker; `<dst>.bad` after a failed download → quarantine it. Quarantine → `<weights>/.bad/<file>.<unix>`, error `…, moved to <path>`.
- [ ] Failing test `download_table` (TestDownload, 9 rows; wiremock serves the body with Range support and counts hits: short file = 2 hits).
- [ ] Implement. `cargo test -p lobo-core local::deps` → pass (3).
- [ ] Commit: `lobo-core: verified local model download`.

## Task 44 — Mac deps: llama-server + agent wiring

**Files:** `src/local/deps.rs`.

Locked interface:
```rust
pub fn new_deps(cfg: MacConfig, logs: LogSource, stop: CancellationToken) -> (lobo_agent::Deps, Arc<MacDeps>);
impl MacDeps { pub async fn wait_llama(&self, timeout: Duration) -> bool; }   // true once exited or never started
```
Mapping onto `lobo_agent::Deps`: `tunnel` = a tunnel that never exits (it keeps the oneshot `Sender` alive inside the struct; a dropped sender would read as "tunnel exited"); `gpu_check` = `check_gpu`; `download` = `download`; `llama` = start (`-m <weights>/<file>` + `LlamaArgs` for host `127.0.0.1`, port, ctx; env = `clean_env(base_env)` + `LLAMA_API_KEY=<key>`; pid recorded; `llama_done` fired on exit) + `wait_healthy(llama_url, poll)`; `metrics` = `Collector{llama_url, api_key}` for llama, `gpu()` for gpu, host → `Err("host metrics: not collected on macOS")`; `killer` = cancels `stop`.
- [ ] Failing tests: `start_llama_args_env` (TestStartLlama: exact arg prefix `-m /w/<q6 file> --alias <alias> --host 127.0.0.1 --port 8931 `, `-c 65536`, `key: sk-x`, `LLAMA_ARG_HOST` from `base_env` does not reach the child — carry-over "secrets never in children": also put `RUNPOD_API_KEY=x` and `LOBO_API_KEY=y` in `base_env` and assert the child prints neither; pid > 0; `wait_llama` true after exit and true when never started); `wait_healthy_polls` (TestWaitHealthy); `new_deps_wiring` (TestNewDepsWiring: tunnel receiver pending after 50 ms; `kill_self` cancels the token; every hook set).
- [ ] Implement. `cargo test -p lobo-core local::deps` → pass (6).
- [ ] Commit: `lobo-core: Mac hooks for the shared agent Runner`.

## Task 45 — `RunConfig` + argv parsing

**Files:** `src/local/supervise.rs`.

Locked interface:
```rust
pub const SUPERVISOR_ARG: &str = "--lobo-local-run";
#[derive(Debug, Clone, PartialEq)]
pub struct RunConfig { pub model: String, pub ctx: i64, pub idle_min: i64, pub boot_id: String, pub port: u16, pub api_port: u16,
                       pub config_path: Option<PathBuf>, pub version: lobo_proto::Manifest }
impl RunConfig {
  pub fn from_args(args: &[String], version: Manifest) -> Result<RunConfig>;  // args AFTER the prefix ("local run" or SUPERVISOR_ARG…)
  pub fn to_args(&self) -> Vec<String>;                                          // inverse; used by LocalProvider::spawn
  pub fn validate(&self) -> Result<()>; }                                        // cmd/lobo/local.go:43-58
```
Flags: `--model` (default `q6`), `--ctx`, `--idle-min`, `--boot-id`, `--port` (8931), `--api-port` (8932), `--config`. Hand-parsed `--flag value` pairs (no clap in core; the app has no clap parser). Unknown flag or stray arg → `Error::Other("unknown argument \"x\"")`. `to_args` order: `[--config <p>] --model … --ctx … --idle-min … --boot-id … --port … --api-port …`.
- [ ] Failing tests: `run_config_from_args_table` (TestLocalRunFlags rows 1–7 minus the cobra-only "hidden" check, which stays in P4; row "extra arg" expects `unknown argument`); `to_args_round_trips`.
- [ ] Implement. `cargo test -p lobo-core local::supervise` → pass (2).
- [ ] Commit: `lobo-core: local run options shared by CLI and app`.

## Task 46 — `supervise`

**Files:** `src/local/supervise.rs`.

Locked (contracts.md): `pub async fn supervise(cfg: RunConfig, cancel: CancellationToken) -> Result<()>`. Plus the test seam `pub(crate) async fn supervise_with(cfg, laptop: Laptop, state: StateFile, deps: lobo_agent::Deps, mac: Arc<MacDeps>, logs: LogSource, cancel) -> Result<()>`.

Flow = `cmd/lobo/local.go:87-147`: `validate`; load the config (`config_path` or `default_path`); `ensure_runtime`; **bind the API port before the state file**; claim state `{pid, port, api_port, model, weights, started_at: now UTC, boot_id}`; Runner with `boot_timeout 8 h`, `expires_at now + 100 y`, `tick 30 s`, `fail_grace 2 min`, `idle = idle_min`; axum server with `lobo_agent::api::router(runner, api_key, logs, version)`; SIGTERM/SIGINT or `cancel` or the Killer end the run; then wait up to 10 s for llama-server; `remove_if(pid, boot_id)` always; a cancel/SIGTERM stop returns `Ok(())`. Log lines go to stdout and a `LogRing::new(5000)` (as `LogSource`).
- [ ] Failing tests (fake `lobo_agent::Deps` built in-test, no llama-server): `supervise_claims_state_and_serves_api` (`GET /api/status` on the api port has our `boot_id`); `supervise_busy_api_port_fails_before_state` (port held → error, no state file); `supervise_cancel_removes_state_ok`; `supervise_second_instance_refused` (live state of a verified supervisor → `local already running`).
- [ ] Implement. `cargo test -p lobo-core local::supervise` → pass (6).
- [ ] Commit: `lobo-core: local supervisor on the shared agent Runner`.

## Task 47 — local provider: identity + instance

**Files:** `src/local/provider.rs`.

Locked interface:
```rust
pub struct Spawner { pub exe: PathBuf, pub args_prefix: Vec<String> }   // contracts.md
impl Spawner { pub fn cli(exe: PathBuf) -> Self;   // (exe, ["local","run"])
               pub fn app(exe: PathBuf) -> Self; } // (exe, [SUPERVISOR_ARG, "local", "run"]) — see Contract additions 5
pub fn command_of(pid: i32) -> Result<String>;     // `ps -ww -o command= -p <pid>`
pub fn is_supervisor(pid: i32, boot_id: &str, ps: &dyn Fn(i32) -> Result<String>) -> bool;  // provider.go:262-279
pub fn instance(s: &LocalState) -> Instance;       // provider.go:294-298: urls from the STATE ports
pub fn log_path(state: &StateFile) -> PathBuf;     // <state dir>/local.log
```
- [ ] Failing tests: `is_supervisor_table` (TestIsSupervisor, 10 rows — carry-over "identity = `local run` + exact `--boot-id`"); `app_argv_passes_identity` (new: `Spawner::app` prefix + `to_args` joined → `is_supervisor` true; guards Go `lobo down` against an app-spawned supervisor); `instance_urls_from_state_ports` (carry-over "endpoints from the running state's ports" — `provider.go:293-298`).
- [ ] Implement. `cargo test -p lobo-core local::provider` → pass (3).
- [ ] Commit: `lobo-core: supervisor identity and local instance`.

## Task 48 — test helper binary

**Files:** `src/bin/testchild.rs`.

Port of `provider_test.go:22-64`. Reads mode from env `LOBO_LOCAL_HELPER` (`ok`, `stubborn`, `fail`, `ok-child`, `stubborn-child`) and state path from `LOBO_TEST_STATE`. Writes `<state dir>/args` (its argv after argv[0], space-joined), `<state dir>/child` pid for `-child` modes (`sleep 60` in its process group), claims state via `StateFile::at(..).claim(..)`, sleeps 60 s, exits 3. `fail`: prints `line 1`..`line 25`, then `boom: weights gone` on stderr, exit 1. `stubborn*`: ignores SIGTERM.
- [ ] Verify: `cargo build -p lobo-core --bin lobo-core-testchild` → exit 0. `LOBO_LOCAL_HELPER=fail target/debug/lobo-core-testchild; echo $?` → `1`.
- [ ] Commit: `lobo-core: helper binary that stands in for lobo local run`.

## Task 49 — local provider: pre-checks

**Files:** `src/local/provider.rs`. Create `tests/local_provider.rs`.

Locked interface:
```rust
pub struct LocalHooks { pub supported: fn() -> Result<()>, pub ensure_runtime: Arc<dyn EnsureRuntime>, pub free_bytes: fn(&Path) -> Result<u64>,
                        pub ps: fn(i32) -> Result<String>, pub state_wait: Duration /*15 s*/, pub stop_wait: Duration /*10 s*/ }
impl Default for LocalHooks { .. }   // real functions
#[async_trait] pub trait EnsureRuntime: Send + Sync { async fn ensure(&self, weights: &Path, note: &(dyn Fn(String) + Sync)) -> Result<PathBuf>; }
pub struct LocalProvider { pub spawner: Spawner, pub config_path: Option<PathBuf>, pub weights: PathBuf, pub port: u16,
                           pub state: StateFile, pub hooks: LocalHooks }
impl Provider for LocalProvider { .. }   // name "local", replaceable false
```
Rent order (`provider.go:47-85`): supported → model known → no live state (`lobo already running: local pid N`) → weights writable (`weights folder <d> is not writable: …`) → space (`not enough space in <d> for <id>: need <n> bytes, <f> free`) → ports `port` and `port+1` free (`port <p> in use (LOBO_LOCAL_PORT)` / `(agent API = LOBO_LOCAL_PORT+1)`) → `ensure_runtime` → spawn.

Test setup helper `test_provider(mode)`: temp state path, temp weights, free port pair, spawner = `Spawner::cli(env!("CARGO_BIN_EXE_lobo-core-testchild"))`, hooks with `supported` ok, fake ensure (records a call), free bytes 1<<50, `state_wait` 5 s, `stop_wait` 500 ms; env for the child passed through `Command::env` (never `set_var`).
- [ ] Failing test `prechecks_before_runtime` (TestProviderPrechecks, 6 rows: runtime not fetched, no state file).
- [ ] Implement. `cargo test -p lobo-core --test local_provider prechecks` → pass (1).
- [ ] Commit: `lobo-core: local rent pre-checks before any download`.

## Task 50 — control traits (moved up: the local provider and release need them)

**Files:** `src/control/mod.rs`.

Locked interface:
```rust
#[async_trait] pub trait AgentApi: Send + Sync {                     // contracts.md, exactly
    async fn status(&self) -> Result<Status>; async fn version(&self) -> Result<Manifest>; async fn logs(&self, n: usize) -> Result<String>; }
#[async_trait] pub trait ReleaseResolver: Send + Sync { async fn resolve(&self, version: &str) -> Result<Resolved>; }
#[async_trait] pub trait Presigner: Send + Sync { async fn presign_get(&self, key: &str, ttl: Duration) -> Result<String>; }
#[derive(Debug, Clone, Default, PartialEq)]
pub struct UpOpts { pub model: String, pub ctx: i64, pub release: String, pub idle_min: i64, pub max_life: Duration, pub timeout: Duration,
                    pub source: String, pub conns: i64, pub cloud: String, pub provider: String, pub min_mbps: i64, pub ssh_key: String, pub image: String }
#[derive(Clone)]
pub struct Deps { pub providers: BTreeMap<String, Arc<dyn Provider>>, pub releases: Arc<dyn ReleaseResolver>,
                  pub presign: Option<Arc<dyn Presigner>>, pub new_agent: Arc<dyn Fn(&str) -> Arc<dyn AgentApi> + Send + Sync>,
                  pub cfg: Laptop, pub clock: Arc<dyn Clock>, pub poll: Duration }   // contracts.md, exactly
pub(crate) async fn list_all(d: &Deps) -> (Vec<Instance>, Option<Error>);   // order runpod, vast, local; errors "<name>: <err>" joined (Multi)
pub const MAX_GPU_RETRIES: u32 = 4; pub const CONTAINER_TIMEOUT: Duration = 6 min; pub const STALE_SLACK: Duration = 15 s; pub const POD_CHECK_EVERY: Duration = 30 s;
```
Add `impl ReleaseResolver for release::BucketReleases` and `impl Presigner for release::Store`.
- [ ] Failing tests: `list_all_keeps_going_on_error` (one provider errors, the other's instances returned, error names the provider); `consts_match_go` (4 / 6 min / 15 s / 30 s — cites `internal/control/up.go:212-226`).
- [ ] Implement. `cargo test -p lobo-core control::` → pass (2).
- [ ] Commit: `lobo-core: control traits, options and deps`.

## Task 51 — local provider: spawn + lifecycle

**Files:** `src/local/provider.rs`, `tests/local_provider.rs`.

Spawn (`provider.go:127-167`): log file `log_path` (dir 0700, file 0600, append), remember the start offset; argv = `spawner.args_prefix` + `RunConfig{model, ctx, idle_min, boot_id, port, api_port: port+1, config_path}.to_args()`; stdin null, stdout/stderr = log; `setsid` in `pre_exec`; reaper task; poll `state.read()` every 100 ms until the pid matches; early exit → `lobo local run exited early (<status>); <log>:\n<last 20 lines since start>`; timeout → kill child, `lobo local run wrote no state in <wait>; …`. `list`/`get` from state (`get` of another pid → `NotFound`).
- [ ] Failing tests (`tests/local_provider.rs`): `lifecycle` (TestProviderLifecycle: instance fields, args file equals `local run --config /cfg/lobo.env --model q6 --ctx 4096 --idle-min 7 --boot-id b1 --port <p> --api-port <p+1>` — Rust order puts `--config` after the prefix, see "Spec issues" 6; weights dir created; second rent `already running`; list; get "1" → NotFound; delete; child dead; state gone; delete again Ok); `child_fails_shows_log_tail` (TestProviderChildFails: `exited early`, `boom: weights gone`, `line 25`, no `line 5\n`, log file exists).
- [ ] Implement. `cargo test -p lobo-core --test local_provider` → pass (3).
- [ ] Commit: `lobo-core: local provider spawns the supervisor through Spawner`.

## Task 52 — local provider: delete

**Files:** `src/local/provider.rs`, `tests/local_provider.rs`.

Delete (`provider.go:224-253`): state for id else Ok; not our supervisor → `remove_if`, Ok, never signalled; SIGTERM; wait `stop_wait` for `kill(-pid, 0) == ESRCH`; not gone → re-check identity (stranger now → `remove_if`, Ok); `killpg(pid, SIGKILL)`; wait 2 s; still there → `local group <pid> survived SIGKILL`; then `remove_if`.
- [ ] Failing tests: `delete_kills_stubborn` (TestProviderDeleteKills: dead, took ≥ `stop_wait`); `delete_kills_group` (TestProviderDeleteKillsGroup, `ok-child` and `stubborn-child`: child pgid = supervisor pid; after delete `killpg(pid, 0)` = ESRCH; state gone — carry-over "SIGKILL the process group", `provider.go:243-250`); `delete_reverifies_before_sigkill` (TestProviderDeleteReverifies: ps hook returns the real command once, then `/usr/bin/vim notes.txt` → 2 calls, still alive, state gone); `delete_never_signals_stranger` (TestProviderDeleteStranger, 5 rows).
- [ ] Implement. `cargo test -p lobo-core --test local_provider` → pass (7).
- [ ] Commit: `lobo-core: local delete kills the whole group, never a stranger`.

---

## F. Control

## Task 53 — `HttpAgent`

**Files:** `src/control/agent_http.rs`.

Locked (contracts.md): `pub struct HttpAgent; impl HttpAgent { pub fn new(base: &str, key: &str) -> Self; }` — base trimmed of `/`, 5 s client. Plus `pub fn on_domain(domain: &str, key: &str) -> Self` (`https://<domain>`). `status` GET `/api/status` (no auth), `version` GET `/api/version` (no auth), `logs` GET `/api/logs?n=<n>` with bearer. Non-200 → `Error::Other("<path>: HTTP <code>")`.
- [ ] Failing test `http_agent_url` (TestHTTPAgentURL: trailing slash base, version `dev`/`abc`, logs need the key, `on_domain` base `https://lobo.example.com`).
- [ ] Implement. `cargo test -p lobo-core control::agent_http` → pass (1).
- [ ] Commit: `lobo-core: agent API client`.

## Task 54 — control testkit (fakes)

**Files:** `src/control/testkit.rs`.

Port of `internal/control/controltest/fakes.go`, names kept: `Created { opts: CreateOpts, cloud_type, min_download_mbps }`, `FakeRunPod` (implements `RunPodApi`; `pods`, `no_cap: BTreeSet<String>`, `max_mbps`, `created`, `deleted`, `create_err`, all behind one `Mutex`), `FakeReleases(Resolved)` (`"missing"` → error), `FakeAgent` (scripted `Vec<Option<Status>>`, one per `status` call, sticks on the last; `calls()`), `FakeLocal` (provider "local", `LOCAL_API_URL`, `LOCAL_AGENT_URL`), `local_boot_script()`, `release()`, `deps(rp, ag, clock) -> Deps` (RunPod on `lobo.example.com`, poll 1 ms, cfg as `fakes.go:208-211`), `boot_script()`, `events(script, no_cap) -> Vec<UpEvent>` (StepClock 1 s). Exposed under `#[cfg(any(test, feature = "testkit"))]` for P4 TUI tests.
- [ ] Failing test `events_helper` (TestEventsHelper) — lands with Task 57; here: `fake_agent_script_sticks_on_last`, `fake_runpod_no_cap` (compile + behaviour of the fakes).
- [ ] Implement. `cargo test -p lobo-core control::testkit` → pass (2).
- [ ] Commit: `lobo-core: control fakes (testkit feature for the CLI TUI tests)`.

## Task 55 — `up`: preflight + options

**Files:** `src/control/up.rs`, `src/control/tests.rs`.

Locked (contracts.md): `pub fn up(d: Deps, o: UpOpts, cancel: CancellationToken) -> tokio::sync::mpsc::Receiver<UpEvent>`.
Event plumbing: one `mpsc::UnboundedSender<UpEvent>` feeds every event (including rent `note` lines from the sync callback). A forwarder task moves them into the returned bounded receiver (cap 16), so a note never blocks or drops. Any `Err` from the inner run becomes a final `UpEvent { phase: "failed", err: Some(e.to_string()), done: true }`. The receiver closes at the end.

Preflight (`up.go:58-130`): provider default `runpod`; unknown → `provider "<p>" is not configured (key missing in the config? run \`lobo config\`)`; anything running → `lobo already running: <prov> <id> (<status>). Run \`lobo down\` first`; local → built-in release (`DEFAULT_MODEL`, `DEFAULT_DEFAULTS`, version `local`); image from flag else `cfg.pod_image`; image + release → error; image → manifest `{version: image, llama_image: image}`; else resolve. Fill model/ctx/idle/max-life from the release, timeout 20 min. Bad options → `bad options: ctx <c> (min 512), idle-min <i> (min 1), max-life <m> (min 1m)`. Model from the catalog.
- [ ] Failing tests (Go ports): `up_already_running` (carry-over "`up` refuses while anything runs" — `up.go:67-73`); `up_unconfigured_provider`; `up_image_and_release_conflict`; `up_rejects_bad_options_before_renting`; `up_overrides`; `up_baked_image` (3 rows); `up_baked_image_needs_no_release_manifest`. (The boot loop is stubbed in this task to rent once and return ready when the first status is `ready`; Task 57 replaces the stub.)
- [ ] Implement. `cargo test -p lobo-core control::tests::up_` → the 7 tests pass.
- [ ] Commit: `lobo-core: up preflight, defaults and option checks`.

## Task 56 — `up`: cloud options + model sources

**Files:** `src/control/up.rs`, `src/control/tests.rs`.

`cloud_opts` = `up.go:133-209`: source default r2 (config `r2`, or empty with a presigner), `ssh` (config `ssh://`), else public; conns 32 for r2/feesh, 8 otherwise, ssh capped at 8; feesh needs `LOBO_FEESH_HTTP_URL`; r2 presigns `models/<file>` for 12 h (`presign model: …`); ssh reads the key file, base64; min MB/s flag > config > 100; r2 gets feesh as fallback; feesh source uses feesh URL, no fallback; ssh uses `model_source` trimmed of `/`.
- [ ] Failing tests: `up_r2_gets_feesh_fallback`; `up_min_mbps`; `up_ssh_source_caps_conns_and_sends_key` (new: config `ssh://lobo@h:22` + key file → `model_url` = source, `model_ssh_key` = base64(file), `dl_conns` 8 when `--conns 16` — cites `up.go:151-152, 204-207`); `up_create_opts_carry_no_laptop_secrets` (new carry-over: a `Laptop` with every secret set; serialize the RunPod payload and the Vast body built from the resulting `CreateOpts`; neither contains the values of `RUNPOD_API_KEY`, `VASTAI_API_KEY`, `R2_ACCESS_KEY`, `R2_SECRET_KEY` — cites `runpod_test.go:45-48`, `vast_test.go:177-180`).
- [ ] Implement. `cargo test -p lobo-core control::tests::up_` → pass (11).
- [ ] Commit: `lobo-core: model source choice and pod create options`.

## Task 57 — `up`: boot loop (happy path, failures, tiers)

**Files:** `src/control/up.rs`, `src/control/tests.rs`.

`boot` = `up.go:234-359`, one step per poll: fresh 16-hex `boot_id` per rent; ignore a status with another non-empty boot id; the uptime heuristic for empty boot ids on re-rents; pod-gone check every `POD_CHECK_EVERY` once seen; phase `image` until the agent answers; progress = phase change or new download bytes; ready → `ReadyInfo` (url, cost, elapsed from start, version + git sha from `/api/version`, attempts, detail, host Mbps, timings, rent seconds when both times are set) + `done`; failed/terminating → message (`watchdog: <reason>` unless the kill reason is `failed`) + last 20 log lines; stall past `timeout` → delete + `terminated` event with `no progress for …`. Poll interval `d.poll` (0 → 3 s) via `tokio::time::sleep`; all time decisions use `d.clock`. Cancel → `Error::Cancelled`. Deletes ignore `cancel`.
- [ ] Failing tests: `up_happy` (phases `create,image,tunnel,download,load,ready`; download bytes; ready URL + git sha; COMMUNITY only; create opts of `control_test.go:52-55`); `events_helper`; `up_agent_failed`; `up_reports_expiry_reason`; `up_timeout`; `up_no_capacity_anywhere` (final err contains `no gpu capacity`); `up_steps_down_network_tiers`; `up_community_only_by_default`; `up_secure_all_tiers_before_community`.
- [ ] Implement. `cargo test -p lobo-core control::tests::up_` → pass (20).
- [ ] Commit: `lobo-core: up boot loop with progress events`.

## Task 58 — `up`: bad hosts and stale status

**Files:** `src/control/up.rs`, `src/control/tests.rs`.

Retry rules: `retriable(detail)` = starts with `gpu: ` or contains `host: `; not for non-replaceable providers; delete then re-rent, up to `MAX_GPU_RETRIES` total (`gave up: <n> pods in a row landed on bad hosts (all deleted)`); container not seen after `CONTAINER_TIMEOUT` → `host: container not started after 6m0s` event, delete, re-rent; `create` event `bad host, renting another pod (<n>/4)`.
- [ ] Failing tests: `up_replaces_pod_with_broken_gpu`; `up_gives_up_after_four_bad_hosts` (carry-over "bad-host replace ×4" — `up.go:125-127, 212`); `up_retries_slow_host_and_ignores_poll_blip`; `up_replaces_pod_whose_container_never_starts` (carry-over "container timeout" — `up.go:339-346`); `up_ignores_previous_pod_status_after_re_rent`; `up_ignores_other_boot_status`; `up_failed_polls_do_not_reset_stall`; `up_detects_pod_gone_while_agent_silent`.
- [ ] Implement. `cargo test -p lobo-core control::tests::up_` → pass (28).
- [ ] Commit: `lobo-core: up replaces bad hosts and ignores stale pods`.

## Task 59 — `up` on this Mac

**Files:** `src/control/up.rs`, `src/control/tests.rs`.

Local rules: create opts carry only key, model, ctx, idle, expiry, boot id; a failure is never re-rented: delete now, `local run stopped: <why> (err=<delete err or none>)\n<logs>`.
- [ ] Failing tests: `up_local` (TestUpLocal: phases `create,gpu,download,load,ready`, URL `http://127.0.0.1:8931/v1`, cost 0, cloud agent 0 calls, create opts empty cloud fields); `up_local_failure_stops_run` (2 rows); `up_on_vast_and_down_across_providers` (first half: up on vast, then runpod up refused `already running: vast`).
- [ ] Implement. `cargo test -p lobo-core control::tests::up_` → pass (31).
- [ ] Commit: `lobo-core: up on this Mac stops instead of re-renting`.

## Task 60 — `snapshot` + `down`

**Files:** `src/control/status.rs`, `src/control/tests.rs`.

Locked (contracts.md): `snapshot(d) -> Result<Snap>`, `down(d) -> Result<f64>`.
`snapshot` = `status.go:26-42` (first instance; cloud pod with empty domain → pod only, 0 agent calls; `at` = clock). `down` = `down.go:12-54`: spend = Σ cost × hours since start (skip unset start); delete each on its provider, collect errors; then up to 5 list checks `d.poll` apart (0 → 2 s); errors `list: …`, `delete <prov> <id>: …`, `lobo instances still listed after delete: <n>`, joined as `Error::Multi`.
- [ ] Failing tests: `down_and_snapshot` (spend 1.0); `down_waits_for_list_to_catch_up`; `down_keeps_going_when_one_provider_fails`; `up_on_vast_and_down_across_providers` (second half: both deleted); `snapshot_local`; `snapshot_no_domain` (2 rows).
- [ ] Implement. `cargo test -p lobo-core control::tests` → pass.
- [ ] Commit: `lobo-core: status snapshot and down across providers`.

## Task 61 — `target`

**Files:** `src/control/status.rs`, `src/control/tests.rs`.

Locked (contracts.md): `target(d) -> Result<(Arc<dyn AgentApi>, String)>` = `events.go:76-89`: first running instance wins even when another provider's list failed; nothing running + list error → that error; nothing running + no domain → `nothing running, and LOBO_DOMAIN is empty: start one with \`lobo up\``; else the domain agent + `https://<domain>/v1`.
- [ ] Failing test `target_table` (TestTarget, 7 rows; agent identity compared with `Arc::as_ptr(..) as *const ()`).
- [ ] Implement. `cargo test -p lobo-core control::tests::target` → pass (1).
- [ ] Commit: `lobo-core: target picks the running instance, local first`.

## Task 62 — pre-checks shared by CLI and app

**Files:** `src/control/precheck.rs`.

Locked interface:
```rust
pub fn check_target(cfg: &Laptop, provider: &str, supported: fn() -> Result<()>) -> Result<()>;   // main.go:83-88
pub fn check_providers(cfg: &Laptop, supported: fn() -> Result<()>) -> Result<()>;               // main.go:92-98
pub fn check_release(cfg: &Laptop) -> Result<()>;                                                // main.go:100-106
pub fn apply_defaults(o: &mut UpOpts, cfg: &Laptop, set: &dyn Fn(&str) -> bool, cfg_path: &Path) -> Result<()>;  // cmd/lobo/defaults.go:16-68
```
`apply_defaults` flag names are Go's: `provider`, `q6` (the model flag), `cloud`, `ctx`, `idle-min`, `max-life`, `min-mbps`. The app passes `set = |f| request_has(f)`.
- [ ] Failing tests: `check_target_table` (TestCheckTarget, 5 rows); `check_providers_table` (TestCheckProviders, 5 rows — carry-over "`down` needs only provider keys"); `check_release_table` (TestCheckRelease, 3 rows — carry-over "`release` needs R2 + bucket"); `apply_defaults_table` (TestApplyDefaults, 13 rows, moved from P4; the test pre-fills `o` the way cobra defaults would: `cloud = "community"`).
- [ ] Implement. `cargo test -p lobo-core control::precheck` → pass (4).
- [ ] Commit: `lobo-core: up/down/release pre-checks and launch defaults`.

## Task 63 — real wiring: `deps_from_config`

**Files:** `src/control/wiring.rs`.

Locked interface (contracts.md signature changes — see Contract additions 2):
```rust
pub struct Wiring { pub config_path: PathBuf, pub spawner: Spawner, pub supported: fn() -> Result<()> }
impl Wiring { pub fn new(config_path: PathBuf, spawner: Spawner) -> Self; }            // supported = local::supported
pub fn providers_from_config(cfg: &Laptop, w: &Wiring) -> BTreeMap<String, Arc<dyn Provider>>;   // main.go:458-476
pub fn local_provider_from_config(cfg: &Laptop, w: &Wiring) -> Option<LocalProvider>;  // None unless supported; config_path made absolute
pub fn deps_from_config(cfg: Laptop, w: &Wiring) -> Result<Deps>;
```
`deps_from_config`: providers as above (local when supported; runpod when keyed; vast when keyed, `max_dph` parsed or 0); releases = `BucketReleases{cfg.bucket_url}`; presign = `Store::new(&cfg.r2)` when `cfg.require_r2()` is ok, else None; `new_agent` = `HttpAgent::new(base, cfg.lobo_api_key)`; clock = `SystemClock`; poll = zero (up 3 s, down 2 s). Same result as Go `Laptop.Providers()` + `providers()` + `deps()` + the `up` presign branch (`main.go:227-232`; presign unused for local because `cloud_opts` is cloud-only).
- [ ] Failing tests: `providers_from_config_table` (TestProviders, 4 rows with the `supported` hook + the unswapped row); `local_instance_urls_from_state` (TestLocalInstanceURLs via `LocalProvider.state = StateFile::at(temp)`: state ports 8931/8932 win over config `9000`); `deps_from_config_sample` (new: sample config with RunPod + Vast + R2 keys → provider names `local?,runpod,vast` matching `cfg.providers()` + local-if-supported; `presign.is_some()`; a config without R2 → `presign.is_none()`; builds without network).
- [ ] Implement. `cargo test -p lobo-core control::wiring` → pass (3).
- [ ] Commit: `lobo-core: deps_from_config, the wiring CLI and app share`.

---

## G. Close

## Task 64 — public API = contracts

**Files:** `src/lib.rs`, `src/local/mod.rs`, `src/provider/mod.rs`. Create `tests/contracts_api.rs`.

- [ ] `tests/contracts_api.rs`: one `#[test] fn contracts_p3_signatures()` that coerces every contracts.md v1.1 P3 item to a typed fn pointer or uses the type (`let _: fn(&Path) -> Result<Laptop> = lobo_core::config::load_laptop;` …, trait objects `Arc<dyn Provider>`, `lobo_core::local::SUPERVISOR_ARG`, `lobo_core::provider::local::LocalProvider`). Compile failure = drift.
- [ ] `lib.rs` re-exports: `pub use error::{Error, Result};`.
- [ ] Verify: `cargo test -p lobo-core --test contracts_api` → pass. `cargo doc -p lobo-core --no-deps` → no warnings.
- [ ] Commit: `lobo-core: public API pinned to contracts v1.1`.

## Task 65 — CI + Makefile

**Files:** `.github/workflows/rust.yml`, `Makefile`.

- [ ] `rust.yml`: path filter adds `tools/corefixtures/**`. After the P1 fixture step: `make core-fixtures && git diff --exit-code crates/lobo-core/fixtures`. Test step env `LOBO_GO_INTEROP: "1"` (Go is set up in that job already). A `macos-14` job runs `cargo test -p lobo-core` (the local tests need macOS for `ps -ww`, `sysctl`, Metal-free fakes; Apple Silicon runner makes `supported()` true).
- [ ] `make rust-lint rust-test` locally → green.
- [ ] Commit. Push `feat/rust`. `gh run watch` → green. Red → `gh run view --log-failed`, fix, push, repeat.

## Task 66 — Phase close (orchestrator)

- [ ] Full local run: `make rust-lint rust-test core-fixtures proto-fixtures && git diff --exit-code && LOBO_GO_INTEROP=1 cargo test -p lobo-core --test go_interop && go test ./...` → all green, tree clean.
- [ ] Name check: every name in the File map, the tasks, "Contract additions" and `contracts.md` v1.1 match (`LocalProvider` not `Provider` for the type, `RunPodApi`, `StateFile`, `Spawner::app`).
- [ ] Optional orchestrator-only smoke, local and free: on the Apple Silicon Mac with existing weights, `cargo run -p lobo-core --example local_smoke` (a 30-line example the orchestrator writes: `deps_from_config` with `Spawner::cli(<go-built lobo>)` → `up` provider local → wait ready → `down`). It drives the Go `lobo local run` through the Rust provider, so it proves the spawn/identity/state interop live. Delete the example after (move to `/tmp/trash`).
- [ ] No cloud smoke in P3. Live RunPod, Vast and local runs with the Rust stack happen in P6 (spec "Tests: Live"). A RunPod boot from Rust `up` before P6 needs the user's OK (a few $).
- [ ] Reconcile spec: P3 "As-built notes" (Go dumper, `StateFile`, `Spawner::app` prefix, self-terminate clients in lobo-agent).
- [ ] Plan status → `done` (P3). `/notify`: "Rust P3 done on feat/rust: lobo-core + Go goldens, CI green. Approve P4 plan next."
- [ ] Merge hygiene: `git merge master` into `feat/rust` every few days; a Go change under `internal/{config,bootstrap,runpod,vast,local,control,release,checks}` or `cmd/lobo/genkey.go` means re-run `make core-fixtures` and port the change in the same merge.

---

## Go test → Rust test (P3 packages, complete)

Rust names are `module::tests::name` unless the path says `tests/<file>.rs`.

**internal/config/config_test.go**

| Go | Rust | Task |
|---|---|---|
| TestLoadLaptop | config::tests::load_laptop_full | 9–10 |
| TestLoadLaptopMissing | tests/config_os_env.rs::load_laptop_ignores_os_env | 9 |
| TestLoadLaptopVastOnly | config::tests::load_laptop_vast_only | 9 |
| TestLoadLaptopNoR2 | config::tests::load_laptop_no_r2_then_require_r2_fails | 9–10 |
| TestLoadAgent | — P2 (`lobo-agent` pod config; pod-side) | P2 |
| TestLoadAgentErrors | — P2 | P2 |
| TestLoadAgentPerProvider | — P2 | P2 |
| TestSetEnvValue | config::envfile::tests::set_env_value_replaces_and_appends | 7 |
| TestSaveKeepsHandEdits | config::envfile::tests::save_keeps_hand_edits | 7 |
| TestSaveNewFileLayout | config::envfile::tests::save_new_file_layout | 8 |
| TestDefaults | config::tests::defaults_parse_and_reject | 11 |
| TestDefaultProvider | config::tests::default_provider_table | 12 |
| TestDefaultPath | config::tests::default_path_xdg_and_home | 12 |
| TestSaveRoundTripSpecialValues | config::envfile::tests::save_round_trip_special_values | 8 |
| TestLoadLaptopIgnoresBadDefaults | config::tests::load_laptop_ignores_bad_defaults | 9 |
| TestLoadLaptopLocalOnly | config::tests::load_laptop_local_only | 9–10 |
| TestRequireCloud | config::tests::require_cloud | 10 |
| TestWeightsPort | config::tests::weights_and_port | 12 |
| TestDefaultsLocal | config::tests::defaults_local_port | 11 |
| TestRequireParts | config::tests::require_parts_table | 10 |

**internal/runpod/runpod_test.go**

| Go | Rust | Task |
|---|---|---|
| TestBuildCreatePayload | provider::runpod::tests::build_create_payload | 19 |
| TestPodFixture | provider::runpod::tests::pod_fixture_decodes | 18 |
| TestClient | provider::runpod::tests::client_rest_calls | 20 |
| TestSelf | — P2 (`lobo-agent` RunPod self-terminate over GraphQL) | P2 |
| TestTimeRoundTrip | provider::runpod::tests::runpod_time_round_trip | 18 |
| TestPayloadMinDownload | provider::runpod::tests::payload_min_download | 19 |

**internal/vast/vast_test.go**

| Go | Rust | Task |
|---|---|---|
| TestRentFastestOfferSkipsTakenAndTried | provider::vast::tests::rent_skips_taken_and_tried | 25 |
| TestCreateBody | provider::vast::tests::create_body_via_rent | 25 |
| TestRentNoCredit | provider::vast::tests::rent_no_credit_stops_at_once | 25 |
| TestCreateBodyBaked | provider::vast::tests::create_body_baked | 24 |
| TestListGetDelete | provider::vast::tests::list_get_delete | 26 |
| TestSelfTerminateAndGone | — P2 (`lobo-agent` Vast self-terminate) | P2 |
| TestBadKey | provider::vast::tests::bad_key_names_env | 22 |
| TestRentUncertainCreateNeverRentsTwice | provider::vast::tests::uncertain_create_never_rents_twice | 26 |

**internal/local/*_test.go**

| Go | Rust | Task |
|---|---|---|
| deps: TestCheckGPU | local::deps::tests::check_gpu_table | 42 |
| deps: TestDownload | local::deps::tests::download_table | 43 |
| deps: TestStartLlama | local::deps::tests::start_llama_args_env | 44 |
| deps: TestGPUMetrics | local::deps::tests::gpu_metrics_vm_stat | 42 |
| deps: TestWaitHealthy | local::deps::tests::wait_healthy_polls | 44 |
| deps: TestNewDepsWiring | local::deps::tests::new_deps_wiring | 44 |
| models: TestList | local::models::tests::list_states | 39 |
| models: TestListMissingWeights | local::models::tests::list_missing_weights | 39 |
| models: TestListJSON | local::models::tests::list_json_shape | 39 |
| models: TestMarkerPath | local::models::tests::marker_path | 39 |
| models: TestMarkerValid | local::models::tests::marker_valid_table | 39 |
| platform: TestSupported | local::platform::tests::supported_matches_target | 36 |
| platform: TestUsableMiB | local::platform::tests::usable_mib_on_apple_silicon | 36 |
| provider: TestMain (helper child) | src/bin/testchild.rs (helper binary, not a test) | 48 |
| provider: TestProviderLifecycle | tests/local_provider.rs::lifecycle | 51 |
| provider: TestProviderDeleteKills | tests/local_provider.rs::delete_kills_stubborn | 52 |
| provider: TestProviderDeleteKillsGroup | tests/local_provider.rs::delete_kills_group | 52 |
| provider: TestProviderDeleteReverifies | tests/local_provider.rs::delete_reverifies_before_sigkill | 52 |
| provider: TestIsSupervisor | local::provider::tests::is_supervisor_table | 47 |
| provider: TestProviderDeleteStranger | tests/local_provider.rs::delete_never_signals_stranger | 52 |
| provider: TestProviderChildFails | tests/local_provider.rs::child_fails_shows_log_tail | 51 |
| provider: TestProviderPrechecks | tests/local_provider.rs::prechecks_before_runtime | 49 |
| runtime: TestRuntimeEnsure | local::runtime::tests::ensure_fetches_once | 41 |
| runtime: TestRuntimeNote | local::runtime::tests::runtime_note | 41 |
| runtime: TestRuntimeRejects | local::runtime::tests::ensure_rejects_table | 41 |
| runtime: TestRuntimeShort | local::runtime::tests::ensure_short_body_fails | 41 |
| state: TestStatePath | local::state::tests::state_path_xdg_and_home | 37 |
| state: TestStateRoundTrip | local::state::tests::state_round_trip | 37 |
| state: TestStateDeadPID | local::state::tests::dead_pid_state_removed | 37 |
| state: TestClaimState | local::state::tests::claim_state_table | 38 |
| state: TestClaimStateConcurrent | local::state::tests::claim_state_concurrent_one_winner | 38 |
| state: TestRemoveStateIf | local::state::tests::remove_state_if_table | 38 |
| state: TestStateCorrupt | local::state::tests::corrupt_state_errors | 37 |

**internal/control/control_test.go** (+ `controltest/fakes.go` → `control::testkit`, Task 54)

| Go | Rust (`control::tests::`) | Task |
|---|---|---|
| TestUpHappy | up_happy | 57 |
| TestUpOverrides | up_overrides | 55 |
| TestUpBakedImage | up_baked_image | 55 |
| TestUpBakedImageNeedsNoReleaseManifest | up_baked_image_needs_no_release_manifest | 55 |
| TestUpImageAndReleaseConflict | up_image_and_release_conflict | 55 |
| TestUpAlreadyRunning | up_already_running | 55 |
| TestUpNoCapacityAnywhere | up_no_capacity_anywhere | 57 |
| TestUpAgentFailed | up_agent_failed | 57 |
| TestUpTimeout | up_timeout | 57 |
| TestDownAndSnapshot | down_and_snapshot | 60 |
| TestEventsHelper | events_helper | 57 |
| TestUpReplacesPodWithBrokenGPU | up_replaces_pod_with_broken_gpu | 58 |
| TestUpGivesUpAfterFourBadHosts | up_gives_up_after_four_bad_hosts | 58 |
| TestUpReportsExpiryReason | up_reports_expiry_reason | 57 |
| TestUpRejectsBadOptionsBeforeRenting | up_rejects_bad_options_before_renting | 55 |
| TestUpRetriesSlowHostAndIgnoresPollBlip | up_retries_slow_host_and_ignores_poll_blip | 58 |
| TestUpReplacesPodWhoseContainerNeverStarts | up_replaces_pod_whose_container_never_starts | 58 |
| TestUpStepsDownNetworkTiers | up_steps_down_network_tiers | 57 |
| TestUpCommunityOnlyByDefault | up_community_only_by_default | 57 |
| TestUpSecureAllTiersBeforeCommunity | up_secure_all_tiers_before_community | 57 |
| TestUpR2GetsFeeshFallback | up_r2_gets_feesh_fallback | 56 |
| TestUpMinMBps | up_min_mbps | 56 |
| TestUpIgnoresPreviousPodStatusAfterReRent | up_ignores_previous_pod_status_after_re_rent | 58 |
| TestUpDetectsPodGoneWhileAgentSilent | up_detects_pod_gone_while_agent_silent | 58 |
| TestDownWaitsForListToCatchUp | down_waits_for_list_to_catch_up | 60 |
| TestUpOnVastAndDownAcrossProviders | up_on_vast_and_down_across_providers | 59–60 |
| TestUpUnconfiguredProvider | up_unconfigured_provider | 55 |
| TestDownKeepsGoingWhenOneProviderFails | down_keeps_going_when_one_provider_fails | 60 |
| TestUpIgnoresOtherBootStatus | up_ignores_other_boot_status | 58 |
| TestUpFailedPollsDoNotResetStall | up_failed_polls_do_not_reset_stall | 58 |
| TestUpLocal | up_local | 59 |
| TestUpLocalFailureStopsRun | up_local_failure_stops_run | 59 |
| TestSnapshotLocal | snapshot_local | 60 |
| TestTarget | target_table | 61 |
| TestHTTPAgentURL | control::agent_http::tests::http_agent_url | 53 |
| TestSnapshotNoDomain | snapshot_no_domain | 60 |

**internal/checks, internal/release, internal/bootstrap**

| Go | Rust | Task |
|---|---|---|
| checks: TestValidateToolCall | checks::tests::validate_tool_call_table | 32 |
| checks: TestReadStream | checks::tests::read_stream_table | 33 |
| release: TestNextVersion | release::tests::next_version_table | 27 |
| release: TestBuildZipAndScan | release::zip::tests::build_zip_and_scan | 28 |
| release: TestResolve | release::tests::resolve_latest_and_version | 29 |
| bootstrap: TestScriptPerProvider | bootstrap::tests::script_per_provider | 15 |
| bootstrap: TestEnv | bootstrap::tests::env_keys | 16 |
| bootstrap: TestEnvBakedOmitsRelease | bootstrap::tests::env_baked_omits_release | 16 |
| bootstrap: TestScriptBaked | bootstrap::tests::script_baked_skips_apt_and_zip | 17 |
| bootstrap: TestScriptTerminatesOnFailure | bootstrap::tests::script_terminates_on_failure | 17 |

**cmd/lobo tests (core logic moves to P3; CLI-only stays P4)**

| Go | Rust | Task / phase |
|---|---|---|
| target_test: TestCheckTarget | control::precheck::tests::check_target_table | 62 |
| target_test: TestCheckProviders | control::precheck::tests::check_providers_table | 62 |
| target_test: TestCheckRelease | control::precheck::tests::check_release_table | 62 |
| target_test: TestProviders | control::wiring::tests::providers_from_config_table | 63 |
| target_test: TestLocalInstanceURLs | control::wiring::tests::local_instance_urls_from_state | 63 |
| target_test: TestWriteOpencode | genkey::tests::write_opencode_table | 34 |
| defaults_test: TestApplyDefaults | control::precheck::tests::apply_defaults_table | 62 |
| defaults_test: TestMaskedUnknownKeys | config::show::tests::masked_unknown_keys | 13 |
| defaults_test: TestShowConfigJSONMasks | config::show::tests::show_masks_secrets | 13 |
| defaults_test: TestParseSetArgs | — P4 (CLI arg parsing) | P4 |
| defaults_test: TestParseSetJSON | — P4 (CLI stdin parsing) | P4 |
| local_test: TestLocalRunFlags | local::supervise::tests::run_config_from_args_table (the "local is hidden" assert stays P4) | 45 / P4 |
| local_test: TestModelsOutput | — P4 (text output of `lobo models`) | P4 |

Counted (`grep -c '^func Test'` per file): 126 Go test functions in these files. 117 move to P3 Rust tests, 1 (`TestMain`) becomes the helper binary, 5 go to P2 (pod-side: 3× TestLoadAgent*, TestSelf, TestSelfTerminateAndGone), 3 stay in P4 (TestParseSetArgs, TestParseSetJSON, TestModelsOutput; plus the "hidden" assert of TestLocalRunFlags). None dropped.

## Carry-over rules landing in P3 → test

| Rule (spec) | Go evidence | Rust test | Task |
|---|---|---|---|
| RunPod REST needs a User-Agent | plans/done/2026-09-25-vast-provider/results.md:36 | http::tests::client_sends_user_agent; provider::runpod::tests::client_sends_user_agent | 3, 20 |
| Vast `runtype: "ssh"` | internal/vast/provider.go:31-38 | provider::vast::tests::create_body_runtype_ssh | 24 |
| Vast DELETE 404 = gone | internal/vast/client.go:187-194 | provider::vast::tests::destroy_404_is_gone | 23 |
| Vast `insufficient_credit` stops at once | internal/vast/client.go:150-153, provider.go:72-74 | create_no_credit; rent_no_credit_stops_at_once | 23, 25 |
| Bootstrap steps time-bounded | internal/bootstrap/bootstrap.go:16,20,49,51 | bootstrap::tests::script_steps_time_bounded | 15 |
| Terminate retried 30× then sleep | bootstrap.go:32-42 | script_terminate_retries_30_then_sleeps; script_terminates_on_failure | 15, 17 |
| `execfail` for a missing agent | bootstrap.go:56-61 | script_execfail_then_die; script_terminates_on_failure (exec row) | 15, 17 |
| Baked image skips apt + zip | bootstrap.go:45-47 | script_baked_skips_apt_and_zip | 17 |
| Secrets never in pod env | bootstrap.go:64; runpod_test.go:45-48; vast_test.go:177-180 | env_never_has_account_secrets; up_create_opts_carry_no_laptop_secrets | 16, 56 |
| Secrets never in releases | internal/release/zip.go:52-82 | release::zip::tests::scan_catches_laptop_secret_values | 28 |
| Secrets never in children | internal/agent/env.go:5-19; local/deps.go:173 | local::deps::tests::start_llama_args_env | 44 |
| Config only from the file, never OS env | internal/config/laptop.go:53-55; config_test.go:49-60 | tests/config_os_env.rs::load_laptop_ignores_os_env; dotenv::tests::never_reads_process_env | 5, 9 |
| `up` refuses while anything runs | internal/control/up.go:67-73 | up_already_running; up_on_vast_and_down_across_providers | 55, 59 |
| `down` needs only provider keys | cmd/lobo/main.go:90-98 | check_providers_table | 62 |
| `release` needs R2 + bucket | cmd/lobo/main.go:100-106 | check_release_table | 62 |
| Supervisor identity = `local run` + exact `--boot-id` | internal/local/provider.go:262-279 | is_supervisor_table; app_argv_passes_identity | 47 |
| SIGKILL the process group | provider.go:243-250 | delete_kills_group | 52 |
| State removed only by pid + boot id | internal/local/state.go:110-129 | remove_state_if_table | 38 |
| Marker `<sha> <size> <mtime_ns>` | internal/local/models.go:41-55 | marker_valid_table; marker_written_by_go_is_valid | 39 |
| Endpoints from the running state's ports | provider.go:293-298; cmd/lobo/target_test.go:139-150 | instance_urls_from_state_ports; local_instance_urls_from_state | 47, 63 |
| Bad-host replace ×4 | internal/control/up.go:125-127, 212 | up_gives_up_after_four_bad_hosts | 58 |
| Container timeout | up.go:214-226, 339-346 | up_replaces_pod_whose_container_never_starts; consts_match_go | 50, 58 |

Not in P3 (other phases own them): "self-terminate over GraphQL with the pod-scoped key" (P2, lobo-agent), agent CUDA/VRAM/slow-download/SSH-streams/PID-1 rules (P2), R2 DNS hijack (P6 test environment).

---

## Self-review

- Spec P3 TL;DR → tasks: config (5–13), RunPod (18–21), Vast (22–26), local (36–52), up/status/down/target (50, 53–61), `lobo test` checks (32–33), logs (`AgentApi::logs`, 53), release upload (27–31). Workspace line → bootstrap (15–17), local supervisor on lobo-agent (42–46), genkey (34–35).
- Non-goal "must read what the old ones wrote": config (Task 8 golden), state file (Task 37 P1 fixture + Task 38 Go interop), markers (Task 39).
- Names vs contracts.md v1.0: `config::{Laptop, default_path, load_laptop, values, save, show, parse_local_port}` ✓; `Laptop::{require_cloud, require_provider_key, require_bucket, require_r2, secret_values, defaults, weights, port, providers, default_provider}` ✓; `provider::{CreateOpts, Provider}` with the exact trait ✓; `provider::{runpod, vast, local}` modules ✓; `Error::NoCapacity`, `Error::NotFound` ✓; `control::{UpOpts, Deps (exact fields), up, snapshot, down, target, AgentApi, HttpAgent::new}` ✓; `deps_from_config` ✗ changed (Contract additions 2); `checks::{chat, tool_call, validate_tool_call}` ✓ (args filled in); `release::{next_version, zip_key, meta_key, LATEST_KEY, build_zip, scan_for_secrets, Store::new, publish, list_release_keys, presign_get, resolve}` ✓; `bootstrap::{script, env}` ✓; `local::{supported, usable_mib, list, ensure_runtime, state_path, read_state, claim_state, remove_state_if, RunConfig, supervise, SUPERVISOR_ARG, Spawner}` ✓ (`Spawner` app prefix changed: Contract additions 5); `genkey::opencode_config` ✓ (args filled in); `Error::kind` ✓.
- `Presigner::presign_get` is async in the trait, `Store::presign_get` is a sync inherent fn (rusty-s3 signing is pure); the trait impl wraps it.
- No TODO or placeholder left. The one fill-in is "Pinned versions" (Task 1) and the P2 name mapping (Task 0), same pattern as P1.
- Task count: 67 (Tasks 0–66), 2 of them orchestrator-only (0, 66).

---

## Contract additions (orchestrator folds these into contracts.md v1.1)

1. `lobo_core::{clock::{Clock, SystemClock, FixedClock, StepClock}, http::{client, USER_AGENT}}`. `Error` variants as in Task 2, plus `Error::is_not_found`, `is_no_capacity`.
2. **Changed:** `control::deps_from_config(cfg: Laptop, w: &Wiring) -> Result<Deps>` with `Wiring { config_path, spawner, supported }` and `Wiring::new(config_path, spawner)`. Also `providers_from_config`, `local_provider_from_config`. Reason: the local provider needs the exe, the argv prefix and the absolute config path, and `cfg` alone has none of them.
3. `control::{ReleaseResolver, Presigner}` traits (named in Deps but not defined in v1.0), `control::HttpAgent::on_domain`, consts `MAX_GPU_RETRIES`, `CONTAINER_TIMEOUT` (P4's TUI hint "re-rent at 6:00"), `STALE_SLACK`, `POD_CHECK_EVERY`.
4. `control::{check_target, check_providers, check_release, apply_defaults}` so the CLI and the app share pre-checks and flag > config > release defaults. `control::testkit` behind feature `testkit` (P4 TUI tests).
5. **Changed:** `Spawner::app(exe)` prefix = `[SUPERVISOR_ARG, "local", "run"]`, not `[SUPERVISOR_ARG]`. Plus `Spawner::cli(exe)`. Reason: identity is `local run` + `--boot-id` (carry-over rule), and a Go `lobo down` must still recognise an app-spawned supervisor during the transition. The app's `main` checks `args[1] == SUPERVISOR_ARG` and passes `args[4..]` to `RunConfig::from_args`.
6. `local::{StateFile, alive, LocalProvider, LocalHooks, EnsureRuntime, is_supervisor, command_of, instance, log_path, marker_path, HF_BASE, RUNTIME_VERSION, runtime_dir}`; `RunConfig` fields (Task 45) with `from_args(args, version)`, `to_args`, `validate`. `provider::local` re-exports `LocalProvider`.
7. `config::{R2Creds fields, Laptop::from_values, Defaults fields, defaults_partial, DEFAULT_LOCAL_PORT, default_path_from, loose_mode, LAYOUT, LayoutGroup, HEADER, set_env_value, PLAIN_KEYS, mask, masked}`.
8. `provider::{POD_NAME, on_domain}`, `provider::runpod::{Pod, RunPodTime, Machine, Client, RunPodApi, RunPodProvider, build_create_payload, NET_TIERS, GPU_TYPE}`, `provider::vast::{Offer, Inst, Client, VastProvider, create_body, search_query, DISK_GB}`.
9. `checks::read_stream`; concrete args `chat(hc, base_url, key, model)`, `tool_call(hc, base_url, key, model)`.
10. `release::{BUCKET, zip_url, BucketReleases}`, `Store::publish(zip, &Resolved)`, `Store::list_release_keys()`, `Store::presign_get(key, ttl) -> String`.
11. `genkey::{OPENCODE_OUT, new_api_key, ensure_api_key(path, rotate), write_opencode}`; `opencode_config(domain, key, port)`.
12. lobo-agent (P2) exports P3 relies on, beyond v1.0 (already in `plan-p2-v1.0.md`; fold them into contracts v1.1 so they stay public): `logring::{LogRing, LogSource}`, `process::{start_process, Proc, last_line}`, `health::wait_healthy`, `fetch::fetch_file`, `Option<&dyn Fn>` progress callbacks in `download`/`hash_file`.

## Spec issues

1. The spec's `lobo-core` line lists "runpod REST+GraphQL". The GraphQL part is pod self-terminate, which runs inside `lobo-agent`. `lobo-core` depends on `lobo-agent`, so the agent cannot call core. Self-terminate (runpod GraphQL + Vast instance-scoped GET/DELETE) and `config.LoadAgent` belong to P2. Their Go tests (TestSelf, TestSelfTerminateAndGone, TestLoadAgent*) are mapped to P2. `plan-p2-v1.0.md` already takes them (`selfkill.rs`, its line 78); the spec line should say so.
2. contracts.md v1.0 `deps_from_config(cfg)` cannot build the local provider (no exe, argv prefix or config path). Fixed by Contract additions 2.
3. contracts.md v1.0 `Spawner` app prefix `[SUPERVISOR_ARG]` breaks the identity carry-over rule and Go interop. Fixed by Contract additions 5.
4. P1's Go-test table puts all `cmd/lobo/*_test.go` in P4. Nine of those tests check logic the app also needs (pre-checks, defaults, providers wiring, opencode, masking, run flags). They move to P3 here; P4 keeps the four CLI-only ones.
5. `UpEvent.err` is a string (P1 wire type = Go `--json`). The app wants `{kind, message}` for up failures too. Options for P5: add `err_kind` (omitempty, Go CLI ignores it) — needs a P1 wire change and a fixture; or accept kind `"up"` for all up failures. P3 does not change the wire type.
6. The spawned argv moves `--config` from before `local run` (Go) to after the prefix (Rust), because the app prefix must come first. Go's cobra reads `--config` in either place, and identity only checks `local run` + `--boot-id`, so both directions still work. The Go test's expected argv string is adjusted in `lifecycle`.
7. Spec line "Local: runtime fetch b11118, models listing + markers, supervisor, state claim/flock, group kill, active endpoints" is fully P3. The spec's "Rust home" says `lobo-core::local + lobo-agent`. Only the Runner, downloader and API router come from `lobo-agent`; the rest is core.


---
<!-- end of plan-p3-v1.0.md -->

# Rust rewrite P4 — `lobo-cli` (binary `lobo`) Implementation Plan v1.0

**Date:** 2026-09-29
**Status:** draft
**Spec:** ./spec.md (spec status: parked — this plan is written on request; exec still needs spec + plan approval)
**Contracts:** ./contracts.md v1.0 + the P3 plan's "Contract additions" 1–12 (`plan-p3-v1.0.md`, to be folded as v1.1). P4 uses those names; its own extras are at the end.
**Phase:** P4 of 6. Needs P1 (`lobo-proto`) and P3 (`lobo-core`) done on `feat/rust`.
**Split with P3:** pre-checks, `apply_defaults`, provider wiring, opencode generation, masking, `config::show` and `RunConfig` parsing/validation live in `lobo-core` (P3 Tasks 13, 34, 45, 62, 63) because the app needs them too. Their Go tests are ported in P3. P4 calls them and tests only the CLI boundary (flags → arguments, output bytes, exit codes).

**Goal:** a Rust `lobo` binary with the same commands, flags, stdout, stderr messages and exit codes as Go `cmd/lobo`. That includes the `up` progress view, the live `status` dashboard, the `lobo config` wizard, every `--json` output, `release`, the hidden `local run` supervisor entry and a brew formula built from Rust.

**Architecture:** `crates/lobo-cli` is a lib (`lobo_cli`) plus a bin (`lobo`). All logic sits in the lib behind one `App` struct with injected seams (deps factory, local-support check, supervisor, clock, TTY flags), so tests run commands in-process. `main.rs` only builds `App::real()` and calls `lobo_cli::run`. Parity is proven three ways: (1) the Go goldens and Go-captured fixtures, byte-for-byte where the Go output is lobo-authored; (2) a replay of ~40 no-network Go CLI runs (`cases.json`) against the Rust binary with `assert_cmd`; (3) a port of every Go test in `cmd/lobo`, `internal/tui`, `internal/configtui`. Go stays on the branch untouched (except one build-tagged capture test) until P6.

**Tech Stack:** Rust 1.98 (edition 2024). clap (derive, no `env` feature), clap_complete, ratatui + crossterm, inquire, anyhow, tokio, tokio-util, tracing + tracing-subscriber, chrono, serde_json, unicode-width. Dev: insta, assert_cmd, predicates, tempfile, serial_test, wiremock. Release: goreleaser 2.13 `builder: rust` + cargo-zigbuild + zig (spike in Task 47 decides).

> For agentic workers: use superpowers:subagent-driven-development to implement task-by-task. Checkbox syntax for tracking.

> Implementer scope (verbatim, every dispatch): writes only the code and unit tests its task names and runs that task's focused unit test. Never runs e2e / integration / live / smoke tests, never rents a GPU or pod, never calls a provider API, never publishes, deploys or touches infra, never runs anything money-bearing. Never sets `TEST_DATABASE_URL` or any test-DB env var.

P4-specific addition to the scope (verbatim, every dispatch): never run the real `lobo up`, `down`, `status`, `logs`, `test` or `release` against a real config; only in-process tests with fakes, or the `test-fakes` binary with `LOBO_TEST_SCENARIO`.

---

## Decisions (locked)

1. **Help parity** = same command path, about/long text, flag set (long, short, value kind, default, help text) in cobra's order, and the same subcommand list; cobra's column layout and `(default "x")` vs clap's `[default: x]` are not compared. The root help (`lobo`, `lobo help`, `lobo -h`, `lobo --help`) is custom in Go and must be byte-identical.
2. **Makefile:** the spec's file table gives the Makefile no phase, and the rollout removes Go at P6. So P4 does NOT touch the Go targets. It adds `rust-*` targets (P1's naming, not `*-rust`, to stay consistent), and the Rust binary installs as `lobo-rs` next to the Go `lobo`. P6 swaps `build`/`install`/`lint`/`test`/`release` to cargo and drops the `lobo-rs` name.
3. **Go code stays** on the branch in P4 (`cmd/lobo`, `internal/tui`, `internal/configtui`). The Swift app still bundles the Go `lobo` until P5/P6, and P1's drift CI needs the Go module. P4 adds one Go file: `cmd/lobo/capture_test.go` (`//go:build capture`), removed with Go at P6.
4. **JSON parity** = same JSON values (numbers compared numerically, same rule as P1 `assert_json_eq`), same key order, one object per line, `\n` after each. Byte escapes may differ (Go writes `\u003c` for `<`; serde_json writes `<`).
5. **Error parity:** every error goes to stderr as `error: <msg>` and exits 1 (Go `main.go:53-56`). Messages lobo writes itself are byte-identical. Messages the parser writes (unknown flag, bad int, wrong arg count) and wrapped OS error text may differ; exit code and the `error: ` prefix may not.
6. **Log-line parity** (human logs on stderr): same level tag (`INF`/`WRN`/`ERR`), message and `key=value` fields in zerolog's order (sorted by key) and quoting. Timestamps are not compared. Colour only when stderr is a TTY (Go's zerolog colours always; the difference is invisible in a terminal).
7. **Extra positional args:** Go leaf commands without an `Args` rule silently ignore extra args (`lobo up foo` runs `up`). Rust rejects them with exit 1. This is a deliberate strictness fix; it is listed under "Spec issues" for the user to confirm.
8. **Wizard:** inquire asks one prompt at a time. Same questions, order, show/hide rules, validation messages, summary and saved result as the huh form. No Shift+Tab back navigation (inquire has none); Esc or Ctrl+C quits without saving. The intro note text changes its last line accordingly.
9. **Test seams:** in-process `lobo_cli::run(&App, argv, &mut Io)` for unit tests. For `assert_cmd` against fakes, the cargo feature `test-fakes` (never on in release builds) lets the binary read `LOBO_TEST_SCENARIO` and swap in `lobo_core::control::testkit` fakes. `make rust-build-lobo` and goreleaser build default features only; Task 51 checks the release binary ignores the variable.
10. **Parity baseline** is `master` HEAD at P4 start, not `5443667`: commit `6a72092` changed `internal/tui/status.go` and added `status_local.golden` after `5443667`.

## Pinned versions

_(Task 1 fills this line from `Cargo.lock`: clap, clap_complete, ratatui, crossterm, inquire, insta, assert_cmd, predicates, anyhow, tracing, tracing-subscriber, unicode-width, serial_test, wiremock, tempfile.)_

---

## File map

**Create**
- `crates/lobo-cli/Cargo.toml` — `[lib] name = "lobo_cli"`, `[[bin]] name = "lobo" path = "src/main.rs"`, `[features] test-fakes = ["lobo-core/testkit"]`, `[[test]] name = "cli_fakes" required-features = ["test-fakes"]`; dev-dependency `lobo-core` with feature `testkit` (TUI golden tests).
- `crates/lobo-cli/build.rs` — `cargo:rerun-if-env-changed=LOBO_VERSION|LOBO_COMMIT|LOBO_DATE`.
- `crates/lobo-cli/src/main.rs` — `App::real()` (+ `test-fakes` swap), tokio runtime, `std::process::exit(run(...))`.
- `crates/lobo-cli/src/lib.rs` — module list, `run`.
- `crates/lobo-cli/src/build_info.rs` — `VERSION`, `COMMIT`, `DATE`.
- `crates/lobo-cli/src/app.rs` — `App`, `Term`, `Io`, `App::real`, config loading.
- `crates/lobo-cli/src/cli.rs` — clap tree (`Cli`, `Cmd`, all args structs), `command()`, `changed()`, `parse_bool_flag`.
- `crates/lobo-cli/src/duration.rs` — `parse_go_duration`.
- `crates/lobo-cli/src/logfmt.rs` — zerolog-console `FormatEvent` for tracing-subscriber.
- `crates/lobo-cli/src/help.rs` — custom root help.
- `crates/lobo-cli/src/bootlog.rs` — `tee_ready`, `report_boot`, `BootLine`.
- `crates/lobo-cli/src/cmd/{mod,version,config,genkey,release,up,down,status,logs,test,models,local,completion}.rs`
- `crates/lobo-cli/src/tui/{mod,styles,up,status,run}.rs`
- `crates/lobo-cli/src/wizard/{mod,state,validate,flow,prompt}.rs`
- `crates/lobo-cli/src/fakes.rs` — `#[cfg(feature = "test-fakes")]` scenarios.
- `crates/lobo-cli/src/snapshots/*.snap`, `crates/lobo-cli/tests/snapshots/*.snap` — insta.
- `crates/lobo-cli/tests/goldens/*.golden` — byte copies of `cmd/lobo/testdata/*.golden` and `internal/tui/testdata/*.golden`.
- `crates/lobo-cli/tests/fixtures/go/help/*.txt` — Go `--help` output per command.
- `crates/lobo-cli/tests/fixtures/go/cases.json` — Go CLI replay cases.
- `crates/lobo-cli/tests/fixtures/go/text/*.txt|*.jsonl` — Go-captured plain up, report_boot, json up/status/down lines.
- `crates/lobo-cli/tests/{help_parity,go_replay,cli_config,cli_fakes}.rs`
- `tools/clifixtures/capture.py` — stdlib-only Python capture of Go help + cases (removed with Go at P6).
- `cmd/lobo/capture_test.go` — `//go:build capture`; writes `text/` fixtures (removed with Go at P6).

**Modify**
- `Cargo.toml` (workspace) — new deps in `[workspace.dependencies]`.
- `Makefile` — `rust-build-lobo`, `rust-install`, `cli-fixtures`, `rust-release-snapshot`; `rust-test`/`rust-lint` gain `--features lobo-cli/test-fakes`. Go targets unchanged.
- `.github/workflows/rust.yml` — add the `cli-fixtures` drift step.
- `.goreleaser.yaml`, `.github/workflows/release.yml` — Rust build per the Task 47 decision.
- `plans/2026-09-29-rust-rewrite/spec.md` — file-table rows for `tools/clifixtures/`, `cmd/lobo/capture_test.go`; P4 as-built notes (Task 52).

**Out of scope for P4**
- Any `lobo-core` / `lobo-agent` / `lobo-proto` logic (gaps go to "Contract additions").
- README (install text flips to Rust at P6 with the Makefile swap).
- The dmg job and the Swift app (P5 replaces them).
- Publishing: no tag, no GitHub release, no tap push.

---

## Task 2 is the parity inventory — the acceptance checklist

Every later task names the rows it satisfies. Row IDs are stable; Task 52 ticks each one. `file:line` = Go source at `6a72092`.

### Global

| ID | Behaviour | Go source |
|---|---|---|
| G-01 | `lobo`, `lobo help`, `lobo -h`, `lobo --help` → custom root help on stdout, exit 0. Byte-identical to `help_config.golden` / `help_noconfig.golden` (colour off). Colour only if stdout is a TTY and `NO_COLOR` is empty. Hidden commands (`release`, `local`, `completion`) never listed. First line `lobo <VERSION>`. | help.go:13-85, main.go:44-50 |
| G-02 | Global `--config <string>`, default `config::default_path()` (`$XDG_CONFIG_HOME/lobo/config.env`, else `~/.config/lobo/config.env`), help `config file (dotenv; the OS env is never read)`. Shown under "Global Flags" on every subcommand help. | main.go:45, config/path.go:16-25 |
| G-03 | `--env` = hidden alias of `--config`. | main.go:46-47 |
| G-04 | Any error → stderr `error: <msg>\n`, exit 1. No usage text after errors. | main.go:44,53-56 |
| G-05 | Unknown command → exit 1, stderr starts `error: ` (Go: `unknown command "x" for "lobo"`). | cobra |
| G-06 | Unknown flag → exit 1, stderr starts `error: ` (Go: `unknown flag: --bogus`). | cobra |
| G-07 | SIGINT cancels the command's context (plain/json `up`, `status --json`, `down`, `logs`, `test`). | main.go:51 |
| G-08 | Hidden `completion bash|zsh|fish|powershell` → a shell script on stdout, exit 0. | main.go:49 |
| G-09 | `load_cfg`: file missing → `no config at {path}. Run \`lobo config\` first`. File mode has group/other bits → WRN log `{path} holds API keys and other users can read it: chmod 600 {path}`. Used by up, down, status, logs, test, models, release, local run. Not by config*, gen-api-key, version. | main.go:64-72 |
| G-10 | Every bool flag accepts `--f`, `--f=true`, `--f=false` (pflag). The Swift app sends `--q6=false` (Store.swift:170). "Given on the command line" is tracked even for `=false` (pflag `Changed`). | pflag |
| G-11 | Extra positional args on leaf commands: Go ignores, Rust rejects, exit 1 (Decision 7). `local run`, `models` reject in Go too (`cobra.NoArgs`). | local.go:67,153 |
| G-12 | `lobo <cmd> --help`, `lobo help <cmd>` → subcommand help, exit 0. Parity per Decision 1. | cobra |
| G-13 | Human logs on stderr in zerolog console form `HH:MM:SS LVL msg k=v …` (Decision 6). | main.go:40 |

### version

| ID | Behaviour | Go source |
|---|---|---|
| V-01 | Short `Print version, commit and build date`. stdout `lobo {VERSION} ({COMMIT}, {DATE})\n`, exit 0. Defaults `dev`, `none`, `unknown`; set at build time (Go `-X`, Rust `LOBO_VERSION`/`LOBO_COMMIT`/`LOBO_DATE`). | main.go:31-36,59-62 |

### config

| ID | Behaviour | Go source |
|---|---|---|
| C-01 | `config`: Short `Set API keys and defaults for \`lobo up\` (form in a terminal)`, Long (3 lines, config.go:22-24). Reads `config::values` (missing file = empty). stdout or stdin not a TTY → stderr `not a terminal: showing {path} instead of the form\n`, then C-04 text on stdout, exit 0. | config.go:18-33 |
| C-02 | Wizard quit or Discard → stdout `nothing saved\n`, exit 0. | config.go:38-41 |
| C-03 | Save → `config::save`; stdout `saved {path}\n`; if LOBO_API_KEY changed and the old one was non-empty → `new LOBO_API_KEY: a running pod keeps the old one until the next \`lobo up\`; \`lobo gen-api-key\` rewrites opencode.lobo.json\n`; if `load_laptop` fails → `still missing before \`lobo up\` works: {err}\n`. | config.go:42-51 |
| C-04 | `config show` (Short `Print the config with API keys masked`): `# {path}`; empty map → `# no config yet: run \`lobo config\` in a terminal`; else per LAYOUT group `\n# {title}` then present keys `K=masked`; unknown keys sorted under `\n# other`. | config.go:56-69,169-200 |
| C-05 | `config show --json` (help `machine-readable: path, exists, masked values, which keys are set`): one line `{"path","exists","values","set"}`; values masked; `set[k] = v != ""`; exists = file stat ok. | config.go:70,154-167 |
| C-06 | Masking: plain keys (config.go:204-209) in clear; every other key, known or not, → `Mask`: `""`→`(not set)`, len<12 → `••••`, else first 4 + `…` + last 4. | config.go:202-216, configtui.go:22-30 |
| C-07 | `config set KEY=value...`: Short/Long config.go:74-76; `--stdin` help `read {"KEY": "value"} JSON from stdin (keeps secrets out of argv)`. Errors: `use KEY=value arguments or --stdin, not both`; `want KEY=value arguments or --stdin`; `want KEY=value with KEY like LOBO_MIN_MBPS, got "{a}"`; `config set --stdin: want a JSON object of strings: {err}`; `config set --stdin: empty object`; `config set --stdin: bad key "{k}" (want like LOBO_MIN_MBPS)`. Key rule `^[A-Z][A-Z0-9_]*$`. `KEY=` removes. Value may contain `=`. stdin limit 1 MiB, first JSON value only. No stdout, exit 0. | config.go:71-151 |
| C-08 | `config get KEY`: exactly 1 arg; stdout value + `\n`; missing → `{KEY} is not set in {path}`. Short `Print one value in clear (e.g. LOBO_API_KEY for a client config)`. | config.go:97-112 |
| C-09 | `config path` → `{path}\n`. Short `Print the config file path`. | config.go:113-117 |

### gen-api-key

| ID | Behaviour | Go source |
|---|---|---|
| K-01 | Short `Create LOBO_API_KEY in the config once (kept across ups) and write opencode.lobo.json with it`; `--rotate` `replace the existing key`. Reads the file raw (missing → `read {path}: …`). Empty key or `--rotate` → new `sk-` + 48 hex, saved, INF `new LOBO_API_KEY written to the config (a running pod keeps the old key until the next \`lobo up\`)`; else INF `keeping existing LOBO_API_KEY (use --rotate for a new one)`. Empty LOBO_DOMAIN → INF `LOBO_DOMAIN is empty: writing only the lobo-local provider (this Mac)`. Writes `./opencode.lobo.json` mode 0600 (content = `genkey::opencode_config`), stdout `wrote {abs path}\n`. | genkey.go:18-60 |

### release (hidden)

| ID | Behaviour | Go source |
|---|---|---|
| R-01 | Short `Build lobo-agent, zip it with release.json, scan for secrets, upload to bucket lobo`. Order: load_cfg → check_release (R2 then bucket) → store → list keys → `next_version(keys, now)` → git_info → build agent → manifest → zip → secret scan → publish → INF `released` (version, git_sha, dirty, zip) → WRN `working tree is dirty: /api/version will say git_dirty=true` if dirty → stdout `{ver}\n`. | main.go:109-173 |
| R-02 | git_info: `git rev-parse --short HEAD` (error `git rev-parse: …`), `git status --porcelain` non-empty = dirty (error `git status: …`). whoami = `user@host`, or host alone. | main.go:175-194 |
| R-03 | Agent build: Go `go build … ./cmd/lobo-agent`. Rust: `cargo zigbuild --release -p lobo-agent --target x86_64-unknown-linux-musl` in the git top-level dir, env `LOBO_VERSION={ver}`, its stdout+stderr → our stderr, failure `build agent: {err}`; binary copied from `target/x86_64-unknown-linux-musl/release/lobo-agent`. Manifest: `built_at` = now UTC truncated to the second, `llama_image` = `DEFAULT_LLAMA_IMAGE`, model ref from `catalog::get(DEFAULT_MODEL)`, `defaults` = `DEFAULT_DEFAULTS`. | main.go:136-161 |

### up

| ID | Behaviour | Go source |
|---|---|---|
| U-01 | Short `Rent a 5090 and boot lobo; shows progress until the API is ready`. Flags in cobra order: `--cloud string` default `community`; `--conns int`; `--ctx int`; `--idle-min int`; `--image string`; `--json`; `--max-life duration`; `--min-mbps int`; `--plain`; `--provider string`; `--q6`; `--release string`; `--source string`; `--ssh string`. Help texts verbatim from main.go:247-260. | main.go:196-262 |
| U-02 | Check order: load_cfg → cloud ∉ {secure, community} → `--cloud: want secure or community, got "{v}"` → `--q6` true sets model `q6` → apply_defaults → check_target → `--ssh` file read, trimmed → ssh_key → deps → presign dropped for provider local → events. | main.go:203-234 |
| U-03 | apply_defaults: flag > config > built-in. `Defaults` error blocks only for bad keys whose flag was not given (map LOBO_PROVIDER→provider, LOBO_MODEL→q6, LOBO_CLOUD→cloud, LOBO_CTX→ctx, LOBO_IDLE_MIN→idle-min, LOBO_MAX_HOURS→max-life, LOBO_MIN_MBPS→min-mbps) → `{err} (in {path}; fix it with \`lobo config\` or by hand)`. Unkeyed provider → `no {RUNPOD_API_KEY|VASTAI_API_KEY} in {path}. Run \`lobo config\` to add it`; unknown → `--provider: want runpod, vast or local, got "{p}"`. Local needs no key. | defaults.go:16-68 |
| U-04 | check_target: provider local → `local::supported()`; else `require_cloud()`. | main.go:83-88 |
| U-05 | Mode: `--json` → U-06; `--plain` or stdout not a TTY → U-07; else TUI (T-rows). | main.go:235-242 |
| U-06 | `--json`: each `UpEvent` as one JSON line on stdout (J rule). Any event with `err` → after the stream, `error: up failed`, exit 1. | main.go:288-308 |
| U-07 | Plain: INF `up` with `phase`, `detail` (non-empty), `download` = `{pct:.1}% {mbps:.0} MB/s` (total > 0), and on ready `url`, `release`, `git_sha`, `usd_per_h`, `boot` (ms, rounded to the second). Error events → ERR `{err}` with `phase`. Exit 1 with the last error. | main.go:264-286 |
| U-08 | TUI quit keys `q`, `ctrl+c` → error `interrupted: the pod keeps booting; check with \`lobo status\`, stop with \`lobo down\``, exit 1. TUI error state → exit 1 with that error. | tui/up.go:177,192-196 |
| U-09 | After every mode: if a ready event with timings was seen → stderr `\nboot timings:\n` + 9 tabwriter rows (padding 2), and one line appended to `./boots.jsonl` (0644): keys `at` (now UTC, RFC3339Nano), `conns`, `ready`, `source`. | timings.go:13-61 |

### down, status, logs, test, models

| ID | Behaviour | Go source |
|---|---|---|
| D-01 | `down`: Short `Delete every lobo pod on every provider`; `--json` help `print {"spent_usd": …} on stdout`. load_cfg → check_providers → `control::down`. JSON `{"spent_usd":X}`; else INF `down: no lobo pods left` with `spent=$X.XX`. | main.go:310-336 |
| D-02 | check_providers: local supported → ok; else `require_provider_key()`. | main.go:90-97 |
| S-01 | `status`: Short `Live dashboard of the running pod`; `--once` `print one snapshot and exit`; `--json` `print one snapshot as JSON (pod, release, agent status) and exit`. load_cfg → check_providers → json (first) → `--once` or not TTY: render_status once (ANSI only on a TTY) → else live TUI. | main.go:338-373, tui.go:27-34 |
| S-02 | Live: fetch now, then every 2 s; `q`/`ctrl+c`/`esc` quit (exit 0); a `down` snapshot quits; on error keep the last good snapshot. Views: `loading…`; `status failed: {err}` + `retrying every 2s · q quit`; `refresh failed: {err}`; footer `q quit · refresh 2s`. | tui/status.go:143-196 |
| L-01 | `logs`: Short `Print the pod's recent agent, llama-server and cloudflared logs`; `-n, --n int` default 200 `number of lines (max 1000)`. load_cfg → target → `logs(n)` → stdout as-is. No check_providers. | main.go:375-399 |
| E-01 | `test`: Short `Smoke test the live API: streamed chat + tool call`. load_cfg → target → version (`lobo not reachable at {base}: {err}`) → status model or `DEFAULT_MODEL` → catalog get → chat (`chat: {err}`), text over 200 bytes → first 200 (char boundary) + `…`; INF `✓ streamed chat` took/release; stdout text; tool call (`tool call: {err}`); validate (`{err}\n{body}`); INF `✓ tool call: arguments is a JSON string` took. | main.go:401-450 |
| M-01 | `models`: Short `Catalog models and their state in the local weights folder`; no args; `--json` `print the listing as JSON (weights, free_bytes, models, runtime)`. Plain rows `%-4s %6.1f GB  %-12s %s` with state `missing` / `on disk` (+ `verified` or `not verified`) / `oversize` / `partial N%`; last line `weights {w} ({free:.1} GB free)`. | local.go:149-192 |

### local (hidden)

| ID | Behaviour | Go source |
|---|---|---|
| LR-01 | `local` hidden, Short `Local-mode internals (spawned by \`lobo up --provider local\`)`. `local run` Short `Supervise llama-server on this Mac and serve the agent API on 127.0.0.1:--api-port`, no args. Flags: `--api-port int` 8932 `agent API port`; `--boot-id string` `echoed in /api/status`; `--ctx int` `context size`; `--idle-min int` `minutes without requests before it stops`; `--model string` `q6` `catalog model id`; `--port int` 8931 `llama-server port`. | local.go:60-83 |
| LR-02 | Validate before anything: catalog error; `--ctx: want > 0, got {n}`; `--idle-min: want > 0, got {n}`; `--port: want 1-65535, got {n}`; `--api-port: want 1-65535 and not --port, got {n}`. | local.go:43-58 |
| LR-03 | Run: load_cfg → `local::supervise(RunConfig, cancel)`; SIGTERM/SIGINT cancel; a cancelled stop is exit 0. | local.go:87-147 |

### TUI

| ID | Behaviour | Go source |
|---|---|---|
| T-01 | Up phases: create `rent pod`, image `boot container`, tunnel, gpu `gpu check`, download `download model`, verify `sha256 verify`, load `load model`, ready. | tui/up.go:14-23 |
| T-02 | `apply_at`: timers from arrival times; going back to an earlier phase resets that phase and every later one; `failed`/`terminated` never change the phase; `err` sticks; `done` sticks. | tui/up.go:37-71 |
| T-03 | `render_up`: header, 8 rows (mark `✓`/`✗`/spinner/`·`, name padded 16, took `%-6s` or running clock `m:ss`, the image hint `usually 15–30 s · re-rent at {clock(CONTAINER_TIMEOUT)} `, detail, download bar + ETA), ready box, error box. Goldens `up_download`, `up_ready`, `up_failed`. | tui/up.go:82-152 |
| T-04 | `render_status`: sections Pod, Release, Agent, GPU/Mac, Host (cloud only), LLM, Watchdog; local hides money, load, Host, expiry. Goldens `status_ready`, `status_downloading`, `status_metrics_unavailable`, `status_down`, `status_local`. | tui/status.go:15-141 |
| T-05 | Palette accent #7D56F4, ok #3FB950, warn #D29922, err #F85149, dim #8B949E; label width 14; bar `█`/`░` width 24; load colour ≥0.95 err, ≥0.8 warn. Helpers `dur`, `num`, `gb`, `clock`. | tui/styles.go:12-89 |

### Wizard (`lobo config` form)

| ID | Behaviour | Go source |
|---|---|---|
| W-01 | Initial state: API key `keep` (or `new` when empty); saved `local` dropped when the Mac can't; fresh file on a capable Mac → `local`; else `runpod`; model `q8`; cloud `community`. | configtui.go:70-96 |
| W-02 | Screens in order and when shown: where (capable Mac only: intro note + provider select incl. local) → keys (intro note unless capable Mac; RunPod secret; Vast secret with provider-key check) → access (domain, API key select, tunnel secret, bucket) → pick (2 keys and not capable Mac) → defaults (min MB/s, model, ctx, idle, max hours) → RunPod cloud (RunPod key set) → Vast max price (Vast key set) → local (capable Mac: weights, port) → confirm with summary. All titles/descriptions verbatim. | configtui.go:247-334 |
| W-03 | Secret input: empty keeps, `-` clears, else trimmed value. | configtui.go:35-45 |
| W-04 | Validators and messages: whole number (`a whole number ≥ {min}, or empty for the default`), positive float (`a price in $/h, e.g. 1.20`), URL (`required` / `a URL like https://pub-….r2.dev`), hostname (`required` / `a bare hostname like lobo.example.com (no https://)`), local port (`a port 1024-65534, or empty for 8931`), provider keys (`set at least one provider key`), tunnel (`required: the pod serves the API through this tunnel`); cloud-only fields pass empty for local. | configtui.go:103-127,170-223 |
| W-05 | Result map (16 keys), provider only when local or both keys, cloud only with RunPod key, Vast price only with Vast key, built-in values and `0` → empty. | configtui.go:130-168 |
| W-06 | Summary rows (16, `%-20s %s`), secrets masked, new key text `(a new key is generated on save)`, empty → dim `default`. | configtui.go:336-371 |
| W-07 | `new_api_key` = `sk-` + 48 lowercase hex (len 51). | configtui.go:48-52 |

### JSON outputs

| ID | Output | Type | Go source |
|---|---|---|---|
| J-01 | `up --json` lines | `lobo_proto::UpEvent` | main.go:293-296 |
| J-02 | `status --json` | `lobo_proto::Snap` | main.go:352-357 |
| J-03 | `config show --json` | `lobo_proto::ConfigShow` (via `config::show`) | config.go:154-167 |
| J-04 | `models --json` | `lobo_proto::Listing` | local.go:172-174 |
| J-05 | `down --json` | `{"spent_usd": f64}` | main.go:327-329 |

### Release packaging

| ID | Behaviour | Source |
|---|---|---|
| B-01 | Archives `lobo_{darwin,linux}_{amd64,arm64}.tar.gz` with `lobo` inside, `checksums.txt`. | .goreleaser.yaml:24-27,58-59 |
| B-02 | Formula `lobo` in `1905/homebrew-tap` (root, `lobo.rb`) via SSH deploy key `HOMEBREW_TAP_KEY`, test `system "#{bin}/lobo", "version"`. | .goreleaser.yaml:29-56 |
| B-03 | Version, commit, date stamped into the binary. | .goreleaser.yaml:16-17 |
| B-04 | Workflow on `v*` tag push; dmg job kept as is (P5 owns it). | release.yml |

---

## Task 0 — Preconditions (orchestrator, no implementer)

- [ ] P3 is `done` on `feat/rust`; `cargo test --workspace` green.
- [ ] contracts.md is v1.1 with the P3 additions and this plan's additions folded in, and P3 shipped them (`lobo_core::control::testkit` behind feature `testkit`, `Wiring`, `control::{check_*, apply_defaults}`, `config::{mask, masked, LAYOUT, loose_mode}`, `genkey::{ensure_api_key, write_opencode, OPENCODE_OUT}`, `local::RunConfig::validate`). If not, stop: P4 cannot build on the v1.0 signatures (see "Spec issues" 3).
- [ ] `git status` clean on `feat/rust`; `git merge master` done (merge-hygiene rule); Go green: `go test ./cmd/lobo/ ./internal/tui/ ./internal/configtui/` → `ok` ×3.
- [ ] Record the parity baseline: `git log -1 --format=%h master` → write it into the "Decisions" item 10 line of this plan.
- [ ] Toolchain for later tasks (local, free): `brew install zig`, `cargo install cargo-zigbuild --locked`, `rustup target add x86_64-unknown-linux-musl aarch64-unknown-linux-musl x86_64-apple-darwin aarch64-apple-darwin`. `goreleaser --version` → 2.13.x (present).
- [ ] Commit: `plans: rust P4 plan`.

## Task 1 — Crate skeleton, pinned versions, build info

**Files:** Create `crates/lobo-cli/{Cargo.toml,build.rs,src/main.rs,src/lib.rs,src/build_info.rs}`. Modify workspace `Cargo.toml`.

Locked interface:
```rust
// build_info.rs
pub const VERSION: &str; // option_env!("LOBO_VERSION") or "dev"
pub const COMMIT: &str;  // option_env!("LOBO_COMMIT") or "none"
pub const DATE: &str;    // option_env!("LOBO_DATE") or "unknown"
```
- [ ] `cargo new --lib crates/lobo-cli --vcs none`; workspace member settings like P1.
- [ ] `cargo add -p lobo-cli clap --features derive` (no `env` feature: config never comes from the OS env), `clap_complete`, `ratatui`, `crossterm`, `inquire`, `anyhow`, `tokio --features rt-multi-thread,macros,signal,sync,time`, `tokio-util`, `tracing`, `tracing-subscriber --features fmt`, `chrono`, `serde --features derive`, `serde_json`, `unicode-width`, `futures`; path deps `lobo-proto`, `lobo-core`. Dev: `insta`, `assert_cmd`, `predicates`, `tempfile`, `serial_test`, `wiremock`. Move versions to `[workspace.dependencies]`.
- [ ] `build.rs`: three `cargo:rerun-if-env-changed` lines, nothing else.
- [ ] Failing test first (`build_info::tests::defaults_when_unset`): with no env at build, `VERSION == "dev"`, `COMMIT == "none"`, `DATE == "unknown"`.
- [ ] Verify: `cargo test -p lobo-cli build_info` → `test result: ok. 1 passed`. `LOBO_VERSION=9.9.9 cargo build -p lobo-cli && strings target/debug/lobo | grep -c 9.9.9` → ≥ 1.
- [ ] Fill "Pinned versions" from `Cargo.lock`.
- [ ] Commit: `lobo-cli: crate skeleton, pinned deps, build info`.

## Task 2 — Verify the parity inventory (orchestrator)

- [ ] `git diff 6a72092 master -- cmd/lobo internal/tui internal/configtui internal/config/envfile.go` → empty, or update the affected rows (text + file:line) in this plan before any implementer starts.
- [ ] Spot-check 10 random rows against the Go source. Any mismatch → fix the row.
- [ ] Commit (only if rows changed): `plans: P4 inventory synced to master`.

## Task 3 — Go capture: help texts

**Files:** Create `tools/clifixtures/capture.py`, `crates/lobo-cli/tests/fixtures/go/help/*.txt`. Modify `Makefile`.

Python 3 stdlib only (no venv, no pip). Interface: `python3 tools/clifixtures/capture.py help <outdir>`.
- Builds `go build -o <tmp>/lobo ./cmd/lobo` (version `dev`).
- Env for every run: `HOME=<tmp>/home`, `XDG_CONFIG_HOME=/home/u/.config`, `NO_COLOR=1`, `HTTPS_PROXY=HTTP_PROXY=http://127.0.0.1:9` (any network call fails fast), `PATH` inherited.
- Files: `root.txt` (`lobo`), `root_help.txt` (`lobo help`), and `<path>.txt` for `--help` of: `up status logs test down config config_show config_set config_get config_path models gen-api-key version release local local_run completion`.
- [ ] Makefile: `cli-fixtures: python3 tools/clifixtures/capture.py help crates/lobo-cli/tests/fixtures/go/help` (Task 4/5 extend it).
- [ ] Verify: `make cli-fixtures && make cli-fixtures && git status --porcelain crates/lobo-cli/tests/fixtures` → only new files, second run no diff. `diff crates/lobo-cli/tests/fixtures/go/help/root.txt cmd/lobo/testdata/help_noconfig.golden` → no output.
- [ ] Commit: `lobo-cli: Go --help fixtures`.

## Task 4 — Go capture: replay cases

**Files:** Modify `tools/clifixtures/capture.py`, `Makefile`. Create `crates/lobo-cli/tests/fixtures/go/cases.json`.

`python3 tools/clifixtures/capture.py cases <out.json>`. Each case: `{name, args, stdin?, config? (file body or null), config_mode? ("600"|"644"), weights_dir? (bool), cwd_tmp: bool, exit, stdout, stderr, stderr_match ("exact"|"prefix"|"log"), files_after: {relpath: body}}`. Normalization (applied to Go now and to Rust in Task 44): temp root → `$TMP`; `sk-[0-9a-f]{48}` → `sk-KEY`; `"free_bytes":\d+` → `"free_bytes":0`; `\(\d+\.\d GB free\)` → `(N GB free)`; log timestamps `^\d\d:\d\d:\d\d ` → empty.

Hard rule in the script: every case must be platform-independent and must fail or finish before any network or child process. Cases with `up|down|status|logs|test|release` must exit non-zero; a zero exit aborts the capture with an error.

Cases (≈40): `version`; `nosuch`; `up --bogus`; `config path`; `config show` missing; `config show --json` missing; `config show` + file (secrets, plain keys, one unknown `FOO_TOKEN`); `config show --json` + same file; `config get LOBO_DOMAIN`; `config get NOPE`; `config get` (no arg); `config set` (no args); `config set --stdin X=1`; `config set lower=1`; `config set LOBO_MIN_MBPS=150 CF_TUNNEL_TOKEN=a=b LOBO_CTX=` on a file (files_after = config body); `config set --stdin` `{"LOBO_CTX":"8192"}` (files_after); `config set --stdin` with `{}`, `[1]`, `{"A":1}`, `nope`, `{"lower":"1"}`; `config` (not a TTY) + file; `up`/`down`/`status`/`logs`/`test`/`models`/`release` with no config; `up --cloud bad`; `up --provider aws`; `up --provider vast` (RunPod key only); `up` with `LOBO_CTX=100`; `up --ctx 100000 --provider runpod` with `LOBO_CTX=100` and key-only config (stops at check_target: `CF_TUNNEL_TOKEN`); `release` without R2 (`R2_`); `release` with R2, no bucket (`LOBO_BUCKET_URL`); `models --json` and `models` with a weights dir holding half of the first catalog file (sparse); `local run --model q2 --ctx 1 --idle-min 1`; `local run --idle-min 1`; `local run --ctx 1`; `local run --ctx 1 --idle-min 1 --port 9000 --api-port 9000`; `local run --ctx 1 --idle-min 1 x`; `gen-api-key` in a temp cwd (files_after: config, `opencode.lobo.json`); `gen-api-key` twice (keeps key); `gen-api-key` with no file; `models` with a 0644 config (WRN line, `stderr_match: "log"`).

- [ ] Extend `cli-fixtures` to also write `cases.json`.
- [ ] Verify: `make cli-fixtures` twice → no diff. `python3 -c "import json;print(len(json.load(open('crates/lobo-cli/tests/fixtures/go/cases.json'))))"` → ≥ 40.
- [ ] Commit: `lobo-cli: Go CLI replay cases`.

## Task 5 — Go capture: text and JSON streams

**Files:** Create `cmd/lobo/capture_test.go` (`//go:build capture`), `crates/lobo-cli/tests/fixtures/go/text/*`, `crates/lobo-cli/tests/goldens/*.golden`. Modify `Makefile`.

`go test -tags capture -run TestCapture ./cmd/lobo/ -args -out <dir>`. It swaps `log` for a `ConsoleWriter{NoColor: true, TimeFormat: "15:04:05"}` over a buffer with `zerolog.TimestampFunc` fixed to `2026-09-25T10:00:00Z`, `TZ=UTC`, and writes:

| File | Content |
|---|---|
| `plain_up_boot.txt` | `plainUp` over `controltest.Events(BootScript(), SECURE no-cap)` |
| `plain_up_failed.txt` | `plainUp` over the tui_test.go:78 failed script |
| `json_up_boot.jsonl`, `json_up_failed.jsonl` | `jsonUp` over `control.Up(ct.Deps(…), UpOpts{Provider:"runpod", Cloud:"community"})`, same scripts (these opts = apply_defaults of a RunPod-only config, no flags) |
| `report_boot.txt`, `boots_line.json` | `reportBoot` stderr and the appended line, on the ready event of the boot run, `at` fixed |
| `status_running.json`, `status_down.json` | `json.Encode(control.Snapshot(ct.Deps(…)))`: one RUNNING `pod1` + ready script; no pods |
| `down_running.json` | `{"spent_usd": …}` from `control.Down` on the running deps |

- [ ] Copy (byte copies, `cp`) `cmd/lobo/testdata/*.golden` and `internal/tui/testdata/*.golden` into `crates/lobo-cli/tests/goldens/`.
- [ ] Extend `cli-fixtures` with the `go test -tags capture` line.
- [ ] Verify: `make cli-fixtures` twice → no diff; `go vet -tags capture ./cmd/lobo/` clean; `go test ./cmd/lobo/` (no tag) still `ok`.
- [ ] Commit: `lobo-cli: Go-captured up/status/down streams and goldens`.

## Task 6 — `run`, `App`, error + exit plumbing

**Files:** `src/lib.rs`, `src/app.rs`, `src/main.rs`. Rows: G-04, G-05, G-06.

Locked interface:
```rust
pub struct Term { pub stdout_tty: bool, pub stdin_tty: bool, pub stderr_tty: bool, pub no_color: bool }
pub struct Io { pub out: Box<dyn Write + Send>, pub err: Box<dyn Write + Send>, pub input: Box<dyn Read + Send> }
pub struct App {
    pub deps: Arc<dyn Fn(Laptop, &Wiring) -> lobo_core::Result<Deps> + Send + Sync>,   // real = control::deps_from_config
    pub local_supported: fn() -> lobo_core::Result<()>,                                   // real = local::supported; goes into Wiring.supported
    pub supervise: Arc<dyn Fn(RunConfig, CancellationToken) -> BoxFuture<'static, lobo_core::Result<()>> + Send + Sync>,
    pub prompter: Arc<dyn Fn() -> Box<dyn wizard::Prompter> + Send + Sync>,   // Task 25/26; real = InquirePrompter
    pub clock: Arc<dyn lobo_core::clock::Clock>,
    pub term: Term,
    pub exe: PathBuf,
}
impl App { pub fn real() -> App; }
pub async fn run(app: &App, argv: Vec<OsString>, io: &mut Io) -> i32;   // 0 ok, 1 any error
```
Error printing: `writeln!(io.err, "error: {e:#}")`. clap parse errors (except help display) → `error: ` + clap's first message line, exit 1 (not clap's 2).
- [ ] Failing tests (`app::tests`): `run(["lobo","nosuch"])` → 1, stderr starts `error: `; `run(["lobo","up","--bogus"])` → 1; a command returning `anyhow!("x: y")` → stderr exactly `error: x: y\n`.
- [ ] Implement with a stub clap tree (only `version`).
- [ ] Verify: `cargo test -p lobo-cli app::tests` → ok.
- [ ] Commit: `lobo-cli: run(), App seams, error and exit code mapping`.

## Task 7 — clap tree: root, globals, bool flags

**Files:** `src/cli.rs`. Rows: G-02, G-03, G-10.

Locked interface:
```rust
#[derive(Parser)] #[command(name = "lobo", bin_name = "lobo", disable_version_flag = true)]
pub struct Cli { #[arg(long = "config", alias = "env", global = true)] pub config: Option<PathBuf>, #[command(subcommand)] pub cmd: Option<Cmd> }
pub enum Cmd { Version, Config(ConfigArgs), GenApiKey(GenKeyArgs), Release, Up(UpArgs), Down(JsonArg), Status(StatusArgs),
               Logs(LogsArgs), Test, Models(JsonArg), Local(LocalArgs), Completion(CompletionArgs) }
pub fn command() -> clap::Command;                      // Cli::command() + help templates (Task 10)
pub fn changed(m: &clap::ArgMatches) -> BTreeSet<String>; // arg ids with ValueSource::CommandLine
pub fn parse_bool_flag(s: &str) -> Result<bool, String>;  // "true|false|1|0|t|f|TRUE|FALSE|True|False" (Go strconv.ParseBool)
```
Every bool flag: `num_args = 0..=1`, `require_equals = true`, `default_missing_value = "true"`, `value_parser = parse_bool_flag`. `--config` default is resolved in `App` (so help shows the real default like cobra does: `default_value_os_t = config::default_path()` on the arg).
- [ ] Failing tests: `--q6`, `--q6=true`, `--q6=false` all parse; `changed` contains `q6` for all three and not for none; `--env /x` sets `config`; `parse_bool_flag` table incl. `yes` → error.
- [ ] Verify: `cargo test -p lobo-cli cli::tests` → ok.
- [ ] Commit: `lobo-cli: clap root, --config/--env, pflag-style bools`.

## Task 8 — clap tree: every command and flag

**Files:** `src/cli.rs`. Rows: U-01, D-01, S-01, L-01, E-01, M-01, C-01, C-05, C-07, C-08, C-09, K-01, R-01, LR-01, V-01, G-08, G-11.

- [ ] Args structs with fields declared in cobra's alphabetical flag order, help strings copied verbatim from the inventory rows, defaults as in Go (`cloud = "community"`, `n = 200`, `model = "q6"`, `port = 8931`, `api_port = 8932`). `--max-life` uses `duration::parse_go_duration` (Task 11). `logs`: `#[arg(short = 'n', long = "n")]`. Hidden: `release`, `local`, `completion`. `config get`: exactly one `KEY`. Leaf commands take no positional args (G-11) except `config set` (`Vec<String>`) and `config get`.
- [ ] Failing tests: parse each `cases.json` args vector that exits 0 in Go without error-from-parser; `lobo up extra` → parse error (G-11); `lobo logs -n 5` and `--n 5` → 5; `lobo config get` → error; `lobo local run --ctx 1 --idle-min 1 x` → error.
- [ ] Verify: `cargo test -p lobo-cli cli::tests` → ok.
- [ ] Commit: `lobo-cli: full clap command tree`.

## Task 9 — Help facts parity test

**Files:** Create `tests/help_parity.rs`. Rows: G-12, Decision 1.

Locked interface (test-local):
```rust
struct HelpFacts { path: String, about: String, flags: Vec<FlagFact>, global: Vec<FlagFact>, subcommands: Vec<(String, String)> }
struct FlagFact { long: String, short: Option<char>, kind: String /* "", "string", "int", "duration" */, default: Option<String>, help: String }
fn parse_cobra(text: &str) -> HelpFacts;              // from fixtures/go/help/*.txt
fn clap_facts(cmd: &clap::Command, path: &[&str]) -> HelpFacts; // introspection, `help` flag dropped on both sides
```
- [ ] Failing test `help_facts_match_go`: for every file in `fixtures/go/help/` except `root*.txt`, `parse_cobra(file) == clap_facts(lobo_cli::cli::command(), path)`. The `completion` subcommand list is compared by names only.
- [ ] Implement `kind` on the clap side from a `value_name` set per arg (`string`/`int`/`duration`, none for bools). Defaults: cobra omits zero defaults; clap facts do the same (`0`, `""`, `false` → `None`).
- [ ] Verify: `cargo test -p lobo-cli --test help_parity` → ok.
- [ ] Commit: `lobo-cli: help parity against the Go --help fixtures`.

## Task 10 — Subcommand help layout + insta snapshots

**Files:** `src/cli.rs`, `tests/help_parity.rs`, `tests/snapshots/`. Rows: G-12.

- [ ] `help_template` close to cobra: `{about-with-newline}\nUsage:\n  {usage}\n\n{subcommands? "Available Commands:"}\n{options "Flags:"}\n` and `--config` under heading `Global Flags`. `lobo help <cmd>` works (clap `help` subcommand stays for non-root).
- [ ] Failing test `help_snapshots`: `insta::assert_snapshot!(name, help_text(path))` for every path in Task 3's list, `XDG_CONFIG_HOME=/home/u/.config` in a subprocess via `assert_cmd` (`lobo <path> --help`).
- [ ] `cargo insta test -p lobo-cli --test help_parity --review`; orchestrator reads each snapshot next to the Go fixture once, then accepts.
- [ ] Verify: `cargo test -p lobo-cli --test help_parity` → ok.
- [ ] Commit: `lobo-cli: cobra-like help template, help snapshots`.

## Task 11 — Go duration parser

**Files:** `src/duration.rs`. Rows: U-01 (`--max-life`).

```rust
pub fn parse_go_duration(s: &str) -> Result<std::time::Duration, String>; // Go time.ParseDuration subset: ns us µs ms s m h, decimals, sequences, "0"; negative → error
```
- [ ] Failing table test: `12h`→43200 s, `1h30m`→5400 s, `1.5h`→5400 s, `90m`, `300ms`, `0`→0, `2h45m30.5s`; errors: `""`, `12`, `1d`, `-1h`, `h`.
- [ ] Verify: `cargo test -p lobo-cli duration` → ok.
- [ ] Commit: `lobo-cli: Go duration syntax for --max-life`.

## Task 12 — zerolog-console log format

**Files:** `src/logfmt.rs`. Rows: G-13, Decision 6.

```rust
pub struct ZerologConsole { pub color: bool, pub tz: chrono::FixedOffset }   // impl FormatEvent
pub fn layer<W: for<'a> MakeWriter<'a> + 'static>(w: W, color: bool) -> impl Layer<Registry>;
```
Line: `HH:MM:SS LVL message k=v k=v\n`; `INF`/`WRN`/`ERR`/`DBG`; fields sorted by key; a string value is quoted with Go `strconv.Quote` rules when any byte is `< 0x20`, `> 0x7e`, space, `\` or `"`; numbers and bools bare. Error events carry the message as-is.
- [ ] Failing tests: format of `info!(phase = "download", detail = "a b", "up")` → `10:00:00 INF up detail="a b" phase=download`; `→` in a value → quoted; float `0.69` bare; compare the `plain_up_boot.txt` first line after timestamp strip (fields driven by a hand-built event).
- [ ] Verify: `cargo test -p lobo-cli logfmt` → ok.
- [ ] Commit: `lobo-cli: zerolog-console log lines via tracing`.

## Task 13 — Root help (byte-exact)

**Files:** `src/help.rs`, `src/snapshots/`. Rows: G-01. Go test: TestRootHelpGolden.

```rust
pub fn root_help(w: &mut dyn Write, color: bool, have_config: bool, cfg_path: &Path) -> io::Result<()>;
```
Groups `Run: up status logs test down`, `Setup: config models gen-api-key version`; command shorts come from `cli::command()` (never hand-copied); `%-12s` / `%-29s` padding; colours per help.go:40-47.
- [ ] Failing test `root_help_golden`: for `(true, "help_config")`, `(false, "help_noconfig")` with `cfg_path=/home/u/.config/lobo/config.env`, color off: bytes == `tests/goldens/<name>.golden` (with `VERSION = "dev"`), plus `insta::assert_snapshot!`. Output contains `models` and `lobo up --provider local`; never `release` or `\x1b[`.
- [ ] Wire G-01 in `run`: no subcommand, `help` with no arg, `-h`, `--help` at root → `root_help(color = term.stdout_tty && !term.no_color, have_config = path exists)`.
- [ ] Verify: `cargo test -p lobo-cli help` → ok; `cargo run -q -p lobo-cli -- --config /home/u/.config/lobo/config.env | diff - crates/lobo-cli/tests/goldens/help_noconfig.golden` → no output.
- [ ] Commit: `lobo-cli: custom root help, byte-identical to Go`.

## Task 14 — Config loading

**Files:** `src/app.rs`. Rows: G-09.

```rust
pub fn load_cfg(path: &Path) -> anyhow::Result<Laptop>;   // missing → exact G-09 text; loose mode → tracing::warn!
```
- [ ] Failing tests: missing file message exact; 0644 file → one WRN line with the exact text; 0600 → none; returns `config::load_laptop` result.
- [ ] Verify: `cargo test -p lobo-cli app::tests::load_cfg` → ok.
- [ ] Commit: `lobo-cli: load_cfg with the loose-mode warning`.

## Task 15 — Gates at the CLI boundary

**Files:** `src/cmd/{up,down,status,release}.rs` (call sites only). Rows: U-04, D-02, R-01. Logic + Go tables: P3 `control::precheck` (TestCheckTarget, TestCheckProviders, TestCheckRelease).

Call sites: `up` → `control::check_target(&cfg, &o.provider, app.local_supported)`; `down`, `status` → `control::check_providers(&cfg, app.local_supported)`; `release` → `control::check_release(&cfg)`. `logs`, `test` call none (Go does not).
- [ ] Failing tests (in-process, `App.local_supported` = a fn returning an error, `App.deps` = a closure that panics if called): `down` with a key-only config → `error: …RUNPOD_API_KEY or VASTAI_API_KEY…`, exit 1, deps never built; `status` same; `release` without R2 → `R2_`; `up --provider runpod` with a key-only config → `CF_TUNNEL_TOKEN`; `logs` with a key-only config reaches `deps` (closure called).
- [ ] Verify: `cargo test -p lobo-cli gates` → ok.
- [ ] Commit: `lobo-cli: pre-checks wired per command`.

## Task 16 — Flags → `apply_defaults`

**Files:** `src/cmd/up.rs`. Rows: U-02, U-03, G-10. Logic + TestApplyDefaults table: P3 `control::precheck::apply_defaults`.

```rust
pub fn up_opts(a: &UpArgs) -> UpOpts;                                      // flag values only (cobra defaults: cloud "community")
pub fn set_fn(changed: &BTreeSet<String>) -> impl Fn(&str) -> bool + '_;    // Go flag names: provider, q6, cloud, ctx, idle-min, max-life, min-mbps
```
- [ ] Failing tests: `changed_names_match_go` — for each of `--provider x`, `--q6`, `--q6=false`, `--cloud secure`, `--ctx 1`, `--idle-min 1`, `--max-life 1h`, `--min-mbps 1`, `set_fn` returns true for exactly the Go flag name; `q6_false_keeps_config_model_out` — config `LOBO_MODEL=q6`, argv `up --q6=false` → after `apply_defaults` model is `""` (release default), the Swift app's call shape; `q6_true_sets_model`; `flag_beats_bad_config_ctx` — config `LOBO_CTX=100`, `--ctx 8192` → no error (one CLI-level repeat of a P3 row, proves `set_fn` is wired).
- [ ] Verify: `cargo test -p lobo-cli cmd::up::tests::defaults` → ok.
- [ ] Commit: `lobo-cli: up flags feed apply_defaults like pflag Changed`.

## Task 17 — Deps wiring through `App`

**Files:** `src/app.rs`. Rows: U-02. Logic + TestProviders/TestLocalInstanceURLs: P3 `control::wiring`.

```rust
pub fn wiring(app: &App, cfg_path: &Path) -> Wiring;   // Wiring { config_path: abs(cfg_path) (as given if abs fails), spawner: Spawner::cli(app.exe.clone()), supported: app.local_supported }
```
`App::real().deps` = `|cfg, w| lobo_core::control::deps_from_config(cfg, w)`. `up` sets `deps.presign = None` for provider `local` (main.go:228).
- [ ] Failing tests: `wiring_abs_config_path` (`rel/config.env` → `cwd/rel/config.env`); `wiring_spawner_is_cli` (prefix `["local","run"]`, exe = `app.exe`); `wiring_passes_supported_seam` (fn pointer equality).
- [ ] Verify: `cargo test -p lobo-cli app::tests::wiring` → ok.
- [ ] Commit: `lobo-cli: Wiring from the CLI (config path, spawner, local seam)`.

## Task 18 — `version`

**Files:** `src/cmd/version.rs`. Rows: V-01.
- [ ] Failing test: `run(["lobo","version"])` → stdout `lobo dev (none, unknown)\n`, exit 0.
- [ ] Verify: `cargo test -p lobo-cli cmd::version` → ok.
- [ ] Commit: `lobo-cli: version`.

## Task 19 — `config path` and `config get`

**Files:** `src/cmd/config.rs`. Rows: C-08, C-09.
- [ ] Failing tests (in-process, temp config): path prints the `--config` value as given (relative stays relative); get prints value; missing key → `error: NOPE is not set in {path}`, exit 1; missing file → `NOPE is not set` too (empty map).
- [ ] Verify: `cargo test -p lobo-cli cmd::config::tests::get_path` → ok.
- [ ] Commit: `lobo-cli: config path/get`.

## Task 20 — `config set`

**Files:** `src/cmd/config.rs`. Rows: C-07. Go tests: TestParseSetArgs, TestParseSetJSON.

```rust
pub fn parse_set_args(args: &[String]) -> anyhow::Result<BTreeMap<String, String>>;
pub fn parse_set_json(r: impl Read) -> anyhow::Result<BTreeMap<String, String>>; // 1 MiB cap, first JSON value only
```
- [ ] Failing tests: port both Go tests (same good and bad inputs); the both/none argument errors exact; `set` writes via `config::save` (temp file content checked for one key).
- [ ] Verify: `cargo test -p lobo-cli cmd::config::tests::parse_set` → ok.
- [ ] Commit: `lobo-cli: config set (args and --stdin)`.

## Task 21 — `config show` text and `--json`

**Files:** `src/cmd/config.rs`, `tests/cli_config.rs`. Rows: C-04, C-05, C-06, J-03. TestMaskedUnknownKeys, TestShowConfigJSONMasks: ported in P3 (`config::show::tests`); the two tests below repeat them at the binary boundary.

```rust
pub fn show_config_text(w: &mut dyn Write, cfg_path: &Path, cur: &BTreeMap<String, String>) -> io::Result<()>;
```
Text masks with `config::masked`; group titles from `config::LAYOUT`. JSON = `config::show(path)` + `serde_json::to_writer` + `\n`.
- [ ] Failing tests: `show_text_masks` (the TestMaskedUnknownKeys keys in a file → text shows clear/masked as the Go table says); `show_text_layout` (groups, `# other` sorted, empty file line); `tests/cli_config.rs::show_json_masks` (assert_cmd, file with `RUNPOD_API_KEY=rpa_SECRETSECRETSECRET`, `LOBO_DOMAIN=lobo.x.cc`, `LOBO_CTX=`): no `SECRETSECRET`, `"LOBO_DOMAIN":"lobo.x.cc"`, `"LOBO_CTX":false`, `"exists":true`; missing file → `"exists":false`, `"values":{}`.
- [ ] Verify: `cargo test -p lobo-cli config` → ok.
- [ ] Commit: `lobo-cli: config show text and --json`.

## Task 22 — Wizard state and result

**Files:** `src/wizard/state.rs`. Rows: W-01, W-03, W-05. Go tests: TestResultKeepClearAndDefaults, TestNewStateFreshFile, TestNewStateLocal, TestResultLocal.

```rust
pub struct WizardState { pub cur: BTreeMap<String,String>, pub runpod: String, pub vast: String, pub tunnel: String, pub api_key: String /* keep|new */,
    pub domain: String, pub bucket: String, pub provider: String, pub model: String, pub cloud: String, pub min_mbps: String, pub ctx: String,
    pub idle: String, pub max_h: String, pub vast_dph: String, pub weights: String, pub port: String, pub local_ok: bool, pub save: bool }
impl WizardState { pub fn new(cur: BTreeMap<String,String>, local_ok: bool) -> Self; pub fn result(&self, new_key: &str) -> BTreeMap<String,String>;
    pub fn runpod_key(&self) -> String; pub fn vast_key(&self) -> String; pub fn both_keys(&self) -> bool; pub fn is_local(&self) -> bool; }
pub fn resolve_secret(old: &str, typed: &str) -> String;
pub fn new_api_key() -> String;   // = lobo_core::genkey::new_api_key()
```
- [ ] Failing tests: the four Go tests case-for-case (same names in snake_case).
- [ ] Verify: `cargo test -p lobo-cli wizard::state` → `4 passed`.
- [ ] Commit: `lobo-cli: wizard state and saved result`.

## Task 23 — Wizard validators and options

**Files:** `src/wizard/validate.rs`. Rows: W-04, C-06. Go tests: TestValidators, TestLocalValidation, TestProviderOptions, TestMask.

```rust
pub type Check = Box<dyn Fn(&str) -> Result<(), String> + Send + Sync>;
pub fn whole_number(min: i64) -> Check; pub fn positive_float(v: &str) -> Result<(), String>;
pub fn https_url(v: &str) -> Result<(), String>; pub fn hostname(v: &str) -> Result<(), String>; pub fn local_port(v: &str) -> Result<(), String>;
impl WizardState { pub fn provider_keys(&self, typed: &str) -> Result<(), String>; pub fn tunnel_token(&self, typed: &str) -> Result<(), String>;
                   pub fn cloud_only<'a>(&'a self, check: &'a dyn Fn(&str) -> Result<(), String>) -> impl Fn(&str) -> Result<(), String> + 'a; }
pub fn provider_options(local_ok: bool) -> Vec<(&'static str, &'static str)>;   // (label, value)
pub const SECRETS: [&str; 4];
```
- [ ] Failing tests: the four Go tests; `mask` test calls `lobo_core::config::mask` (P3 addition 7) with the Go table.
- [ ] Verify: `cargo test -p lobo-cli wizard::validate` → `4 passed`.
- [ ] Commit: `lobo-cli: wizard validators and provider options`.

## Task 24 — Wizard summary

**Files:** `src/wizard/state.rs`. Rows: W-06. Go tests: TestSummaryMasksSecrets, TestSummaryLocalRows.

```rust
impl WizardState { pub fn summary(&self, color: bool) -> String; }   // 16 rows "%-20s %s", no trailing newline
```
- [ ] Failing tests: both Go tests; plus `summary_rows_order` (16 labels in Go order, color off).
- [ ] Verify: `cargo test -p lobo-cli wizard::state::tests::summary` → ok.
- [ ] Commit: `lobo-cli: wizard summary`.

## Task 25 — Wizard flow over a `Prompter`

**Files:** `src/wizard/flow.rs`, `src/wizard/prompt.rs` (trait only). Rows: W-02. Go test: TestFormBuilds.

```rust
pub struct Abort;
pub trait Prompter {
    fn note(&mut self, title: &str, body: &str) -> Result<(), Abort>;
    fn secret(&mut self, title: &str, desc: &str, check: &dyn Fn(&str) -> Result<(), String>) -> Result<String, Abort>;
    fn text(&mut self, title: &str, desc: &str, initial: &str, check: &dyn Fn(&str) -> Result<(), String>) -> Result<String, Abort>;
    fn select(&mut self, title: &str, desc: &str, options: &[(&str, &str)], current: &str) -> Result<String, Abort>;
    fn confirm(&mut self, title: &str, desc: &str, yes: &str, no: &str, default: bool) -> Result<bool, Abort>;
}
pub fn run_wizard(p: &mut dyn Prompter, path: &Path, cur: BTreeMap<String,String>, local_ok: bool) -> Option<BTreeMap<String,String>>; // None = quit or Discard
```
Group titles (`1/4 · GPU providers` …) are passed as a prefix of each prompt title: `"{group} · {field}"`. Intro note last line: `Enter: next · Esc: quit without saving`.
- [ ] Failing tests with a `Scripted` prompter (records every title, answers from a queue, asserts the queue ends empty): `form_builds` (both `local_ok` values, answers all defaults, `Some(result)`); `flow_order_cloud` (fresh file, not capable: exact title list); `flow_order_local` (capable Mac: where group first, no pick group); `flow_pick_shown_with_two_keys`; `abort_mid_way_saves_nothing`; `discard_saves_nothing`; a validator rejection is re-asked (Scripted returns a bad then a good value; asserts the check was called with both).
- [ ] Verify: `cargo test -p lobo-cli wizard::flow` → ok.
- [ ] Commit: `lobo-cli: wizard flow, same screens and order as the huh form`.

## Task 26 — inquire prompter + `config` command

**Files:** `src/wizard/prompt.rs`, `src/cmd/config.rs`. Rows: C-01, C-02, C-03.

`InquirePrompter` maps: `note` → print styled title + body; `secret` → `Password::new(title).with_help_message(desc).without_confirmation().with_display_mode(Masked).with_validator(..)`; `text` → `Text` with `initial_value`; `select` → `Select` with `starting_cursor` at `current`; `confirm` → `Select` over `[yes, no]` (inquire `Confirm` has no custom labels). `InquireError::OperationCanceled | OperationInterrupted` → `Abort`. Render config accent #7D56F4.
- [ ] Failing tests (in-process, non-TTY `Term`): `config` prints the not-a-terminal line on stderr and the show text on stdout, exit 0; with a scripted prompter in `App.prompter` → `saved {path}`, API key note when the key changed from non-empty, `still missing …` when `load_laptop` fails; Discard → `nothing saved`.
- [ ] Orchestrator (manual, free): `cargo run -p lobo-cli -- --config /tmp/lobo-wiz/config.env config` in a real terminal; walk through once; `cat` the file; move `/tmp/lobo-wiz` to `/tmp/trash`.
- [ ] Verify: `cargo test -p lobo-cli cmd::config` → ok.
- [ ] Commit: `lobo-cli: lobo config wizard on inquire`.

## Task 27 — `gen-api-key`

**Files:** `src/cmd/genkey.rs`. Rows: K-01. TestWriteOpencode: P3 `genkey::tests::write_opencode_table`.

Flow: `genkey::ensure_api_key(path, rotate)` → `(key, written)` → INF line per K-01 → `config::values(path)` for `LOBO_DOMAIN` and the port (`Laptop { local_port, .. }.port()`) → empty domain INF line → `genkey::write_opencode(Path::new(genkey::OPENCODE_OUT), domain, &key, port)` → stdout `wrote {abs}`.
- [ ] Failing tests (in-process, temp cwd): first run writes a key (`sk-` + 48 hex) and logs the "new" line; second run keeps it and logs "keeping"; `--rotate` changes it; missing file → `error: read {path}: …`; empty domain → the local-only INF line; stdout `wrote {abs}`; file mode 0600.
- [ ] Verify: `cargo test -p lobo-cli cmd::genkey` → ok.
- [ ] Commit: `lobo-cli: gen-api-key`.

## Task 28 — `models`

**Files:** `src/cmd/models.rs`. Rows: M-01, J-04. Go test: TestModelsOutput.

```rust
pub fn write_models(w: &mut dyn Write, weights: &Path, json: bool) -> anyhow::Result<()>;
```
- [ ] Failing test `models_output`: port the Go test (sparse half file for `all()[0]` via `File::set_len`; JSON has the 4 keys; `on_disk` values; runtime absent; plain has `partial 50%`, `missing`, `weights {w}`, `GB free`). Plus `models_row_format`: exact row bytes for on-disk verified, not verified, oversize.
- [ ] Verify: `cargo test -p lobo-cli cmd::models` → ok.
- [ ] Commit: `lobo-cli: models listing`.

## Task 29 — `local run`

**Files:** `src/cmd/local.rs`. Rows: LR-01, LR-02, LR-03. Go test: TestLocalRunFlags.

```rust
pub struct LocalRunArgs { pub model: String, pub ctx: i64, pub idle_min: i64, pub boot_id: String, pub port: i64, pub api_port: i64 } // clap, i64 like pflag
impl LocalRunArgs { pub fn to_run_config(&self, config_path: &Path) -> anyhow::Result<RunConfig>; }
```
`to_run_config`: ports outside `1..=65535` → Go's `--port: want 1-65535, got {n}` / `--api-port: want 1-65535 and not --port, got {n}` (clap parses i64 so Go's text survives; `u16` would turn it into a parser error); then `RunConfig { …, config_path: Some(path), version: Manifest { version: VERSION, git_sha: COMMIT, ..Default::default() } }` (Go serves `{"version","git_sha"}`, local.go:128) and `RunConfig::validate()` (P3) for the other rules. `supervise` loads the config itself and owns logging (P3 Task 46); the CLI installs no tracing subscriber for `local run`. The command maps `Ok(())` to exit 0.
- [ ] Failing test `local_run_flags`: the 7 Go cases through `run()` with `App.supervise` swapped for a recorder (error cases never call it; success cases record the exact `RunConfig`); `--port 70000` → the Go message; `local` is hidden in `cli::command()` (the one assert P3 left to P4).
- [ ] Verify: `cargo test -p lobo-cli cmd::local` → ok.
- [ ] Commit: `lobo-cli: hidden local run → lobo_core::local::supervise`.

## Task 30 — TUI styles and helpers

**Files:** `src/tui/styles.rs`. Rows: T-05.

```rust
pub fn bar(frac: f64, width: usize, st: Style) -> Vec<Span<'static>>;
pub fn load_style(frac: f64) -> Style;
pub fn dur(d: chrono::Duration) -> String;   pub fn clock(d: chrono::Duration) -> String;
pub fn num(n: i64) -> String;                pub fn gb(b: i64) -> String;
pub fn row(k: &str, v: Vec<Span<'static>>) -> Line<'static>;   // label width 14, dim
pub fn pad(spans: Vec<Span<'static>>, w: usize) -> Vec<Span<'static>>;   // lipglossPad: pad to w, else one space
pub fn boxed(lines: Vec<Line<'static>>, border: Color) -> Vec<Line<'static>>; // ╭─╮ │ │ ╰─╯, padding 0 1, width = widest line
pub fn to_plain(t: &Text) -> String;   pub fn to_ansi(t: &Text) -> String;   // lines joined by "\n", final "\n"
```
Widths via `unicode-width`. `dur`/`clock` round to the second like Go (`Round`: half away from zero).
- [ ] Failing tests: `dur` (0s, 59s, 1m00s, 5m12s, 1h30m, 11h56m), `clock` (0:05, 6:00), `num` (0, 999, 1,000, 182,340), `gb`, `bar(0.432, 24)` → 10 full + 14 empty, `boxed` of two lines → exact 4-line string, `to_plain` of a styled line drops styles.
- [ ] Verify: `cargo test -p lobo-cli tui::styles` → ok.
- [ ] Commit: `lobo-cli: TUI styles and helpers`.

## Task 31 — `UpState::apply_at`

**Files:** `src/tui/up.rs`. Rows: T-01, T-02. Go test: TestUpStateErr.

```rust
pub struct UpState { pub phase: String, pub details: BTreeMap<String,String>, pub event: UpEvent, pub err: Option<String>, pub done: bool,
                     pub at: DateTime<Utc>, started: BTreeMap<String, DateTime<Utc>>, took: BTreeMap<String, chrono::Duration> }
impl UpState { pub fn apply_at(&mut self, e: &UpEvent, at: DateTime<Utc>); pub fn took(&self, phase: &str) -> Option<chrono::Duration>; }
pub const PHASES: [(&str, &str); 8];
```
`UpState::default().at` = `DateTime::<Utc>::MIN_UTC`.
- [ ] Failing tests: `up_state_err` (create, then failed with err + done → phase `create`, err set, done); `re_rent_resets_later_timers` (the second half of TestUpBootContainerHintAndReRentReset).
- [ ] Verify: `cargo test -p lobo-cli tui::up::tests::up_state` → ok.
- [ ] Commit: `lobo-cli: up state folding`.

## Task 32 — `render_up` + goldens

**Files:** `src/tui/up.rs`, `src/snapshots/`. Rows: T-03. Go tests: TestUpGolden, TestUpBootContainerHintAndReRentReset.

```rust
pub fn render_up(s: &UpState, spin: &str) -> Text<'static>;
```
Test helper `up_until(script, phase) -> UpState` = Go `upUntil`: events from `lobo_core::control::testkit::events(script, &["SECURE"]).await`, t0 `2026-09-25T10:00:00Z`, 7 s apart, render clock 5 s after the last.
Golden helper `golden(name, got)`: `assert_eq!(got, include_str!("../../tests/goldens/{name}.golden"))` (byte-exact, the parity contract) and `insta::assert_snapshot!(name, got)` (review workflow; insta may normalise whitespace, so the byte check is the gate).
- [ ] Failing tests: `up_golden` (`up_download`, `up_ready`, `up_failed` with spin `*`); `boot_container_hint` (create at t0, image at t0+2 s, at = t0+20 s → contains `0:18`, `re-rent at 6:00`, `2s`).
- [ ] Verify: `cargo test -p lobo-cli tui::up` → ok.
- [ ] Commit: `lobo-cli: up progress rendering, Go goldens pass`.

## Task 33 — `render_status` + goldens

**Files:** `src/tui/status.rs`. Rows: T-04. Go test: TestStatusGolden.

```rust
pub fn render_status<Tz: TimeZone>(s: &Snap, tz: &Tz) -> Text<'static> where Tz::Offset: std::fmt::Display;
```
Zero `GoTime` renders as `00:00:00`. Tests pass `&Utc` (Go test sets `time.Local = UTC`); production passes `&chrono::Local`.
- [ ] Failing test `status_golden`: the 5 Go cases built exactly like tui_test.go:82-117 → `golden(...)` for each.
- [ ] Verify: `cargo test -p lobo-cli tui::status::tests::status_golden` → ok.
- [ ] Commit: `lobo-cli: status dashboard rendering, Go goldens pass`.

## Task 34 — `StatusModel`

**Files:** `src/tui/status.rs`. Rows: S-02. Go test: TestStatusModelKeepsLastSnapOnError.

```rust
pub enum Flow { Continue, Quit }
pub enum StatusMsg { Snap(Result<Snap, String>), Key(crossterm::event::KeyEvent) }
pub struct StatusModel { pub snap: Option<Snap>, pub err: Option<String> }
impl StatusModel { pub fn update(&mut self, m: StatusMsg) -> Flow; pub fn view<Tz: TimeZone>(&self, tz: &Tz) -> Text<'static> where Tz::Offset: Display; }
```
- [ ] Failing tests: `status_model_keeps_last_snap_on_error` (port); `quit_keys` (`q`, `esc`, ctrl+c → Quit; `x` → Continue); `down_snap_quits`; `views` (loading, failed + retry line, refresh failed + footer); zero `Snap` renders without panic.
- [ ] Verify: `cargo test -p lobo-cli tui::status` → ok.
- [ ] Commit: `lobo-cli: status model`.

## Task 35 — `UpModel`

**Files:** `src/tui/up.rs`. Rows: U-08.

```rust
pub enum UpMsg { Event(Option<UpEvent>), Key(KeyEvent), Tick(DateTime<Utc>) }
pub struct UpModel { pub state: UpState, frame: usize }
impl UpModel { pub fn new() -> Self; pub fn update(&mut self, m: UpMsg, now: DateTime<Utc>) -> Flow; pub fn view(&self) -> Text<'static>; pub fn err(&self) -> Option<&str>; }
pub const SPINNER: [&str; 8];   // bubbles spinner.Dot frames: ⣾ ⣽ ⣻ ⢿ ⡿ ⣟ ⣯ ⣷
```
- [ ] Failing tests: `Event(None)` → Quit; a `done` event → Quit; `q`/ctrl+c → Quit with the U-08 error text; `Tick` advances the frame and sets `state.at`.
- [ ] Verify: `cargo test -p lobo-cli tui::up::tests::model` → ok.
- [ ] Commit: `lobo-cli: up model`.

## Task 36 — TUI terminal loops

**Files:** `src/tui/run.rs`. Rows: U-05, U-08, S-02.

```rust
pub async fn run_up(rx: mpsc::Receiver<UpEvent>, clock: Arc<dyn Clock>) -> anyhow::Result<()>;          // Err = model err
pub async fn run_status(deps: &Deps, clock: Arc<dyn Clock>) -> anyhow::Result<()>;
pub fn draw_up(f: &mut Frame, m: &UpModel);  pub fn draw_status(f: &mut Frame, m: &StatusModel, tz: &chrono::Local);
```
ratatui `Terminal::with_options(Viewport::Inline(h))` (bubbletea default is inline, output stays in scrollback), `h` = rendered line count capped at the terminal height; raw mode on, restored on every exit path (guard type with `Drop`). Up ticks every 100 ms (spinner); status fetches at start and every 2 s via `control::snapshot`.
- [ ] Failing tests (TestBackend, fixed size): `draw_up` of the `up_ready` state on `TestBackend::new(120, 20)` → `insta::assert_snapshot!(terminal.backend())`; `draw_status` of `status_ready` on `TestBackend::new(80, 40)` with `Utc`.
- [ ] Verify: `cargo test -p lobo-cli tui::run` → ok.
- [ ] Orchestrator (manual, free): a small `#[ignore]` example `cargo run -p lobo-cli --example tui_demo` that feeds the boot script into `run_up` with a 300 ms delay per event; watch it once in a real terminal; check the screen is restored after `q`.
- [ ] Commit: `lobo-cli: ratatui loops for up and status`.

## Task 37 — Boot report + `boots.jsonl`

**Files:** `src/bootlog.rs`. Rows: U-09.

```rust
pub fn tee_ready(rx: mpsc::Receiver<UpEvent>) -> (mpsc::Receiver<UpEvent>, Arc<Mutex<Option<ReadyInfo>>>);
#[derive(Serialize)] pub struct BootLine<'a> { pub at: GoTime, pub conns: i64, pub ready: &'a ReadyInfo, pub source: &'a str } // alphabetical = Go map order
pub fn report_boot(w: &mut dyn Write, r: Option<&ReadyInfo>, source: &str, conns: i64, log: &Path, now: DateTime<Utc>);
```
Table = Go `tabwriter(minwidth 0, tabwidth 0, padding 2, ' ')` with a trailing empty cell: every column padded to its widest cell (rune count) + 2.
- [ ] Failing tests: `report_boot` stderr bytes == `fixtures/go/text/report_boot.txt`; appended line JSON-equal to `boots_line.json`, keys in order `at, conns, ready, source`; two calls → two lines; `None` or no timings → writes nothing.
- [ ] Verify: `cargo test -p lobo-cli bootlog` → ok.
- [ ] Commit: `lobo-cli: boot timings report and boots.jsonl`.

## Task 38 — `up`: plain and JSON consumers

**Files:** `src/cmd/up.rs`. Rows: U-06, U-07, J-01.

```rust
pub async fn json_up(rx: mpsc::Receiver<UpEvent>, out: &mut dyn Write) -> anyhow::Result<()>;  // "up failed" if any err
pub async fn plain_up(rx: mpsc::Receiver<UpEvent>) -> anyhow::Result<()>;                      // logs via tracing
```
- [ ] Failing tests: `json_up` over the Go-captured events (decode `json_up_boot.jsonl` into `UpEvent`s, feed a channel) → output lines JSON-equal to the file; the failed file → `Err("up failed")`. `plain_up` with the `logfmt` layer on a buffer (`tracing::subscriber::with_default`) → lines equal to `plain_up_boot.txt` / `plain_up_failed.txt` after timestamp strip.
- [ ] Verify: `cargo test -p lobo-cli cmd::up::tests::consumers` → ok.
- [ ] Commit: `lobo-cli: up --json and --plain output`.

## Task 39 — `up` command glue

**Files:** `src/cmd/up.rs`. Rows: U-02, U-05, U-08, U-09.

Order exactly U-02. Mode switch U-05 (`term.stdout_tty`). Presign: `if o.provider == "local" { deps.presign = None }`. `report_boot(stderr, ready, &o.source, o.conns, Path::new("boots.jsonl"), clock.now())` after the mode returns, before returning its error. `--ssh` read error → returned as is.
- [ ] Failing tests (in-process, `App.deps` = a closure returning `lobo_core::control::testkit::deps(..)`): `--cloud bad` message exact and deps never built; `--q6` sets model q6; `--provider local` → presign `None` (deps closure records it); not-TTY without flags → plain; `--json` → JSON lines; `--ssh <missing>` → error; `boots.jsonl` appears in the temp cwd after a ready run.
- [ ] Verify: `cargo test -p lobo-cli cmd::up` → ok.
- [ ] Commit: `lobo-cli: up command`.

## Task 40 — `status`

**Files:** `src/cmd/status.rs`. Rows: S-01, J-02.
- [ ] Failing tests (in-process, fake deps): `--json` → one line JSON-equal to `status_running.json`; down deps → equal to `status_down.json`; `--once` with `Term.stdout_tty = false` → stdout == `to_plain(&render_status(&snap, &chrono::Local))`; TTY + `--once` → contains `\x1b[`; `check_providers` failure stops before deps.
- [ ] Verify: `cargo test -p lobo-cli cmd::status` → ok.
- [ ] Commit: `lobo-cli: status command`.

## Task 41 — `down`

**Files:** `src/cmd/down.rs`. Rows: D-01, D-02, J-05.
- [ ] Failing tests: `--json` → JSON-equal to `down_running.json`; plain → INF line `down: no lobo pods left spent=$X.XX` (timestamp stripped); check_providers failure message.
- [ ] Verify: `cargo test -p lobo-cli cmd::down` → ok.
- [ ] Commit: `lobo-cli: down command`.

## Task 42 — `logs` and `test`

**Files:** `src/cmd/logs.rs`, `src/cmd/test.rs`. Rows: L-01, E-01.
- [ ] Failing tests: `logs -n 5` → the fake agent's `logs(5)` text on stdout unchanged (fake returns `last log line`, no newline added). `test` with fake deps whose target base is a `wiremock` server (local, free): chat stream + tool-call bodies from `lobo_core` check fixtures → stdout = chat text, two INF lines; 250-byte reply → 200 bytes + `…`; multibyte char at byte 199 → cut at the char boundary, no panic; unreachable version → `lobo not reachable at {base}: …`; invalid tool call → `{err}\n{body}`.
- [ ] Verify: `cargo test -p lobo-cli cmd::logs cmd::test` → ok.
- [ ] Commit: `lobo-cli: logs and test commands`.

## Task 43 — `release`

**Files:** `src/cmd/release.rs`. Rows: R-01, R-02, R-03.

```rust
pub fn git_info(dir: &Path) -> anyhow::Result<(String, bool)>;
pub fn whoami() -> String;
pub fn agent_build_command(top: &Path, ver: &str) -> std::process::Command;   // cargo zigbuild … (R-03), not run in tests
pub fn release_manifest(ver: &str, sha: &str, dirty: bool, built_at: DateTime<Utc>, built_by: &str) -> Manifest;
```
- [ ] Failing tests: `git_info` on a temp `git init` repo with one commit → 7-char sha, clean; after touching a file → dirty; outside a repo → `git rev-parse: …`. `agent_build_command` program/args/env/cwd exact. `release_manifest` truncates to the second and uses the P1 pins. Command order: missing R2 → gate error before any store call (store is never built in tests).
- [ ] Verify: `cargo test -p lobo-cli cmd::release` → ok. No test runs the real publish.
- [ ] Commit: `lobo-cli: release command (agent via cargo zigbuild)`.

## Task 44 — completion + SIGINT

**Files:** `src/cmd/completion.rs`, `src/lib.rs`. Rows: G-07, G-08.
- [ ] `completion <shell>` via `clap_complete::generate` for bash, zsh, fish, powershell; hidden.
- [ ] `run` owns one `CancellationToken`; `tokio::signal::ctrl_c` cancels it; passed to `control::up` and used as a `select!` arm around `snapshot`, `down`, `target`, `logs`, `test`.
- [ ] Failing tests: `completion zsh` stdout non-empty and contains `lobo`; `completion` absent from `root_help`; cancelling the token during a fake `up` ends the stream and returns an error (exit 1).
- [ ] Verify: `cargo test -p lobo-cli cmd::completion app::tests::cancel` → ok.
- [ ] Commit: `lobo-cli: completion and Ctrl-C cancellation`.

## Task 45 — Go replay test

**Files:** Create `tests/go_replay.rs`. Rows: all rows touched by `cases.json` (G-04..G-11, V-01, C-*, K-01, R-01 gates, U-02/U-03/U-04, M-01, LR-02).

- [ ] Failing test `replay_go_cases`: for each case, build the same temp layout (config body + mode, weights dir, temp cwd, same env as Task 3/4 incl. `HTTPS_PROXY=http://127.0.0.1:9`), run `assert_cmd::Command::cargo_bin("lobo")`, normalise with the Task 4 rules, then assert: exit code equal; stdout equal (JSON cases: JSON-equal per line); stderr per `stderr_match`; every `files_after` file equal (config bodies byte-equal, JSON files JSON-equal). The failure message names the case.
- [ ] Fix every mismatch in the owning module (not in the test). A mismatch that comes from `lobo-core` goes to the orchestrator as a P3 bug, not patched in the CLI.
- [ ] Verify: `cargo test -p lobo-cli --test go_replay` → ok.
- [ ] Commit: `lobo-cli: replay of Go CLI runs passes`.

## Task 46 — `test-fakes` binary + assert_cmd JSON tests

**Files:** `src/fakes.rs`, `src/main.rs`, `tests/cli_fakes.rs`. Rows: J-01, J-02, J-05, Decision 9.

```rust
#[cfg(feature = "test-fakes")] pub fn scenario(name: &str) -> Option<App>;   // "boot" | "failed" | "running" | "down"
```
`main.rs`: `#[cfg(feature = "test-fakes")]` reads `LOBO_TEST_SCENARIO`; unknown name → `error: unknown test scenario`. Scenarios mirror Task 5 (same scripts, same fixed clock, same `UpOpts` result).
- [ ] Failing tests (`cargo test -p lobo-cli --features test-fakes --test cli_fakes`), each with a temp RunPod-only config: `LOBO_TEST_SCENARIO=boot lobo up --json` → stdout JSON-equal to `json_up_boot.jsonl`, exit 0; `failed` → `json_up_failed.jsonl`, stderr `error: up failed`, exit 1; `running lobo status --json` → `status_running.json`; `down lobo status --json` → `status_down.json`; `running lobo down --json` → `down_running.json`; `boot lobo up --plain` → stderr log lines equal to `plain_up_boot.txt` after timestamp strip, and `report_boot.txt` content at the end; `boot lobo up --json --q6=false` → `changed` has q6, model stays release default (Swift app call shape).
- [ ] Verify: the command above → ok.
- [ ] Commit: `lobo-cli: assert_cmd JSON parity on lobo-core fakes`.

## Task 47 — Spike: brew build tool (orchestrator only)

Options (scores = fit for "brew formula from the Rust build, same tap, you do nothing"):
- **A. goreleaser `builder: rust` + cargo-zigbuild — 9/10.** Same file, same tap, same deploy key, same archive names. Costs: zig + cargo-zigbuild in CI.
- **B. goreleaser `builder: prebuilt` (cargo builds in the workflow, goreleaser packages + brews) — 7/10.** Same tap and key. Costs: the per-target build matrix moves into release.yml by hand.
- **C. cargo-dist homebrew installer — 4/10.** Costs: needs a PAT secret `HOMEBREW_TAP_TOKEN` (a user action), regenerates release.yml (the dmg job must be re-added), new archive names.

Recommend A. B is inserted before C because it keeps the deploy key; see "Spec issues" 5.

Spike for A (time box 60 min, ≤ 2 config-fix attempts), on a scratch copy of `.goreleaser.yaml` (`/tmp/lobo-spike/.goreleaser.yaml`, `goreleaser … -f`):
```yaml
builds:
  - id: lobo
    builder: rust
    binary: lobo
    dir: .
    flags: [--release, -p, lobo-cli]
    env: [LOBO_VERSION={{.Version}}, LOBO_COMMIT={{.ShortCommit}}, LOBO_DATE={{.Date}}]
    targets: [x86_64-apple-darwin, aarch64-apple-darwin, x86_64-unknown-linux-musl, aarch64-unknown-linux-musl]
```
(archives, brews, checksum, changelog unchanged.)

Pass = ALL of:
1. `goreleaser check -f /tmp/lobo-spike/.goreleaser.yaml` → exit 0.
2. `goreleaser release --snapshot --clean --skip=publish -f …` → exit 0 on this Mac.
3. `ls dist/*.tar.gz` → exactly `lobo_darwin_amd64.tar.gz lobo_darwin_arm64.tar.gz lobo_linux_amd64.tar.gz lobo_linux_arm64.tar.gz`, each holding `lobo`.
4. `file` on the 4 binaries → Mach-O x86_64, Mach-O arm64, ELF x86-64 statically linked, ELF aarch64 statically linked.
5. The darwin/arm64 binary: `lobo version` → `lobo <snapshot version> (<short sha>, <date>)`, not `dev`.
6. `dist/homebrew/lobo.rb` exists (path as goreleaser writes it for a skipped publish), has 4 url+sha256 pairs and the `system "#{bin}/lobo", "version"` test.

Fail on any → try B with the same six checks (build step: `cargo zigbuild --release -p lobo-cli --target <t>` ×4, `prebuilt.path: target/{{ .Target }}/release/lobo`). B fails → C: stop and `/notify` the user: "C needs a HOMEBREW_TAP_TOKEN PAT secret; approve?".

- [ ] Record the result (which option, the 6 checks with their output lines) in this plan under "Spike result".
- [ ] Commit: `plans: P4 brew spike result`.

## Task 48 — `.goreleaser.yaml` + `release.yml`

**Files:** Modify `.goreleaser.yaml`, `.github/workflows/release.yml`. Rows: B-01..B-04.

For A:
- `.goreleaser.yaml`: `builds:` replaced by the spike block; header comment updated (`Release the Rust lobo CLI …`); rest unchanged.
- `release.yml` job `release`: `runs-on: macos-15` (native Apple SDK for the darwin targets; cargo-zigbuild for linux musl); steps: checkout (fetch-depth 0) → `dtolnay/rust-toolchain@stable` reading `rust-toolchain.toml`, targets the four triples → `mlugg/setup-zig@v2` → `taiki-e/install-action@v2` with `tool: cargo-zigbuild` → `Swatinem/rust-cache@v2` → `goreleaser/goreleaser-action@v6` (`version: "~> v2"`, `args: release --clean`, same env). `setup-go` removed from this job. The `dmg` job is unchanged (P5 owns it).
- [ ] Verify locally: `goreleaser check` → exit 0; `actionlint .github/workflows/release.yml` if installed, else say not run.
- [ ] Commit: `release: goreleaser builds the Rust lobo (rust builder, zigbuild)`.

## Task 49 — Local brew install from a temp tap (orchestrator, free)

- [ ] `make rust-release-snapshot` (Task 50 target) → the six checks of Task 47 still pass.
- [ ] `brew list --versions lobo` → if installed, `brew unlink lobo` and note it.
- [ ] `brew tap-new --no-git local/lobotest`; copy `dist/homebrew/lobo.rb` into `$(brew --repository)/Library/Taps/local/homebrew-lobotest/Formula/lobo.rb`; rewrite each darwin url to `file://<abs>/dist/lobo_darwin_<arch>.tar.gz` (sha stays).
- [ ] `brew install --formula local/lobotest/lobo` → ok; `brew test local/lobotest/lobo` → ok; `lobo version` → the snapshot version.
- [ ] Clean up: `brew uninstall local/lobotest/lobo && brew untap local/lobotest`; if unlinked before, `brew link lobo`; `lobo version` → the user's previous version again.
- [ ] Record the output lines under "Spike result".

## Task 50 — Makefile + CI

**Files:** Modify `Makefile`, `.github/workflows/rust.yml`. Decision 2.

```make
RUST_ENV := LOBO_VERSION=$(VERSION) LOBO_COMMIT=$(COMMIT) LOBO_DATE=$(DATE)
rust-build-lobo:        # release build, default features only
	$(RUST_ENV) cargo build --release -p lobo-cli
	install -d $(BIN) && install -m 0755 target/release/lobo $(BIN)/lobo-rs
rust-install: rust-build-lobo      # side by side with the Go lobo until P6
	install -d $(PREFIX)/bin && install -m 0755 $(BIN)/lobo-rs $(PREFIX)/bin/lobo-rs
rust-release-snapshot:
	goreleaser release --snapshot --clean --skip=publish
```
`rust-test` → `cargo test --workspace --features lobo-cli/test-fakes`. `rust-lint` → clippy with `--features lobo-cli/test-fakes`. `cli-fixtures` from Tasks 3–5. Go targets untouched.
- [ ] rust.yml: add a step after the P1 fixture step: `actions/setup-python@v5` (3.x) then `make cli-fixtures && git diff --exit-code crates/lobo-cli/tests/fixtures crates/lobo-cli/tests/goldens`.
- [ ] Verify: `make rust-lint rust-test rust-build-lobo` → exit 0; `bin/lobo-rs version` → `lobo <git describe> (<sha>, <date>)`.
- [ ] Commit, push `feat/rust`, `gh run watch` → green; red → `gh run view --log-failed`, fix, push, repeat.

## Task 51 — Release-binary seam check (orchestrator)

- [ ] `LOBO_TEST_SCENARIO=boot bin/lobo-rs --config /nonexistent/config.env up --json; echo $?` → `error: no config at /nonexistent/config.env. Run \`lobo config\` first`, `1` (the release build has no fakes).
- [ ] `strings bin/lobo-rs | grep -c LOBO_TEST_SCENARIO` → `0`.

## Task 52 — Phase close (orchestrator)

- [ ] `make rust-lint rust-test && go test ./... && git diff --exit-code` → green, clean.
- [ ] Tick every inventory row: each has a named test in the Go→Rust table or in Tasks 6–46. List any unticked row here and stop.
- [ ] Names check: `UpEvent`, `Snap`, `ConfigShow`, `Listing`, `ReadyInfo`, `Laptop`, `UpOpts`, `Deps`, `Wiring`, `Spawner`, `RunConfig` match contracts.md v1.1 exactly.
- [ ] Spec: file-table rows for `tools/clifixtures/` and `cmd/lobo/capture_test.go` (both removed at P6); P4 "As-built notes" (brew option chosen, `lobo-rs` side-by-side name, G-11 decision, wizard back-navigation loss).
- [ ] Plan status → `done`. `/notify`: "Rust P4 done on feat/rust: lobo CLI parity (Go replay + goldens + JSON), brew via <A|B>. Try `make rust-install` → `lobo-rs`. Approve P5 plan next."

---

## Go test → Rust test (every Go test function in cmd/lobo, internal/tui, internal/configtui)

"P3" = the Go test's logic moved to `lobo-core` and is ported in `plan-p3-v1.0.md` (its table, lines 1237-1253). P4 then adds only a CLI-boundary test, named in the third column.

| Go test (file:line) | Rust home of the Go assertions | P4 CLI-boundary test | Task |
|---|---|---|---|
| TestApplyDefaults (cmd/lobo/defaults_test.go:12) | P3 `control::precheck::tests::apply_defaults_table` | `cmd::up::tests::defaults::{changed_names_match_go, q6_false_keeps_config_model_out, q6_true_sets_model, flag_beats_bad_config_ctx}` | 16 |
| TestMaskedUnknownKeys (defaults_test.go:62) | P3 `config::show::tests::masked_unknown_keys` | `cmd::config::tests::show_text_masks` | 21 |
| TestParseSetArgs (defaults_test.go:72) | `cmd::config::tests::parse_set_args` | — | 20 |
| TestShowConfigJSONMasks (defaults_test.go:84) | P3 `config::show::tests::show_masks_secrets` | `tests/cli_config.rs::show_json_masks` | 21 |
| TestParseSetJSON (defaults_test.go:96) | `cmd::config::tests::parse_set_json` | — | 20 |
| TestRootHelpGolden (help_test.go:15) | `help::tests::root_help_golden` | — | 13 |
| TestLocalRunFlags (local_test.go:16) | P3 `local::supervise::tests::run_config_from_args_table` | `cmd::local::tests::local_run_flags` (7 rows through clap + the "local is hidden" assert) | 29 |
| TestModelsOutput (local_test.go:58) | `cmd::models::tests::models_output` | — | 28 |
| TestCheckTarget (target_test.go:25) | P3 `control::precheck::tests::check_target_table` | `cmd::tests::gates` (up row) | 15 |
| TestCheckProviders (target_test.go:51) | P3 `control::precheck::tests::check_providers_table` | `cmd::tests::gates` (down, status rows) | 15 |
| TestCheckRelease (target_test.go:75) | P3 `control::precheck::tests::check_release_table` | `cmd::tests::gates` (release row) | 15 |
| TestProviders (target_test.go:96) | P3 `control::wiring::tests::providers_from_config_table` | `app::tests::wiring::{wiring_abs_config_path, wiring_spawner_is_cli, wiring_passes_supported_seam}` | 17 |
| TestLocalInstanceURLs (target_test.go:140) | P3 `control::wiring::tests::local_instance_urls_from_state` | — (no CLI logic) | — |
| TestWriteOpencode (target_test.go:152) | P3 `genkey::tests::write_opencode_table` | `cmd::genkey::tests` (key keep/rotate, messages, file mode) | 27 |
| TestUpBootContainerHintAndReRentReset (internal/tui/tui_test.go:59) | `tui::up::tests::boot_container_hint` + `tui::up::tests::re_rent_resets_later_timers` | — | 31, 32 |
| TestUpGolden (tui_test.go:75) | `tui::up::tests::up_golden` (3 goldens) | — | 32 |
| TestStatusGolden (tui_test.go:96) | `tui::status::tests::status_golden` (5 goldens) | — | 33 |
| TestUpStateErr (tui_test.go:120) | `tui::up::tests::up_state_err` | — | 31 |
| TestStatusModelKeepsLastSnapOnError (tui_test.go:129) | `tui::status::tests::status_model_keeps_last_snap_on_error` | — | 34 |
| TestMask (internal/configtui/configtui_test.go:8) | `wizard::validate::tests::mask` (calls `lobo_core::config::mask`) | — | 23 |
| TestResultKeepClearAndDefaults (configtui_test.go:16) | `wizard::state::tests::result_keep_clear_and_defaults` | — | 22 |
| TestNewStateFreshFile (configtui_test.go:39) | `wizard::state::tests::new_state_fresh_file` | — | 22 |
| TestValidators (configtui_test.go:49) | `wizard::validate::tests::validators` | — | 23 |
| TestSummaryMasksSecrets (configtui_test.go:64) | `wizard::state::tests::summary_masks_secrets` | — | 24 |
| TestNewStateLocal (configtui_test.go:72) | `wizard::state::tests::new_state_local` | — | 22 |
| TestResultLocal (configtui_test.go:94) | `wizard::state::tests::result_local` | — | 22 |
| TestLocalValidation (configtui_test.go:124) | `wizard::validate::tests::local_validation` | — | 23 |
| TestSummaryLocalRows (configtui_test.go:159) | `wizard::state::tests::summary_local_rows` | — | 24 |
| TestProviderOptions (configtui_test.go:169) | `wizard::validate::tests::provider_options` | — | 23 |
| TestFormBuilds (configtui_test.go:181) | `wizard::flow::tests::form_builds` | — | 25 |

30 Go tests (measured: `grep -c '^func Test'` = 14 + 5 + 11), none dropped: 21 ported in P4, 9 ported in P3 (+ the TestLocalRunFlags rows), with P4 boundary tests for 8 of those 9. Go lines: cmd/lobo tests 450, internal/tui 144, internal/configtui 187 = 781. New Rust-only tests (help facts, Go replay, JSON on fakes, TUI TestBackend, log format, duration, boot report) sit in Tasks 6–46.

---

## Spike result

_(Task 47 and Task 49 fill this: chosen option, the six check lines, the brew install lines.)_

---

## Self-review

- Spec P4 requirements → tasks: same commands/flags/output/exit codes → Tasks 2–29, 37–46 (inventory rows); `up` progress view → 31–32, 35–36, 38–39; live `status` dashboard → 33–34, 36, 40; `lobo config` wizard → 22–26; `--json` everywhere → 21, 28, 38, 40, 41, 46; `release` → 43; brew formula from the Rust build → 47–49; `lobo local run` → 29; goreleaser-first, cargo-dist fallback with a pass/fail check → 47; release.yml + .goreleaser.yaml → 48; Makefile build/install/lint/test → 50; insta snapshots from today's goldens → 13, 32, 33; help parity from Go output → 3, 9, 10, 13; deterministic clock + fixed terminal size → 31–33, 36; every Go test mapped → table above (30/30).
- Names vs contracts.md v1.0 + P3 additions: `lobo_core::control::{Deps, UpOpts, up, snapshot, down, target, AgentApi, Wiring, deps_from_config, check_target, check_providers, check_release, apply_defaults, testkit, CONTAINER_TIMEOUT}`, `lobo_core::config::{Laptop, default_path, load_laptop, values, save, show, mask, masked, LAYOUT, loose_mode, DEFAULT_LOCAL_PORT}`, `lobo_core::local::{supported, list, supervise, RunConfig, Spawner, SUPERVISOR_ARG}`, `lobo_core::release::{next_version, zip_key, build_zip, scan_for_secrets, zip_url, Store}`, `lobo_core::genkey::{ensure_api_key, write_opencode, new_api_key, OPENCODE_OUT}`, `lobo_core::checks::{chat, tool_call, validate_tool_call}`, `lobo_core::clock::{Clock, SystemClock, StepClock}`, `lobo_proto::{UpEvent, ReadyInfo, Snap, ConfigShow, Listing, Manifest, Resolved, GoTime, DEFAULT_MODEL, DEFAULT_LLAMA_IMAGE, DEFAULT_DEFAULTS}` are used as written. P4's own extras are under "Contract additions".
- No live test anywhere. The only non-unit orchestrator steps are local and free: the Go capture, the manual TUI/wizard look, goreleaser `--snapshot`, and a brew install from a temp tap.
- No placeholder left except the two "fill after running" blocks (Pinned versions, Spike result), same pattern as P1.

## Contract additions

P4 needs the P3 plan's additions 1–7 and 9–11 as written there (Wiring, testkit, prechecks, config helpers, RunConfig, checks args, release Store methods, genkey). On top of those:

1. **Error text rule:** every `lobo_core::Error` Display that the Go CLI prints today (require_* messages, `apply_defaults` messages, provider and control errors, `ensure_api_key`'s `read <path>: …` prefix) stays byte-identical to Go. P4's replay (Task 45) compares stderr exactly.
2. `control::UpOpts: Default` (P4 builds it from flags; cobra-style zero values).
3. `control::testkit` must build outside `cfg(test)` of lobo-core (feature `testkit`) and expose `step clock` deps usable from another crate: `deps(rp, ag, clock: Arc<dyn Clock>) -> Deps`, `events(script, no_cap) -> Vec<UpEvent>` as P3 Task 54 locks them, plus the `FakeRunPod::pods` field public (P4 scenario `running` seeds one RUNNING `pod1`).
4. `lobo_core::local::supervise` owns all logging for `local run` (JSON lines to stdout + the `LogRing`), and installs no global subscriber that would clash with a caller's (it may use a scoped `tracing::subscriber::with_default` or its own writer).
5. None beyond P3 for `LAYOUT`: P4 uses `LayoutGroup { title, keys }` as P3 Task 6 locks it (listed here only so the orchestrator keeps those fields pub).

## Spec issues

1. **Parity baseline is stale.** Spec says `5443667`; `6a72092` (branch `fix/app-silent`, not on master yet) changed `internal/tui/status.go` and added `status_local.golden`. P4 uses master HEAD at P4 start (Decision 10). Spec Goal 4 should say so.
2. **Go removal timing conflicts.** The file table says Go parts go "in the phase that replaces each part"; the rollout says P6, and P1 keeps Go for its drift CI. If P3 removed `internal/*`, Go `cmd/lobo` would not compile before P4. P4 assumes removal at P6 (Decision 3). Fix the file-table wording.
3. **contracts v1.0 is not enough for P4.** `deps_from_config(cfg)` has no config path, spawner or local seam, and there are no fakes for the TUI goldens. The P3 plan's additions fix this; P4 depends on them plus its own 5 small additions. P1's Go-test table still assigns all `cmd/lobo` tests to P4; the P3 plan moved 9 of them. P1's table should point to P3.
4. **Makefile has no phase.** P4 adds `rust-*` targets and installs `lobo-rs`; the swap is P6 (Decision 2). Spec rollout P6 should list it.
5. **cargo-dist fallback needs a user action.** Its homebrew publish uses a PAT secret, not the tap deploy key, which breaks "You do: nothing". P4 inserts goreleaser `prebuilt` (B) before cargo-dist (C).
6. **`lobo release` needs a musl cross toolchain on the Mac** (zig + cargo-zigbuild). The spec only builds the agent in Docker. New dev dependency; `release` errors with an install hint when it is missing.
7. **Extra positional args** (Decision 7): Go ignores `lobo up foo`; Rust rejects it. Confirm or ask for bug-compatible behaviour.
8. **Wizard back navigation** (Decision 8): huh's Shift+Tab back is lost with inquire.
9. **Byte parity limits:** cobra-formatted help, parser error texts, OS error texts, JSON escapes and log colours are not byte-identical (Decisions 1, 4, 5, 6). The spec's "same output" should name these exceptions.


---
<!-- end of plan-p4-v1.0.md -->

# Rust rewrite P5 — Tauri 2 app Implementation Plan v1.0

**Date:** 2026-09-29
**Status:** draft
**Spec:** ./spec.md (spec status: parked — this plan is written on request; exec still needs spec + plan approval)
**Contracts:** ./contracts.md v1.0 (this plan needs v1.1: see "Contract additions")
**Phase:** P5 of 6. Starts after P4 is done on `feat/rust`.

**Goal:** `app/` replaces `macos/`. Same menu bar item, panel, Settings window, notifications and renders as the Swift app at `6a72092` (branch `fix/app-silent`, incl. panel-on-launch/reopen). It links `lobo-core` directly. No CLI subprocess, no JSON parsing of CLI output.

**Architecture:** The Swift `Store` becomes a Rust controller inside the Tauri process (`app/src-tauri`). A pure reducer (`store.rs`) holds every state rule and is unit-tested without Tauri. An async controller (`controller.rs`) runs the poll loop, the `up` stream, the stop sequence and notifications against a `Backend` trait (real = `lobo-core`, test = fake). Every state change emits one `PanelState` on event `lobo://state` and updates the tray. The Svelte UI renders `PanelState` and sends actions as Tauri commands. It computes nothing that the tray or a notification also needs. The local supervisor runs as the same binary re-exec'd with `--lobo-local-run`.

**Tech Stack:** Rust 1.98 (edition 2024), tauri 2, tauri-build 2, tauri-plugin-positioner (feature `tray-icon`), tauri-plugin-notification, tauri-plugin-clipboard-manager, tauri-plugin-opener, tauri-plugin-dialog, tiny-skia (tray + app icon drawing), ts-rs. UI: TypeScript, Svelte 5, vite, vitest, svelte-check, pnpm. Versions pinned in Task 2.

> For agentic workers: use superpowers:subagent-driven-development to implement task-by-task. Checkbox syntax for tracking.

> Implementer scope (verbatim, every dispatch): writes only the code and unit tests its task names and runs that task's focused unit test. Never runs e2e / integration / live / smoke tests, never rents a GPU or pod, never calls a provider API, never publishes, deploys or touches infra, never runs anything money-bearing. Never sets `TEST_DATABASE_URL` or any test-DB env var.

Extra rule for P5 implementers: never launch the built app, never run `cargo tauri dev`, never start `lobo-core` providers for real. The orchestrator does all app runs (Task 40).

---

## Decisions (scored for the goal "parity, no drift, works behind the notch")

**D1. Where the Store logic lives**
- **A. Rust controller + pure reducer, TS renders `PanelState` — 9/10.** Tray title, tray icon and notifications need phase, step and progress while no window is open. A hidden WKWebView gets its timers throttled, so a TS state machine would stall the tray. Loses 1: `now`-based countdowns (kill-in, uptime) are formatted in TS and in Rust; a shared case file (Task 12) guards the pair.
- **B. TS store, Rust only thin commands (contract v1.0 `up` streaming to TS) — 4/10.** Tray and notifications break when no webview is visible. Port rules and readiness would be re-typed in TS.
- **C. Split per feature (boot log in TS, poll in Rust) — 3/10.** Two owners of `phase`. The Swift bugs came from exactly this.
- Pick A. Rule for the split: anything that decides state, readiness, ports, providers, defaults, validation, menu text or notification text is Rust (`lobo-core` when the CLI shares it, else `app/src-tauri/src/store.rs`). TS maps `PanelState` to strings, colours and layout only.

**D2. Panel surfaces**
- **A. Two windows: `panel` (tray popover: no decorations, hides on blur, TrayCenter) and `main` (titled window, opened on launch/reopen) — 8/10.** Same Svelte view in both. Matches Swift (MenuBarExtra + PanelWindow). Loses 2: a second webview (~30 MB, unmeasured) while `main` is open.
- **B. One window, toggle decorations at runtime — 5/10.** `set_decorations` with an overlay titlebar on macOS flickers and loses the transparent titlebar (guess, not tested). Saves one webview.
- **C. Only a titled window, no popover — 3/10.** Loses the tray panel, which is the product.

**D3. Tray text**
- **A. `TrayIcon::set_title` next to a 16 pt non-template icon — 8/10.** Verified on docs.rs (tauri 2.7 `TrayIcon::set_title`: macOS supported, Windows unsupported, Linux only with an icon). Loses 2: the title uses the system font, not SF Mono with monospaced digits like Swift (`StatusItem.swift:11`). Unverified how much it jumps; checked in Task 40.
- **B. Draw icon + text into one image — 5/10.** Monospaced, but the image width changes per word, template mode is lost for the whole item, and text rendering needs a font dependency.
- Pick A. Fallback if the user rejects the width jumps in Task 40: B as a new task. It needs a font rasteriser (`fontdue`) and a bundled monospaced font; SF Mono can't be bundled (licence), so the menu bar text would not match the system font either.

**D4. dmg**
- **A. `cargo tauri build --bundles app`, then today's `hdiutil` step — 8/10.** Same image as `fix/app-silent` (`Makefile:61-69`), proven on `macos-15`. Loses 2: one more Makefile block than the bundler.
- **B. `cargo tauri build --bundles dmg` — 6/10.** Tauri's `bundle_dmg.sh` drives Finder via AppleScript to lay out the window; on headless CI that is a known fragile step (not measured here). Also names the file `lobocode_<ver>_aarch64.dmg`, so release.yml changes.

**D5. Where `macos/` is removed:** here, in P5. Spec "File-level changes": "Removed on the branch … in the phase that replaces each part." P5 replaces the app. Removal is the last code task (Task 38), after the user approved the renders, and after the Swift baseline renders were taken (Task 0).

---

## File map

**Create**
- `app/src-tauri/Cargo.toml` — package `lobocode-app`, `[[bin]] name = "lobocode"`, workspace member.
- `app/src-tauri/build.rs` — `tauri_build::build()`.
- `app/src-tauri/tauri.conf.json` — productName `lobocode`, mainBinaryName `lobocode`, identifier `io.github.1905.lobocode`, `bundle.macOS.minimumSystemVersion` `13.0`, `bundle.macOS.signingIdentity` `-` (ad-hoc), `bundle.icon` from `icons/`, `build.frontendDist` `../ui/dist`, `build.devUrl` `http://localhost:5173`, `app.windows` `[]` (all windows are created in Rust).
- `app/src-tauri/Info.plist` — `LSUIElement` true (merged into the bundle's plist by tauri-bundler).
- `app/src-tauri/capabilities/default.json` — windows `panel`, `main`, `settings`: `core:default`, `core:window:allow-set-size`, `core:event:default`. Nothing else (clipboard, opener, dialog are Rust-side only).
- `app/src-tauri/icons/*` — generated by `cargo tauri icon` (Task 25). Committed.
- `app/src-tauri/src/main.rs` — argv switch: `--lobo-local-run` → supervisor, else Tauri app.
- `app/src-tauri/src/lib.rs` — `pub fn run()`, module list.
- `app/src-tauri/src/types.rs` — `Phase`, `Target`, `Step`, `StepMark`, `PanelState`, `AppError` (+ ts-rs).
- `app/src-tauri/src/store.rs` — pure reducer (port of `Store.swift` + `StatusItem.swift` extension).
- `app/src-tauri/src/fmt.rs` — `duration`, used in notification text.
- `app/src-tauri/src/backend.rs` — `trait Backend`, `CoreBackend`, `#[cfg(test)] FakeBackend`.
- `app/src-tauri/src/controller.rs` — poll loop, start, stop, dismiss, notifications.
- `app/src-tauri/src/notify.rs` — `trait Notifier`, Tauri impl.
- `app/src-tauri/src/prefs.rs` — saved target (`prefs.json` in app config dir).
- `app/src-tauri/src/commands.rs` — every `#[tauri::command]`.
- `app/src-tauri/src/tray.rs` — tray build + update.
- `app/src-tauri/src/icons.rs` — tiny-skia drawing: tray icon per phase, 1024 px app icon.
- `app/src-tauri/src/windows.rs` — create/show `panel`, `main`, `settings`.
- `app/src-tauri/src/supervisor.rs` — `--lobo-local-run` entry.
- `app/src-tauri/examples/render_icons.rs` — writes tray PNGs + `icon_1024.png` to a dir.
- `app/src-tauri/tests/fixtures.rs` — writes `app/ui/src/fixtures/panel_*.json` + `settings.json` from the reducer (like ts-rs export).
- `app/ui/package.json`, `pnpm-lock.yaml`, `vite.config.ts`, `tsconfig.json`, `svelte.config.js`, `index.html`.
- `app/ui/src/main.ts` — mounts by `?view=panel|settings|render`.
- `app/ui/src/lib/theme.css` — tokens (Task 26).
- `app/ui/src/lib/fmt.ts`, `fmt.test.ts`.
- `app/ui/src/lib/view.ts`, `view.test.ts` — `PanelState` → view models.
- `app/ui/src/lib/settings.ts`, `settings.test.ts` — form fields + diff.
- `app/ui/src/lib/api.ts` — typed `invoke` wrappers + `onState`.
- `app/ui/src/widgets/{Logo,RasterBar,Scanlines,GlitchText,Cursor,BracketButton,LinkButton,BracketPicker}.svelte`.
- `app/ui/src/panel/{Panel,Header,Footer,SetupCard,StartCard,LocalStart,BootLog,ReadyCard,FailCard}.svelte`.
- `app/ui/src/settings/Settings.svelte`.
- `app/ui/src/render/Render.svelte` — every fixture state, one per URL.
- `app/ui/src/fixtures/*.json` — generated (Task 24) + `time_cases.json` (hand-written, Task 12).
- `app/ui/src/gen/*.ts` — ts-rs output of app types. Committed.
- `tools/render_review.py` — builds the review page (Swift baseline vs Tauri, per state).

**Modify**
- `Cargo.toml` — `members` adds `app/src-tauri`.
- `crates/lobo-proto/src/control.rs` — `UpRequest`. `crates/lobo-proto/src/config.rs` — `Readiness`. P1 `ts_exports_have_no_bigint` expected list gains both.
- `crates/lobo-core/src/{config,control,local}.rs` — contract additions (Tasks 3–7), only what P3 did not already ship.
- `Makefile` — `mac`, `dmg`, `install-mac` rebuilt; new `app-test`, `app-lint`, `app-render`, `app-icons`, `app-fixtures`.
- `.github/workflows/release.yml` — `dmg` job builds with Rust + pnpm + tauri-cli.
- `.github/workflows/rust.yml` — ubuntu jobs `--exclude lobocode-app`; new `app` job on `macos-15`.
- `README.md` — app section. `docs/img/{menubar_ready,panel_boot,panel_ready,settings}.png` — new renders.
- `.gitignore` — `app/ui/node_modules/`, `app/ui/dist/`.

**Remove (Task 38, `mv` to `/tmp/trash`, then `git add -A`)**
- `macos/` (all of it, incl. `make_icns.py`, `Info.plist`, `Tests/`).

**Out of scope for P5**
- Windows/Linux app builds. Code signing / notarization. Any new user-facing feature. Local performance.
- Changing panel look. This is a port: same strings, colours, sizes. The ux-design-guide redesign flow (variants, quiet pass) does not apply; its inventory, every-state renders, own-eyes check of each screenshot and review page do.

---

## Locked interfaces

### lobo-proto additions (Task 8)

```rust
/// What the app asks `up` for. None = config default (the CLI's "flag not set").
/// The app always sends provider and model (explicit Q8 must beat a saved LOBO_MODEL=q6, Store.swift:168).
pub struct UpRequest { pub provider: Option<String>, pub model: Option<String>, pub ctx: Option<i64>,
                       pub source: Option<String>, pub cloud: Option<String> }
/// Config readiness, computed by lobo-core from the file with the CLI's own rules.
pub struct Readiness { pub exists: bool, pub cloud_ready: bool, pub ready: bool, pub local_supported: bool,
                       pub providers: Vec<String>, pub default_provider: String, pub default_model: String,
                       pub local_port: u16, pub error: Option<String> }
```
Both follow the P1 wire rules (derives, `#[ts(export)]`, `i64` → `#[ts(type = "number")]`).

### lobo-core additions (Tasks 3–7; skip any P3 already shipped under the same name)

```rust
pub mod config {
    pub fn readiness(path: &Path) -> lobo_proto::Readiness;
      // exists = file exists. Load error → all false, error = Some(msg).
      // cloud_ready = exists && require_cloud() ok && LOBO_API_KEY set.
      // ready = cloud_ready || (local_supported && exists && LOBO_API_KEY set).
      // providers = Laptop::providers(); default_provider = Laptop::default_provider() (may be "local");
      // default_model = Defaults.model or DEFAULT_MODEL; local_port = Laptop::port().
    pub fn validate_set(set: &BTreeMap<String, String>) -> std::result::Result<(), String>;
      // SettingsView.swift:253-265 rules and exact messages; port rule via parse_local_port.
    pub fn new_api_key() -> String;               // "sk-" + 48 lowercase hex (24 random bytes)
    pub fn mask(s: &str) -> String;               // Go configtui.Mask: "" → "(not set)", < 12 chars → "••••", else "abcd…wxyz"
}
pub mod control {
    pub fn resolve_up(cfg: &Laptop, cfg_path: &Path, req: &UpRequest) -> Result<UpOpts>;
      // port of cmd/lobo/defaults.go applyDefaults + main.go checkTarget; P4 CLI calls it too
      // (flags set → Some). Same error texts.
    pub fn deps_with_spawner(cfg: Laptop, spawner: local::Spawner) -> Result<Deps>;
      // deps_from_config(cfg) == deps_with_spawner(cfg, Spawner { exe: current lobo, args_prefix: ["local","run"] })
}
pub mod local {
    pub fn free_bytes_nearest(path: &Path) -> Option<u64>;   // statfs of path, or of its nearest existing parent; "~/" expanded
    impl RunConfig { pub fn parse(args: &[String]) -> Result<RunConfig>; }
      // the exact flag set of `lobo local run` incl. global --config; P4 CLI clap maps to the same struct
    // is_supervisor(pid, boot_id): a process matches if its argv has ("local","run") OR SUPERVISOR_ARG,
    // AND "--boot-id <boot_id>" — both as whole args.
}
```

### app crate (Tasks 9–23)

```rust
// types.rs  (ts-rs, #[ts(export, export_to = "../gen/")] so P1's proto/ file list stays lobo-proto only)
pub enum Phase { Loading, NoConfig, Off, Booting, Ready, Stopping, Failed { message: String } } // serde tag = "kind"
impl Phase { pub fn word(&self) -> &'static str; }            // SCAN SETUP OFF BOOT RUN STOP FAIL
pub enum Target { Local, Cloud }                              // lowercase
pub enum Step { Rent, Container, Tunnel, Gpu, Download, Load, Ready } // lowercase, this order
impl Step { pub fn steps(local: bool) -> &'static [Step]; pub fn label(self, local: bool) -> &'static str;
            pub fn from_up_phase(p: &str) -> Option<Step>; pub fn index(self) -> usize; }
pub struct StepMark { pub step: Step, pub at_s: f64 }         // seconds since boot_start
pub struct PanelState {
    pub phase: Phase, pub target: Target, pub provider: String, pub model: String,
    pub snap: Option<Snap>, pub config: Option<ConfigShow>, pub readiness: Option<Readiness>,
    pub models: Option<Listing>, pub catalog_ids: Vec<String>,   // DEFAULT_MODEL first, then the rest of catalog::all()
    pub download: Option<DownloadProgress>, pub steps: Vec<StepMark>, pub boot_start_ms: Option<i64>,
    pub up_phase: Option<String>, pub last_detail: String, pub warning: Option<String>, pub log_tail: Vec<String>,
    pub ready_url: Option<String>,
    // derived by the reducer, so TS never recomputes them:
    pub is_local: bool, pub boot_steps: Vec<Step>, pub current_step: Option<Step>, pub endpoint: Option<String>,
    pub menu_text: String, pub boot_progress: f64,
}
pub struct AppError { pub kind: String, pub message: String }  // from lobo_core::Error::kind() + Display

// store.rs — pure, no I/O, no clock (now passed in)
pub struct Store { /* PanelState fields + up_running, model_auto_picked, user_stopped, panel_open, saved_target */ }
impl Store {
    pub fn new(saved_target: Option<Target>, local_supported: bool) -> Store;
    pub fn view(&self, now: DateTime<Utc>) -> PanelState;
    pub fn poll_interval(&self) -> Duration;                      // Store.swift:121-127
    pub fn derive(s: &Snap, up_running: bool, current: &Phase) -> Phase;   // Store.swift:243-254
    pub fn default_target(saved: Option<Target>, r: Option<&Readiness>, cfg: Option<&ConfigShow>,
                          models: Option<&Listing>, local_supported: bool) -> Target;   // Store.swift:153-160
    pub fn apply_config(&mut self, c: ConfigShow, r: Readiness);               // Store.swift:176-189 (w/o I/O)
    pub fn apply_models(&mut self, m: Listing);                                // Store.swift:191-203
    pub fn apply_snap(&mut self, s: Snap, now: DateTime<Utc>) -> Vec<Note>;    // Store.swift:224-241
    pub fn begin_up(&mut self, now: DateTime<Utc>) -> UpRequest;              // Store.swift:256-266 + upArgs
    pub fn handle_event(&mut self, ev: &UpEvent, now: DateTime<Utc>) -> Vec<Note>;  // Store.swift:279-298
    pub fn up_ended(&mut self, last_err: Option<&str>) -> Vec<Note>;           // Store.swift:300-308
    pub fn begin_stop(&mut self) -> bool;                                       // returns is_local at press time
    pub fn stop_failed(&mut self, msg: String); pub fn stop_done(&mut self);
    pub fn dismiss(&mut self); pub fn choose(&mut self, t: Target); pub fn set_provider(&mut self, p: String);
    pub fn set_model(&mut self, m: String); pub fn set_warning(&mut self, w: Option<String>);
    pub fn poll_failed(&mut self, msg: String);                                 // S3: warning; loading → off
    pub fn set_panel_open(&mut self, open: bool);
}
pub struct Note { pub title: String, pub body: String }         // a notification to send

// fmt.rs
pub fn duration(secs: f64) -> String;                           // Store.swift:408-411

// backend.rs
#[async_trait] pub trait Backend: Send + Sync {
    fn config_path(&self) -> PathBuf;
    async fn config(&self) -> Result<(ConfigShow, Readiness), AppError>;
    async fn models(&self) -> Result<Listing, AppError>;          // local::list(Laptop::weights())
    async fn snapshot(&self) -> Result<Snap, AppError>;
    fn up(&self, req: UpRequest, cancel: CancellationToken) -> Result<mpsc::Receiver<UpEvent>, AppError>;
    async fn down(&self) -> Result<f64, AppError>;
    async fn api_key(&self) -> Result<String, AppError>;          // clear value, only for the clipboard
    async fn save(&self, set: BTreeMap<String, String>) -> Result<(), AppError>;  // validate_set, then config::save
}
pub struct CoreBackend { pub path: PathBuf, pub spawner: local::Spawner }  // path = $LOBO_APP_CONFIG or config::default_path()

// controller.rs
pub struct Controller;   // Arc; holds Mutex<Store>, Arc<dyn Backend>, Arc<dyn Notifier>, Arc<dyn Clock>, emit: Arc<dyn Fn(PanelState)+Send+Sync>
impl Controller {
    pub fn new(b: Arc<dyn Backend>, n: Arc<dyn Notifier>, clock: Arc<dyn Clock>, prefs: Prefs,
               emit: Arc<dyn Fn(PanelState) + Send + Sync>) -> Arc<Controller>;
    pub fn spawn_loops(self: &Arc<Self>);                         // poll loop + 1 s tick (tray countdown)
    pub async fn refresh(&self, models: bool); pub async fn load_config(&self, models: bool);
    pub fn start(self: &Arc<Self>); pub fn stop(self: &Arc<Self>); pub fn dismiss(self: &Arc<Self>);
    pub fn choose(&self, t: Target); pub fn set_provider(&self, p: String); pub fn set_model(&self, m: String);
    pub fn panel_shown(self: &Arc<Self>, open: bool);            // open → refresh(models: true)
    pub fn state(&self) -> PanelState;
}
// Rule: never hold the Store mutex across an .await.

// notify.rs
pub trait Notifier: Send + Sync { fn send(&self, n: &Note); }

// prefs.rs
pub struct Prefs { pub target: Option<Target> }
impl Prefs { pub fn load(dir: &Path) -> Prefs; pub fn save(&self, dir: &Path) -> std::io::Result<()>; }  // <dir>/prefs.json

// icons.rs
pub fn tray_icon(phase: &Phase, progress: f64) -> (Vec<u8> /*RGBA*/, u32, u32, bool /*template*/); // 32×32 px = 16 pt @2x
pub fn app_icon_1024() -> tiny_skia::Pixmap;

// supervisor.rs
pub fn main(args: &[String]) -> i32;   // args after SUPERVISOR_ARG; RunConfig::parse → tokio runtime → local::supervise; SIGINT/SIGTERM cancel
```

Tauri commands (`commands.rs`). TS calls only these:

| Command | Args → Result | Swift source |
|---|---|---|
| `get_state` | → `PanelState` | initial render |
| `start` / `stop` / `dismiss` | → `()` | `Store.start/stop/dismiss` |
| `choose_target` | `t: Target` | `Store.choose` |
| `set_provider` / `set_model` | `v: String` | pickers |
| `refresh` | `models: bool` | panel onAppear |
| `copy_api_key` | → `Result<(), AppError>` | `Store.copyAPIKey` (key never enters the webview) |
| `copy_text` | `s: String` | endpoint copy |
| `config_show` | → `Result<ConfigShow, AppError>` | contract |
| `config_save` | `set: BTreeMap<String,String>` → `Result<(), AppError>` | contract; runs `validate_set`, saves, then `load_config(models: true)` + `refresh(false)` |
| `local_models` | → `Result<Listing, AppError>` | contract |
| `catalog` | → `Vec<Model>` | contract |
| `free_bytes` | `path: String` → `Option<u64>` | `SettingsView.freeBytes` |
| `gen_api_key` | → `String` | `Mask.newAPIKey` |
| `choose_weights` | `start: String` → `Option<String>` | `SettingsView.chooseWeights` (dialog plugin, folder, can create) |
| `open_settings` / `reveal_config` / `open_config` / `quit` | → `()` | footer + file box |

Event: `lobo://state` → `PanelState`, emitted after every Store change and on the 1 s tick only when `menu_text` changed.

Contract v1.0 `snapshot`, `up` (+ `lobo://up`), `cancel_up`, `down` are not exposed to TS. The controller calls them through `Backend`. See "Contract additions".

---

## Pinned versions

_(Task 2 fills this line from `Cargo.lock` and `app/ui/pnpm-lock.yaml`: tauri, tauri-build, tauri-plugin-positioner, tauri-plugin-notification, tauri-plugin-clipboard-manager, tauri-plugin-opener, tauri-plugin-dialog, tiny-skia, async-trait, tauri-cli; svelte, vite, @sveltejs/vite-plugin-svelte, vitest, svelte-check, typescript, @tauri-apps/api, pnpm.)_

---

## Task 0 — Preconditions + Swift baseline renders (orchestrator)

- [ ] On `feat/rust`, tree clean. P4 status `done`. `git merge master` (merge-hygiene rule). ⚠ `6a72092` (`fix/app-silent`) must be on master and merged in; `grep -c applicationShouldHandleReopen macos/Sources/Lobocode/PanelWindow.swift` → `1`. If `0`: stop, the user merges `fix/app-silent` first.
- [ ] `cargo tauri --version` → `tauri-cli 2.x`. Measured 2026-09-29 on this Mac: `tauri-cli 1.5.11` → too old. Fix: `cargo install tauri-cli --version "^2" --locked`. `pnpm --version`, `node --version` present.
- [ ] Baseline renders from the Swift app (no CLI needed): `cd macos && swift build -c release && .build/release/Lobocode --render ../plans/2026-09-29-rust-rewrite/p5-baseline` → prints `rendered to …`. Expect 10 `panel_*.png`, 10 `menubar_*.png`, `settings.png`, `icon_1024.png`.
- [ ] `cd macos && swift test` → all pass (records the Swift test count for Task 41).
- [ ] Commit: `plans: rust P5 plan + Swift baseline renders`.

## Task 1 — Parity inventory = acceptance checklist (orchestrator)

The table below was built from the Swift source at `6a72092`. Orchestrator re-reads every cited line, fixes any wrong cite, and freezes it. Task 41 ticks every row. Ids are referenced by tasks as `[P#]`.

### Views and states

| Id | State / surface | What the user sees | Source |
|---|---|---|---|
| P1 | loading | `scanning providers` (dim) + blinking cursor | PanelView.swift:27-28 |
| P2 | setup (noConfig) | `no usable config yet` (amber bold 12); config path (dim 10, selectable; fallback `~/.config/lobo/config.env`); Apple Silicon: `needs an api key. cloud also needs a provider key, domain, tunnel token and bucket URL.` else `needs a provider key (RunPod or Vast), domain, tunnel token and bucket URL.`; `[ SETUP ]` amber wide → Settings | PanelView.swift:86-100 |
| P3 | off, target picker | `> target [local] cloud` only when local supported | PanelView.swift:108-111 |
| P4 | off cloud, no keys | row `> cloud  no keys` + `[ SETUP ]` amber wide | PanelView.swift:121-125 |
| P5 | off cloud, ready | provider picker if >1 provider else row `> provider  runpod`; model picker `q8 q6`; row `> limits  ≥100MB/s idle 30m max 12h` (11 pt; empty/0 → defaults 100/30/12); `[ START ]` green wide, Enter = default action | PanelView.swift:126-155 |
| P6 | off local, models loaded | one row per model: `[q6]` green / ` q8 ` dim, `22.1 GB`, state `✓ on disk` (text) / `partial 43%` (cyan, capped 99) / `↓ download` (dim); line `<weights> · 958.9 GB free` (10 pt dim, middle-truncated); click row = pick; `[ START ]` | PanelView.swift:159-201 |
| P7 | off local, models nil | model picker `q8 q6` + `[ START ]` | PanelView.swift:168-170 |
| P8 | booting | one row per boot step: `[ OK ]` green / `[ >> ]` cyan + cursor / `[ .. ]` faint; label (cloud: rent container tunnel gpu download load ready; local: start metal model load ready); step time `m:ss` dim; last detail 9 pt faint, 2 lines, middle-truncated; `T+m:ss` 11 bold cyan; `[ ABORT ]` red | PanelView.swift:203-247, Store.swift:33-61 |
| P9 | booting, download line | under the current download step, indent 56: bar 14 cells copper; `verify sha256` when `verifying`; else `12.4/28.6G` + `713MB/s` (local: text colour; cloud: green ≥100 else amber) + ETA `m:ss` dim if >0 | PanelView.swift:238-267 |
| P10 | booting, verify w/o download | `verify` (10 pt) when `upPhase == verify` or agent stage `verify` | PanelView.swift:241-243 |
| P11 | ready | `endpoint <url> copy`; `api key <masked> copy` (copies the clear key); `copy` → `copied` green for 1.2 s; tiles `gen`/`prompt` value 22 bold green glow + `tok/s` (≥100 → `%.0f`, else `%.1f`, nil `—`); | PanelView.swift:270-354 |
| P12 | ready, memory row | label `memory` (local) / `vram` (cloud), bar 12, `%.1f/%.1f GB` (MB/1024); cloud only `gpu 87%` (green if >0 else dim); row hidden when no GPU data | PanelView.swift:282-291 |
| P13 | ready, bottom row | `idle-stop ` (local) / `idle-kill ` + countdown (cloud <300 s amber); countdown frozen while `requests_processing > 0`, else `kill_in_s − (now − snap.at)`; `  ·  T+<uptime>`; right: local `local · $0`, cloud `$%.2f` spent = cost/h × elapsed; `[ STOP ]` red wide | PanelView.swift:292-322, Store.swift:371-375 |
| P14 | stopping | `[ .. ]` cyan + `stopping llama.cpp` (local) / `deleting pod on <provider>` + cursor | PanelView.swift:37-42 |
| P15 | failed | `[FAIL]` red bold + message; log box: last 5 of log tail, 9 pt faint, middle-truncated, card bg; buttons: pod present → `[ STOP <provider> ]` red, else `[ RETRY ]` green; `[ DISMISS ]` dim | PanelView.swift:356-386 |
| P16 | warning line | `! <poll error>` amber 10 pt, 2 lines, under any state | PanelView.swift:13-15 |
| P17 | header | block logo (gradient); raster bar (sweeps while boot/stop/loading); `sys:` + glitch word (SCAN SETUP OFF BOOT RUN STOP FAIL, phase colour); version right (9 pt faint, middle-truncated); detail line: pod → `provider · detail · $0.69/h` (cost only if >0); no pod + booting → `starting local` / `renting <provider>`; no pod + off → cloud `no pod · $0.00/h`, local empty | PanelView.swift:49-84, Store.swift:14-24 |
| P18 | footer | `settings ⌘,` (⌘,), `config` (reveal file in Finder), `quit ⌘q` (⌘q) | PanelView.swift:388-407 |
| P19 | panel frame | width 340, body padding 14, spacing 12, bg, dark | PanelView.swift:8-23 |
| P20 | menu bar item | icon + text: `lobo` loading, `setup`, `off`, `stop`, `FAIL`; booting: `42%` during download, else step label, else `boot`; ready: `45 t/s` if processing and gen_tps>0, else `27m` (ceil minutes left), else `run` | StatusItem.swift:28-48 |
| P21 | tray icon | 16×16, rounded frame (1.5,1.5,13,13) r3 stroke 1.5; ready = filled inner (inset 3, r1) `#2EE57A`; boot/stop/loading = fill from bottom, height = max(0.12, progress), α0.9, `#00C8E6`; failed = X stroke 1.5 `#FF4D5E`; setup = 3 px dot `#FFB020`; off = frame only, system secondary label colour; non-template | StatusItem.swift:51-99 |
| P22 | boot progress | step index / step count, download adds bytes/total share; ready = 1 | StatusItem.swift:17-26 |
| P23 | panel window | title `lobocode`, transparent titlebar, full-size content, dark, bg `#0B0D10`, centred, reused; shown at launch unless `--background`, and on every reopen (Finder/Spotlight/Launchpad) | PanelWindow.swift:1-43 |
| P24 | settings window | title `lobocode · config`, same style, 520×640, scrolls; opened by footer, SETUP buttons, `--settings` at launch (+0.5 s) | SettingsWindow.swift:1-26, LobocodeApp.swift:8-13, SettingsView.swift:23-31 |
| P25 | settings header | logo + `config` (10 dim) + raster bar active while saving | SettingsView.swift:34-40 |
| P26 | settings file box | path (11, selectable); `plain KEY=value lines, shared with the lobo CLI. edit it by hand any time: this window only touches the keys it shows.`; `reveal in finder`; `open in editor` (disabled when the file does not exist) | SettingsView.swift:84-99 |
| P27 | settings sections | `// providers  (one is enough)`: runpod key, vast key. `// access`: domain (`lobo.example.com`), api key, tunnel token, bucket url (`https://pub-….r2.dev`). `// defaults for lobo up  (empty = built-in)`: provider picker (targets = local if supported + keyed providers; only when >1; default runpod if present else first), model `q8 q6` (def q8), min MB/s `100`, context `65536`, idle min `30`, max hours `12`, runpod cloud `community secure` (def community), vast max $/h `1.20`, pod image `ghcr.io/1905/lobocode@sha256:…`. `// local` (Apple Silicon only): weights, port `8931` | SettingsView.swift:42-71 |
| P28 | secret field | placeholder `not set`, or `<masked>  (empty = keep, - = remove)`; typed value never shown back | SettingsView.swift:139-147 |
| P29 | api key row | masked current or `not set`; after `generate`: `<mask>  (new, unsaved)` amber | SettingsView.swift:149-157 |
| P30 | weights row | text field (placeholder = listing weights or `~/Library/Application Support/lobo/weights`), `[choose…]` folder dialog (can create, start at typed or current folder), `<n> GB free` under it: saved folder → listing's number; typed/chosen → measured on nearest existing parent | SettingsView.swift:159-213 |
| P31 | pickers | `[o]` green / ` o ` dim | SettingsView.swift:215-226, Theme.swift:195-215 |
| P32 | save | no LOBO_API_KEY and no new key → generate one; only changed keys (plain trimmed ≠ current; secret `-` = remove, empty = keep; new api key); none → `nothing changed` dim; validation error red; OK → `saved N key(s)` green, reload config + form, refresh; error → `save failed: <msg>` red; `[ REVERT ]` dim reloads form; `[ SAVE ]` green ⌘S, disabled while saving | SettingsView.swift:228-291 |
| P33 | validation texts | `<KEY>: whole number ≥ <min>, or empty` (MIN_MBPS 1, CTX 512, IDLE_MIN 1, MAX_HOURS 1; `0` allowed); `LOBO_VAST_MAX_DPH: a price like 1.20`; `LOBO_DOMAIN: bare hostname, no https://`; `LOBO_LOCAL_PORT: whole number 1024-65534, or empty` | SettingsView.swift:253-265 |
| P34 | app icon | 1024 canvas, 824 tile r185 continuous, bg fill, line stroke 10; `L` cyan + `C` magenta 5×7 pixel glyphs, cell 52, gap 40, cell inset 5% | Renderer.swift:139-153, PixelGlyph.swift:1-32 |

### Store behaviour

| Id | Behaviour | Source |
|---|---|---|
| S1 | Poll interval: booting/stopping/loading 3 s; ready 5 s panel open / 15 s closed; else 10 s / 30 s | Store.swift:121-127 |
| S2 | Start of app: load config + models once, then poll forever; 1 s clock tick; ask notification permission | Store.swift:102-119 |
| S3 | refresh: config missing or not ready → reload config (no models); still not ready → phase setup, stop. Status error → warning = message; loading → off. OK → warning cleared, apply; `models && down` → re-list models | Store.swift:207-221 |
| S4 | Panel open (either window) → `refresh(models: true)`; closed → panel_open false | LobocodeApp.swift:18-19, PanelWindow.swift:14-15 |
| S5 | derive: failed + down + no up → keep failed; stopping + !down → stopping; down → booting if up running else off; no agent status → booting; stage ready → ready; failed/terminating → booting if up running else failed(stage_detail or stage); else booting | Store.swift:243-254 |
| S6 | apply: download kept from status when stage download/verify; ready → off without user stop → notify `lobo stopped` / `stopped by itself (idle or expiry)`; off/ready clears user_stopped; booting with no boot_start → pod.started_at or now; resumed boot fills all steps ≤ agent stage with now − t0; off/ready with no up → clear boot_start, steps, download | Store.swift:224-241 |
| S7 | start: guard no up running; phase booting; boot_start now; clear steps, download, up_phase, log_tail, ready_url; last_detail `starting llama.cpp…` (local) / `renting <provider>…`; request = provider (`local` or picked), model explicit | Store.swift:256-277, 168-171 |
| S8 | up event: up_phase = phase; first time a step is seen → mark at now − t0; download replaced; detail → last_detail + log tail (keep 6); err → split lines into log tail (keep 8); ready → ready_url, phase ready, notify `lobo ready` / `<url> · <m:ss> · $0.69/h` (or `local`) | Store.swift:279-298 |
| S9 | up ended with error and phase not ready/stopping → failed(last non-empty log line, else error, else `exit`), notify `lobo boot failed` / msg; always refresh(models: true) | Store.swift:300-308 |
| S10 | stop: user_stopped; phase stopping; cancel up (SIGINT) and wait ≤15 s, then kill; down; error → failed(`down: <msg>`); cloud only: wait 10 s, down again (late create); OK → off; could not run → failed(`down did not run: <msg>`); refresh(models: true) | Store.swift:310-338 |
| S11 | dismiss → off, refresh | Store.swift:340-343 |
| S12 | load config: set config; if no up and not booting → provider = default provider, model = default model, auto-pick reset; apply default target | Store.swift:176-189 |
| S13 | load models (local supported only): once per config load, no LOBO_MODEL set, picked model not on disk, another is → pick that one; apply default target | Store.swift:191-203 |
| S14 | default target: not supported → cloud; saved pick; LOBO_PROVIDER=local → local; no provider keys and a model on disk → local; else cloud. Applied only when no up, not booting, not ready | Store.swift:153-166 |
| S15 | target pick saved (`lobo.target` in UserDefaults), previews never read it | Store.swift:99, 144-150 |
| S16 | is_local: live pod provider == local, else target == local | Store.swift:135-138 |
| S17 | endpoint: ready_url; else local → `http://127.0.0.1:<port>/v1`; else `https://<LOBO_DOMAIN>/v1`; else none | Store.swift:354-359 |
| S18 | local port: LOBO_LOCAL_PORT in 1024-65534, else 8931 | Store.swift:361-369 |
| S19 | config readiness: providers = keyed runpod, vast; default provider = LOBO_PROVIDER if keyed else first else runpod; default model q6 only if LOBO_MODEL=q6; cloud_ready = exists + a provider + DOMAIN, API_KEY, TUNNEL, BUCKET set; ready = cloud_ready or (Apple Silicon + exists + API_KEY) | Models.swift:97-124 |
| S20 | model state: size>0 and on_disk≥size → on disk; on_disk>0 → partial fraction; else missing | Models.swift:149-155 |
| S21 | notifications only inside a real .app bundle | Store.swift:379-393 |
| S22 | copy API key reads the clear key (not the masked one) | Store.swift:345-352 |
| S23 | LOBO_APP_CONFIG env = config path override (dev, tests) | CLI.swift:15-21 |
| S24 | `--render <dir>`, `--settings`, `--background` argv | main.swift:3-8, LobocodeApp.swift:9-12, PanelWindow.swift:32-35 |

### Theme tokens (Theme.swift)

| Id | Token | Value | Source |
|---|---|---|---|
| T1 | bg / card / line | `#0B0D10` / `#12161B` / `#1F252D` | Theme.swift:5-7 |
| T2 | text / dim / faint | `#D6DEE8` / `#6B7685` / `#3A424D` | Theme.swift:8-10 |
| T3 | green / cyan / magenta / amber / red | `#39FF88` / `#00E5FF` / `#FF2BD6` / `#FFB020` / `#FF4D5E` | Theme.swift:11-15 |
| T4 | copper gradient | left→right cyan, `#8A7BFF`, magenta | Theme.swift:16 |
| T5 | font | system monospaced; sizes 9, 10, 11, 12, 22; bold where noted | Theme.swift:18-20 |
| T6 | phase colours | ready green; boot/loading/stop cyan; failed red; setup amber; off dim | Theme.swift:22-30 |
| T7 | logo | 3-line block art, 9 pt bold, line spacing −1, gradient | Theme.swift:39-53 |
| T8 | raster bar | 2 px, gradient α0.35; active: white α0.9 slice 25% wide sweeps in 1.6 s; static with reduced motion | Theme.swift:55-78 |
| T9 | scanlines | 1 px white α0.03 every 3 px, header only | Theme.swift:80-92 |
| T10 | glitch text | on change: 9 frames × 30 ms from noise `!<>-_\/[]{}=+*^?#%&01`, spaces kept, settles left→right; off with reduced motion | Theme.swift:94-125 |
| T11 | cursor | `█` 10 pt cyan, blinks 0.5 s; solid with reduced motion | Theme.swift:127-137 |
| T12 | bracket button | `[ LABEL ]` 12 bold; padding 7/10; radius 4; bg colour α0.06 (pressed 0.25); border α0.7; hover: fill colour, text bg, glow r8 α0.45, 120 ms ease-out; disabled faint | Theme.swift:139-173 |
| T13 | link button | 10 pt, colour (default dim), hover → text | Theme.swift:175-193 |
| T14 | picker | `> label` dim, width 84; 12 pt | Theme.swift:195-215 |
| T15 | tile / box | radius 6, card fill, 1 px line stroke; field radius 4, padding 5/8; settings label width 110, form padding 22, spacing 18 | PanelView.swift:339-353, SettingsView.swift:101-119 |
| T16 | header box | padding 14/14/10, card + scanlines, 1 px line bottom; footer padding 14/9, card, 1 px line top | PanelView.swift:66-70, 401-405 |

### Formats

| Id | Format | Source |
|---|---|---|
| F1 | duration: `m:ss`, `h:mm:ss` from 1 h, rounded, never negative | Store.swift:408-411 |
| F2 | GB: bytes/1e9, 1 decimal | Store.swift:413 |
| F3 | bar: `▓`×round(f·w) + `░`, f clamped 0…1, NaN → 0 | Store.swift:415-420 |
| F4 | mask (app): <12 chars `••••`, else first 4 + `…` + last 4 | SettingsView.swift:294-296 |
| F5 | new API key: `sk-` + 48 hex (51 chars) | SettingsView.swift:297-301 |

- [ ] Re-check every cite against `git show 6a72092:<file>`. Fix wrong line numbers here.
- [ ] Commit: `plans: rust P5 parity inventory frozen`.

## Task 2 — Scaffold + pinned versions

**Files:** Create `app/src-tauri/{Cargo.toml,build.rs,tauri.conf.json,Info.plist,capabilities/default.json,src/main.rs,src/lib.rs}`, `app/ui/{package.json,vite.config.ts,tsconfig.json,svelte.config.js,index.html,src/main.ts}`. Modify `Cargo.toml`, `.gitignore`.

- [ ] `cargo new --bin app/src-tauri --name lobocode-app --vcs none`; bin name `lobocode`; workspace member; `[lints] workspace = true`.
- [ ] `cargo add -p lobocode-app tauri --features tray-icon,image-png,macos-private-api`, `tauri-plugin-positioner --features tray-icon`, `tauri-plugin-notification`, `tauri-plugin-clipboard-manager`, `tauri-plugin-opener`, `tauri-plugin-dialog`, `tiny-skia`, `async-trait`, `tokio --features rt-multi-thread,macros,signal,time,sync`, `tokio-util`, `serde --features derive`, `serde_json`, `chrono`, `ts-rs`, `thiserror`, path deps `lobo-proto`, `lobo-core`. `cargo add -p lobocode-app --build tauri-build`. Shared versions go to `[workspace.dependencies]`.
- [ ] `pnpm create vite app/ui --template svelte-ts` equivalent by hand; `pnpm -C app/ui add @tauri-apps/api@^2`; `pnpm -C app/ui add -D svelte@^5 vite @sveltejs/vite-plugin-svelte vitest svelte-check typescript`. Scripts: `dev`, `build` (`vite build`), `test` (`vitest run`), `check` (`svelte-check --fail-on-warnings`).
- [ ] `vite.config.ts`: `server.port 5173`, `strictPort`, `server.fs.allow` includes `../../crates/lobo-proto/fixtures` (vitest reads P1 fixtures).
- [ ] `tauri.conf.json` as in the File map. `Info.plist`: `LSUIElement` true only.
- [ ] `main.rs` / `lib.rs`: `lib::run()` builds an empty Tauri app with the five plugins registered. Nothing else yet.
- [ ] Verify: `pnpm -C app/ui install && pnpm -C app/ui build` → `dist/index.html`. `cargo build -p lobocode-app` → exit 0. `cargo clippy -p lobocode-app -- -D warnings` → clean.
- [ ] Fill "Pinned versions" from the lockfiles. Record `cargo tauri --version` there.
- [ ] Commit: `app: tauri 2 + svelte 5 scaffold`.

## Task 3 — lobo-core: `config::readiness` [S19]

**Files:** Modify `crates/lobo-core/src/config.rs` (+ tests). Needs `Readiness` from Task 8: run Task 8 before Task 3 (numbering groups by crate, not order).

- [ ] Failing tests (port `testReadyNeedsAPIKey` + `ConfigShow` provider tests, LobocodeTests.swift:240-251), temp config files: 4 cloud keys + runpod key, no API key → `ready false`; + API key → `cloud_ready true, ready true`; only API key → `cloud_ready false`, `ready == local_supported`; missing file → `exists false, ready false`; `LOBO_PROVIDER=vast` with only runpod key → `default_provider "runpod"`; `LOBO_PROVIDER=local` → `"local"`; `LOBO_MODEL=q6` → `default_model "q6"`; `LOBO_LOCAL_PORT=80` → `local_port 8931`; bad bucket URL → `cloud_ready false` (CLI rule, see Spec issues #5); unparsable file → `error Some`.
- [ ] Implement per the locked doc comment. `cargo test -p lobo-core readiness` → pass.
- [ ] Commit: `lobo-core: config readiness for the app (CLI rules)`.

## Task 4 — lobo-core: `validate_set`, `new_api_key`, `mask` [P33, F4, F5]

**Files:** Modify `crates/lobo-core/src/config.rs`.

- [ ] Failing tests (port LobocodeTests.swift:211-238, 253-259): `LOBO_CTX=100` → Err; `LOBO_CTX=0` → Ok; `LOBO_DOMAIN=https://x` → Err `LOBO_DOMAIN: bare hostname, no https://`; `LOBO_LOCAL_PORT=80` → Err exactly `LOBO_LOCAL_PORT: whole number 1024-65534, or empty`; `""`, `1024`, `8931`, `65534` → Ok; `1023 65535 0 -1 89.31 port " 8931"` → Err each (note: `0` is Err here although `parse_local_port("0")` is Ok — Swift behaviour, keep it); `LOBO_VAST_MAX_DPH=0` → Err; `new_api_key()` len 51, prefix `sk-`, hex; `mask("")` `(not set)`, `mask("short")` `••••`, `mask("sk-0123456789abcdef")` `sk-0…cdef`.
- [ ] Implement. `cargo test -p lobo-core validate_set new_api_key mask` → pass.
- [ ] Commit: `lobo-core: settings validation, api key generator, mask`.

## Task 5 — lobo-core: `control::resolve_up`

**Files:** Modify `crates/lobo-core/src/control.rs`. If P4 already ported `applyDefaults` into lobo-cli, move it here and make lobo-cli call it (same tests move too).

- [ ] Failing tests (port `cmd/lobo/defaults_test.go` table + `checkTarget`): empty request → provider `default_provider()`, model/ctx/idle/max-life/min-mbps/cloud from `Defaults`; request `model Some("q8")` with `LOBO_MODEL=q6` → `q8`; provider `local` on a non-arm64 host → Err `local mode needs macOS on Apple Silicon…` (use the injected `supported` seam P3 uses); provider `vast` without key → Err `no VASTAI_API_KEY in <path>. Run \`lobo config\` to add it`; provider `x` → Err `--provider: want runpod, vast or local, got "x"`; cloud provider with missing tunnel → Err `config: cloud needs CF_TUNNEL_TOKEN`; bad `LOBO_CTX` in file with `ctx Some(4096)` → Ok (flag overrides a bad key).
- [ ] Implement. `cargo test -p lobo-core resolve_up` → pass.
- [ ] Commit: `lobo-core: resolve_up (flag > config > default) shared by CLI and app`.

## Task 6 — lobo-core: spawner wiring + supervisor identity

**Files:** Modify `crates/lobo-core/src/control.rs`, `crates/lobo-core/src/local/*.rs`.

- [ ] Failing tests: `is_supervisor` on a helper child started as `<exe> --lobo-local-run --boot-id B1 --model q6` → true for `B1`, false for `B2`; `<exe> local run --boot-id B1` → true (unchanged); `<exe> --lobo-local-run-x --boot-id B1` → false (whole arg). `deps_with_spawner(cfg, Spawner{exe:"/x/lobocode", args_prefix:["--lobo-local-run"]})` → the local provider's spawn argv starts with `/x/lobocode --lobo-local-run` (assert through the provider's argv builder, no process started).
- [ ] Implement. `cargo test -p lobo-core supervisor spawner` → pass.
- [ ] Commit: `lobo-core: app spawner and --lobo-local-run supervisor identity`.

## Task 7 — lobo-core: `RunConfig::parse`, `free_bytes_nearest`

**Files:** Modify `crates/lobo-core/src/local/*.rs`.

- [ ] Failing tests: `parse(["--model","q6","--ctx","65536","--idle-min","30","--boot-id","b","--port","8931","--api-port","8932"])` → matching fields; `--config /p` accepted before the flags; unknown flag → Err; missing `--boot-id` → Err. `free_bytes_nearest("/tmp/lobocode-no-such-dir/weights")` → `Some(_)` (port LobocodeTests.swift:237); `"~/"` expands.
- [ ] Implement. If P4's clap `local run` parses its own flags, make it build `RunConfig` through the same field names (no second default table).
- [ ] `cargo test -p lobo-core run_config free_bytes` → pass.
- [ ] Commit: `lobo-core: RunConfig::parse for the app re-exec, free_bytes_nearest`.

## Task 8 — lobo-proto: `UpRequest`, `Readiness`

**Files:** Modify `crates/lobo-proto/src/control.rs`, `crates/lobo-proto/src/config.rs`, `crates/lobo-proto/src/lib.rs` (P1 expected TS list). Create `app/ui/src/proto/{UpRequest,Readiness}.ts` (generated).

- [ ] Failing tests: `UpRequest::default()` serialises all five fields as `null`; `Readiness` JSON round-trip of a hand-written value; P1 `ts_exports_have_no_bigint` expected list + `UpRequest`, `Readiness`.
- [ ] Implement. `make proto-ts && cargo test -p lobo-proto` → pass.
- [ ] Commit: `lobo-proto: UpRequest and Readiness for the app`.

## Task 9 — app types + TS export

**Files:** Create `app/src-tauri/src/types.rs`, `app/ui/src/gen/*.ts`. Modify `lib.rs`.

- [ ] Failing tests: `Phase::Failed{message:"x"}` serialises `{"kind":"failed","message":"x"}`; `Phase::word` table (7 rows, [P17]); `Step::steps(true)` labels `start metal model load ready`; `Step::steps(false)` = all 7; `from_up_phase`: `create→rent`, `image→container`, `boot→container`, `tunnel`, `gpu`, `download→download`, `verify→download`, `load`, `ready`, `warp→None` (port LobocodeTests.swift:87-96); `Step::Gpu.label(false)` = `gpu`.
- [ ] ts-rs with `export_to = "../gen/"`. Test `app_ts_exports`: `app/ui/src/gen/` holds exactly `Phase, Target, Step, StepMark, PanelState, AppError` and no `bigint`.
- [ ] `cargo test -p lobocode-app types app_ts_exports` → pass.
- [ ] Commit: `app: panel state types (ts-rs)`.

## Task 10 — reducer: derive + poll interval [S1, S5]

**Files:** Create `app/src-tauri/src/store.rs`.

- [ ] Failing tests: port `testDerive` (LobocodeTests.swift:47-57) row for row, using `Snap` built in Rust. `poll_interval` table: booting/stopping/loading → 3 s; ready open/closed → 5/15 s; off open/closed → 10/30 s.
- [ ] Implement. `cargo test -p lobocode-app store::derive store::poll` → pass.
- [ ] Commit: `app: store derive + poll interval`.

## Task 11 — reducer: config, models, default target [S12–S16, S20]

- [ ] Failing tests: port `testDefaultTarget` (LobocodeTests.swift:139-153) row for row (fixture: P1 `listing.json` with q6 on disk; "empty" = same with `on_disk 0`). `apply_config` while booting keeps provider/model; while off sets them from `Readiness`. `apply_models` auto-pick: LOBO_MODEL empty, q8 picked, q8 partial, q6 on disk → q6; second `apply_models` without a new `apply_config` → no change; `LOBO_MODEL=q8` set → never auto-picks. `choose(Local)` sets target and marks it saved. `is_local`: pod provider `local` wins over target cloud.
- [ ] Implement. `cargo test -p lobocode-app store::target store::models` → pass.
- [ ] Commit: `app: store config/models/target rules`.

## Task 12 — reducer: menu text, progress, endpoint + shared time cases [P20, P22, S17, S18, F1]

**Files:** Modify `store.rs`. Create `app/src-tauri/src/fmt.rs`, `app/ui/src/fixtures/time_cases.json`.

`time_cases.json` (hand-written; Rust and vitest both read it):
```json
{ "duration": [[0,"0:00"],[72,"1:12"],[8342,"2:19:02"],[-5,"0:00"],[59.6,"1:00"]],
  "kill_left": [{"kill_in_s":1634,"since_snap_s":34,"processing":0,"left_s":1600,"menu":"27m"},
                {"kill_in_s":1634,"since_snap_s":34,"processing":1,"left_s":1634,"menu":"28m"},
                {"kill_in_s":10,"since_snap_s":40,"processing":0,"left_s":0,"menu":"0m"}] }
```
- [ ] Failing tests: `fmt::duration` over `time_cases.duration`. Port `testUpEventsDriveSteps` + `testLocalBoot` menu/progress asserts (LobocodeTests.swift:59-72, 98-125): download 50/100 → `menu_text "50%"`, `boot_progress (4+0.5)/7`; local after `gpu` → `metal`, `0.2`. Ready: processing 1 + gen_tps 45.1 → `45 t/s`; not processing → `kill_left.menu` column (note row 2: processing but gen_tps 0 → minutes, frozen). Menu words for loading/setup/off/stop/failed. Endpoint: port `testLocalEndpointWithoutReadyEvent` (LobocodeTests.swift:127-137) using `Readiness.local_port 9000`.
- [ ] Implement `view(now)` filling the derived fields. `cargo test -p lobocode-app store::menu store::endpoint fmt` → pass.
- [ ] Commit: `app: menu text, boot progress, endpoint; shared time cases`.

## Task 13 — reducer: apply_snap [S6]

- [ ] Failing tests: ready → off with `user_stopped false` returns one `Note{"lobo stopped","stopped by itself (idle or expiry)"}`; after `begin_stop` → no note; booting snap with no boot_start → `boot_start_ms` = pod `started_at`; resumed boot (no up running, agent stage `download`, no steps) → steps rent…download marked, `at_s` = now − t0, container/tunnel included for cloud, skipped for local; off with no up → steps, download, boot_start cleared; status stage `verify` with download → `download` kept.
- [ ] Implement. `cargo test -p lobocode-app store::apply_snap` → pass.
- [ ] Commit: `app: store apply_snap`.

## Task 14 — reducer: begin_up, handle_event, up_ended [S7–S9]

- [ ] Failing tests: `begin_up` local q6 → `UpRequest{provider:Some("local"), model:Some("q6"), ctx/source/cloud None}`; cloud vast q8 → `Some("vast")`, `Some("q8")` (port `upArgs` asserts LobocodeTests.swift:103-108); `last_detail` `starting llama.cpp…` / `renting vast…`; state cleared. `handle_event`: port `testUpEventsDriveSteps`; detail appended, tail kept at 6; err `"a\nb"` adds two lines, tail kept at 8; ready → `ready_url`, phase ready, `Note{"lobo ready","https://x/v1 · 0:00 · $0.70/h"}`; local ready → body ends `· local`. `up_ended(Some("boom"))` while booting with tail `["x",""]` → failed `x` + `Note{"lobo boot failed","x"}`; `up_ended(Some(..))` while ready or stopping → no change; `up_ended(None)` → no change.
- [ ] Implement. `cargo test -p lobocode-app store::up` → pass.
- [ ] Commit: `app: store up events`.

## Task 15 — reducer: stop, dismiss, warning [S10, S11, S3]

- [ ] Failing tests: `begin_stop` returns `is_local` at press time and sets stopping + user_stopped; `stop_failed("down: x")` → failed; `stop_done` → off; `dismiss` → off; `set_warning` round trip; `poll_failed("x")` while loading → off + warning `x`, while ready → stays ready + warning.
- [ ] Implement. `cargo test -p lobocode-app store::stop` → pass.
- [ ] Commit: `app: store stop/dismiss/poll failure`.

## Task 16 — Backend trait + CoreBackend + FakeBackend

**Files:** Create `app/src-tauri/src/backend.rs`.

- [ ] `CoreBackend` maps: `config` → `config::show` + `config::readiness`; `models` → `load_laptop` + `local::list(weights())`; `snapshot` → `load_laptop` → `deps_with_spawner` → `control::snapshot`; `up` → `resolve_up` → `control::up`; `down` → `control::down`; `api_key` → `config::values["LOBO_API_KEY"]`; `save` → `validate_set` (Err → `AppError{kind:"invalid"}`) → `config::save`. Config path: `LOBO_APP_CONFIG` if set, else `config::default_path()` [S23]. Spawner: `Spawner{exe: current_exe(), args_prefix: [SUPERVISOR_ARG]}`. Every `lobo_core::Error` → `AppError{kind: e.kind(), message: e.to_string()}`.
- [ ] `FakeBackend` (cfg(test)): scripted responses, records call names in order (`config`, `models`, `snapshot`, `up`, `down`), `up` returns a channel the test feeds.
- [ ] Failing test for `CoreBackend::save` with a temp config: invalid port → `AppError{kind:"invalid", message:"LOBO_LOCAL_PORT: whole number 1024-65534, or empty"}`, file untouched; valid set → file has the key, comments kept.
- [ ] `cargo test -p lobocode-app backend` → pass.
- [ ] Commit: `app: backend trait over lobo-core + fake`.

## Task 17 — Controller: loops + refresh [S2–S4]

**Files:** Create `app/src-tauri/src/controller.rs`, `app/src-tauri/src/notify.rs`.

- [ ] Failing tests (tokio paused clock, FakeBackend, recording Notifier, `emit` into a Vec): port `testModelsNotListedOnPlainRefresh` (LobocodeTests.swift:188-209): start → calls `config, models, snapshot`; plain `refresh(false)` → `snapshot`; `refresh(true)` with down → `snapshot, models`; `load_config(true)` → `config, models`. Not-ready config → phase setup, no `snapshot` call. Snapshot error → `warning` set, loading → off. Loop wakes after `poll_interval` (advance paused time 3 s in booting → second `snapshot`). `panel_shown(true)` → `refresh(true)` runs at once.
- [ ] Implement. Store mutex never held across `.await` (clippy `await_holding_lock` deny in this module).
- [ ] `cargo test -p lobocode-app controller::refresh` → pass.
- [ ] Commit: `app: controller poll loop and refresh`.

## Task 18 — Controller: start + stop [S7–S10]

- [ ] Failing tests: `start` twice → one `up` call; events fed through the fake channel reach the store in order; channel closes after an event with `err` → failed + one `lobo boot failed` note + `refresh(true)`; ready event → one `lobo ready` note. `stop` while up runs: cancel token fires; fake up task that ignores cancel is dropped after 15 s (paused clock); cloud → `down`, 10 s, `down`; local → one `down`; `down` Err → failed `down: <msg>`; always ends with `refresh(true)`.
- [ ] Implement. `cargo test -p lobocode-app controller::up controller::stop` → pass.
- [ ] Commit: `app: controller start/stop sequence`.

## Task 19 — prefs [S15]

**Files:** Create `app/src-tauri/src/prefs.rs`.

- [ ] Failing tests: missing file → `target None`; save `Local` then load → `Local`; garbage file → `None` (no panic).
- [ ] Implement. Dir = Tauri `app_config_dir()` (= `~/Library/Application Support/io.github.1905.lobocode`). Render/test stores never read it (`Store::new(None, …)`).
- [ ] `cargo test -p lobocode-app prefs` → pass.
- [ ] Commit: `app: saved target pick`.

## Task 20 — Tauri commands

**Files:** Create `app/src-tauri/src/commands.rs`. Modify `lib.rs`.

- [ ] Every command from the Locked interfaces table, thin: call Controller / Backend / plugin, map errors to `AppError`. `copy_api_key` and `copy_text` use `tauri_plugin_clipboard_manager` from Rust. `reveal_config` → opener `reveal_item_in_dir`; `open_config` → opener `open_path` (no-op when the file does not exist); `choose_weights` → dialog `blocking_pick_folder` on a blocking task, start dir = arg `~`-expanded or listing weights. `free_bytes` → `local::free_bytes_nearest`. `gen_api_key` → `config::new_api_key`.
- [ ] Unit tests for the pure mapping helpers only (`AppError::from(lobo_core::Error)` keeps `kind`). Commands themselves are exercised in Task 40.
- [ ] `cargo test -p lobocode-app commands && cargo clippy -p lobocode-app -- -D warnings` → pass.
- [ ] Commit: `app: tauri commands`.

## Task 21 — `--lobo-local-run` supervisor entry

**Files:** Create `app/src-tauri/src/supervisor.rs`. Modify `main.rs`.

- [ ] `main.rs`: if `args[1] == lobo_core::local::SUPERVISOR_ARG` → `std::process::exit(supervisor::main(&args[2..]))` before any Tauri code runs (no window, no tray, no Dock icon). `--render` is NOT handled here (renders come from the UI, Task 33).
- [ ] `supervisor::main`: `RunConfig::parse` error → print `lobocode --lobo-local-run: <err>` to stderr, return 2. Else tokio runtime, cancel on SIGINT/SIGTERM, `local::supervise`, Err → stderr + 1, Ok → 0.
- [ ] Failing test: `supervisor::main(&["--bogus".into()])` → 2. Integration-free.
- [ ] `cargo test -p lobocode-app supervisor` → pass.
- [ ] Commit: `app: --lobo-local-run re-exec runs the local supervisor`.

## Task 22 — Tray icon drawing [P21, D3]

**Files:** Create `app/src-tauri/src/icons.rs`, `app/src-tauri/examples/render_icons.rs`.

- [ ] Failing tests on the 32×32 RGBA output (2× scale of the Swift 16 pt geometry): ready → centre pixel `#2EE57A`; booting progress 0 → bottom inner row filled cyan `#00C8E6`, top inner row empty (0.12 floor); progress 1 → top inner row filled; failed → centre pixel red `#FF4D5E` (X crossing); setup → centre amber `#FFB020`, corner of inner rect empty; off → template flag `true`, frame pixels opaque black, centre empty; all others template `false`.
- [ ] Implement with tiny-skia (anti-aliased stroke 3 px at 2×, radius 6).
- [ ] Example: `cargo run -p lobocode-app --example render_icons -- <dir>` writes `tray_{loading,setup,off,boot_0,boot_50,ready,stop,fail}.png` and `icon_1024.png` (Task 25).
- [ ] `cargo test -p lobocode-app icons` → pass.
- [ ] Commit: `app: tray icon drawing per phase`.

## Task 23 — Tray wiring

**Files:** Create `app/src-tauri/src/tray.rs`. Modify `lib.rs`.

- [ ] `TrayIconBuilder` with id `lobo`, initial icon = loading, `icon_as_template` per `tray_icon()`, title `lobo`, no menu, `show_menu_on_left_click(false)`.
- [ ] `update(app, &PanelState)`: set icon + template flag when phase or rounded progress (2% steps) changed; `set_title(Some(menu_text))` when changed. Called from the `emit` closure.
- [ ] Tray event: first call `tauri_plugin_positioner::on_tray_event(app, &event)`. Left click Up: `panel` visible → hide; else `move_window(Position::TrayCenter)`, show, focus, `controller.panel_shown(true)`.
- [ ] Unit test for the change-detection helper `fn tray_changes(prev: Option<&TrayLook>, next: &TrayLook) -> (bool /*icon*/, bool /*title*/)`.
- [ ] `cargo test -p lobocode-app tray` → pass.
- [ ] Commit: `app: tray item with status title and panel toggle`.

## Task 24 — Render fixtures from the reducer

**Files:** Create `app/src-tauri/tests/fixtures.rs`, `app/ui/src/fixtures/panel_*.json`, `settings.json`.

Each fixture file: `{ "name": …, "now_ms": <fixed>, "state": PanelState }`. `now_ms` = `2026-09-29T12:00:00Z`. Scenarios = `Renderer.swift:40-124` one-to-one (same values, same names), built by driving `Store` methods (not by writing `PanelState` by hand), plus the states the Swift renders missed:

| Name | Source |
|---|---|
| `off`, `boot`, `ready`, `fail`, `setup`, `off_local`, `boot_local`, `verify_local`, `ready_local`, `fail_local` | Renderer.swift:50-123 |
| `loading` [P1], `stopping` [P14 cloud], `stopping_local`, `off_nokeys` [P4], `off_one_provider` [P5 row], `off_local_nomodels` [P7], `boot_verify_sha` [P9 verifying], `fail_pod` [P15 STOP provider], `ready_warning` [P16], `ready_kill_soon` (cloud, 240 s left) | new |

`settings.json` = `{ config: sampleConfig, readiness, models: sampleModels, local_supported: true }` from Renderer.swift:29-38.

- [ ] Test `write_render_fixtures` writes the files; test `render_fixtures_cover_inventory` asserts the name list above and that every `Phase` variant appears at least once.
- [ ] Makefile `app-fixtures: cargo test -p lobocode-app --test fixtures`. Verify: run twice → `git status --porcelain app/ui/src/fixtures` shows no change on the second run.
- [ ] Commit: `app: render fixtures generated from the reducer`.

## Task 25 — App icon [P34]

**Files:** Modify `icons.rs`. Create `app/src-tauri/icons/*` (generated).

- [ ] Failing tests on `app_icon_1024()`: pixel (512, 100) = bg `#0B0D10`; (10, 10) transparent (outside the 824 tile); centre of the first `L` cell = cyan; centre of the first filled `C` cell = magenta; a `.` cell centre = bg. Glyph rows = PixelGlyph.swift:30-31 exactly.
- [ ] Implement (tile r185 approximated as a rounded rect; "continuous" squircle not reproduced — note it in the review page).
- [ ] `make app-icons` = `cargo run -p lobocode-app --example render_icons -- bin/app-renders && cargo tauri icon bin/app-renders/icon_1024.png -o app/src-tauri/icons`. Replaces `macos/make_icns.py` (sips + iconutil).
- [ ] Verify `ls app/src-tauri/icons/icon.icns` exists. `cargo test -p lobocode-app icons::app` → pass.
- [ ] Commit: `app: LC pixel monogram icon via tiny-skia + cargo tauri icon`.

## Task 26 — TS: theme tokens + fmt [T1–T16, F1–F3]

**Files:** Create `app/ui/src/lib/theme.css`, `fmt.ts`, `fmt.test.ts`.

- [ ] `theme.css`: `:root` custom properties `--bg --card --line --text --dim --faint --green --cyan --magenta --amber --red`, `--copper: linear-gradient(90deg, var(--cyan), #8A7BFF, var(--magenta))`, `--mono: ui-monospace, "SF Mono", Menlo, monospace`, `--fs-9 … --fs-22`. `color-scheme: dark`. `@media (prefers-reduced-motion: reduce)` stops every animation. `.render` class (set by the render route) also stops them and shows the settled frame.
- [ ] `fmt.ts`: `duration(s)`, `gb(bytes)`, `bar(frac, width)`, `tps(v?: number)` (≥100 `%.0f`, else `%.1f`, missing `—`), `usd(v)` `$%.2f`.
- [ ] Failing vitest: `duration` over `time_cases.json`; `gb(22082528352)` `22.1`; `bar(0.5,4)` `▓▓░░`; `bar(NaN,2)` `░░`; `tps(503.9)` `504`, `tps(45.1)` `45.1`, `tps()` `—`.
- [ ] `pnpm -C app/ui test fmt` → pass.
- [ ] Commit: `ui: theme tokens and formatters`.

## Task 27 — TS: view mapping [P5–P17]

**Files:** Create `app/ui/src/lib/view.ts`, `view.test.ts`.

Locked exports (pure; `now` passed in):
```ts
headerDetail(s: PanelState): string
limits(values: Record<string,string>): string
modelRows(s: PanelState): { id: string; picked: boolean; size: string; state: 'on'|'partial'|'missing'; pct?: number }[]
localFooter(l: Listing): string
stepRows(s: PanelState): { step: Step; label: string; mark: 'ok'|'cur'|'wait'; at?: string }[]
downloadLine(s: PanelState): null | { kind: 'verify' } | { kind: 'sha'; bar: string } | { kind: 'bytes'; bar: string; gb: string; mbps: string; mbpsTone: 'text'|'green'|'amber'; eta?: string }
bootElapsed(s: PanelState, nowMs: number): string                     // "T+1:12"
ready(s: PanelState, nowMs: number): { endpoint: string; apiKey: string; gen: string; prompt: string;
  mem?: { label: 'memory'|'vram'; bar: string; text: string; gpu?: { text: string; hot: boolean } };
  kill?: { label: string; text: string; warn: boolean }; uptime: string; cost: string }
fail(s: PanelState): { message: string; tail: string[]; primary: { label: string; action: 'stop'|'start'; tone: 'red'|'green' } }
stopping(s: PanelState): string
```
- [ ] Failing vitest, fixtures from `app/ui/src/fixtures/panel_*.json` and P1's `crates/lobo-proto/fixtures/{snap_running,snap_down,listing,listing_empty,config_show}.json`: `headerDetail` boot = `vast · offer 51401937, 18877 Mbps down, California, US · $0.73/h`; off cloud `no pod · $0.00/h`; off local `""`. `limits({})` = `≥100MB/s idle 30m max 12h`; `LOBO_IDLE_MIN "0"` → `30m`. `modelRows(off_local)`: q6 picked on disk, q8 partial 43. `stepRows(boot)`: rent…gpu ok with times, download cur, load/ready wait. `downloadLine(boot)` bytes `12.4/28.6G`, `713MB/s` green, eta present; `boot_local` `88MB/s` tone text; `verify_local` → `verify`; `boot_verify_sha` → `sha`. `ready(ready)`: gen `45.1`, prompt `504`, vram `29316/32607` → `28.6/31.8 GB`, gpu `gpu 87%` hot, kill label `idle-kill ` over `time_cases.kill_left`, cost `$1.60` at 8342 s × $0.69/h; `ready_local`: `memory`, no gpu, `idle-stop `, cost `local · $0`; `ready_kill_soon` warn. `fail(fail_pod)` primary `STOP runpod` red; `fail(fail)` `RETRY` green; tail = last 5.
- [ ] Implement. `pnpm -C app/ui test view` → pass.
- [ ] Commit: `ui: PanelState → view mapping`.

## Task 28 — TS: settings form logic [P27–P32]

**Files:** Create `app/ui/src/lib/settings.ts`, `settings.test.ts`.

Locked exports:
```ts
export const PLAIN_KEYS: string[]      // SettingsView.swift:228-230, same order
export const SECRET_KEYS: string[]     // RUNPOD_API_KEY, VASTAI_API_KEY, CF_TUNNEL_TOKEN
export type Fields = { secrets: Record<string,string>; plain: Record<string,string>; newApiKey?: string }
loadFields(c: ConfigShow | undefined): Fields
changes(f: Fields, current: Record<string,string>): Record<string,string>
secretHint(c: ConfigShow | undefined, key: string): string   // "not set" | "<masked>  (empty = keep, - = remove)"
providerTargets(r: Readiness): { options: string[]; def: string } | null   // null = hidden (≤1 option)
pickerValue(f: Fields, key: string, def: string): string
savedMessage(n: number): string        // "saved 1 key" / "saved 3 keys"
maskNew(key: string): string           // "sk-0…cdef  (new, unsaved)" (F4 rule)
```
- [ ] Failing vitest: port `testSettingsChanges` + weights trim (LobocodeTests.swift:211-236) → exact result maps; `providerTargets` local+runpod+vast → `["local","runpod","vast"]` def `runpod`; local only → `null`; `savedMessage(1)` `saved 1 key`.
- [ ] Implement. Validation is NOT here: `config_save` returns the Rust message (Task 4). `pnpm -C app/ui test settings` → pass.
- [ ] Commit: `ui: settings form diff`.

## Task 29 — TS: api + boot

**Files:** Create `app/ui/src/lib/api.ts`. Modify `main.ts`.

- [ ] `api.ts`: one typed wrapper per command (names = commands table), `onState(cb)` over `listen('lobo://state')`, `inTauri()` = `'__TAURI_INTERNALS__' in window`. Outside Tauri every call throws `not in tauri` (render mode never calls them).
- [ ] `main.ts`: `?view=panel` → Panel, `?view=settings` → Settings, `?view=render` → Render. Panel view: `get_state()` then `onState`; a 1 s `now` ticker (for T+, countdowns); `ResizeObserver` on the root → `getCurrentWindow().setSize(new LogicalSize(340, h))`.
- [ ] `pnpm -C app/ui check && pnpm -C app/ui build` → 0 errors.
- [ ] Commit: `ui: api wrappers and view entry`.

## Task 30 — TS: widgets [T7–T14]

**Files:** Create `app/ui/src/widgets/*.svelte`.

- [ ] `Logo` (art = Theme.swift:41-45 byte-exact, gradient via `background-clip: text`), `RasterBar {active}`, `Scanlines`, `GlitchText {text, color}` (T10 timings), `Cursor`, `BracketButton {label, tone, wide, disabled, onclick}`, `LinkButton {label, tone, onclick, disabled}`, `BracketPicker {label, options, value, onpick}`.
- [ ] Svelte 5 runes only (`$props`, `$state`, `$derived`). No component tests; covered by renders.
- [ ] `pnpm -C app/ui check` → 0 errors, 0 warnings.
- [ ] Commit: `ui: widgets (logo, raster bar, glitch, buttons, picker)`.

## Task 31 — TS: panel header, footer, setup, start [P1–P7, P16–P19]

**Files:** Create `app/ui/src/panel/{Panel,Header,Footer,SetupCard,StartCard,LocalStart}.svelte`.

- [ ] Strings exactly as the inventory. `Panel` switches on `phase.kind`. Enter triggers START in off states (`keydown` on window, only when the START button is shown). ⌘, → `open_settings`, ⌘q → `quit`.
- [ ] Model picker options = `state.catalog_ids` (DEFAULT_MODEL first → `q8 q6`, matches P5/P7).
- [ ] `pnpm -C app/ui check` → clean.
- [ ] Commit: `ui: panel header/footer/setup/start`.

## Task 32 — TS: boot log, ready, fail, stopping [P8–P15]

**Files:** Create `app/ui/src/panel/{BootLog,ReadyCard,FailCard}.svelte`; stopping + loading inline in `Panel`.

- [ ] `copy` → `copied` (green) for 1.2 s per row; endpoint → `copy_text`, key → `copy_api_key`.
- [ ] `pnpm -C app/ui check` → clean.
- [ ] Commit: `ui: boot log, ready card, fail card`.

## Task 33 — TS: settings view + render route [P24–P33, S24]

**Files:** Create `app/ui/src/settings/Settings.svelte`, `app/ui/src/render/Render.svelte`.

- [ ] Settings: on mount `config_show` + `local_models`; weights free: saved folder → listing number; typed → `free_bytes` debounced 300 ms; `generate` → `gen_api_key`; save flow = P32; ⌘S saves. Error from `config_save` → `save failed: <message>` red, except `kind == "invalid"` → message alone red (P33 texts). Width 520, scrolls in the window (640 high).
- [ ] Render route: `?view=render&state=<name>` shows one panel fixture at 340 px wide on the bg, clock frozen at `now_ms`, `.render` class on. `?view=render&state=settings` shows Settings from `settings.json` without calling Tauri. `?view=render&state=menubar_<name>` shows the menu bar strip: bg `#2A2A2E`, padding 3/8, the tray PNG for that phase from `app/ui/public/tray/` (copied there by `make app-render`, Task 35), then `menu_text` in 12 px mono white (Renderer.swift:127-137). `?view=render` alone lists every link.
- [ ] `pnpm -C app/ui check && pnpm -C app/ui build` → clean.
- [ ] Commit: `ui: settings window and render route`.

## Task 34 — Windows, launch, reopen [P23, P24, S4, S24, D2]

**Files:** Create `app/src-tauri/src/windows.rs`. Modify `lib.rs`.

- [ ] `setup`: `set_activation_policy(Accessory)`; build tray; create Controller (emit → `app.emit("lobo://state", s)` + `tray::update`); request notification permission; `spawn_loops`.
- [ ] `panel` window: url `index.html?view=panel`, 340×(content), `decorations(false)`, `resizable(false)`, `always_on_top(true)`, `skip_taskbar(true)`, `visible(false)`, bg `#0B0D10`. `WindowEvent::Focused(false)` → hide + `panel_shown(false)`.
- [ ] `main` window (window mode): url `index.html?view=panel`, title `lobocode`, `title_bar_style(Overlay)`, `hidden_title(false)`, `theme(Dark)`, bg `#0B0D10`, not resizable, centred on first show, reused (`CloseRequested` → `prevent_close` + hide + `panel_shown(false)`), top padding for the 28 px titlebar via `?view=panel&chrome=window`.
- [ ] `settings` window: url `index.html?view=settings`, title `lobocode · config`, 520×640, same style, reused.
- [ ] Launch: unless `--background` in argv → show `main`. `--settings` → show `settings` after 500 ms. `RunEvent::Reopen { .. }` (macOS) → show `main`. ⚠ `RunEvent::Reopen` field names: verify against docs.rs for the pinned tauri version before coding; tauri 2 has `Reopen { has_visible_windows, .. }` (from memory, not verified in this plan).
- [ ] `open_settings` command shows `settings`.
- [ ] Unit test: argv flag parser `fn launch_flags(args: &[String]) -> LaunchFlags { background, settings }`.
- [ ] `cargo test -p lobocode-app windows && cargo clippy -p lobocode-app -- -D warnings` → pass.
- [ ] Commit: `app: panel popover, window on launch/reopen, settings window`.

## Task 35 — Makefile

**Files:** Modify `Makefile`.

Targets (all `mv`-to-trash, never `rm`, like today):
- `app-test: cargo test -p lobocode-app && pnpm -C app/ui test`
- `app-lint: cargo clippy -p lobocode-app --all-targets -- -D warnings && pnpm -C app/ui check`
- `app-icons:` as Task 25.
- `app-fixtures:` as Task 24.
- `app-render: app-icons app-fixtures` then copy `bin/app-renders/tray_*.png` → `app/ui/public/tray/`, then prints `run: pnpm -C app/ui dev, open http://localhost:5173/?view=render`.
- `mac:` `pnpm -C app/ui install --frozen-lockfile && pnpm -C app/ui build && cargo tauri build --bundles app --config '{"version":"$(APP_VERSION)"}'`, then move `target/release/bundle/macos/lobocode.app` to `$(MAC_APP)` (old one to trash first). `APP_VERSION` = `$(VERSION)` without `v` when it is semver-like, else `0.0.0-dev` (tauri needs semver).
- `dmg: mac` — unchanged body (D4).
- `install-mac: mac` — unchanged body.
- `mac` no longer depends on `build-lobo` and no longer copies `lobo` into Resources (spec goal: no CLI in the app).
- [ ] Verify: `make -n mac dmg install-mac` prints the expected commands. The real `make mac` runs in Task 40 (orchestrator).
- [ ] Commit: `make: mac/dmg/install-mac build the Tauri app`.

## Task 36 — CI: rust.yml app job + release dmg job

**Files:** Modify `.github/workflows/rust.yml`, `.github/workflows/release.yml`.

- [ ] `rust.yml` ubuntu jobs: `--workspace --exclude lobocode-app` (tauri needs webkit2gtk on Linux; no Linux app build). New job `app` on `macos-15`: rust toolchain, `pnpm/action-setup@v4`, `actions/setup-node@v4` (node LTS, pnpm cache), `make app-lint app-test`, `make app-fixtures && git diff --exit-code app/ui/src/fixtures app/ui/src/gen`.
- [ ] `release.yml` `dmg` job: drop `setup-go`; add rust toolchain + `Swatinem/rust-cache@v2` + pnpm + node + `cargo install tauri-cli --version "<pinned>" --locked`; `make dmg`; upload step unchanged.
- [ ] `actionlint` on both files if installed, else say it was not run.
- [ ] Commit: `ci: app job on macos-15; release dmg from the Tauri build`.

## Task 37 — Renders + review page (orchestrator)

- [ ] `make app-render`; start `pnpm -C app/ui dev` in the background.
- [ ] Screenshots via the `/playwright-cli` skill (never Playwright MCP): one PNG per render URL at device scale 2, panel 340 px wide, settings 520 px, menu bar strips. Use the WebKit engine if playwright-cli offers it (Tauri renders in WKWebView); else Chromium and say so on the review page. Output `bin/app-renders/ui/<name>.png`.
- [ ] Look at every screenshot yourself. Fix overflow, clipping, wrong font, missing state, then re-shoot.
- [ ] `tools/render_review.py` (Python, per the scripting rule): pairs `p5-baseline/panel_<name>.png` (Swift) with `bin/app-renders/ui/<name>.png` (Tauri) per state, new-only states on their own, tray PNGs, icon; notes the WebKit/Chromium fact and the icon squircle note; writes one HTML page. Publish as a private artifact.
- [ ] `/notify`: "P5 renders ready: Swift vs Tauri per state, review + approve before merge" with the artifact link.
- [ ] Wait for the user's approval. Fix requested changes in new tasks; re-shoot.

## Task 38 — Remove `macos/` (after render approval)

- [ ] `mkdir -p /tmp/trash && mv macos /tmp/trash/macos.$(date +%Y%m%d-%H%M%S) && git add -A`.
- [ ] `grep -rn "macos/" Makefile .github README.md` → no hits (except history in `plans/`).
- [ ] Commit: `app: remove the Swift app (replaced by app/)`.

## Task 39 — README + docs images

**Files:** Modify `README.md`, `docs/img/{menubar_ready,panel_boot,panel_ready,settings}.png`.

- [ ] App section: drop "It runs the same `lobo` CLI"; say it uses the same config file and runs the local model itself. Install text (dmg, Gatekeeper, removable volume) unchanged. Development line: `app/` Tauri + Svelte, `make app-test`. Cold-audience register (no contractions).
- [ ] Copy the approved renders to `docs/img/` (same names).
- [ ] Commit: `readme: Tauri app`.

## Task 40 — Orchestrator app runs (no implementer; free, no cloud)

- [ ] `make install-mac` → `/Applications/lobocode.app` (or `~/Applications`).
- [ ] Launch from Finder: `open /Applications/lobocode.app` → `main` window shows the panel [P23]. Close it, `open` again → it reopens (Reopen) [P23]. Tray item visible with title; click → popover under the item [D2].
- [ ] Tray title width jump check across `off → boot → 42% → 27m` (D3). Screenshot the menu bar. If the user rejects it, add the D3-B task.
- [ ] Setup state with `LOBO_APP_CONFIG=/tmp/lobo-p5-empty.env open …` (missing file) → SETUP card; Settings opens; SAVE creates the file with a generated API key; state leaves setup.
- [ ] Local boot on the real config (free): pick local q6 → START → steps `start metal model load ready`, ready card, endpoint `http://127.0.0.1:8931/v1`. `ps -ww -o command= -p <pid from state file>` shows `…/lobocode --lobo-local-run … --boot-id <id>`. `curl -s -H "Authorization: Bearer $key" http://127.0.0.1:8931/v1/models` → 200 (key read from the config file, never from the OS env). Copy buttons put the endpoint and the clear key on the clipboard.
- [ ] STOP → phase off within ~5 s, supervisor process gone, state file removed. `lobo status` from the P4 CLI agrees (off).
- [ ] Quit the app while local runs → supervisor keeps running (setsid, like the CLI); relaunch → app shows ready from the snapshot.
- [ ] Notifications: ready, stopped-by-itself (set `LOBO_IDLE_MIN=1` locally, wait), boot failed (`LOBO_LOCAL_PORT` pointing at a port in use) [S6, S8, S9].
- [ ] No cloud boot in P5. Cloud paths are covered by the reducer tests with fakes; the live cloud check is P6.
- [ ] `make dmg` → `bin/lobocode.dmg` opens, shows the app + Applications link.

## Task 41 — Phase close (orchestrator)

- [ ] Every inventory row P1–P34, S1–S24, T1–T16, F1–F5 ticked, each with the task or render that proves it. Any row not proven → new task, not a tick.
- [ ] Swift test → Rust/TS home check: every `LobocodeTests.swift` test (17) has a named port (table below). Count matches Task 0.
- [ ] `make rust-lint rust-test app-lint app-test && git status --porcelain` → green, clean.
- [ ] Reconcile spec: P5 "As-built notes" (Rust controller, two windows, D3 title font, contract v1.1 changes).
- [ ] Plan status → `done`. `/notify`: "Rust P5 done on feat/rust: Tauri app, renders approved, local boot OK. Approve P6 plan next."

---

## Swift test → new home

| Swift test (LobocodeTests.swift) | New home |
|---|---|
| testDecodeReadyStatus, testDecodeOffStatus, testDecodeUpEvents, testNanosecondDates | lobo-proto round-trip fixtures (P1) |
| testDerive | store::derive (Task 10) |
| testUpEventsDriveSteps | store::up + store::menu (Tasks 12, 14) |
| testDecodeModels | lobo-proto `listing.json` (P1) + fmt.test `gb` (Task 26) + store model state (Task 11) |
| testLocalSteps | types (Task 9) |
| testLocalBoot | store::up + store::menu (Tasks 12, 14) |
| testLocalEndpointWithoutReadyEvent | store::endpoint (Task 12) |
| testDefaultTarget | store::target (Task 11) |
| testTargetPersists | prefs (Task 19) |
| testModelsNotListedOnPlainRefresh | controller::refresh (Task 17) |
| testSettingsChanges | settings.test (Task 28) + lobo-core validate_set (Task 4) |
| testSettingsLocal | lobo-core validate_set + free_bytes_nearest (Tasks 4, 7) + settings.test (Task 28) |
| testReadyNeedsAPIKey | lobo-core readiness (Task 3) |
| testFormat | fmt.rs / fmt.test (Tasks 12, 26) + new_api_key (Task 4) |

---

## Self-review

- Spec P5 → tasks: menu bar item + status word (Tasks 22, 23); panel states off/boot/ready/fail/setup cloud + local (Tasks 10–15, 27, 31, 32); Settings, all keys, weights picker (Tasks 28, 33, 20 `choose_weights`); notifications (Tasks 14, 17, 18); renders of every state for review (Tasks 24, 33, 37); calls lobo-core directly (Task 16); supervisor re-exec (Tasks 6, 7, 21); Finder/reopen shows the panel (Task 34); positioner TrayCenter (Task 23); ts-rs types (Tasks 8, 9); unsigned dev build + dmg (Tasks 35, 36); vitest mapping (Tasks 26–28); no Windows/Linux (File map "Out of scope").
- Names match contracts.md v1.0: `config_show`, `config_save`, `local_models`, `catalog`, `AppError { kind, message }`, `UpRequest` in lobo-proto, `SUPERVISOR_ARG`, `Spawner { exe, args_prefix }`, `control::up/snapshot/down`, `config::show/save/values/parse_local_port/default_path`, `local::list/supervise/RunConfig`, `Error::kind`. Departures are listed under "Contract additions", not silently used.
- No TODO or placeholder, except "Pinned versions" (filled by Task 2 from lockfiles, same as P1) and one flagged unverified API detail (`RunEvent::Reopen` field names, checked in Task 34 before coding).
- Task sizes: Tasks 1, 24, 27, 34, 37 and 40 are larger than 5 minutes. They are listed as single tasks because splitting them would separate a test from what it tests.

## Contract additions (for contracts.md v1.1)

1. `lobo_proto::{UpRequest, Readiness}` — shapes in "Locked interfaces". `UpRequest` fields are all `Option` (None = config default); contract v1.0 did not say.
2. `lobo_core::config::{readiness, validate_set, new_api_key, mask}`.
3. `lobo_core::control::resolve_up(cfg, cfg_path, req) -> Result<UpOpts>` — `applyDefaults` + `checkTarget` live in `cmd/lobo` today; the app needs them without the CLI. P4 should call this too.
4. `lobo_core::control::deps_with_spawner(cfg, Spawner) -> Result<Deps>` — `deps_from_config` has no way to pass the app's Spawner.
5. `lobo_core::local::is_supervisor` must accept `SUPERVISOR_ARG` as well as `local run` (P3 port of `internal/local/provider.go:268-280`). Without it `down` and `status` cannot see or kill an app-started supervisor.
6. `lobo_core::local::{RunConfig::parse, free_bytes_nearest}`.
7. App command surface: add `get_state, start, stop, dismiss, choose_target, set_provider, set_model, refresh, copy_api_key, copy_text, free_bytes, gen_api_key, choose_weights, open_settings, reveal_config, open_config, quit` and event `lobo://state` (`PanelState`). Drop from the TS surface: `snapshot`, `up` + `lobo://up`, `cancel_up`, `down` (the Rust controller calls them; D1).
8. `AppError` lives in the app crate (`app/src-tauri/src/types.rs`), exported to `app/ui/src/gen/`.

## Spec issues

1. Contract v1.0 streams `up` to TS. That puts the boot state machine in a webview that is hidden most of the time, while the tray and notifications need it. Plan moves it to Rust (D1). Contract change #7.
2. Supervisor identity is argv `local run` (`internal/local/provider.go:268-280`). The spec's `--lobo-local-run` re-exec breaks it unless P3 changes the check (#5). If P3 already shipped, Task 6 changes it here.
3. The spec says "no CLI subprocess", but `applyDefaults`/`checkTarget` and the local run flag parsing are CLI code today. They move to lobo-core (#3, #6). P4 plan must be told.
4. `rust.yml` (P1) runs `cargo test --workspace` on ubuntu. Adding the Tauri crate breaks it (webkit2gtk). Task 36 excludes it and adds a macOS job.
5. Readiness: Swift `cloudReady` checked only that 4 keys are set. The CLI (`RequireCloud`) also rejects a non-URL `LOBO_BUCKET_URL`. The plan uses the CLI rule, so a bad bucket URL now shows the SETUP path instead of a failed boot. Small behaviour change; user to confirm.
6. Target pick was in macOS `UserDefaults` (`lobo.target`). The Tauri app stores it in `prefs.json`. The old pick is not migrated; first launch uses the default-target rule. Say so in the release notes, or add a one-line `defaults read` import (not planned).
7. Tray title font: tauri's `set_title` uses the system font, not monospaced digits (D3). Not measured. Decided at Task 40 with the user.
8. `cargo tauri` on this Mac is 1.5.11 (measured). P1 Task 0 only checks presence; P5 Task 0 installs 2.x.
9. Spec "weights picker" read as the Settings weights-folder row (text + `[choose…]` + free space). The model picker is separate (panel, Settings). If the spec meant a model download picker, that is a new feature and out of scope.
10. Render review adds 10 states the Swift renders never covered (Task 24 list). No new UI, only more frames to review.
11. Screenshots come from a browser, the app renders in WKWebView. WebKit engine if playwright-cli supports it; else Chromium, stated on the review page.


---
<!-- end of plan-p5-v1.0.md -->

# Rust rewrite P6 — cutover Implementation Plan v1.0

**Date:** 2026-09-29
**Status:** draft
**Spec:** ./spec.md (spec status: parked — this plan is written on request; exec still needs spec + plan approval)
**Contracts:** ./contracts.md v1.0
**Phase:** P6 of 6. Starts only when P1–P5 plans are all `done`.

**Goal:** prove the Rust build on RunPod, Vast and local, port the e2e suite, remove every Go and Swift file, get the user's OK on a live report, merge `feat/rust` → master, and ship the first Rust release (brew, pod image, dmg).

**Architecture:** no new runtime code. P6 adds one crate (`crates/lobo-e2e`, the live API suite as `#[ignore]` tests), deletes Go/Swift by `mv` to `/tmp/trash`, and rewires Makefile, CI and docs to cargo only. Live checks run from two places: local mode on this Mac, cloud from the Dell in a throwaway container with a pinned `*.r2.dev` IP (home DNS hijacks `*.r2.dev`). A Go reference binary, built before the removal, drives the old-data checks. One model review of the whole branch runs at the end, then `/simplify`, then the user approves, then merge + release.

**Tech Stack:** Rust (workspace toolchain from P1), `reqwest` blocking client + `serde_json` + `regex` in `lobo-e2e`, the musl cross-build P2 set up for `lobo-agent`, Docker on the Dell only (never on this Mac), `gh`, goreleaser-rust or cargo-dist (whichever P4 picked), Tauri bundle (P5).

> For agentic workers: use superpowers:subagent-driven-development to implement task-by-task. Checkbox syntax for tracking. Tasks marked **(orchestrator)** are never dispatched.

> Implementer scope (verbatim, every dispatch): writes only the code and unit tests its task names and runs that task's focused unit test. Never runs e2e / integration / live / smoke tests, never rents a GPU or pod, never calls a provider API, never publishes, deploys or touches infra, never runs anything money-bearing. Never sets `TEST_DATABASE_URL` or any test-DB env var.

Implementer model: Opus 5.5 (`model: "opus"`). No per-task model review. The orchestrator checks each report and commit (build + focused tests).

---

## File map

**Create**
- `crates/lobo-e2e/Cargo.toml` — `publish = false`, deps: `lobo-core`, `lobo-proto`, `reqwest` (blocking, json, rustls), `serde`, `serde_json`, `regex`, `tokio` (rt only, for `block_on` into async `lobo-core` calls), `tempfile`.
- `crates/lobo-e2e/src/lib.rs` — shared setup `env()`, wire structs, request helpers, tool fixtures, pure parsers (unit-tested, not ignored).
- `crates/lobo-e2e/tests/live.rs` — the 23 live tests, all `#[ignore]`.
- `plans/2026-09-29-rust-rewrite/results.md` — master-port ledger, compat results, live report (redacted: no domain, bucket id, tunnel id, IPs).

**Remove** (`mv` to `/tmp/trash/<name>.<stamp>`, then `git add -A`; never `rm` / `git rm`)
- `cmd/` (lobo, lobo-agent), `internal/` (15 packages), `e2e/`, `go.mod`, `go.sum`, `.golangci.yml`
- `macos/` (Sources, Tests, Package.swift, Info.plist, make_icns.py, …)
- `tools/protofixtures/` (P1 dumper), then `tools/` if empty
- Go/Swift drift tests in Rust: the P1 `release.rs` test that `include_str!`s `internal/release/manifest.go`, and every other `include_str!`/`include_bytes!`/path literal into `internal/`, `cmd/`, `macos/`, `e2e/` (P5's included; found by grep in Task 10)

**Modify**
- `Makefile` — Go/Swift targets out, cargo/Tauri targets under the old names (Task 11).
- `.github/workflows/rust.yml` — drop the Go fixture-drift job and Go path filters.
- `.github/workflows/pod-image.yml`, `.github/workflows/release.yml`, `.goreleaser.yaml` (or cargo-dist config) — no `setup-go`, no Go paths, no `internal/release/manifest.go` sed.
- `.gitignore` — drop `macos/.build/`. `.dockerignore` — drop `macos`, `e2e`; keep `target` ignored.
- `README.md` — install from source, dev commands, layout, menu bar app text.
- `.env.example`, `opencode.json.example` — verify only (grep 2026-09-29: neither mentions Go, Swift or a Go command). Edit only if Rust key names or `gen-api-key` output differ.
- `docs/img/{panel_boot,panel_ready,menubar_ready,settings}.png` — replace the Swift renders with the approved P5 Tauri renders.
- `plans/2026-09-29-rust-rewrite/spec.md` — status, as-built notes, file-table fix (Spec issues 1).
- Project memory `lobo-architecture.md` — Rust layout, e2e command, new version (Task 30).

**Out of scope for P6**
- Any new feature or behaviour change. A live failure is fixed to Go parity, nothing more.
- Code signing / notarization. Deleting `feat/rust` or other branches (ask the user).
- Local performance (5.4 tok/s).

---

## Scratch + safety rules for this phase

- Scratch dir on the Mac: `/tmp/lobo-p6/` (`mkdir -p`, `chmod 700`). Files with config content: `chmod 600`.
- Scratch dir on the Dell: `/tmp/lobocode_p6live/`. Removed at the end by `mv` into the Dell's `/tmp/trash/`.
- Docker: only on the Dell. Every run is `--rm`, named `lobocode_p6live_<purpose>`, with an explicit host-path `-v`. Never on this Mac (project rule: no local docker pulls).
- Rent cheapest only: RunPod `--cloud community` (the default), Vast default sort. Never `--cloud secure` without asking the user.
- Budget cap for all live work in P6: **$5**. Over the cap → stop, `/notify`, wait.
- After any `lobo up` that reached a provider: always run `down` before debugging, even if a later step failed.
- Push over the SSH alias: `git push git@github-1f47e:1905/lobocode.git <ref>` (the HTTPS gh token lacks `workflow` scope, and this phase edits workflows).
- The repo is public: `results.md` and commits never contain the private domain, bucket id, tunnel id, model-server IP or pod SSH hosts. Write `lobo.example.com`, `pub-<id>.r2.dev`.

---

## Task 0 — Preconditions (orchestrator)

- [ ] `plan-p1…p5` status lines all `done`. Any other → stop, ask.
- [ ] `git checkout feat/rust && git status --porcelain` → empty. Dirty → stop, ask (never bare `git stash`).
- [ ] `make rust-lint rust-test` (or the P4/P5 names if already renamed) → exit 0. Record test count.
- [ ] `go build ./... && go test ./...` → still green on the branch (Go is present until Task 10).
- [ ] Record the fork point, before any merge: `FORK=$(git rev-parse "$(git rev-list --first-parent feat/rust --not origin/master | tail -1)^")`, write it into `results.md` header. Expected: a master sha at or after `6a72092`.
- [ ] `ssh dell 'uname -m; docker version --format {{.Server.Version}}; df -h /tmp | tail -1'` → `x86_64`, a version, > 5 GB free.
- [ ] `ls /Volumes/Extreme/_lobocode` → Q6 + Q8 GGUFs present (local checks need the drive). Missing → `/notify` "plug in the Extreme drive", wait.
- [ ] Scored decisions for the user, sent once with `/notify` before Task 18 (the rest can run while waiting):
  - Version of the first Rust release:
    - **A. `v0.2.0` — 8/10.** Minor bump fits a rewrite with the same CLI surface. Loses points: no signal to users that the binary is a new language.
    - **B. `v1.0.0` — 4/10.** Claims stability the Rust build has not earned yet (days of use).
    - **C. `v0.1.1` — 3/10.** A patch number hides a full rewrite.
    - Recommend A.
  - Publish the Rust agent zip before merge (Task 18). This moves `releases/latest.json` for everyone, including master's Go CLI. P2 proved Go CLI → Rust agent works. Rollback in Task 18. Needs a yes.

## Task 1 — Merge master into `feat/rust` (orchestrator)

Merge-hygiene rule: re-sync before the final review, never at merge time.

- [ ] `git fetch origin && git merge origin/master`. Go files still exist on the branch, so Go-side changes land in Go files without delete conflicts.
- [ ] Conflicts: Rust files win for Rust paths. Go paths take master's version (they are removed in Task 10, but Task 2 ports their logic first).
- [ ] `go test ./... && cargo test --workspace` → green. `make proto-fixtures && git diff --exit-code crates/lobo-proto/fixtures crates/lobo-proto/catalog.json` → no diff. A diff = a wire type changed on master → fix the Rust type now, re-run.
- [ ] Commit (merge commit message default). Push `feat/rust`. `gh run watch` → green; red → `gh run view --log-failed`, fix, push, repeat.

## Task 2 — Port master-side Go/Swift changes to Rust

**List (orchestrator):**
```sh
git log --reverse --no-merges --format='%h %s' "$FORK"..origin/master -- cmd internal macos e2e docker .github Makefile .goreleaser.yaml
```
Write every line into `results.md` → "Master port ledger":

| master sha | subject | Rust change | commit | test |
|---|---|---|---|---|

Rules for each row:
- Already ported in an earlier phase merge → cite the Rust commit + the test that covers it. No new work.
- Pure Go/Swift housekeeping with no behaviour (lint, comments, Go-only build flags) → `n/a` + one-line reason.
- Everything else → one implementer task below.

**Per-row implementer task** (2–5 min each, one commit each):
- [ ] Read the master diff: `git show <sha> -- cmd internal macos e2e`.
- [ ] If the commit added or changed a Go test: port that test first into the Rust home from the P1 "Go test → Rust home" table. Run it → it fails.
- [ ] Port the behaviour into the matching crate (`lobo-agent`, `lobo-core`, `lobo-cli`, `app/`). Run the focused test → it passes.
- [ ] Commit: `port: <master sha> <subject>`.
- [ ] Orchestrator: fill the ledger row. `cargo test --workspace` → green.

Done when every ledger row has a Rust commit, a prior-phase citation or `n/a`.

## Task 3 — Go reference binary + baselines (orchestrator)

Build before Task 10 removes the source. Used by Tasks 14, 15 and 21.

- [ ] `mkdir -p /tmp/lobo-p6 && chmod 700 /tmp/lobo-p6`
- [ ] `go build -o /tmp/lobo-p6/lobo-go ./cmd/lobo` → `/tmp/lobo-p6/lobo-go version` prints a version.
- [ ] `/tmp/lobo-p6/lobo-go config show --json > /tmp/lobo-p6/go-config-show.json` (values are masked by the command). `chmod 600`.
- [ ] `/tmp/lobo-p6/lobo-go models --json > /tmp/lobo-p6/go-models.json` → lists Q6 + Q8 with `verified: true` (markers on the Extreme drive).
- [ ] Record the last Go agent release (needed by Task 21):
  ```sh
  R2HOST=$(sed -n 's#^LOBO_BUCKET_URL=https://\([^/]*\).*#\1#p' ~/.config/lobo/config.env)
  curl -s --resolve "$R2HOST:443:104.18.54.45" "https://$R2HOST/releases/latest.json" | jq -r .manifest.version
  ```
  → a version string. Store it as `GO_REL` in `/tmp/lobo-p6/vars` and in `results.md`. The pinned IP is the Dell workaround from the spec; if curl fails, the IP is stale → `dig +short @1.1.1.1 $R2HOST` from a non-home network or ask the user.

## Task 4 — `lobo-e2e` crate + shared helpers

**Files:** Create `crates/lobo-e2e/Cargo.toml`, `crates/lobo-e2e/src/lib.rs`.

Where the endpoint and key come from (same as the Go suite): the Go `TestMain` calls `config.LoadLaptop("../.env")` — the repo `.env`, read as a file. It never reads the OS env for keys. ✓ The Rust port keeps that:
- Config path: `LOBO_E2E_CONFIG` if set (a **path**, needed when the test binary runs on the Dell), else `<workspace>/.env` (`concat!(env!("CARGO_MANIFEST_DIR"), "/../../.env")`).
- Keys, domain, bucket URL: only from that file via `lobo_core::config::load_laptop`. Never `std::env::var` for a key.

Locked interface:
```rust
pub struct Env { pub cfg: lobo_core::config::Laptop, pub root: String /* https://<domain> */, pub base: String /* root + "/v1" */,
                 pub alias: String, pub st0: lobo_proto::Status, pub hc: reqwest::blocking::Client /* 5 min timeout */ }
pub fn env() -> &'static Env;       // OnceLock. First call: load config, HttpAgent status (15 s), must be Stage::Ready,
                                    // alias = catalog::get(st0.model).alias. Any failure → panic!("e2e: pod not ready (run `lobo up`): …")
pub fn config_path_from(var: Option<std::ffi::OsString>) -> PathBuf;   // pure; env() calls it with std::env::var_os("LOBO_E2E_CONFIG")
pub fn block_on<F: Future>(f: F) -> F::Output;                          // current_thread runtime, for lobo-core async fns
#[derive(Serialize, Deserialize, Default, Clone)] pub struct Msg { role, content (skip if empty), tool_calls: Vec<ToolCall> (skip if empty), tool_call_id (skip if empty) }
#[derive(Serialize, Deserialize, Default, Clone)] pub struct ToolCall { id, r#type: String /* rename "type" */, function: Function { name, arguments } }
#[derive(Deserialize)] pub struct ChatResp { id, model, choices: Vec<Choice { finish_reason, message: Msg }>, usage: Usage { prompt_tokens, completion_tokens, total_tokens } }
pub fn req(method: reqwest::Method, path: &str, key: &str, body: Option<&serde_json::Value>) -> (u16, Vec<u8>); // "" key = no Authorization header
pub fn chat(body: serde_json::Value) -> ChatResp;   // sets "model" = alias, POST /v1/chat/completions with the key, panics unless 200 + ≥1 choice
pub fn user(s: &str) -> Vec<Msg>;
pub fn weather_tool() -> serde_json::Value;  pub fn read_file_tool() -> serde_json::Value;   // byte-for-byte the Go maps
pub fn sse_data(line: &str) -> Option<&str>;  // strip "data: " prefix, else None
pub fn go_block(text: &str) -> Option<String>; // regex (?s)```(?:go)?\s*\n(.*?)```
```
Deviation from Go, on purpose: `HttpAgent::new` takes a base URL in the contracts, the Go `NewHTTPAgent` took a domain. `env()` passes `root`. If P3 shipped a domain signature, follow P3.

- [ ] Failing unit tests (not ignored) in `lib.rs`: `config_path_from(None)` ends with `/.env` and not with `crates/.env`; `config_path_from(Some("/x/c.env"))` = `/x/c.env`; `sse_data("data: [DONE]") == Some("[DONE]")`, `sse_data(": ping") == None`; `go_block` finds the body in a ```` ```go ```` block and in a bare ```` ``` ```` block, returns `None` for no block; `weather_tool()` JSON equals the Go literal (write the expected JSON inline).
- [ ] Implement. `cargo test -p lobo-e2e --lib` → pass. `cargo clippy -p lobo-e2e --all-targets -- -D warnings` → clean.
- [ ] Commit: `lobo-e2e: crate + shared setup (config file only, never OS env keys)`.

## Task 5 — Pod API + auth tests

**Files:** Create `crates/lobo-e2e/tests/live.rs`.

Every test: `#[test] #[ignore = "live: needs a running pod, see make e2e"]`, starts with `let e = env();`.

- [ ] Port, same assertions and log lines (`println!` in place of `t.Logf`):
  - `pod_api_version_matches_latest_release` — GET `/api/version` no key → 200, `Manifest` with non-empty `version` + `git_sha`; `block_on(release::resolve(.., &e.cfg.bucket_url, ""))` → only a note if versions differ.
  - `pod_api_status` — `st0.gpu` name contains `5090`; `llama` and `host` present; `kill_in_s > 0`; `expires_at` in the future.
  - `pod_api_logs_need_key` — `/api/logs` no key → 401; `/api/logs?n=20` with key → 200 and body contains `llama`.
  - `llm_rejects_missing_or_wrong_key` — `""` and `"sk-wrong"` on chat → 401; `/v1/models` no key → 401. Collect all failures, then panic once (Go used `t.Errorf`).
  - `models` — `/v1/models` with key → 200, body contains `"<alias>"`.
- [ ] `cargo test -p lobo-e2e --no-run` → compiles. `cargo test -p lobo-e2e` → `5 ignored`.
- [ ] Commit: `lobo-e2e: pod API + auth tests`.

## Task 6 — Chat tests

**Files:** Modify `crates/lobo-e2e/tests/live.rs`.

- [ ] Port: `chat_basic` (391, finish `stop`, usage > 0), `chat_system_prompt_followed` (contains `HELLO`, all caps), `chat_max_tokens_truncates` (finish `length`, completion ≤ 16), `chat_stop_sequence` (no `6`), `chat_no_thinking_leak` (no `<think>`), `chat_streamed` (`block_on(checks::chat(..))`, ≥ 20 chars), `stream_chunks_and_done` (content-type `text/event-stream`, ≥ 3 chunks, `[DONE]` seen, log first-chunk time; read the blocking body with `BufReader::lines()` + `sse_data`), `json_mode` (`response_format json_object`, parse `{name, age}`, age 36, name contains `Ada`).
- [ ] Put `chat_basic`'s body in `fn assert_chat_basic()` so `over_context_fails_cleanly` can call it (Go called `TestChatBasic(t)`).
- [ ] `cargo test -p lobo-e2e --no-run` → compiles. `cargo test -p lobo-e2e` → `13 ignored`.
- [ ] Commit: `lobo-e2e: chat tests`.

## Task 7 — Tool-call tests

**Files:** Modify `crates/lobo-e2e/tests/live.rs`.

- [ ] Port: `tool_call_basic_arguments_is_string` (`checks::tool_call` then `checks::validate_tool_call`), `tool_call_picks_right_tool_and_args` (`read_file`, `path` contains `go.mod`, id non-empty, type `function`), `tool_call_not_forced` (no tool calls, `Paris`), `tool_call_round_trip` (assistant msg with tool_calls + tool msg `{"location":"Bali","temp_c":31,"condition":"thunderstorm"}` → finish `stop`, lowercase answer has `31` and `thunder`), `tool_call_streamed` (each `delta.tool_calls[].function.arguments` fragment must be a JSON **string** — decode as `serde_json::Value`, panic if not `Value::String`; concatenated args parse, `location` contains `Tokyo`, name `get_weather`, finish `tool_calls`; lines up to 1 MiB).
- [ ] `cargo test -p lobo-e2e` → `18 ignored`.
- [ ] Commit: `lobo-e2e: tool-call tests`.

## Task 8 — Code, context and load tests

**Files:** Modify `crates/lobo-e2e/tests/live.rs`.

- [ ] Port:
  - `generates_compiling_go` — skip (print + return) if `go` is not on PATH. Same prompt, `go_block`, write `main.go` + `go.mod` (`module x\n\ngo 1.22\n`) into a `tempfile::TempDir`. Run `go vet .` and `go build -o /dev/null .`, compile only, never execute. Child env: `env_clear()`, then `PATH`, `HOME`, `GOCACHE` (under the temp dir), `CGO_ENABLED=0`, `GOPROXY=off`, `GOFLAGS=-mod=mod`, `GOTOOLCHAIN=local`. (Go passed the whole OS env to the child; the port passes no secrets.) Assert `func Reverse(` and `[]rune`.
  - `long_prompt_near_context` — 400 filler lines, needle after line 217, answer contains `PELICAN-42`.
  - `over_context_fails_cleanly` — `"word "` × `st0.ctx * 2`; 200 → fail; `code >= 500 && code != 500` → fail; then `assert_chat_basic()`.
  - `concurrent_requests_queue` — 3 requests in `std::thread::scope`, all 200.
  - `zz_metrics_counters_moved` — fresh status; `prompt_tokens_total` and `gen_tokens_total` both greater than in `st0`; `idle_s ≤ 60`; skip if llama metrics are missing (Go `t.Skipf`).
- [ ] Order: libtest sorts test names; `make e2e` runs `--test-threads=1`, so `zz_…` runs last, as in Go. Doc comment on `zz_metrics_counters_moved` says so.
- [ ] `cargo test -p lobo-e2e` → `23 ignored`, lib tests pass. `cargo test -p lobo-e2e -- --ignored --list | tail -3` → last line `zz_metrics_counters_moved: test`.
- [ ] Commit: `lobo-e2e: code, context, load and metrics tests`.

### Go → Rust e2e map (all 23 + setup)

| Go (`e2e/e2e_test.go`) | Rust (`crates/lobo-e2e`) | Task |
|---|---|---|
| `TestMain` (config `../.env`, status must be ready, alias from catalog) | `lib.rs::env()` (OnceLock, panics if not ready) | 4 |
| helpers `do`, `chat`, `user`, `weatherTool`, `readFileTool`, `goBlock` | `req`, `chat`, `user`, `weather_tool`, `read_file_tool`, `go_block` | 4 |
| `TestPodAPIVersionMatchesLatestRelease` | `pod_api_version_matches_latest_release` | 5 |
| `TestPodAPIStatus` | `pod_api_status` | 5 |
| `TestPodAPILogsNeedKey` | `pod_api_logs_need_key` | 5 |
| `TestLLMRejectsMissingOrWrongKey` | `llm_rejects_missing_or_wrong_key` | 5 |
| `TestModels` | `models` | 5 |
| `TestChatBasic` | `chat_basic` (+ `assert_chat_basic`) | 6 |
| `TestChatSystemPromptFollowed` | `chat_system_prompt_followed` | 6 |
| `TestChatMaxTokensTruncates` | `chat_max_tokens_truncates` | 6 |
| `TestChatStopSequence` | `chat_stop_sequence` | 6 |
| `TestChatNoThinkingLeak` | `chat_no_thinking_leak` | 6 |
| `TestChatStreamed` | `chat_streamed` | 6 |
| `TestStreamChunksAndDone` | `stream_chunks_and_done` | 6 |
| `TestJSONMode` | `json_mode` | 6 |
| `TestToolCallBasicArgumentsIsString` | `tool_call_basic_arguments_is_string` | 7 |
| `TestToolCallPicksRightToolAndArgs` | `tool_call_picks_right_tool_and_args` | 7 |
| `TestToolCallNotForced` | `tool_call_not_forced` | 7 |
| `TestToolCallRoundTrip` | `tool_call_round_trip` | 7 |
| `TestToolCallStreamed` | `tool_call_streamed` | 7 |
| `TestGeneratesCompilingGo` | `generates_compiling_go` | 8 |
| `TestLongPromptNearContext` | `long_prompt_near_context` | 8 |
| `TestOverContextFailsCleanly` | `over_context_fails_cleanly` | 8 |
| `TestConcurrentRequestsQueue` | `concurrent_requests_queue` | 8 |
| `TestZZMetricsCountersMoved` | `zz_metrics_counters_moved` | 8 |

## Task 9 — `make e2e` (implementer)

**Files:** Modify `Makefile`.

- [ ] `e2e:` → `cargo test -p lobo-e2e --release -- --ignored --test-threads=1 --nocapture`. Comment: `# Live API suite: needs a running pod (lobo up). Reads the repo .env, or LOBO_E2E_CONFIG=<path>.`
- [ ] Verify: `make -n e2e` prints that command. Do not run it (live).
- [ ] Commit: `make e2e runs the Rust live suite`.

## Task 10 — Remove Go + Swift

**Files:** Remove the "Remove" list. Modify the drift-test files found below.

- [ ] Find Rust code that reads Go/Swift paths: `grep -rnE 'include_(str|bytes)!|"\.\./' crates app/src-tauri | grep -E 'internal/|cmd/|macos/|e2e/|\.go"|\.swift"'` → delete each test that uses one (P1 `release.rs` manifest.go guard, P5 guards). Keep the fixtures in `crates/lobo-proto/fixtures/`: they are now frozen Go-era golden files, the old-data compatibility record.
- [ ] Find non-Rust references into Go/Swift: `grep -rnE 'macos/|internal/|cmd/lobo|tools/protofixtures|go\.mod' --exclude-dir=plans --exclude-dir=.git --exclude-dir=target --exclude-dir=node_modules --exclude-dir=cmd --exclude-dir=internal --exclude-dir=macos --exclude-dir=e2e .` → list; each hit is fixed in Tasks 11–13 or here.
- [ ] Move (one command per path, same stamp):
  ```sh
  S=$(date +%Y%m%d-%H%M%S); mkdir -p /tmp/trash
  mv cmd /tmp/trash/lobocode-cmd.$S
  mv internal /tmp/trash/lobocode-internal.$S
  mv e2e /tmp/trash/lobocode-e2e.$S
  mv go.mod /tmp/trash/lobocode-go.mod.$S
  mv go.sum /tmp/trash/lobocode-go.sum.$S
  mv .golangci.yml /tmp/trash/lobocode-golangci.yml.$S
  mv macos /tmp/trash/lobocode-macos.$S
  mv tools/protofixtures /tmp/trash/lobocode-protofixtures.$S
  ls -A tools        # empty → mv tools /tmp/trash/lobocode-tools.$S ; not empty → leave it
  git add -A
  ```
  No `rm`, `rmdir` or `git rm` (user rule).
- [ ] `git status --short | grep -v '^D ' ` → only the drift-test edits. `git status --short | grep -c '^D '` → the Go/Swift file count (record it).
- [ ] `cargo build --workspace && cargo test --workspace` → green.
- [ ] `grep -rIl --exclude-dir=plans --exclude-dir=.git --exclude-dir=target --exclude-dir=node_modules -i 'go build\|go test\|go run\|swift build\|golangci' .` → only `Makefile`, `README.md`, workflow files that Tasks 11–13 fix next, plus `crates/lobo-e2e` (`go vet`/`go build` in `generates_compiling_go` is intended).
- [ ] Commit: `rust cutover: remove Go (cmd, internal, e2e, go.mod) and Swift (macos); drop drift tests`.

## Task 11 — Makefile, cargo only

**Files:** Modify `Makefile`.

Target names stay; bodies change. Remove `build-lobo`, `build-agent` (Go), `mac` (Swift), `proto-fixtures`, the `AGENT_ENV` and Go `LDFLAGS` vars.

| Target | Body |
|---|---|
| `build` | `cargo build --workspace --release` + the musl `lobo-agent` build from P2 |
| `build-agent` | the P2 musl build of `lobo-agent` (keep P2's command) |
| `install` | build `lobo`, install to `$(PREFIX)/bin`, same PATH warning and `.env` copy as today |
| `lint` | `cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings` (+ the P5 UI lint if P5 added one) |
| `test` | `cargo test --workspace` (+ P5 `vitest run` if P5 added it) |
| `release` | build `lobo` + agent, then `lobo release $(DEV_CONFIG)` |
| `e2e` | Task 9 |
| `mac` / `dmg` / `install-mac` | P5's `cargo tauri build` flow; `install-mac` keeps the `/Applications`-else-`~/Applications` rule and the `mv` to `/tmp/trash` of the old app |
| `proto-ts` | keep from P1 |

Drop the `rust-*` aliases from P1; CI (Task 12) calls the plain names.

- [ ] Edit. Verify: `make -n build test lint install e2e release dmg install-mac` → no `go `, `swift`, `golangci`, `macos/` in the output.
- [ ] `make lint test` → exit 0.
- [ ] Commit: `Makefile: cargo and Tauri only`.

## Task 12 — CI + release config, no Go

**Files:** Modify `.github/workflows/rust.yml`, `.github/workflows/pod-image.yml`, `.github/workflows/release.yml`, `.goreleaser.yaml` (or the cargo-dist config P4 chose).

- [ ] `rust.yml`: delete job step 4 (`setup-go` + `make proto-fixtures` diff). Path filter: drop `internal/**`, `cmd/**`, `tools/protofixtures/**`; add `app/**`. Steps call `make lint`, `make test`, `make proto-ts && git diff --exit-code app/ui/src/proto`.
- [ ] `pod-image.yml`: paths → `crates/**`, `Cargo.toml`, `Cargo.lock`, `docker/pod/**`, the workflow file. The `llama=` line reads `DEFAULT_LLAMA_IMAGE` from `crates/lobo-proto/src/release.rs` (P2 should have done this; verify `grep -n manifest.go .github/workflows/pod-image.yml` → nothing).
- [ ] `release.yml`: no `actions/setup-go`. `release` job builds Rust with the P4 tool. `dmg` job: Rust toolchain + Node/pnpm for the P5 UI, `make dmg`, uploads the dmg under the same name `lobocode.dmg`.
- [ ] `.goreleaser.yaml` (if P4 kept goreleaser): `builds` use the rust builder, `binary: lobo`, darwin + linux, amd64 + arm64, same archive names `lobo_{{ .Os }}_{{ .Arch }}`, same `brews` block (tap `1905/homebrew-tap`, key `HOMEBREW_TAP_KEY`, test `lobo version`). Description text unchanged.
- [ ] `docker/pod/Dockerfile`: `grep -n golang docker/pod/Dockerfile` → nothing (P2 made it Rust).
- [ ] `actionlint .github/workflows/*.yml` if installed, else say it was not run.
- [ ] Orchestrator: release dry run, no publish: `goreleaser release --snapshot --clean` (or `dist build` for cargo-dist) → four archives in `dist/`; `tar -xzf dist/lobo_darwin_arm64.tar.gz -C /tmp/lobo-p6/snap && /tmp/lobo-p6/snap/lobo version` → prints the snapshot version.
- [ ] Commit: `ci: Rust-only workflows and release config`. Push. `gh run watch` → rust.yml + pod-image.yml green (pod image `sha-<HEAD>` built; note its digest from the run summary for Task 20).

## Task 13 — README, examples, screenshots

**Files:** Modify `README.md`, `docs/img/*.png`; verify `.env.example`, `opencode.json.example`.

- [ ] README "From source": `(Go 1.26+)` → `(Rust, toolchain pinned by rust-toolchain.toml; Node + pnpm for the app)`. `make install` / `make install-mac` lines stay.
- [ ] README "Menu bar app": "It runs the same `lobo` CLI and uses the same config file." → "It links the same core library as the CLI and uses the same config file."
- [ ] README "Development": `make test` · `make lint` · `make e2e` (live API suite, needs `lobo up`; `LOBO_E2E_CONFIG=<path>` to point at another config) · `make release`. Layout: `crates/lobo-proto` wire types, `crates/lobo-agent` pod agent, `crates/lobo-core` config/providers/control, `crates/lobo-cli` the `lobo` binary, `crates/lobo-e2e` live suite, `app/` Tauri menu bar app, `plans/` specs and measured results.
- [ ] `.env.example`: diff its key list against `lobo_core::config::Laptop` env names: `grep -o '^[A-Z0-9_]*=' .env.example | sort` vs the keys in `crates/lobo-core/src/config*.rs`. Same set → no edit.
- [ ] `opencode.json.example`: run `target/release/lobo gen-api-key --help` and compare the provider block shape (`npm`, `options.baseURL`, `apiKey: {env:LOBO_API_KEY}`, model ids = catalog aliases). Same → no edit.
- [ ] Orchestrator: copy the P5 approved renders into `docs/img/` under the four existing names. Look at each with Read before commit. A missing state → `/notify` the user, keep the old PNG for that one.
- [ ] Commit: `docs: README for the Rust build; Tauri screenshots`.

## Task 14 — Old config compat (orchestrator, free, offline)

The Rust binary must read what Go wrote. The real config file is read only; writes go to a copy.

- [ ] `cargo build --release -p lobo-cli` → `target/release/lobo`. Always call this path in Tasks 14–16 (`~/.local/bin/lobo` is still the Go build).
- [ ] `target/release/lobo config path` → `~/.config/lobo/config.env` (same as `lobo-go config path`).
- [ ] `target/release/lobo config show --json > /tmp/lobo-p6/rust-config-show.json && diff <(jq -S . /tmp/lobo-p6/go-config-show.json) <(jq -S . /tmp/lobo-p6/rust-config-show.json)` → no diff.
- [ ] Write round trip on a copy:
  ```sh
  cp -p ~/.config/lobo/config.env /tmp/lobo-p6/cfg-copy.env
  target/release/lobo --config /tmp/lobo-p6/cfg-copy.env config set LOBO_IDLE_MIN=29
  diff ~/.config/lobo/config.env /tmp/lobo-p6/cfg-copy.env; stat -f %Lp /tmp/lobo-p6/cfg-copy.env
  /tmp/lobo-p6/lobo-go --config /tmp/lobo-p6/cfg-copy.env config get LOBO_IDLE_MIN
  ```
  → diff shows only the `LOBO_IDLE_MIN` line; comments and order unchanged; mode `600`; Go reads `29`.
- [ ] `target/release/lobo models --json > /tmp/lobo-p6/rust-models.json && diff <(jq -S . /tmp/lobo-p6/go-models.json) <(jq -S . /tmp/lobo-p6/rust-models.json)` → no diff (Go-written markers read as verified; `free_bytes` may differ by a few bytes written since → compare with `jq 'del(.free_bytes)'` if so).
- [ ] Record ✓/✗ per line in `results.md` → "Compat".

## Task 15 — Old local state compat (orchestrator, local, free)

- [ ] Stale state: a Go-shaped file with a dead pid.
  ```sh
  mkdir -p /tmp/lobo-p6/state/lobo
  jq '.pid = 999999' crates/lobo-proto/fixtures/local_state.json > /tmp/lobo-p6/state/lobo/local.json
  XDG_STATE_HOME=/tmp/lobo-p6/state target/release/lobo status --json | jq .down
  ls /tmp/lobo-p6/state/lobo/local.json
  ```
  → `true`; the file is gone (dead pid = crashed run, removed by the program as in Go). (`XDG_STATE_HOME` is a path, not a key; allowed.)
- [ ] Go-started local run, Rust controls it:
  - `/tmp/lobo-p6/lobo-go up --provider local --plain` → ends with `url=http://127.0.0.1:8931/v1`.
  - `target/release/lobo status --json | jq '{down, provider: .pod.provider, boot: .status.boot_id, stage: .status.stage}'` → `down: false`, `local`, the boot id in `~/.local/state/lobo/local.json`, `ready`.
  - `target/release/lobo test` → `✓ streamed chat` then `✓ tool call: arguments is a JSON string`.
  - `target/release/lobo logs -n 20` → contains `llama`.
  - `target/release/lobo up --provider local --plain; echo $?` → refuses (already running), exit ≠ 0, the Go run keeps serving.
  - `target/release/lobo down` → `down: no lobo pods left`. Then `pgrep -fl 'local run'` → nothing; `lsof -nP -iTCP:8931 -iTCP:8932 -sTCP:LISTEN` → nothing; `ls ~/.local/state/lobo/local.json` → gone.
- [ ] Record in `results.md` → "Compat".

## Task 16 — Local live, Rust end to end (orchestrator, free)

- [ ] `target/release/lobo up --provider local --plain` → `url=http://127.0.0.1:8931/v1`. Record boot seconds (`boot=` field).
- [ ] `target/release/lobo status --json | jq '.status.stage, .status.llama.gen_tps'` → `"ready"`, a number.
- [ ] `target/release/lobo test` → both ✓ lines. Then `status --json` again → `gen_tps` (expected about 5, spec measured 5.4).
- [ ] `target/release/lobo logs -n 20` → contains `llama`.
- [ ] App reads a CLI-started run: `open bin/lobocode.app` (the P5 build) → `screencapture -x /tmp/lobo-p6/app-local-ready.png` → Read it: panel shows ready, local endpoint, tok/s. Press nothing.
- [ ] `target/release/lobo down` → no process on 8931/8932, state file gone.
- [ ] `tail -1 boots.jsonl | jq .provider` → `"local"` (the Rust CLI appends to the existing Go-era file; `jq -c . boots.jsonl >/dev/null` → every line still parses).
- [ ] Row in the live report.

## Task 17 — Dell runner (orchestrator)

Cloud checks run from the Dell because the home network DNS-hijacks `*.r2.dev` (spec carry-over rule; pinned Cloudflare IP `104.18.54.45`, from project memory, re-checked in Task 3).

- [ ] Linux binaries, built on the Mac with the P2 musl toolchain (same command family as `make build-agent`):
  - `lobo`: `cargo <P2 tool> build --release --target x86_64-unknown-linux-musl -p lobo-cli`
  - e2e test binary: `cargo <P2 tool> test --release --target x86_64-unknown-linux-musl -p lobo-e2e --no-run --message-format=json | jq -r 'select(.profile.test and .target.name=="live") | .executable'` → one path.
- [ ] `ssh dell 'mkdir -p /tmp/lobocode_p6live && chmod 700 /tmp/lobocode_p6live'`; `scp` both binaries (e2e one as `lobo-e2e`) and `~/.config/lobo/config.env` → `/tmp/lobocode_p6live/config.env`; `ssh dell chmod 600 /tmp/lobocode_p6live/config.env`. ⚠ This puts account keys on the Dell for the length of P6. They leave in Task 30.
- [ ] Image check (no anonymous volumes): `ssh dell "docker pull golang:1.26 >/dev/null && docker image inspect golang:1.26 --format '{{json .Config.Volumes}}'"` → `null`. `golang:1.26` has CA certs and `go` (for `generates_compiling_go`).
- [ ] The run pattern for every cloud command below (`<purpose>` changes per command so names never collide):
  ```sh
  ssh dell docker run --rm --name lobocode_p6live_<purpose> --add-host "$R2HOST:104.18.54.45" \
    -v /tmp/lobocode_p6live:/work -w /work golang:1.26 ./lobo --config /work/config.env <args>
  ```
  Written below as `DELL <purpose> <args>`. `boots.jsonl` from cloud runs lands in `/tmp/lobocode_p6live/` on the Dell.
- [ ] Smoke, free: `DELL version version` → the feat/rust version. `DELL status status --json | jq .down` → `true`.

## Task 18 — Publish the Rust agent release (orchestrator, infra)

Needs the user's yes from Task 0. Moves `latest` for every CLI, Go included.

- [ ] `make release` (Rust `lobo release` with the repo `.env`) → prints the new version and `published`. The secret scan runs inside it.
- [ ] `curl -s --resolve "$R2HOST:443:104.18.54.45" "https://$R2HOST/releases/latest.json" | jq -r '.manifest.version, .manifest.git_sha'` → the new version, `git rev-parse --short HEAD`. Store as `RUST_REL`.
- [ ] Rollback if Tasks 19–21 show the Rust zip is broken: master's Go CLI users run `lobo up --release $GO_REL`; full rollback = on master (clean tree, `git checkout master`), `make release` publishes a fresh Go zip as latest.

## Task 19 — RunPod live: Rust CLI + Rust agent (zip) + e2e (orchestrator, money-bearing)

Cost: RunPod community $0.69/h (measured 2026-09-23/26). Good host: ready in 50–92 s; a whole check ~10 min ≈ $0.12. Bad community hosts are common (5/5 broken on 2026-09-29), each replace adds minutes; earlier `down` totals were $0.04–0.24. Estimate $0.10–0.40, unmeasured for the Rust build.

- [ ] `DELL up up --provider runpod --plain` → last line has `url=https://<domain>/v1 release=$RUST_REL usd_per_h=0.69 boot=…`. Record boot seconds.
- [ ] `DELL status status --json | jq '{stage: .status.stage, rel: .version.version, gpu: .status.gpu.name}'` → `ready`, `$RUST_REL`, contains `5090`.
- [ ] `DELL test test` → `✓ streamed chat`, `✓ tool call: arguments is a JSON string`.
- [ ] `DELL logs logs -n 50` → contains `llama` and `lobo-agent`.
- [ ] e2e:
  ```sh
  ssh dell docker run --rm --name lobocode_p6live_e2e --add-host "$R2HOST:104.18.54.45" \
    -e LOBO_E2E_CONFIG=/work/config.env -v /tmp/lobocode_p6live:/work -w /work golang:1.26 \
    ./lobo-e2e --ignored --test-threads=1 --nocapture
  ```
  → `test result: ok. 23 passed; 0 failed` (Go baseline: 23/23 in 30 s, 2026-09-23). Save the output to `/tmp/lobo-p6/e2e-runpod.txt`.
- [ ] `DELL status2 status --json | jq .status.llama.gen_tps` → tok/s (Go baseline 43–50).
- [ ] `DELL down down --json` → `{"spent_usd": …}`. `DELL status3 status --json | jq .down` → `true`.
- [ ] Row in the live report.

## Task 20 — Vast live: Rust CLI + Rust agent (baked image) (orchestrator, money-bearing)

Cost: Vast $0.68–0.73/h (measured 2026-09-25), ready in ~348 s, that check cost $0.08. Estimate $0.08–0.20.

- [ ] `DELL upv up --provider vast --image ghcr.io/1905/lobocode@sha256:<digest from Task 12> --plain` → ready line, `release=` shows the image's version (`sha-<HEAD>`).
- [ ] `DELL statusv status --json` → `ready`, gpu `5090`, provider `vast`.
- [ ] `DELL testv test` → both ✓.
- [ ] `DELL logsv logs -n 50` → contains `llama`.
- [ ] `DELL statusv2 status --json | jq .status.llama.gen_tps` → tok/s.
- [ ] `DELL downv down --json` → spent; then `status --json | jq .down` → `true`.
- [ ] Row in the live report.

## Task 21 — Mixed versions: Rust CLI → last Go agent (zip) (orchestrator, money-bearing)

Proves an old pod release still works under the new CLI. Cost: as Task 19, $0.10–0.40.

- [ ] `DELL upm up --provider runpod --release $GO_REL --plain` → ready, `release=$GO_REL`.
- [ ] `DELL statusm status --json | jq '.version.version, .status.stage'` → `$GO_REL`, `ready` (Go agent JSON decoded by Rust types).
- [ ] `DELL testm test` → both ✓. `DELL logsm logs -n 20` → contains `llama`.
- [ ] `DELL downm down --json` → spent; `status --json | jq .down` → `true`.
- [ ] Rows in the live report + "Compat".

## Task 22 — Live report (orchestrator)

- [ ] Fill `results.md`. Template:

```markdown
## Live report (Rust cutover, <date>)

Branch `feat/rust` @ <sha>. Rust agent release <RUST_REL>. Last Go agent release <GO_REL>. Image sha-<sha> (<digest>).

| # | Provider | CLI | Agent (path) | Boot s | Gen tok/s | `lobo test` | status | logs | e2e | down OK | $ spent | Notes |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| 1 | local | Go | Go supervisor | | | ✓/✗ (Rust CLI) | | | — | | 0 | Task 15 |
| 2 | local | Rust | Rust supervisor | | | | | | — | | 0 | app screenshot ✓/✗ |
| 3 | runpod | Rust | Rust (zip) | | | | | | 23/23 | | | hosts replaced: n |
| 4 | vast | Rust | Rust (baked image) | | | | | | — | | | |
| 5 | runpod | Rust | Go <GO_REL> (zip) | | | | | | — | | | mixed |

Total spent: $… (cap $5).

### Compat
| Check | Result |
|---|---|
| `config show --json` Go vs Rust | ✓/✗ |
| `config set` on a copy: one line changed, comments kept, mode 600, Go reads it | |
| `models --json` Go vs Rust (Go markers verified) | |
| stale Go state file removed | |
| Go local run: Rust status/test/logs/down; Rust up refuses | |
| boots.jsonl: Rust appends, all lines parse | |
| Rust CLI → Go agent pod | |
```

- [ ] Commit: `plans: rust P6 live report` (redaction rule applies).

## Task 23 — Final review: `/rival-codex review` (orchestrator)

The one model review of the whole rewrite (user rule). Runs only after Task 22 is all ✓.

- [ ] `Skill(rival-codex)` with args `review` on the whole branch diff `master...feat/rust`. Present its full output verbatim in a fenced block.
- [ ] Codex fails (crash, exit ≠ 0, timeout, quota) → self-review with `feature-dev:code-reviewer`, one dispatch per crate (`lobo-proto`, `lobo-agent`, `lobo-core`, `lobo-cli`, `lobo-e2e`, `app/`) because the diff is the whole codebase. Say plainly in `results.md` and the notify: "Codex failed: <reason>. Self-review."
- [ ] While the review runs: read-only prep only. No code changes.

## Task 24 — Triage and fix findings

- [ ] Orchestrator: verify each finding against the code (reviewers can be wrong). List in `results.md` → "Review": id, verdict (holds / false positive + one-line reason), fix commit.
- [ ] Each finding that holds = one implementer task: failing test first, fix, focused test passes, commit `review: <id> <short>`.
- [ ] `make lint test` → green.

## Task 25 — `/simplify` (orchestrator)

- [ ] `Skill(simplify)` on the branch diff, after Task 24 fixes are in (so the cleanup sees the final shape).
- [ ] Apply what it proposes that keeps behaviour. One commit per item: `simplify: <short>`.
- [ ] `make lint test` → green.

## Task 26 — Re-check after fixes (orchestrator)

- [ ] `git diff <Task 22 sha>..HEAD --stat -- crates/lobo-agent crates/lobo-core docker app/src-tauri` → runtime paths touched?
  - `lobo-agent`, `lobo-core::{provider,control,bootstrap,release}`, `docker/` touched → one RunPod smoke: rebuild linux `lobo`, `DELL up`, `DELL test`, `DELL down` (+ new `make release` first if the agent changed). ~$0.15.
  - `lobo-core::local` or `app/src-tauri` touched → Task 16 again (free).
  - Nothing touched → no smoke; write "no runtime path changed" in `results.md`.
- [ ] Push. `gh run watch` → green.

## Task 27 — User approval (orchestrator)

- [ ] `/notify`: "Rust cutover live report ready: plans/2026-09-29-rust-rewrite/results.md on feat/rust (open it locally). 5 runs, e2e 23/23, $<total>. Approve merge to master?" No localhost links, no tunnels.
- [ ] Wait for the user's answer. No merge without it.

## Task 28 — Merge to master (orchestrator)

- [ ] `git fetch origin && git log --oneline feat/rust..origin/master` → empty. Not empty → Tasks 1–2 for the new commits, then Task 26 rules, then continue.
- [ ] `git checkout master && git pull --ff-only && git merge --no-ff feat/rust -m "rust cutover: Go and Swift replaced by the cargo workspace (feat/rust)"`.
- [ ] `make lint test` on master → green.
- [ ] `git push git@github-1f47e:1905/lobocode.git master`. `gh run watch` for each run (rust.yml, pod-image.yml) → green. Red → `gh run view --log-failed`, fix on master, push, repeat.

## Task 29 — First Rust release (orchestrator)

- [ ] `git tag -a v0.2.0 -m "lobo v0.2.0: Rust build"` (or the version the user picked). `git push git@github-1f47e:1905/lobocode.git v0.2.0`.
- [ ] `gh run watch` for release.yml (release + dmg jobs) and pod-image.yml → green.
- [ ] Assets: `gh release view v0.2.0 --json assets --jq '.assets[].name'` → `lobo_darwin_amd64.tar.gz`, `lobo_darwin_arm64.tar.gz`, `lobo_linux_amd64.tar.gz`, `lobo_linux_arm64.tar.gz`, `checksums.txt`, `lobocode.dmg`.
- [ ] Formula: `gh api repos/1905/homebrew-tap/contents/lobo.rb --jq .content | base64 -d | grep -E 'version|url'` → v0.2.0 URLs.
- [ ] Brew: `brew update && (brew upgrade 1905/tap/lobo || brew install 1905/tap/lobo)`; `"$(brew --prefix)/bin/lobo" version` → `v0.2.0`; `brew test 1905/tap/lobo` → pass. (`~/.local/bin/lobo` comes first on PATH; call brew's path explicitly.)
- [ ] Pod image, no pull:
  ```sh
  T=$(curl -s "https://ghcr.io/token?scope=repository:1905/lobocode:pull" | jq -r .token)
  curl -sI -H "Authorization: Bearer $T" -H 'Accept: application/vnd.oci.image.manifest.v1+json,application/vnd.docker.distribution.manifest.v2+json' \
    https://ghcr.io/v2/1905/lobocode/manifests/v0.2.0 | grep -i -E '^HTTP|docker-content-digest'
  ```
  → `HTTP/2 200` + a digest.
- [ ] dmg: `gh release download v0.2.0 -p lobocode.dmg -D /tmp/lobo-p6/` → `hdiutil attach -nobrowse /tmp/lobo-p6/lobocode.dmg` → `ls /Volumes/lobocode/lobocode.app/Contents/MacOS` → one binary; `hdiutil detach /Volumes/lobocode`.
- [ ] Replace the local Go installs: `make install` (→ `~/.local/bin/lobo version` = v0.2.0) and `make install-mac` (the Swift app goes to `/tmp/trash` by the target's `mv`). Tell the user both changed.

## Task 30 — Close (orchestrator)

- [ ] Spec: status `done`; "As-built notes" for P6 (e2e config path knob, Dell runner, first Rust version, what the review changed); fix the file-table row (Spec issues 1).
- [ ] Every `plan-p*.md` status → `done`.
- [ ] `mkdir -p plans/done && mv plans/2026-09-29-rust-rewrite plans/done/` then `git add -A`. Commit `plans: rust rewrite done`. Push over the SSH alias. `gh run watch` (path filters may skip every workflow; then nothing to watch, say so).
- [ ] Dell cleanup: `ssh dell 'mkdir -p /tmp/trash && mv /tmp/lobocode_p6live /tmp/trash/lobocode_p6live.$(date +%Y%m%d-%H%M%S)'`. `ssh dell 'docker ps -a --filter name=lobocode_p6live'` → empty.
- [ ] Project memory: update `lobo-architecture.md` (Rust workspace layout, `make e2e` = `lobo-e2e` with `LOBO_E2E_CONFIG`, released v0.2.0, Go/Swift gone) and `MEMORY.md` line.
- [ ] `/notify`: "Rust cutover merged, v0.2.0 released (brew, image, dmg). Go and Swift gone. Old feat/rust branch kept; delete it?"

---

## Self-review (2026-09-29)

- Coverage vs the task brief: e2e port with full name map (23 + setup) ✓; config source checked (Go reads the repo `.env` file via `LoadLaptop`, never OS env) ✓; live sequences for RunPod, Vast, local with markers ✓; costs from measured earlier runs, labelled unmeasured for Rust ✓; Dell pinned-IP workaround ✓; live report template ✓; removal by `mv` + `git add -A`, every path listed ✓; old config, old state file, Go-agent pod checks ✓; exit gate order: live green → Codex review → fix → `/simplify` → re-test + conditional smoke → user approval → merge → `gh run watch` → tag + release → brew check → `plans/done` ✓; merge master + port ledger ✓.
- Order risk checked: Go source is removed only after the master merge (Task 1), the port (Task 2) and the Go reference build (Task 3). Removing earlier would turn every master Go change into a modify/delete conflict and lose the Go binary the compat checks need.
- Money-bearing steps: Tasks 18–21, 26 (conditional). All orchestrator. Budget cap $5. `down` always runs after an `up`.
- Things I did not verify: that `cargo zigbuild test --no-run` (or P2's tool) cross-builds a test binary; that libtest runs tests in sorted order under `--test-threads=1` beyond the `--list` output; that `104.18.54.45` still serves the bucket (Task 3 checks it); that `golang:1.26` declares no `VOLUME` (Task 17 checks it); the P4/P5 Makefile target names.
- Type/name consistency: `lobo_core::config::load_laptop`, `control::HttpAgent::new`, `release::resolve`, `checks::{chat, tool_call, validate_tool_call}`, `lobo_proto::catalog::get`, `Stage::Ready` match contracts v1.0 names. `Laptop` field names (`domain`, `lobo_api_key`, `bucket_url`) are assumed snake_case of the Go fields.

## Contract additions

- `checks::chat` / `checks::tool_call` signatures are `..` in contracts v1.0. `lobo-e2e` needs `(hc: &reqwest::Client, base_v1: &str, key: &str, alias: &str)`. Fold into v1.1 as P3 shipped them.
- `HttpAgent::new(base, key)`: contracts say base URL, Go took a domain. Confirm in v1.1.
- `Laptop` field names: contracts say "same env names", which reads as env keys, not Rust fields. Pin the field names in v1.1.

## Spec issues

1. File table says Go/Swift are removed "in the phase that replaces each part". P1 and P6 remove everything at P6 (the P1 drift CI needs Go; earlier removal breaks `git merge master`). Fix the row.
2. P6 has no live check of the Tauri app. This plan adds one screenshot (app shows a CLI-started local run). A Start/Stop from the panel is still only covered by P5 renders.
3. The e2e suite cannot target local mode: it needs `https://LOBO_DOMAIN` and a `5090`. "e2e on one of them" can only mean a cloud pod.
4. New knob `LOBO_E2E_CONFIG` (a path) is needed to run the suite on the Dell. Not in the spec.
5. Testing the Rust zip path before merge means `lobo release`, which moves `latest.json` for master's Go CLI too. "Master keeps the working Go build" holds only because P2 proved Go CLI → Rust agent. Needs an explicit user OK.
6. The spec names no version for the first Rust release. Plan proposes v0.2.0.
7. Linux cross-builds of `lobo` and the e2e test binary on the Mac depend on P2's musl toolchain choice. The spec does not say which.
8. README screenshots `docs/img/*.png` are Swift renders; the spec file table does not list `docs/img`.
9. Local mode had never run live on Go (project memory). Task 15 is the first live Go local run, so a Go-side local bug could block a Rust compat check.


---
<!-- end of plan-p6-v1.0.md -->

