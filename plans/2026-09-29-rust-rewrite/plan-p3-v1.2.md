# Rust rewrite P3 — `lobo-core` Implementation Plan v1.2

**Date:** 2026-09-29
**Status:** approved for implementation after plan correction (user: full auto, 2026-09-29). Earlier review covered v1.0 only.
**Spec:** ./spec.md (full-auto implementation authorized, 2026-09-29)
**Contracts:** ./contracts.md v1.2 (names and signatures used exactly; extras listed under "Contract additions")
**Phase:** P3 of 6. Needs P1 (`lobo-proto`) and P2 (`lobo-agent`) done on `feat/rust`.

Execution update (2026-09-29): the user moved GPU rentals to final P6 E2E. P3 requires P2 code, static builds and CI; P2 live acceptance remains pending.

**Goal:** `crates/lobo-core` holds all laptop logic: config file, RunPod/Vast/local providers, up/status/down/target, `lobo test` checks, release (zip, secret scan, R2), bootstrap script, local supervisor, opencode genkey. Behaviour = Go at the recorded master fork after `6a72092` is merged, proven by a Rust port of every Go test in these packages plus golden files written by the Go code.

**Architecture:** Go stays untouched and keeps building until P6 (the Go CLI and the Swift app still use it). `lobo-core` depends on `lobo-proto` (wire types) and `lobo-agent` (Runner, API router, downloader, process helpers). A second Go dumper, `tools/corefixtures`, writes golden files from the real Go functions (`config.Save`, `godotenv.Read`, `bootstrap.Script/Env`, `runpod.BuildCreatePayload`, `vast.CreateBody`, the `gen-api-key` output). Rust tests compare bytes against them. CI re-runs the dumper, so any Go change shows up as a diff. No live calls in P3: every HTTP peer is `wiremock`, every child process is a fake script or a test helper binary.

**Tech Stack:** Rust 1.98.1 (edition 2024). tokio, tokio-util (CancellationToken), reqwest (rustls, json, stream), async-trait, serde + serde_json, chrono, thiserror, tracing, url, rand, hex, sha2, base64, rusty-s3, zip, tar + flate2, nix (signal, process, fs), libc (sysctlbyname). Dev: wiremock, tempfile. No russh in P3: the SSH model source lives in `lobo-agent` (P2).

> Execution: the primary agent implements task-by-task and records checked work. No unavailable skill or model is required.

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

From contracts.md: `Deps` + traits `Tunnel`, `GpuCheck`, `Download`, `Llama`, `Metrics`, `Killer`; `Runner`, `RunnerConfig`; `api::router(runner, api_key, logs: LogSource, version: bytes::Bytes)` (version = the exact `/api/version` body); `source::HttpSource`; `download::{download, hash_file}`; `process::{clean_env, LlamaArgs}`; `metrics::Collector`.

