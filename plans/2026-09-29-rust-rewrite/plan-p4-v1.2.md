# Rust rewrite P4 — `lobo-cli` (binary `lobo`) Implementation Plan v1.2

**Date:** 2026-09-29
**Status:** approved for implementation after plan correction (user: full auto, 2026-09-29). Earlier review covered v1.0 only.
**Spec:** ./spec.md (full-auto implementation authorized, 2026-09-29)
**Contracts:** ./contracts.md v1.2. P4 uses those names; its own extras are at the end.
**Phase:** P4 of 6. Needs P1 (`lobo-proto`) and P3 (`lobo-core`) done on `feat/rust`.
**Split with P3:** pre-checks, `apply_defaults`, provider wiring, opencode generation, masking, `config::show` and `RunConfig` parsing/validation live in `lobo-core` (P3 Tasks 13, 34, 45, 62, 63) because the app needs them too. Their Go tests are ported in P3. P4 calls them and tests only the CLI boundary (flags → arguments, output bytes, exit codes).

**Goal:** a Rust `lobo` binary with the same commands, flags, stdout, stderr messages and exit codes as Go `cmd/lobo`. That includes the `up` progress view, the live `status` dashboard, the `lobo config` wizard, every `--json` output, `release`, the hidden `local run` supervisor entry and a brew formula built from Rust.

**Architecture:** `crates/lobo-cli` is a lib (`lobo_cli`) plus a bin (`lobo`). All logic sits in the lib behind one `App` struct with injected seams (deps factory, local-support check, supervisor, clock, TTY flags), so tests run commands in-process. `main.rs` only builds `App::real()` and calls `lobo_cli::run`. Parity is proven three ways: (1) the Go goldens and Go-captured fixtures, byte-for-byte where the Go output is lobo-authored; (2) a replay of ~40 no-network Go CLI runs (`cases.json`) against the Rust binary with `assert_cmd`; (3) a port of every Go test in `cmd/lobo`, `internal/tui`, `internal/configtui`. Go stays on the branch untouched (except one build-tagged capture test) until P6.

**Tech Stack:** Rust 1.98.1 (edition 2024). clap (derive, no `env` feature), clap_complete, ratatui + crossterm, inquire, anyhow, tokio, tokio-util, tracing + tracing-subscriber, chrono, serde_json, unicode-width. Dev: insta, assert_cmd, predicates, tempfile, serial_test, wiremock. Release: goreleaser 2.13 `builder: rust` + cargo-zigbuild + zig (spike in Task 47 decides).

> Execution: the primary agent implements task-by-task and records checked work. No unavailable skill or model is required.

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
7. **Extra positional args:** Go leaf commands without an `Args` rule silently ignore extra args (`lobo up foo` runs `up`). Rust preserves this behavior. Commands with explicit Go Args rules keep those rules. Capture extra-argument cases in the Go replay fixtures.
8. **Wizard:** inquire asks one prompt at a time. Same questions, order, show/hide rules, validation messages, summary and saved result as the huh form. No Shift+Tab back navigation (inquire has none); Esc or Ctrl+C quits without saving. The intro note text changes its last line accordingly.
9. **Test seams:** in-process `lobo_cli::run(&App, argv, &mut Io)` for unit tests. For `assert_cmd` against fakes, the cargo feature `test-fakes` (never on in release builds) lets the binary read `LOBO_TEST_SCENARIO` and swap in `lobo_core::control::testkit` fakes. `make rust-build-lobo` and goreleaser build default features only; Task 51 checks the release binary ignores the variable.
10. **Parity baseline** is `master` at P4 start, `3117f9b5a64beb466c5b78d5f69bed5bdbb115e0`, not `5443667`: commit `6a72092` changed `internal/tui/status.go` and added `status_local.golden` after `5443667`.

## Pinned versions

