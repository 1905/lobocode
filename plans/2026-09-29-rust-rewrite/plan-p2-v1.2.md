# Rust rewrite P2 — `lobo-agent` Implementation Plan v1.2

**Date:** 2026-09-29
**Status:** approved for implementation after plan correction (user: full auto, 2026-09-29). Earlier review covered v1.0 only.
**Spec:** ./spec.md (full-auto implementation authorized, 2026-09-29)
**Contracts:** ./contracts.md v1.2 (this plan asks for one contract change and several additions, listed at the end)
**Phase:** P2 of 6. Needs P1 done on `feat/rust` (workspace, `lobo-proto`, `rust.yml`).

**Goal:** a Rust `lobo-agent` (library + static `x86_64-unknown-linux-musl` binary) that does everything the Go pod agent does today, with the same `/api/*` JSON, baked into `ghcr.io/1905/lobocode` by the pod image workflow, and proven on one real RunPod pod and one real Vast instance driven by the unchanged Go CLI.

**Architecture:** one crate `crates/lobo-agent`. The library holds the parts the local supervisor (P3) will reuse: `Runner` + its dependency traits, watchdog, downloader (HTTP + SSH sources), metrics, `/api` router, process helpers, self-terminate clients. The pod wiring (cloudflared, CUDA check, model download policy, llama-server start, env config) lives in `lobo_agent::pod`, so it is unit-tested too. `src/main.rs` is thin: clap, logging, `pod::main_flow`. The Go agent keeps building on the branch until P6. The live check uses `lobo up --image <digest>`, so no Go code changes in P2.

**Tech Stack:** Rust 1.98.1 (edition 2024), tokio, tokio-util (`CancellationToken`), async-trait, reqwest (rustls, bundled webpki roots, ring, http2, stream), axum, russh (client + in-process test server), sha2, hex, base64, url, bytes, futures-util, regex-lite, thiserror, tracing + tracing-subscriber (json), clap (derive), chrono, serde_json, `lobo-proto`. Dev: wiremock, tempfile, tokio test-util. Local musl build: cargo-zigbuild + zig.

> Execution: the primary agent implements task-by-task and records checked work. No unavailable skill or model is required.

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
| CI musl build | `rust.yml` job on ubuntu with `musl-tools`; `pod-image.yml` builds inside `rust:1.98.1-alpine` | Alpine's native target is musl, static by default; no cross tooling in the image build. |
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
- `plans/2026-09-29-rust-rewrite/plan-p1-v1.1.md` — "Go test → Rust home" table: move `TestLoadAgent*` (config), `TestSelf` (runpod), `TestSelfTerminateAndGone` (vast) rows to P2.

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
- [ ] `lib.rs`: modules, `pub const VERSION: &str` = `option_env!("LOBO_VERSION")` or `"dev"`, `LLAMA_ADDR`, `AGENT_ADDR`. Crate-root re-exports (contract): `pub use error::{Error, Result}; pub use runner::{Deps, Runner, RunnerConfig, Tunnel, GpuCheck, Download, Llama, Metrics, Killer}; pub use logring::{LogRing, LogSource};`. Add the `runner` and `logring` lines when those modules land (Tasks 5, 19).
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
- [ ] Implement. `cargo test -p lobo-agent logring && cargo test -p lobo-agent process` → 4 passed.
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
- [ ] Implement. `cargo test -p lobo-agent health && cargo test -p lobo-agent fetch` → 5 passed.
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
- [ ] Implement. `cargo test -p lobo-agent source::tests && cargo test -p lobo-agent download::tests::default_tuning` → 4 passed.
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
- [ ] Implement. `cargo test -p lobo-agent download::tests::stall && cargo test -p lobo-agent download::tests::single_stalled` → 2 passed.
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
- [ ] Implement. Add the crate-root `pub use runner::{Deps, Runner, RunnerConfig, Tunnel, GpuCheck, Download, Llama, Metrics, Killer};` in `lib.rs` (P3 writes `lobo_agent::Deps`). `cargo test -p lobo-agent runner::tests::happy_path && cargo test -p lobo-agent runner::tests::status_timings` → 2 passed.
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
- [ ] Implement. `cargo test -p lobo-agent pod::tests::gpu && cargo test -p lobo-agent pod::tests::free_mib` → 6 passed.
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

FROM rust:1.98.1-alpine AS build
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
- [ ] `/notify`: "Rust agent live check done: RunPod <ok/fail>, Vast <ok/fail>. Digest <…>. Continuing P3 under full-auto authorization."

## Task 31 — Phase close (orchestrator)

- [ ] `make rust-lint rust-test rust-agent && go test ./...` → all green; `git status` clean.
- [ ] Every Go test row in the mapping table below has a passing Rust test (`cargo test -p lobo-agent -- --list | wc -l` ≥ 70).
- [ ] Update `plan-p1-v1.1.md` Go-test table (moved rows, see File map → Modify). Update `contracts.md` with the accepted additions (new version v1.1).
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

## Historical self-review (before the current correction)

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

## Historical spec issues (resolved by execution.md unless marked pending) found

1. "SSH source ≤ 8 streams" is enforced on the laptop (`internal/control/up.go:150-152`), not in the agent. P2 adds an agent-side clamp as a second guard; P3 must keep the laptop clamp.
2. `execfail` is in the bootstrap script (`internal/bootstrap/bootstrap.go:56-61`), which is P3. P2 can only keep the exec path stable.
3. Go never sets a User-Agent (`internal/runpod/self.go:31-32`, `internal/vast/client.go:77-78`); it sends `Go-http-client/1.1` by default. reqwest sends none, so Rust must set one on every request. The spec rule is right, but it is new code, not a port. Whether RunPod GraphQL accepts `lobo-agent/<ver>` is unverified until Task 30.
4. `contracts.md` types `/api/version` as `Manifest`; Go serves raw bytes. Contract change requested above.
5. `contracts.md` lists "macOS sysctl/ps" under `lobo_agent::metrics`; in Go these live in `internal/local/sysctl_*.go` with local tests. P2 does not port them; the P3 plan decides the home.
6. The Dell pinned-IP workaround for the `*.r2.dev` hijack is not written down anywhere in the repo (only `spec.md:101`).
7. `Dockerfile` copies only `crates/`. If P5 adds `app/src-tauri` as a workspace member, the image build breaks (missing member). P5 must keep it out of `members` or the Dockerfile must copy it.
8. `CleanEnv` does not strip `R2_*` (`internal/agent/env.go:12-13`). The pod never has R2 keys; the local llama-server on the laptop could inherit exported `R2_*` shell vars. Unchanged in P2 (parity); flag for P3.
9. Go's API server has `ReadHeaderTimeout: 10s` (`cmd/lobo-agent/main.go:177`); `axum::serve` has no such knob. The agent binds `127.0.0.1` only and cloudflared is the only client, so P2 accepts the gap.

## Contract alignment v1.1

- Item 1: Task 1 `lib.rs` and Task 19 add the crate-root re-exports `pub use error::{Error, Result}; pub use runner::{Deps, Runner, RunnerConfig, Tunnel, GpuCheck, Download, Llama, Metrics, Killer}; pub use logring::{LogRing, LogSource};`.
- Doc ref: header `contracts.md v1.0` → `v1.1`.
