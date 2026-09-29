# Rust rewrite P6 — cutover Implementation Plan v1.2

**Date:** 2026-09-29
**Status:** approved for implementation after plan correction (user: full auto, 2026-09-29). Earlier review covered v1.0 only.
**Spec:** ./spec.md (full-auto implementation authorized, 2026-09-29)
**Contracts:** ./contracts.md v1.2
**Phase:** P6 of 6. Starts only when P1–P5 plans are all `done`.

**Goal:** prove the Rust build on RunPod, Vast and local, port the e2e suite, remove every Go and Swift file, publish a live report, merge `feat/rust` → master, and ship the first Rust release (brew, pod image, dmg).

**Architecture:** no new runtime code. P6 adds one crate (`crates/lobo-e2e`, the live API suite as `#[ignore]` tests), deletes Go/Swift by `mv` to `/tmp/trash`, and rewires Makefile, CI and docs to cargo only. Live checks run from two places: local mode on this Mac, cloud from the Dell in a throwaway container with a pinned `*.r2.dev` IP (home DNS hijacks `*.r2.dev`). A Go reference binary, built before the removal, drives the old-data checks. One model review of the whole branch runs at the end, then `/simplify`, then verify and merge + release under full-auto authorization.

**Tech Stack:** Rust (workspace toolchain from P1), `reqwest` blocking client + `serde_json` + `regex` in `lobo-e2e`, the musl cross-build P2 set up for `lobo-agent`, Docker on the Dell only (never on this Mac), `gh`, goreleaser-rust or cargo-dist (whichever P4 picked), Tauri bundle (P5).

> Execution: the primary agent implements task-by-task and records checked work. No unavailable skill or model is required. Tasks marked **(orchestrator)** are never dispatched.

> Implementer scope (verbatim, every dispatch): writes only the code and unit tests its task names and runs that task's focused unit test. Never runs e2e / integration / live / smoke tests, never rents a GPU or pod, never calls a provider API, never publishes, deploys or touches infra, never runs anything money-bearing. Never sets `TEST_DATABASE_URL` or any test-DB env var.

No per-task external model review. The primary agent checks each batch with builds and focused tests. Keep the one final Codex code review in P6.

---

## File map

**Create**
- `crates/lobo-e2e/Cargo.toml` — `publish = false`, deps: `lobo-core`, `lobo-proto`, `reqwest` (blocking, json, rustls), `serde`, `serde_json`, `regex`, `tokio` (rt only, for `block_on` into async `lobo-core` calls), `tempfile`.
- `crates/lobo-e2e/src/lib.rs` — shared setup `env()`, wire structs, request helpers, tool fixtures, pure parsers (unit-tested, not ignored).
- `crates/lobo-e2e/tests/live.rs` — the 23 live tests, all `#[ignore]`.
- `plans/2026-09-29-rust-rewrite/results.md` — master-port ledger, compat results, live report (redacted: no domain, bucket id, tunnel id, IPs).

**Remove** (`mv` to `/tmp/trash/<name>.<stamp>`, then `git add -A`; never `rm` / `git rm`)
- `cmd/` (lobo, lobo-agent), `internal/` (15 packages), `e2e/`, `go.mod`, `go.sum`, `.golangci.yml`
- `macos/` — already removed in P5 Task 38 (v1.1 decision). P6 only verifies: `test ! -e macos` → exit 0.
- All three Go fixture generators (Codex finding 7): `tools/protofixtures/` (P1), `tools/corefixtures/` (P3), `tools/clifixtures/` (P4, Python driving the Go CLI), `cmd/lobo/capture_test.go` (P4; goes with `cmd/`). Then `tools/` if empty.
- Go-dependent Rust tests: `crates/lobo-core/tests/go_interop.rs` (`LOBO_GO_INTEROP=1`). The generated fixtures (`crates/*/fixtures/**`, `crates/lobo-cli/tests/fixtures/go/**`) STAY as frozen golden files.
- Makefile targets `proto-fixtures`, `core-fixtures`, `cli-fixtures` (or whatever names P1/P3/P4 shipped).
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
- Push through the verified GitHub CLI HTTPS credential helper: `git push https://github.com/1905/lobocode.git <ref>` (verify current permissions; do not assume stale SSH aliases or token restrictions).
- The repo is public: `results.md` and commits never contain the private domain, bucket id, tunnel id, model-server IP or pod SSH hosts. Write `lobo.example.com`, `pub-<id>.r2.dev`.

---

## Task 0 — Preconditions (orchestrator)