Also used here, as locked in `plan-p2-v1.0.md` (Task 0 re-checks them against the built crate; if P2 shipped a different name, Task 0 records the mapping and every task uses P2's name):
- `logring::{LogRing, LogSource}` — `LogRing::new(n)`, `write`, `tail(n)`; `LogSource = Arc<LogRing>`; `impl std::io::Write for &LogRing`.
- `process::{clean_env, CleanEnv::filter, LlamaArgs { model_path, alias, host, port, ctx }, start_process, Proc { pid, exited }, last_line}` — `start_process(program, args, env: &[(String,String)], logs: LogSource, tag: Option<&'static str>, kill: CancellationToken) -> Result<Proc>`; it always `env_clear()`s, so the caller passes the full child env.
- `health::wait_healthy(base: &str, poll: Duration, cancel: CancellationToken) -> Result<()>`.
- `fetch::fetch_file(url: &str, dst: &Path, mode: u32, size: i64, sha: &str) -> Result<()>` (5 min bound, size + sha check).
- `download::download(cancel, src: &dyn Source, dst, sha, on_progress: Option<&(dyn Fn(DownloadProgress) + Sync)>)` renames a sha mismatch to `<dst>.bad` (Go `agent.Download`, used by `local/deps.go:148`). `download::hash_file` likewise takes `Option<&(dyn Fn(DownloadProgress) + Sync)>`.
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
- [ ] Implement. `cargo test -p lobo-core clock && cargo test -p lobo-core http` → pass (2).
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
    async fn rent(&self, o: &CreateOpts, cancel: CancellationToken, note: &(dyn Fn(String) + Sync)) -> Result<Instance>;
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
  pub async fn publish_version(&self, zip: &Path, r: &Resolved) -> Result<()>; // zip + version JSON only
  pub async fn publish(&self, zip: &Path, r: &Resolved) -> Result<()>;  // store.go:47-68
}
```
Publish: HEAD zip key and meta key first (200 → `Error::Release("release <v> already exists (<key>); run make release again")`, 404 → continue, other → error). Then PUT zip (`Content-Type: application/zip`), then meta json, then `latest.json` (`application/json`, `Cache-Control: no-cache`, pretty JSON). Every request is a presigned URL (1 h) sent with reqwest. Versioned PUTs sign and send `If-None-Match: *`; HEAD alone cannot prevent concurrent publishers overwriting the same release. A 412 fails the publish before latest. Add a conditional-write conflict test. R2 supports these conditions: https://developers.cloudflare.com/r2/api/s3/api/. This is documented support; live verification remains pending P6.
- [ ] Failing tests (wiremock, requests recorded): `publish_order_zip_meta_latest` (HEAD, HEAD, PUT zip, PUT meta, PUT latest; each URL query has `X-Amz-Signature`); `publish_refuses_existing_release` (HEAD 200 → error, no PUT); `list_follows_continuation` (two XML pages → all keys).
- [ ] Implement. `cargo test -p lobo-core release::store` → pass (5).
- [ ] Commit: `lobo-core: R2 publish (zip → meta → latest) and listing`.

Candidate release tests (Task 31): `publish_version` writes only immutable zip and per-version JSON. A fake store rejects any latest PUT. `publish` calls it, then moves latest only after both uploads succeed. Failed scan/upload never moves latest.

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
pub async fn ensure_runtime(weights: &Path, cancel: CancellationToken, note: &(dyn Fn(String) + Sync)) -> Result<PathBuf>;   // contracts.md
pub async fn ensure_runtime_with(weights: &Path, pin: &RuntimePin, cancel: CancellationToken, note: &(dyn Fn(String) + Sync)) -> Result<PathBuf>;
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
`check_gpu`: `<llama_server> --list-devices`, env = `CleanEnv::filter(base_env)`, 1 min bound, log `[gpu-check] <out>`; needs `MTL0`; then `min_free_mib(model.size) <= usable`, else `<id> needs <x.x> GB, this Mac allows ~<y> GB to the GPU`.
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
Mapping onto `lobo_agent::Deps`: `tunnel` = a tunnel that never exits (it keeps the oneshot `Sender` alive inside the struct; a dropped sender would read as "tunnel exited"); `gpu_check` = `check_gpu`; `download` = `download`; `llama` = start (`-m <weights>/<file>` + `LlamaArgs` for host `127.0.0.1`, port, ctx; env = `CleanEnv::filter(base_env)` + `LLAMA_API_KEY=<key>`; pid recorded; `llama_done` fired on exit) + `wait_healthy(llama_url, poll)`; `metrics` = `Collector{llama_url, api_key, http: lobo_agent::http::client(), nvidia_smi, proc_dir}` for llama (all 5 fields; `nvidia_smi` and `proc_dir` are set but unused on the Mac), `gpu()` for gpu, host → `Err("host metrics: not collected on macOS")`; `killer` = cancels `stop`.
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

Flow = `cmd/lobo/local.go:87-147`: `validate`; load the config (`config_path` or `default_path`); `ensure_runtime`; **bind the API port before the state file**; claim state `{pid, port, api_port, model, weights, started_at: now UTC, boot_id}`; Runner with `boot_timeout 8 h`, `expires_at now + 100 y`, `tick 30 s`, `fail_grace 2 min`, `idle = idle_min`; axum server with `lobo_agent::api::router(runner, api_key, logs, version)`, where `version` is `Bytes` of `{"git_sha":…,"version":…}` built from `cfg.version` (sorted keys, no newline; Go `local.go:127`); SIGTERM/SIGINT or `cancel` or the Killer end the run; then wait up to 10 s for llama-server; `remove_if(pid, boot_id)` always; a cancel/SIGTERM stop returns `Ok(())`. Log lines go to stdout and a `LogRing::new(5000)` (as `LogSource`). `supervise` installs no global tracing subscriber; it owns its logging.
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
#[async_trait] pub trait EnsureRuntime: Send + Sync { async fn ensure(&self, weights: &Path, cancel: CancellationToken, note: &(dyn Fn(String) + Sync)) -> Result<PathBuf>; }
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

Delete (`provider.go:224-253`): state for id else Ok; not our supervisor → `remove_if`, Ok, never signalled; SIGTERM; wait `stop_wait` for `kill(-pid, 0) == ESRCH`; not gone → if the leader pid is ALIVE and no longer our supervisor (pid reused) → `remove_if`, Ok, never signalled (`provider.go:239-241`); a DEAD leader with live children still gets the group kill (the pgid stays ours while the group lives, `provider.go:242-243`); `killpg(pid, SIGKILL)`; wait 2 s; still there → `local group <pid> survived SIGKILL`; then `remove_if`.
- [ ] Failing tests: `delete_kills_stubborn` (TestProviderDeleteKills: dead, took ≥ `stop_wait`); `delete_kills_group` (TestProviderDeleteKillsGroup, `ok-child` and `stubborn-child`: child pgid = supervisor pid; after delete `killpg(pid, 0)` = ESRCH; state gone — carry-over "SIGKILL the process group", `provider.go:243-250`); `delete_reverifies_before_sigkill` (TestProviderDeleteReverifies: ps hook returns the real command once, then `/usr/bin/vim notes.txt` → 2 calls, still alive, state gone); `delete_never_signals_stranger` (TestProviderDeleteStranger, 5 rows); `delete_dead_leader_live_child_killed` (new, no Go twin: supervisor exits on SIGTERM, its child ignores SIGTERM → group SIGKILLed, `killpg(pid, 0)` = ESRCH, then state gone).
- [ ] Implement. `cargo test -p lobo-core --test local_provider` → pass (8).
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

Port of `internal/control/controltest/fakes.go`, names kept: `Created { opts: CreateOpts, cloud_type, min_download_mbps }`, `FakeRunPod` (implements `RunPodApi`; `pods`, `no_cap: BTreeSet<String>`, `max_mbps`, `created`, `deleted`, `create_err`; one pub state struct behind one `Mutex`, with pub fields `pods`, `no_cap`, `created`, `deleted`, `create_err`, so P4 can seed pods from another crate), `FakeReleases(Resolved)` (`"missing"` → error), `FakeAgent` (scripted `Vec<Option<Status>>`, one per `status` call, sticks on the last; `calls()`), `FakeLocal` (provider "local", `LOCAL_API_URL`, `LOCAL_AGENT_URL`), `local_boot_script()`, `release()`, `deps(rp, ag, clock) -> Deps` (RunPod on `lobo.example.com`, poll 1 ms, cfg as `fakes.go:208-211`), `boot_script()`, `pub async fn events(script, no_cap: &[&str]) -> Vec<UpEvent>` (async: it drives `control::up`; StepClock 1 s). Exposed under `#[cfg(any(test, feature = "testkit"))]` for P4 TUI tests.
- [ ] Failing test `events_helper` (TestEventsHelper) — lands with Task 57; here: `fake_agent_script_sticks_on_last`, `fake_runpod_no_cap` (compile + behaviour of the fakes).
- [ ] Implement. `cargo test -p lobo-core control::testkit` → pass (2).
- [ ] Commit: `lobo-core: control fakes (testkit feature for the CLI TUI tests)`.