Cargo.lock pins: clap 4.6.7, clap_complete 4.6.11, ratatui 0.30.2, crossterm 0.29.0, inquire 0.9.4, insta 1.48.0, assert_cmd 2.2.2, predicates 3.1.4, anyhow 1.0.104, tracing 0.1.44, tracing-subscriber 0.3.23, unicode-width 0.2.2, serial_test 3.5.0, wiremock 0.6.5, tempfile 3.27.0.

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
| G-11 | Extra positional args on leaf commands: Go and Rust both ignore them where Go has no Args rule (Decision 7). `local run`, `models` reject in Go too (`cobra.NoArgs`). | local.go:67,153 |
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
| R-03 | Agent build: Go `go build … ./cmd/lobo-agent`. Rust: `cargo zigbuild --release --locked -p lobo-agent --target x86_64-unknown-linux-musl --target-dir <top>/target` in the git top-level dir, env `LOBO_VERSION={ver}`, its stdout+stderr → our stderr, failure `build agent: {err}`; binary copied from `target/x86_64-unknown-linux-musl/release/lobo-agent`. Manifest: `built_at` = now UTC truncated to the second, `llama_image` = `DEFAULT_LLAMA_IMAGE`, model ref from `catalog::get(DEFAULT_MODEL)`, `defaults` = `DEFAULT_DEFAULTS`. | main.go:136-161 |

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
| U-08 | TUI quit keys `q`, `ctrl+c` cancel the owned operation and wait for cleanup. Exit 1 with `interrupted: startup cancelled and cleanup completed` only after successful cleanup; otherwise report the cleanup failure. TUI error state exits 1 with that error. This deliberately changes Go behavior. | v1.2 cancellation contract; tui/up.go:177,192-196 |
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
| T-02 | `apply_at`: timers from arrival times; going back to an earlier phase resets that phase and every later one; `failed`/`terminated`/`cancelled` never change the phase; `err` sticks; `done` sticks. | tui/up.go:37-71 |
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
- [ ] contracts.md is v1.2 with the P3 additions and this plan's additions folded in, and P3 shipped them (`lobo_core::control::testkit` behind feature `testkit`, `Wiring`, `control::{check_*, apply_defaults}`, `config::{mask, masked, LAYOUT, loose_mode}`, `genkey::{ensure_api_key, write_opencode, OPENCODE_OUT}`, `local::RunConfig::validate`). If not, stop: P4 cannot build on the v1.0 signatures (see "Spec issues" 3).
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
    pub cancel: CancellationToken, // main owns signals; tests cancel without global signal handlers
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