- [ ] `plan-p1…p5` status lines all `done`. Any other → finish the missing phase checks before cutover.
- [ ] `git checkout feat/rust && git status --porcelain` → empty. Dirty → inspect and preserve existing work; commit only task-owned changes (never bare `git stash`).
- [ ] `make rust-lint rust-test` (or the P4/P5 names if already renamed) → exit 0. Record test count.
- [ ] `go build ./... && go test ./...` → still green on the branch (Go is present until Task 10).
- [ ] Record the fork point, before any merge: `FORK=$(git rev-parse "$(git rev-list --first-parent feat/rust --not origin/master | tail -1)^")`, write it into `results.md` header. Expected: a master sha at or after `6a72092`.
- [ ] `ssh dell 'uname -m; docker version --format {{.Server.Version}}; df -h /tmp | tail -1'` → `x86_64`, a version, > 5 GB free.
- [ ] `ls /Volumes/Extreme/_lobocode` → Q6 + Q8 GGUFs present (local checks need the drive). Missing → `/notify` "plug in the Extreme drive", wait.
- [ ] First Rust release is `v0.2.0`. Confirm that tag is unused before creating it.
- [ ] Candidate uploads use `lobo release --no-promote`, then every zip smoke pins `--release $RUST_REL`. Save current latest bytes/hash before testing and verify unchanged afterwards. Normal promotion happens only after merged release verification.

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
                 pub alias: String, pub st0: lobo_proto::Status, pub hc: reqwest::blocking::Client /* 5 min timeout */,
                 pub ahc: reqwest::Client /* lobo_core::http::client(5 min); for the async checks::* and release::resolve */ }
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
  - `pod_api_version_matches_latest_release` — GET `/api/version` no key → 200, `Manifest` with non-empty `version` + `git_sha`; `block_on(release::resolve(&e.ahc, &e.cfg.bucket_url, ""))` → only a note if versions differ.
  - `pod_api_status` — `st0.gpu` name contains `5090`; `llama` and `host` present; `kill_in_s > 0`; `expires_at` in the future.
  - `pod_api_logs_need_key` — `/api/logs` no key → 401; `/api/logs?n=20` with key → 200 and body contains `llama`.
  - `llm_rejects_missing_or_wrong_key` — `""` and `"sk-wrong"` on chat → 401; `/v1/models` no key → 401. Collect all failures, then panic once (Go used `t.Errorf`).
  - `models` — `/v1/models` with key → 200, body contains `"<alias>"`.
- [ ] `cargo test -p lobo-e2e --no-run` → compiles. `cargo test -p lobo-e2e` → `5 ignored`.
- [ ] Commit: `lobo-e2e: pod API + auth tests`.

## Task 6 — Chat tests

**Files:** Modify `crates/lobo-e2e/tests/live.rs`.

- [ ] Port: `chat_basic` (391, finish `stop`, usage > 0), `chat_system_prompt_followed` (contains `HELLO`, all caps), `chat_max_tokens_truncates` (finish `length`, completion ≤ 16), `chat_stop_sequence` (no `6`), `chat_no_thinking_leak` (no `<think>`), `chat_streamed` (`block_on(checks::chat(&e.ahc, &e.base, &e.cfg.lobo_api_key, &e.alias))`, ≥ 20 chars), `stream_chunks_and_done` (content-type `text/event-stream`, ≥ 3 chunks, `[DONE]` seen, log first-chunk time; read the blocking body with `BufReader::lines()` + `sse_data`), `json_mode` (`response_format json_object`, parse `{name, age}`, age 36, name contains `Ada`).
- [ ] Put `chat_basic`'s body in `fn assert_chat_basic()` so `over_context_fails_cleanly` can call it (Go called `TestChatBasic(t)`).
- [ ] `cargo test -p lobo-e2e --no-run` → compiles. `cargo test -p lobo-e2e` → `13 ignored`.
- [ ] Commit: `lobo-e2e: chat tests`.

## Task 7 — Tool-call tests

**Files:** Modify `crates/lobo-e2e/tests/live.rs`.