## Task 55 — `up`: preflight + options

**Files:** `src/control/up.rs`, `src/control/tests.rs`.

Locked (contracts.md): `pub fn up(d: Deps, o: UpOpts, cancel: CancellationToken) -> UpOperation`.
Event plumbing: `UpOperation` owns cancellation and worker completion (contracts.md). `take_events()` supplies the existing bounded receiver to CLI/UI consumers. A note forwarder may buffer synchronous notes, but consumer backpressure/closure cannot prevent cleanup. The caller drains events while awaiting `wait()`. A worker panic, delete failure or uncertain create returns an error through completion, even when no event can be delivered.

Preflight (`up.go:58-130`): provider default `runpod`; unknown → `provider "<p>" is not configured (key missing in the config? run \`lobo config\`)`; anything running → `lobo already running: <prov> <id> (<status>). Run \`lobo down\` first`; local → built-in release (`DEFAULT_MODEL`, `DEFAULT_DEFAULTS`, version `local`); image from flag else `cfg.pod_image`; image + release → error; image → manifest `{version: image, llama_image: image, model: ModelRef{id: DEFAULT_MODEL}, defaults: DEFAULT_DEFAULTS}` (`up.go:84`); else resolve. Fill model/ctx/idle/max-life from the release, timeout 20 min. Bad options → `bad options: ctx <c> (min 512), idle-min <i> (min 1), max-life <m> (min 1m)`. Model from the catalog.
- [ ] Failing tests (Go ports): `up_already_running` (carry-over "`up` refuses while anything runs" — `up.go:67-73`); `up_unconfigured_provider`; `up_image_and_release_conflict`; `up_rejects_bad_options_before_renting`; `up_overrides`; `up_baked_image` (3 rows); `up_baked_image_needs_no_release_manifest`; `up_baked_image_uses_builtin_defaults` (new: `--image` and `cfg.pod_image` rows, no model/ctx/idle/max-life overrides, resolver panics if called → rent opts carry model `q8`, ctx 65536, idle 30, max-life 12 h). (The boot loop is stubbed in this task to rent once and return ready when the first status is `ready`; Task 57 replaces the stub.)
- [ ] Implement. `cargo test -p lobo-core control::tests::up_` → the 8 tests pass.
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