- [ ] Args structs with fields declared in cobra's alphabetical flag order, help strings copied verbatim from the inventory rows, defaults as in Go (`cloud = "community"`, `n = 200`, `model = "q6"`, `port = 8931`, `api_port = 8932`). `--max-life` uses `duration::parse_go_duration` (Task 11). `logs`: `#[arg(short = 'n', long = "n")]`. Hidden: `release`, `local`, `completion`. `config get`: exactly one `KEY`. Go leaf commands without an Args rule accept ignored positional tails (G-11); `config set` consumes its values, while `config get`, `models` and `local run` enforce their explicit argument rules.
- [ ] Failing tests: parse each `cases.json` args vector that exits 0 in Go without error-from-parser; `lobo up extra` → parse success with ignored extra argument (G-11); `lobo logs -n 5` and `--n 5` → 5; `lobo config get` → error; `lobo local run --ctx 1 --idle-min 1 x` → error.
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
- [ ] Failing tests: `show_text_masks` (the TestMaskedUnknownKeys keys in a file → text shows clear/masked as the Go table says); `show_text_layout` (groups, `# other` sorted, empty file line); `cli_go_replay::replay_go_cli` (show JSON cases) (assert_cmd, file with `RUNPOD_API_KEY=rpa_SECRETSECRETSECRET`, `LOBO_DOMAIN=lobo.x.cc`, `LOBO_CTX=`): no `SECRETSECRET`, `"LOBO_DOMAIN":"lobo.x.cc"`, `"LOBO_CTX":false`, `"exists":true`; missing file → `"exists":false`, `"values":{}`.
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
pub enum PromptError { Aborted, Failed(anyhow::Error) }
pub trait Prompter {
    fn note(&mut self, title: &str, body: &str) -> Result<(), PromptError>;
    fn secret(&mut self, title: &str, desc: &str, check: &dyn Fn(&str) -> Result<(), String>) -> Result<String, PromptError>;
    fn text(&mut self, title: &str, desc: &str, initial: &str, check: &dyn Fn(&str) -> Result<(), String>) -> Result<String, PromptError>;
    fn select(&mut self, title: &str, desc: &str, options: &[(&str, &str)], current: &str) -> Result<String, PromptError>;
    fn confirm(&mut self, title: &str, desc: &str, yes: &str, no: &str, default: bool) -> Result<bool, PromptError>;
}
pub fn run_wizard(p: &mut dyn Prompter, path: &Path, cur: BTreeMap<String,String>, local_ok: bool) -> anyhow::Result<Option<BTreeMap<String,String>>>; // None = quit or Discard; terminal failures stay errors
```
Group titles (`1/4 · GPU providers` …) are passed as a prefix of each prompt title: `"{group} · {field}"`. Intro note last line: `Enter: next · Esc: quit without saving`.
- [ ] Failing tests with a `Scripted` prompter (records every title, answers from a queue, asserts the queue ends empty): `form_builds` (both `local_ok` values, answers all defaults, `Some(result)`); `flow_order_cloud` (fresh file, not capable: exact title list); `flow_order_local` (capable Mac: where group first, no pick group); `flow_pick_shown_with_two_keys`; `abort_mid_way_saves_nothing`; `discard_saves_nothing`; a validator rejection is re-asked (Scripted returns a bad then a good value; asserts the check was called with both).
- [ ] Verify: `cargo test -p lobo-cli wizard::flow` → ok.
- [ ] Commit: `lobo-cli: wizard flow, same screens and order as the huh form`.

## Task 26 — inquire prompter + `config` command

**Files:** `src/wizard/prompt.rs`, `src/cmd/config.rs`. Rows: C-01, C-02, C-03.

`InquirePrompter` maps: `note` → print styled title + body; `secret` → `Password::new(title).with_help_message(desc).without_confirmation().with_display_mode(Masked).with_validator(..)`; `text` → `Text` with `initial_value`; `select` → `Select` with `starting_cursor` at `current`; `confirm` → `Select` over `[yes, no]` (inquire `Confirm` has no custom labels). `InquireError::OperationCanceled | OperationInterrupted` → `PromptError::Aborted`; other errors → `PromptError::Failed`. Render config accent #7D56F4.
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
`UpState::default().at` = `DateTime::<Utc>::MIN_UTC`. `failed`/`terminated`/`cancelled` never change the phase (T-02; `cancelled` is the last event of a cancelled `control::up`).
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
- [ ] Failing tests: `Event(None)` → Quit; a `done` event → Quit; `q`/ctrl+c → Quit with `interrupted = true`. The command emits U-08 only after owned cleanup succeeds. `Tick` advances the frame and sets `state.at`.
- [ ] Verify: `cargo test -p lobo-cli tui::up::tests::model` → ok.
- [ ] Commit: `lobo-cli: up model`.

## Task 36 — TUI terminal loops

**Files:** `src/tui/run.rs`. Rows: U-05, U-08, S-02.

```rust
pub async fn run_up(rx: &mut mpsc::Receiver<UpEvent>, ready: &mut Option<ReadyInfo>, clock: Arc<dyn Clock>, cancel: &CancellationToken, no_color: bool) -> anyhow::Result<()>;
pub async fn run_status(deps: &Deps, cancel: &CancellationToken, no_color: bool) -> anyhow::Result<()>;
pub fn draw_up(f: &mut Frame, m: &UpModel);  pub fn draw_status(f: &mut Frame, m: &StatusModel, tz: &chrono::Local);
```
ratatui `Terminal::with_options(Viewport::Inline(h))` (bubbletea default is inline, output stays in scrollback), `h` = rendered line count capped at the terminal height; raw mode on, restored on every exit path (guard type with `Drop`). Up ticks every 100 ms (spinner); status fetches at start and every 2 s via `control::snapshot`.

Implementation note: Ratatui 0.30.2 cannot change an inline viewport's configured height through `resize`. Recreate the terminal at its prior origin when content height changes. Poll crossterm input without blocking on each 100 ms tick; do not mix EventStream with the cursor queries used by inline initialization/resizing. The command retains the event receiver and UpOperation through cleanup. A consumer error or quit requests cancellation, then completion is awaited; cleanup failures take precedence.
- [ ] Failing tests (TestBackend, fixed size): `draw_up` of the `up_ready` state on `TestBackend::new(120, 20)` → `insta::assert_snapshot!(terminal.backend())`; `draw_status` of `status_ready` on `TestBackend::new(80, 40)` with `Utc`.
- [ ] Verify: `cargo test -p lobo-cli tui::run` → ok.
- [ ] Orchestrator (manual, free): a small `#[ignore]` example `cargo run -p lobo-cli --example tui_demo` that feeds the boot script into `run_up` with a 300 ms delay per event; watch it once in a real terminal; check the screen is restored after `q`.
- [ ] Commit: `lobo-cli: ratatui loops for up and status`.