- [ ] Port: `tool_call_basic_arguments_is_string` (`block_on(checks::tool_call(&e.ahc, &e.base, &e.cfg.lobo_api_key, &e.alias))` then `checks::validate_tool_call`), `tool_call_picks_right_tool_and_args` (`read_file`, `path` contains `go.mod`, id non-empty, type `function`), `tool_call_not_forced` (no tool calls, `Paris`), `tool_call_round_trip` (assistant msg with tool_calls + tool msg `{"location":"Bali","temp_c":31,"condition":"thunderstorm"}` → finish `stop`, lowercase answer has `31` and `thunder`), `tool_call_streamed` (each `delta.tool_calls[].function.arguments` fragment must be a JSON **string** — decode as `serde_json::Value`, panic if not `Value::String`; concatenated args parse, `location` contains `Tokyo`, name `get_weather`, finish `tool_calls`; lines up to 1 MiB).
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
  test ! -e macos     # removed in P5
  mv tools/protofixtures /tmp/trash/lobocode-protofixtures.$S
  mv tools/corefixtures /tmp/trash/lobocode-corefixtures.$S
  mv tools/clifixtures /tmp/trash/lobocode-clifixtures.$S
  mv crates/lobo-core/tests/go_interop.rs /tmp/trash/lobocode-go_interop.rs.$S
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

- [ ] `rust.yml`: delete every Go step: P1 step 4 (`setup-go` + `make proto-fixtures` diff), P3's core-fixtures drift step and `LOBO_GO_INTEROP=1` run, P4's cli-fixtures step. Path filter: drop `internal/**`, `cmd/**`, `tools/**`; add `app/**`. Check: `grep -rnE 'setup-go|go run|go test|LOBO_GO_INTEROP|protofixtures|corefixtures|clifixtures' .github Makefile crates app` → no hits (except `crates/lobo-e2e` `generates_compiling_go`, which needs a Go toolchain only when run by hand). Steps call `make lint`, `make test`, `make proto-ts && git diff --exit-code app/ui/src/proto`.
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

Publish immutable candidate objects only. Do not change the shared latest pointer.