Cancellation + completion (v1.2): use the `UpOperation` contract. Pass the token to Provider::rent, EnsureRuntime::ensure and runtime download/extraction. Local preparation stops cooperatively; blocking extraction is joined before return. Recheck before spawn. Track the child PID immediately and kill its process group on cancellation/handshake failure, even before it writes state. A submitted cloud create is awaited using its normal HTTP timeout. No later tier/offer/replacement may start after cancellation. Delete returned instances with an independent cleanup timeout. Preserve Vast uncertain-create adoption; use the before/after lobo-instance list on RunPod too. Never retry an ambiguous create. If reconciliation cannot prove the outcome, return a typed error naming provider and boot ID; record an unresolved create locally and refuse subsequent up until reconciliation clears it. Do not report cancellation success or OFF on cleanup failure. This is a deliberate correction of Go's cancellation behavior.
- [ ] Failing tests: `up_happy` (phases `create,image,tunnel,download,load,ready`; download bytes; ready URL + git sha; COMMUNITY only; create opts of `control_test.go:52-55`); `events_helper`; `up_agent_failed`; `up_reports_expiry_reason`; `up_timeout`; `up_no_capacity_anywhere` (final err contains `no gpu capacity`); `up_steps_down_network_tiers`; `up_community_only_by_default`; `up_secure_all_tiers_before_community`.
- [ ] Failing tests (v1.1, cancellation): `up_cancel_during_slow_rent_deletes_result` (testkit provider whose `rent` sleeps 20 s on a paused clock and ignores cancel; cancel at 1 s → receiver still open at 19 s; after rent returns: exactly one `delete(id)`, last event `cancelled`, then `None`); `up_cancel_stops_replace_loop` (bad host, cancel before attempt 2 → one rent total); `up_cancel_during_runtime_prep_spawns_nothing` (local, `ensure_runtime` blocked → no spawn); `up_cancel_after_spawn_deletes_local`.
- [ ] Add regression tests: cancel between provider tiers/offers; rent completes after 121 s; event receiver dropped/full; worker panic; failed delete; uncertain create appears during reconciliation; unresolved create blocks another up; cancellation while extraction/spawn handshake runs. Completion must distinguish clean cancellation from unresolved cleanup.
- [ ] Implement. `cargo test -p lobo-core control::tests::up_` → all pass; record the measured count.
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

- [ ] `tests/contracts_api.rs`: one `#[test] fn contracts_p3_signatures()` that coerces every lobo-core item in contracts.md v1.1 without a `(P5)` tag to a typed fn pointer or uses the type (`let _: fn(&Path) -> Result<Laptop> = lobo_core::config::load_laptop;` …, trait objects `Arc<dyn Provider>`, `lobo_core::local::SUPERVISOR_ARG`, `lobo_core::provider::local::LocalProvider`). Compile failure = drift.
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
- [ ] Plan status → `done` (P3). `/notify`: "Rust P3 done on feat/rust: lobo-core + Go goldens, CI green. Continuing P4 under full-auto authorization."
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