## Task 37 — Boot report + `boots.jsonl`

**Files:** `src/bootlog.rs`. Rows: U-09.

```rust
// Consumers record ReadyInfo directly while reading events. No detached tee task.
#[derive(Serialize)] pub struct BootLine<'a> { pub at: GoTime, pub conns: i64, pub ready: &'a ReadyInfo, pub source: &'a str } // alphabetical = Go map order
pub fn report_boot(w: &mut dyn Write, r: Option<&ReadyInfo>, source: &str, conns: i64, log: &Path, now: DateTime<Utc>) -> std::io::Result<()>;
```
Table = Go `tabwriter(minwidth 0, tabwidth 0, padding 2, ' ')` with a trailing empty cell: every column padded to its widest cell (rune count) + 2.
- [ ] Failing tests: `report_boot` stderr bytes == `fixtures/go/text/report_boot.txt`; appended line JSON-equal to `boots_line.json`, keys in order `at, conns, ready, source`; two calls → two lines; `None` or no timings → writes nothing.
- [ ] Verify: `cargo test -p lobo-cli bootlog` → ok.
- [ ] Commit: `lobo-cli: boot timings report and boots.jsonl`.

## Task 38 — `up`: plain and JSON consumers

**Files:** `src/cmd/up.rs`. Rows: U-06, U-07, J-01.

```rust
pub async fn json_up(rx: &mut mpsc::Receiver<UpEvent>, out: &mut dyn Write, ready: &mut Option<ReadyInfo>) -> anyhow::Result<()>;  // "up failed" if any err
pub async fn plain_up(rx: &mut mpsc::Receiver<UpEvent>, ready: &mut Option<ReadyInfo>) -> anyhow::Result<()>;                      // logs via tracing
```
- [ ] Failing tests: `json_up` over the Go-captured events (decode `json_up_boot.jsonl` into `UpEvent`s, feed a channel) → output lines JSON-equal to the file; the failed file → `Err("up failed")`. `plain_up` with the `logfmt` layer on a buffer (`tracing::subscriber::with_default`) → lines equal to `plain_up_boot.txt` / `plain_up_failed.txt` after timestamp strip.
- [ ] Verify: `cargo test -p lobo-cli cmd::up::tests::consumers` → ok.
- [ ] Commit: `lobo-cli: up --json and --plain output`.

## Task 39 — `up` command glue

**Files:** `src/cmd/up.rs`. Rows: U-02, U-05, U-08, U-09.

Order exactly U-02. Mode switch U-05 (`term.stdout_tty`). Presign: `if o.provider == "local" { deps.presign = None }`. `report_boot(stderr, ready, &o.source, o.conns, &app.boot_log, clock.now())` after the mode and owned worker finish, before returning their error. `App.boot_log` defaults to `boots.jsonl`; tests inject a temporary path without changing the process working directory. `--ssh` read error → returned as is.
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
- [x] Verify: `cargo test --locked -p lobo-cli --test agent_commands` → four tests pass. Requests are cancelled without leaving work running; exact log bytes and UTF-8 preview boundaries are covered.
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
- [x] Verify: `cargo test --locked -p lobo-cli cmd::release` → three tests pass, including a loopback store check that parses --no-promote and rejects any latest write. No test runs a live publish. Await the build child and any started publication; cancellation is checked before publication. Explicit target-dir prevents inherited CARGO_TARGET_DIR from changing the binary path.
- [ ] Commit: `lobo-cli: release command (agent via cargo zigbuild)`.