- [ ] Save `releases/latest.json` bytes/hash privately. Run `bin/lobo release --no-promote` with the repo config. Record the new version as RUST_REL.
- [ ] Fetch `releases/lobo-$RUST_REL.json` (using release::meta_key's exact format) and zip. Verify manifest SHA and zip hash. Re-fetch latest; bytes/hash must be unchanged.
- [ ] Candidate failure needs no public rollback: keep latest unchanged, fix, upload a fresh immutable version and repeat the failed check.

## Task 19 — RunPod live: Rust CLI + Rust agent (zip) + e2e (orchestrator, money-bearing)

Cost: RunPod community $0.69/h (measured 2026-09-23/26). Good host: ready in 50–92 s; a whole check ~10 min ≈ $0.12. Bad community hosts are common (5/5 broken on 2026-09-29), each replace adds minutes; earlier `down` totals were $0.04–0.24. Estimate $0.10–0.40, unmeasured for the Rust build.

- [ ] `DELL up up --provider runpod --release $RUST_REL --plain` → last line has `url=https://<domain>/v1 release=$RUST_REL usd_per_h=0.69 boot=…`. Record boot seconds.
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
- [ ] Codex fails (crash, exit ≠ 0, timeout, quota) → primary-agent self-review by crate, with the failure clearly recorded. Do not claim an independent review passed. Say plainly in `results.md` and the notify: "Codex failed: <reason>. Self-review."
- [ ] While the review runs: read-only prep only. No code changes.

## Task 24 — Triage and fix findings

- [ ] Orchestrator: verify each finding against the code (reviewers can be wrong). List in `results.md` → "Review": id, verdict (holds / false positive + one-line reason), fix commit.
- [ ] Each finding that holds = one implementer task: failing test first, fix, focused test passes, commit `review: <id> <short>`.
- [ ] `make lint test` → green.

## Task 25 — `/simplify` (orchestrator)

- [ ] A direct simplification pass on the branch diff (no unavailable skill required), after Task 24 fixes are in (so the cleanup sees the final shape).
- [ ] Apply what it proposes that keeps behaviour. One commit per item: `simplify: <short>`.
- [ ] `make lint test` → green.

## Task 26 — Re-check after fixes (orchestrator)

- [ ] `git diff <Task 22 sha>..HEAD --stat -- crates app docker Cargo.toml Cargo.lock rust-toolchain.toml .cargo Makefile .github .goreleaser.yaml tools` → which areas changed? (v1.1: every executable, protocol and UI path; Codex finding 8.)
  - `lobo-cli` or `lobo-proto` touched → rebuild `lobo`, rerun the old-data compat checks, Tasks 14–15 (free) + one RunPod smoke below.
  - `app/ui` touched → Task 16 again (free) + re-render the changed states.
  - `lobo-agent`, `lobo-core::{provider,control,bootstrap,release}`, `docker/` touched → one RunPod smoke: rebuild linux `lobo`, `DELL up`, `DELL test`, `DELL down` (+ new `release --no-promote` candidate first if the agent changed, pin its version). ~$0.15.
  - `lobo-core::local` or `app/src-tauri` touched → Task 16 again (free).
  - Nothing touched → no smoke; write "no runtime path changed" in `results.md`.
  - Record the exact tested sha in `results.md`. The merge in Task 28 must be that sha or a descendant that only touched `plans/`/docs (`git diff <tested>..HEAD --stat -- crates app docker Cargo.toml Cargo.lock rust-toolchain.toml .cargo Makefile .github .goreleaser.yaml tools` → empty).
- [ ] Push. `gh run watch` → green.

## Task 27 — QA report (orchestrator)

- [ ] Notify the user with exact tested revision, shipped candidate, smoke results, review result and pending full checks. Full-auto authorization already permits merge and release; do not ask again.
- [ ] Send the first QA notification as soon as the candidate's required CI and smoke checks pass. Finish the full suites, live matrix, failure/recovery tests and review before claiming final acceptance.

## Task 28 — Merge to master (orchestrator)

- [ ] `git fetch origin && git log --oneline feat/rust..origin/master` → empty. Not empty → Tasks 1–2 for the new commits, then Task 26 rules, then continue.
- [ ] `git checkout master && git pull --ff-only && git merge --no-ff feat/rust -m "rust cutover: Go and Swift replaced by the cargo workspace (feat/rust)"`.
- [ ] `make lint test` on master → green.
- [ ] `git push https://github.com/1905/lobocode.git master`. `gh run watch` for each run (rust.yml, pod-image.yml) → green. Red → `gh run view --log-failed`, fix on master, push, repeat.

## Task 29 — First Rust release (orchestrator)

- [ ] `git tag -a v0.2.0 -m "lobo v0.2.0: Rust build"` (chosen in execution.md; check the tag is unused). `git push https://github.com/1905/lobocode.git v0.2.0`.
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
- [ ] After merged/tagged binaries pass release smoke, publish a fresh agent release from that exact source through the normal release command. Verify latest points to its manifest and zip. If promotion fails, keep the old latest and report it; never claim release complete.
- [ ] Replace the local Go installs: `make install` (→ `~/.local/bin/lobo version` = v0.2.0) and `make install-mac` (the Swift app goes to `/tmp/trash` by the target's `mv`). Tell the user both changed.

## Task 30 — Close (orchestrator)

- [ ] Spec: status `done`; "As-built notes" for P6 (e2e config path knob, Dell runner, first Rust version, what the review changed); fix the file-table row (Spec issues 1).
- [ ] Only the active phase plans status → `done`. Preserve superseded review records unchanged.
- [ ] `mkdir -p plans/done && mv plans/2026-09-29-rust-rewrite plans/done/` then `git add -A`. Commit `plans: rust rewrite done`. Push through the verified remote. `gh run watch` (path filters may skip every workflow; then nothing to watch, say so).
- [ ] Dell cleanup: `ssh dell 'mkdir -p /tmp/trash && mv /tmp/lobocode_p6live /tmp/trash/lobocode_p6live.$(date +%Y%m%d-%H%M%S)'`. `ssh dell 'docker ps -a --filter name=lobocode_p6live'` → empty.
- [ ] Project memory: update `lobo-architecture.md` (Rust workspace layout, `make e2e` = `lobo-e2e` with `LOBO_E2E_CONFIG`, released v0.2.0, Go/Swift gone) and `MEMORY.md` line.
- [ ] `/notify`: "Rust cutover merged, v0.2.0 released (brew, image, dmg). Go and Swift gone. Full validation results are in the report. feat/rust is kept."

---

## Historical self-review (before the current correction) (2026-09-29)

- Coverage vs the task brief: e2e port with full name map (23 + setup) ✓; config source checked (Go reads the repo `.env` file via `LoadLaptop`, never OS env) ✓; live sequences for RunPod, Vast, local with markers ✓; costs from measured earlier runs, labelled unmeasured for Rust ✓; Dell pinned-IP workaround ✓; live report template ✓; removal by `mv` + `git add -A`, every path listed ✓; old config, old state file, Go-agent pod checks ✓; exit gate order: live green → Codex review → fix → `/simplify` → re-test + conditional smoke → user approval → merge → `gh run watch` → tag + release → brew check → `plans/done` ✓; merge master + port ledger ✓.
- Order risk checked: Go source is removed only after the master merge (Task 1), the port (Task 2) and the Go reference build (Task 3). Removing earlier would turn every master Go change into a modify/delete conflict and lose the Go binary the compat checks need.
- Money-bearing steps: Tasks 18–21, 26 (conditional). All orchestrator. Budget cap $5. `down` always runs after an `up`.
- Things I did not verify: that `cargo zigbuild test --no-run` (or P2's tool) cross-builds a test binary; that libtest runs tests in sorted order under `--test-threads=1` beyond the `--list` output; that `104.18.54.45` still serves the bucket (Task 3 checks it); that `golang:1.26` declares no `VOLUME` (Task 17 checks it); the P4/P5 Makefile target names.
- Type/name consistency: `lobo_core::config::load_laptop`, `control::HttpAgent::new`, `release::resolve`, `checks::{chat, tool_call, validate_tool_call}`, `lobo_proto::catalog::get`, `Stage::Ready` match contracts v1.0 names. `Laptop` field names (`domain`, `lobo_api_key`, `bucket_url`) are assumed snake_case of the Go fields.

## Contract additions

- `checks::chat` / `checks::tool_call` signatures are `..` in contracts v1.0. `lobo-e2e` needs `(hc: &reqwest::Client, base_v1: &str, key: &str, alias: &str)`. Fold into v1.1 as P3 shipped them.
- `HttpAgent::new(base, key)`: contracts say base URL, Go took a domain. Confirm in v1.1.
- `Laptop` field names: contracts say "same env names", which reads as env keys, not Rust fields. Pin the field names in v1.1.

## Historical spec issues (resolved by execution.md unless marked pending)

1. File table says Go/Swift are removed "in the phase that replaces each part". P1 and P6 remove everything at P6 (the P1 drift CI needs Go; earlier removal breaks `git merge master`). Fix the row.
2. P6 has no live check of the Tauri app. This plan adds one screenshot (app shows a CLI-started local run). A Start/Stop from the panel is still only covered by P5 renders.
3. The e2e suite cannot target local mode: it needs `https://LOBO_DOMAIN` and a `5090`. "e2e on one of them" can only mean a cloud pod.
4. New knob `LOBO_E2E_CONFIG` (a path) is needed to run the suite on the Dell. Not in the spec.
5. Testing the Rust zip path before merge means `lobo release`, which moves `latest.json` for master's Go CLI too. "Master keeps the working Go build" holds only because P2 proved Go CLI → Rust agent. Needs an explicit user OK.
6. The spec names no version for the first Rust release. Plan proposes v0.2.0.
7. Linux cross-builds of `lobo` and the e2e test binary on the Mac depend on P2's musl toolchain choice. The spec does not say which.
8. README screenshots `docs/img/*.png` are Swift renders; the spec file table does not list `docs/img`.
9. Local mode had never run live on Go (project memory). Task 15 is the first live Go local run, so a Go-side local bug could block a Rust compat check.

## Review fixes v1.1

Codex plan review of the v1.0 bundle (session f79006e4, 6/10). Confirmed findings applied here:
- F7 (high): cutover removes all three Go fixture generators, their Makefile targets and CI steps, and `go_interop.rs`; a grep check proves CI needs no Go.
- F8 (med): the post-fix smoke rule covers every path under `crates`, `app`, `docker`; the tested sha is recorded and the merge must match it.
- Open item 1: `macos/` is removed in P5; P6 verifies absence.

Contract alignment (contracts v1.1):
- Item 27: Task 4 `Env` gains `pub ahc: reqwest::Client` (`lobo_core::http::client(5 min)`); Tasks 5–7 pass it to `release::resolve`, `checks::chat`, `checks::tool_call`.
- Doc ref: header `contracts.md v1.0` → `v1.1`.

## Current execution additions (v1.2)

- Source/build changes invalidate release evidence too. Task 26 includes all manifests, lockfiles, toolchain files, generators and workflow scripts. Documentation-only descendants may reuse smoke evidence only after checking this full path set.
- P6 Task 8 uses one shared Tokio runtime that stays alive for Env async clients. Do not create/drop a runtime for each block_on call. Run metrics-last explicitly in the e2e driver, rather than assuming libtest ordering.
- Task 18 does not promote latest. No approval or rollback by rebuilding Go is required. The final tagged Rust release is the promotion boundary.
- Normal repository CI delivers candidates. There is no Jira issue identified for this repository; do not post to unrelated Sputnik issues. Notify via the installed notify skill at QA readiness and final completion.