## Historical self-review (before the current correction)

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

## Historical spec issues (resolved by execution.md unless marked pending)

1. The spec's `lobo-core` line lists "runpod REST+GraphQL". The GraphQL part is pod self-terminate, which runs inside `lobo-agent`. `lobo-core` depends on `lobo-agent`, so the agent cannot call core. Self-terminate (runpod GraphQL + Vast instance-scoped GET/DELETE) and `config.LoadAgent` belong to P2. Their Go tests (TestSelf, TestSelfTerminateAndGone, TestLoadAgent*) are mapped to P2. `plan-p2-v1.0.md` already takes them (`selfkill.rs`, its line 78); the spec line should say so.
2. contracts.md v1.0 `deps_from_config(cfg)` cannot build the local provider (no exe, argv prefix or config path). Fixed by Contract additions 2.
3. contracts.md v1.0 `Spawner` app prefix `[SUPERVISOR_ARG]` breaks the identity carry-over rule and Go interop. Fixed by Contract additions 5.
4. P1's Go-test table puts all `cmd/lobo/*_test.go` in P4. Nine of those tests check logic the app also needs (pre-checks, defaults, providers wiring, opencode, masking, run flags). They move to P3 here; P4 keeps the four CLI-only ones.
5. `UpEvent.err` is a string (P1 wire type = Go `--json`). The app wants `{kind, message}` for up failures too. Options for P5: add `err_kind` (omitempty, Go CLI ignores it) — needs a P1 wire change and a fixture; or accept kind `"up"` for all up failures. P3 does not change the wire type.
6. The spawned argv moves `--config` from before `local run` (Go) to after the prefix (Rust), because the app prefix must come first. Go's cobra reads `--config` in either place, and identity only checks `local run` + `--boot-id`, so both directions still work. The Go test's expected argv string is adjusted in `lifecycle`.
7. Spec line "Local: runtime fetch b11118, models listing + markers, supervisor, state claim/flock, group kill, active endpoints" is fully P3. The spec's "Rust home" says `lobo-core::local + lobo-agent`. Only the Runner, downloader and API router come from `lobo-agent`; the rest is core.

## Review fixes v1.1

Codex plan review of the v1.0 bundle (session f79006e4, 6/10). Confirmed findings applied here:
- F1 (high): `up` cancellation + completion. Receiver closes only after the worker ends; in-flight rent awaited then deleted; no rent after cancel; 4 new tests.
- F5 (high): local delete keeps Go's `alive(pid)` guard; a dead leader's live children are still group-killed; new test `delete_dead_leader_live_child_killed`.
- F6 (high): baked-image manifest carries `DEFAULT_MODEL` + `DEFAULT_DEFAULTS`; new test `up_baked_image_uses_builtin_defaults`.

Contract alignment (contracts v1.1):
- Item 2: "Assumed `lobo-agent` API": `api::router(.., version: bytes::Bytes)`.
- Item 3: same section: progress callbacks are `Option<&(dyn Fn(DownloadProgress) + Sync)>`.
- Item 4: Task 42: child env = `CleanEnv::filter(base_env)`.
- Item 5: Task 44: child env = `CleanEnv::filter(base_env)`.
- Item 6: Task 44: `Collector` gets all 5 fields (`http`, `nvidia_smi`, `proc_dir` set but unused on the Mac).
- Item 7: Task 46: `/api/version` body = `Bytes` of `{"git_sha","version"}` from `cfg.version`; no global tracing subscriber.
- Item 8: Task 54: `pub async fn events(script, no_cap: &[&str])`.
- Item 9: Task 54: `FakeRunPod` state struct and its fields are `pub`.
- Item 10: Task 64: pins every lobo-core contract item without a `(P5)` tag.
- Doc ref: header `contracts.md v1.0` → `v1.1`.

## Execution record

Tasks 0–13 implemented as one foundation/config batch. 36 core tests and clippy pass.
The Go generator produces 77 deterministic fixtures. CI checks fixture drift.
The task checklists above describe the original sequence; separate red-first commits were not recorded.
Tasks 14–26 implemented in the provider batch. 80 core tests pass, including cancellation during create and delayed Vast reconciliation. Tasks 27–35 implemented in the release/checks batch. 105 core tests and clippy pass. Tasks 36 onward remain pending.