## Task 44 — completion + SIGINT

**Files:** `src/cmd/completion.rs`, `src/lib.rs`. Rows: G-07, G-08.
- [ ] `completion <shell>` via `clap_complete::generate` for bash, zsh, fish, powershell; hidden.
- [ ] `run` uses `App.cancel`; `tokio::signal::ctrl_c` cancels it; passed to `control::up` and used as a `select!` arm around read-only `snapshot`, `target`, `logs`, `test`. Check cancellation before `down`; once deletion begins, await deletion and verification even after Ctrl-C. Dropping an issued delete would lose cleanup ownership.
- [ ] Failing tests: `completion zsh` stdout non-empty and contains `lobo`; `completion` absent from `root_help`; cancelling the token during a fake `up` ends the stream and returns an error (exit 1).
- [ ] Verify: `cargo test -p lobo-cli cmd::completion && cargo test -p lobo-cli app::tests::cancel` → ok.
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
- [ ] Failing tests (`cargo test -p lobo-cli --features test-fakes --test cli_fakes`), each with a temp RunPod-only config: `LOBO_TEST_SCENARIO=boot lobo up --json` → stdout JSON-equal to `json_up_boot.jsonl`, exit 0; `failed` → `json_up_failed.jsonl`, stderr `error: up failed`, exit 1; `running lobo status --json` → `status_running.json`; `down lobo status --json` → `status_down.json`; `running lobo down --json` → `down_running.json`; `boot lobo up --plain` → stderr log lines equal to `plain_up_boot.txt` after timestamp strip, and a boot timing table at the end (zero timings from BootScript; Task 37 byte-checks the representative nonzero table); `boot lobo up --json --q6=false` → `changed` has q6, model stays release default (Swift app call shape).
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
    flags: [--release, --locked, --package=lobo-cli]
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
- `release.yml` job `release`: `runs-on: macos-15` (native Apple SDK for the darwin targets; cargo-zigbuild for linux musl); steps: checkout (fetch-depth 0) → `dtolnay/rust-toolchain@master` with explicit 1.98.1 (same pin as `rust-toolchain.toml`), targets the four triples → `mlugg/setup-zig@v2` with version 0.16.0 → `taiki-e/install-action@v2` with `tool: cargo-zigbuild@0.23.4` → `Swatinem/rust-cache@v2` → `goreleaser/goreleaser-action@v6` (`version: "v2.13.3"`, `args: release --clean`, same env). `setup-go` removed from this job. The `dmg` job is unchanged (P5 owns it).
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
- [ ] Plan status → `done`. `/notify`: "Rust P4 done on feat/rust: lobo CLI parity (Go replay + goldens + JSON), brew via <A|B>. Try `make rust-install` → `lobo-rs`. Continuing P5 under full-auto authorization."

---

## Go test → Rust test (every Go test function in cmd/lobo, internal/tui, internal/configtui)

"P3" = the Go test's logic moved to `lobo-core` and is ported in `plan-p3-v1.2.md` (its table, lines 1240-1256). P4 then adds only a CLI-boundary test, named in the third column.

| Go test (file:line) | Rust home of the Go assertions | P4 CLI-boundary test | Task |
|---|---|---|---|
| TestApplyDefaults (cmd/lobo/defaults_test.go:12) | P3 `control::precheck::tests::apply_defaults_table` | `cli::tests::explicit_flag_names_and_globals`; `cli_boundary::defaults_keep_explicit_false_and_flag_precedence` | 16 |
| TestMaskedUnknownKeys (defaults_test.go:62) | P3 `config::show::tests::masked_unknown_keys` | `cli_go_replay::replay_go_cli` (show text cases) | 21 |
| TestParseSetArgs (defaults_test.go:72) | `cmd::config::tests::parse_set` (argv rows) | — | 20 |
| TestShowConfigJSONMasks (defaults_test.go:84) | P3 `config::show::tests::show_masks_secrets` | `cli_go_replay::replay_go_cli` (show JSON cases) | 21 |
| TestParseSetJSON (defaults_test.go:96) | `cmd::config::tests::parse_set` (stdin rows) | — | 20 |
| TestRootHelpGolden (help_test.go:15) | `help::tests::root_help_golden` | — | 13 |
| TestLocalRunFlags (local_test.go:16) | P3 `local::supervise::tests::run_config_from_args_table` | `cli_boundary::local_run_flags_and_config_forwarding` (7 rows through clap + the "local is hidden" assert) | 29 |
| TestModelsOutput (local_test.go:58) | `cmd::models::tests::models_output` | — | 28 |
| TestCheckTarget (target_test.go:25) | P3 `control::precheck::tests::check_target_table` | `cli_boundary::gates_reject_before_dependencies` (up row) | 15 |
| TestCheckProviders (target_test.go:51) | P3 `control::precheck::tests::check_providers_table` | `cli_boundary::gates_reject_before_dependencies` (down, status rows) | 15 |
| TestCheckRelease (target_test.go:75) | P3 `control::precheck::tests::check_release_table` | `cli_boundary::gates_reject_before_dependencies` (release row) | 15 |
| TestProviders (target_test.go:96) | P3 `control::wiring::tests::providers_from_config_table` | `cli_boundary::local_run_flags_and_config_forwarding`; P3 `deps_from_config_sample` | 17 |
| TestLocalInstanceURLs (target_test.go:140) | P3 `control::wiring::tests::local_instance_urls_from_state` | — (no CLI logic) | — |
| TestWriteOpencode (target_test.go:152) | P3 `genkey::tests::write_opencode_table` | `cli_boundary::gen_key_reuse_rotate_and_permissions` (key keep/rotate, messages, file mode) | 27 |
| TestUpBootContainerHintAndReRentReset (internal/tui/tui_test.go:59) | `tui_parity::up_goldens` + `tui_parity::up_state_timers_and_terminal_events` | — | 31, 32 |
| TestUpGolden (tui_test.go:75) | `tui_parity::up_goldens` (3 goldens) | — | 32 |
| TestStatusGolden (tui_test.go:96) | `tui_parity::status_goldens` (5 goldens) | — | 33 |
| TestUpStateErr (tui_test.go:120) | `tui_parity::up_state_timers_and_terminal_events` | — | 31 |
| TestStatusModelKeepsLastSnapOnError (tui_test.go:129) | `tui_parity::models_keep_errors_and_handle_keys` | — | 34 |
| TestMask (internal/configtui/configtui_test.go:8) | P3 `config::show::tests::masked_unknown_keys`; P4 `wizard::state::tests::summary_masks_secrets_and_local_rows` | — | 23 |
| TestResultKeepClearAndDefaults (configtui_test.go:16) | `wizard::state::tests::result_keep_clear_and_defaults` | — | 22 |
| TestNewStateFreshFile (configtui_test.go:39) | `wizard::state::tests::new_state_fresh_file_and_local` | — | 22 |
| TestValidators (configtui_test.go:49) | `wizard::validate::tests::validators` | — | 23 |
| TestSummaryMasksSecrets (configtui_test.go:64) | `wizard::state::tests::summary_masks_secrets_and_local_rows` | — | 24 |
| TestNewStateLocal (configtui_test.go:72) | `wizard::state::tests::new_state_fresh_file_and_local` | — | 22 |
| TestResultLocal (configtui_test.go:94) | `wizard::state::tests::result_local` | — | 22 |
| TestLocalValidation (configtui_test.go:124) | `wizard::validate::tests::local_validation_and_options` | — | 23 |
| TestSummaryLocalRows (configtui_test.go:159) | `wizard::state::tests::summary_masks_secrets_and_local_rows` | — | 24 |
| TestProviderOptions (configtui_test.go:169) | `wizard::validate::tests::local_validation_and_options` | — | 23 |
| TestFormBuilds (configtui_test.go:181) | `wizard::flow::tests::flow_order_cloud_and_local` | — | 25 |

30 Go tests (measured: `grep -c '^func Test'` = 14 + 5 + 11), none dropped: 21 ported in P4, 9 ported in P3 (+ the TestLocalRunFlags rows), with P4 boundary tests for 8 of those 9. Go lines: cmd/lobo tests 450, internal/tui 144, internal/configtui 187 = 781. New Rust-only tests (help facts, Go replay, JSON on fakes, TUI TestBackend, log format, duration, boot report) sit in Tasks 6–46.

---

## Spike result

_(Task 47 and Task 49 fill this: chosen option, the six check lines, the brew install lines.)_

---

## Historical self-review (before the current correction)

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

## Historical spec issues (resolved by execution.md unless marked pending)

1. **Parity baseline is stale.** Spec says `5443667`; `6a72092` (branch `fix/app-silent`, not on master yet) changed `internal/tui/status.go` and added `status_local.golden`. P4 uses master HEAD at P4 start (Decision 10). Spec Goal 4 should say so.
2. **Go removal timing conflicts.** The file table says Go parts go "in the phase that replaces each part"; the rollout says P6, and P1 keeps Go for its drift CI. If P3 removed `internal/*`, Go `cmd/lobo` would not compile before P4. P4 assumes removal at P6 (Decision 3). Fix the file-table wording.
3. **contracts v1.0 is not enough for P4.** `deps_from_config(cfg)` has no config path, spawner or local seam, and there are no fakes for the TUI goldens. The P3 plan's additions fix this; P4 depends on them plus its own 5 small additions. P1's Go-test table still assigns all `cmd/lobo` tests to P4; the P3 plan moved 9 of them. P1's table should point to P3.
4. **Makefile has no phase.** P4 adds `rust-*` targets and installs `lobo-rs`; the swap is P6 (Decision 2). Spec rollout P6 should list it.
5. **cargo-dist fallback needs a user action.** Its homebrew publish uses a PAT secret, not the tap deploy key, which breaks "You do: nothing". P4 inserts goreleaser `prebuilt` (B) before cargo-dist (C).
6. **`lobo release` needs a musl cross toolchain on the Mac** (zig + cargo-zigbuild). The spec only builds the agent in Docker. New dev dependency; `release` errors with an install hint when it is missing.
7. **Extra positional args** (Decision 7): Resolved: preserve Go behavior, including `lobo up foo`. No strictness change.
8. **Wizard back navigation** (Decision 8): huh's Shift+Tab back is lost with inquire.
9. **Byte parity limits:** cobra-formatted help, parser error texts, OS error texts, JSON escapes and log colours are not byte-identical (Decisions 1, 4, 5, 6). The spec's "same output" should name these exceptions.

## Contract alignment v1.1

- Item 11: row T-02 and Task 31: `failed`/`terminated`/`cancelled` never change the phase.
- Doc ref: header → `contracts.md v1.1` (P3 additions are folded in there).
- Doc ref: Go-test note → `plan-p3-v1.2.md` (its table, lines 1240-1256).

## Current execution additions (v1.2)

- Task 8: use hidden positional tails only on the Go commands that ignore extra arguments. Keep unknown flags as errors. Capture args before/after flags and after `--` in Go fixtures.
- Release command: add hidden `--no-promote` (false by default). True calls Store::publish_version; ordinary release calls Store::publish. Help/release defaults stay compatible. Add a fake-store test that forbids any latest write.
- Tasks 36–39: retain the UpOperation outside the event renderer. After EOF, Ctrl+C, quit, or stdout failure, cancel as needed and await completion before exit. Event tee/render tasks never own the worker. A successful ready event cannot hide a failed completion. Test plain, JSON and TUI cancel with a 121 s rent and failed cleanup.
- Keep the one final external code review at P6. No new model review spend per phase.


## As built — first P4 batch, 2026-09-30

- Implemented crate/build info, full flag tree, help metadata checks and snapshots, duration parsing, log formatting, CLI I/O/error seams, config commands/wizard, API-key generation, model listing and local supervisor entry.
- Go fixture capture: 53 no-network replay cases, 19 help outputs, 9 control/report outputs and 10 copied goldens. Repeated generation produced no drift. Go vet with the capture tag passed.
- All 53 replay cases passed. Root help matches Go bytes. Help metadata matches 17 command paths. Local supervisor tests verify exact config forwarding and reject invalid flags before spawning.
- The actual inquire terminal UI passed Save and Escape smoke checks in isolated pseudo-terminals. Save wrote a 0600 file; Escape wrote nothing.
- Prompt errors now distinguish user cancellation from terminal failures. Inquire 0.9 Password requires a static validator; the adapter checks borrowed form state after each masked prompt and re-prompts without printing the secret. Text validators remain in inquire.
- Wizard prices reject non-finite values, matching core validation. App carries a CancellationToken so main and tests own cancellation explicitly.
- Live control commands, terminal dashboards, release packaging and fake-provider binary tests remain pending. Those command handlers currently return explicit implementation-pending errors after prechecks. This batch is not a replacement for the Go CLI.

- GoReleaser 2.13.3 Rust builder checks only `-p=` or `--package=` in a virtual workspace. The first spike failed before compiling. Use `--package=lobo-cli`, verified against https://github.com/goreleaser/goreleaser/blob/v2.13.3/internal/builders/rust/build.go#L210. Four-target archive validation remains pending.

## P4 acceptance evidence — current implementation

Named tests below supersede the task drafts' proposed test paths. Packaging rows B-01–B-04 remain open until the spike and Homebrew checks finish.

| Inventory rows | Evidence |
|---|---|
| G-01–G-06, G-09, G-11–G-12, V-01 | `cli_go_replay::replay_go_cli` (53 Go cases), `help_parity::{help_facts_match_go, help_snapshots}`, `cli_boundary::run_errors_and_root_help`, `help::tests::root_help_golden` |
| G-07 | `cli_control::{cancel_awaits_a_121_second_create_and_reports_cleanup_failure, down_keeps_issued_deletes_owned_after_ctrl_c}`; `agent_commands::cancelling_read_only_commands_stops_pending_http`; isolated Ctrl-C PTY run |
| G-08 | `cli_boundary::completion_emits_scripts_for_all_supported_shells` |
| G-10 | `cli::tests::{tree_and_booleans, explicit_flag_names_and_globals}`; `cli_boundary::defaults_keep_explicit_false_and_flag_precedence` |
| G-13 | `logfmt` unit test; `cli_control::consumers_match_go`; `cli_fakes::executable_plain_output_and_unknown_scenario` |
| C-01–C-09, J-03 | Go replay config cases; `cmd::config::tests::parse_set`; `cli_boundary::config_wizard_save_abort_discard_and_failure` |
| K-01, W-07 | `cli_boundary::gen_key_reuse_rotate_and_permissions`; P3 API-key tests |
| R-01–R-03 | `cli_boundary::gates_reject_before_dependencies`; `cmd::release::tests::{git_info_clean_dirty_and_missing, build_command_and_manifest, candidate_flag_never_writes_latest}`; P3 store tests cover candidate isolation and failed publication. Live publication remains P6. |
| U-01–U-09, J-01 | Help facts, Go replay, all eight `cli_control` tests, `cli_fakes::executable_json_outputs_match_go`; ready/q/Ctrl-C/resize PTY runs |
| D-01–D-02, J-05 | `cli_control::{status_and_down_commands_match_go, down_keeps_issued_deletes_owned_after_ctrl_c}`; executable JSON replay |
| S-01–S-02, J-02 | `cli_control::status_and_down_commands_match_go`; `tui_parity::{status_goldens, models_keep_errors_and_handle_keys}`; executable JSON replay; status-q PTY run |
| L-01, E-01 | Four `agent_commands` loopback HTTP tests |
| M-01, J-04 | `cmd::models::tests::models_output`; Go replay models cases |
| LR-01–LR-03 | `cli_boundary::local_run_flags_and_config_forwarding`; P3 supervisor tests |
| T-01–T-05 | All five `tui_parity` tests (eight Go goldens, ten snapshots); `tui::styles::tests::durations_numbers_and_bars` |
| W-01–W-06 | Four `wizard::state` tests, two `wizard::validate` tests, three `wizard::flow` tests; isolated Save/Escape PTY runs |

The external review remains scheduled once, in P6. Local checks do not establish live provider or app acceptance.

- Acceptance audit added explicit completion smoke coverage for bash/zsh/fish/PowerShell and a CLI-to-store candidate publication test. Both candidate and ordinary publication paths are exercised against loopback HTTP only.
