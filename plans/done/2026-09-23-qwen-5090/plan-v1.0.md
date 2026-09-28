# Lobo Implementation Plan v1.0

**Date:** 2026-09-23
**Status:** superseded by v1.1 (reviewed: Astra 5/10)
**Spec:** ./spec.md (approved rev 2)

**Goal:** `make up` rents a RunPod 5090 that serves Qwen3.5-27B uncensored at `https://lobo.example.com/v1` on its own, kills itself when idle, and reports its release via `/api/version`; `make status/down/release` control it from the laptop.

**Architecture:** Two Go binaries in one module. `lobo` (laptop, cobra + bubbletea) talks only to the RunPod REST API, R2 and the pod's `/api/*`. `lobo-agent` (pod, PID 1, shipped in a zip from private R2 bucket `lobo`) runs cloudflared, downloads the GGUF, supervises `llama-server` from the official `ghcr.io/ggml-org/llama.cpp:server-cuda-*` image, serves `/api/*` and self-terminates.

**Tech Stack:** Go 1.26 · `spf13/cobra` · `rs/zerolog` · `joho/godotenv` · `go-playground/validator/v10` · `charmbracelet/bubbletea` + `bubbles` + `lipgloss` · `minio/minio-go/v7` (R2 S3 API) · llama.cpp `server-cuda` image · cloudflared.

> For agentic workers: use superpowers:subagent-driven-development to implement task-by-task. Checkbox syntax for tracking.

## Exec rules (apply to every task)

- Branch `feat/lobo-v1` off `master` in the main checkout. No worktrees. Commit per task only after its review is clean.
- Tasks tagged **[ORCH]** are live/money/infra: only the orchestrator (Opus/Fable) runs them. Tasks tagged **[IMPL]** may go to any implementer model.
- Every Sonnet dispatch prompt includes verbatim: "You write only the code and unit tests this task names and run only this task's focused unit test. You never run e2e/integration/live/smoke tests, never rent a GPU or pod, never call RunPod/Cloudflare/R2/HF or any provider API, never publish, deploy or touch infra, never run anything that costs money."
- Sonnet-written tasks get `/rival-fable` on the diff before the next dispatch; HIGH/CRIT fixed first.
- Keys come from this repo's `.env` only. Never the OS environment. Never commit `.env`.
- No `rm`: move to `/tmp/trash/`. No Docker pulls/runs on the laptop.
- Test commands: `go test ./internal/<pkg>/... -run <Name> -v`. Build: `go build ./...`. Lint: `golangci-lint run ./...`.

## File map

**Create**
```
go.mod, go.sum, Makefile, .golangci.yml, .env.example, README.md, opencode.json.example
cmd/lobo/main.go                     cmd/lobo-agent/main.go
internal/config/{laptop.go,agent.go,config_test.go}
internal/model/{catalog.go,catalog_test.go}
internal/watchdog/{watchdog.go,watchdog_test.go}
internal/metrics/{llama.go,gpu.go,metrics_test.go,testdata/llama_metrics.txt,testdata/nvidia_smi.csv}
internal/agent/{download.go,download_test.go,logring.go,logring_test.go,proc.go,runner.go,runner_test.go,api.go,api_test.go,status.go}
internal/runpod/{client.go,payload.go,runpod_test.go,testdata/pod.json}
internal/release/{version.go,manifest.go,zip.go,store.go,release_test.go}
internal/control/{events.go,up.go,down.go,status.go,control_test.go}
internal/checks/{checks.go,checks_test.go}
internal/tui/{up.go,status.go,styles.go,tui_test.go,testdata/*.golden}
plans/2026-09-23-qwen-5090/results.md
```

**Modify**
```
.gitignore            add bin/, dist/
plans/2026-09-23-qwen-5090/spec.md   status + as-built notes at the end
```

**Out of scope:** CI workflows, Dockerfiles, any Python package, anything under `docs/`.

## Locked interfaces (all tasks must match these names)

```go
// internal/model
type Model struct{ ID, File, SHA256 string; Size int64 }
func Get(id string) (Model, error)          // "q8" | "q6"; unknown → error naming valid ids
func (m Model) Key() string                 // "models/" + File, in bucket `lobo`

// internal/config
type Laptop struct{ RunPodAPIKey, LoboAPIKey, CFTunnelToken, R2AccountID, R2AccessKey, R2SecretKey, R2Endpoint, Domain string }
func LoadLaptop(envPath string) (Laptop, error)   // godotenv.Read(envPath) ONLY, never os.Getenv
type Agent struct{ LoboAPIKey, CFTunnelToken, Model, ModelURL, RunPodPodID, RunPodAPIKey string; Ctx, IdleMin, MaxHours int }
func LoadAgent() (Agent, error)                   // os env (pod side), validator; error names the env var

// internal/watchdog
type Sample struct{ At time.Time; OK bool; Processing int; PromptTokens int64 }
type Config struct{ Idle, MaxLife time.Duration }
type Decision struct{ Kill bool; Reason string; KillIn time.Duration } // Reason: "idle" | "max_life" | ""
type State struct{ /* unexported */ }
func NewState(start time.Time) *State
func (s *State) Observe(sm Sample)
func (s *State) Decide(now time.Time, cfg Config) Decision
func (s *State) IdleFor(now time.Time) time.Duration

// internal/metrics
type Llama struct{ RequestsProcessing int; RequestsTotal, PromptTokensTotal, GenTokensTotal int64; PromptTPS, GenTPS float64 }
func ParseLlama(r io.Reader) (Llama, error)
type GPU struct{ Name string; VRAMUsedMB, VRAMTotalMB, UtilPct int }
func ParseNvidiaSMI(csv string) (GPU, error)       // output of --query-gpu=name,memory.used,memory.total,utilization.gpu --format=csv,noheader,nounits

// internal/agent
type Stage string // "boot","tunnel","download","load","ready","failed"
type DownloadProgress struct{ Bytes, Total int64; MBps float64 }
type Status struct{ Stage Stage; StageDetail string; Download DownloadProgress; UptimeS, IdleS, KillInS int64; KillReason string; GPU metrics.GPU; Llama metrics.Llama; Model string; Ctx int } // JSON field names = spec §Pod API
func Download(ctx context.Context, url, dst, sha256 string, conns int, onProgress func(DownloadProgress)) error
type LogRing struct{ /* unexported */ } ; func NewLogRing(n int) *LogRing ; func (r *LogRing) Write(p []byte) (int, error) ; func (r *LogRing) Tail(n int) []string
type Killer interface{ KillSelf(ctx context.Context) error }
type Runner struct{ /* deps injected: StartTunnel, Download, StartLlama, WaitHealthy funcs; Killer; clock */ }
func (r *Runner) Run(ctx context.Context) error
func (r *Runner) Status() Status
func NewAPI(key string, version []byte, status func() Status, logs *LogRing) http.Handler // routes /api/version /api/status /api/logs

// internal/runpod
type Pod struct{ ID, Name, DesiredStatus, ImageName string; CostPerHr float64; LastStartedAt time.Time; /* + fields confirmed from testdata/pod.json */ }
type CreateOpts struct{ Image, ReleaseURL, ReleaseSHA256, ModelURL, LoboAPIKey, CFTunnelToken, Model string; Ctx, IdleMin, MaxHours int; CloudType string }
func BuildCreatePayload(o CreateOpts) map[string]any
type Client struct{ /* base URL, key, http.Client */ } ; func New(key string) *Client
func (c *Client) Create(ctx, CreateOpts) (Pod, error); List(ctx) ([]Pod, error); Get(ctx, id) (Pod, error); Delete(ctx, id) error
var ErrNoCapacity = errors.New("runpod: no gpu capacity")

// internal/release
type Manifest struct{ Version, GitSHA string; GitDirty bool; BuiltAt time.Time; BuiltBy, LlamaImage string; Model struct{ File, SHA256 string }; Defaults struct{ Ctx, IdleMin, MaxHours int } } // JSON names = spec release.json
func NextVersion(existingKeys []string, today time.Time) string   // "YYYY.MM.DD-N"
func BuildZip(agentBin string, m Manifest, out string) (sha256 string, err error)
type Store struct{ /* minio client, bucket "lobo" */ } ; func NewStore(config.Laptop) (*Store, error)
func (s *Store) ListReleases(ctx) ([]string, error); Upload(ctx, zipPath, m Manifest, sha string) error; Latest(ctx) (Latest, error); Presign(ctx, key string, ttl time.Duration) (string, error) // used for both release zip and model key
type Latest struct{ Version, Key, SHA256 string }

// internal/control
type Event struct{ Phase string; Detail string; Progress float64; Err error; Done bool } // Phase: "create","image","running","tunnel","download","load","ready","failed","terminated"
func Up(ctx context.Context, d Deps, o UpOpts) <-chan Event
func Down(ctx context.Context, d Deps) (spentUSD float64, err error)
func Snapshot(ctx context.Context, d Deps) (Snap, error)        // Snap{Pod *runpod.Pod; Version *release.Manifest; Status *agent.Status; Down bool}
type Deps struct{ RunPod RunPodAPI; Store ReleaseAPI; Agent AgentAPI; Clock func() time.Time } // interfaces so tests use fakes
type UpOpts struct{ Model string; Ctx int; Release string; IdleMin, MaxHours int; Timeout time.Duration }

// internal/checks
func ValidateToolCall(body []byte) error        // choices[0].message.tool_calls[0].function.arguments must be a JSON string that parses to an object
func Chat(ctx, baseURL, key, model string) (string, error)          // streamed
func ToolCall(ctx, baseURL, key, model string) ([]byte, error)      // non-streamed, spec §10 get_weather payload
```

---

## Task 0 — baseline [IMPL]

**Files:** Create `go.mod`, `Makefile`, `.golangci.yml`, `cmd/lobo/main.go`, `cmd/lobo-agent/main.go`. Modify `.gitignore`.

- [ ] `git checkout -b feat/lobo-v1 master`
- [ ] `go mod init github.com/1905/lobotomized-ai`
- [ ] `go get` cobra, zerolog, godotenv, validator/v10, bubbletea, bubbles, lipgloss, minio-go/v7 at latest stable. Record the resolved import paths in go.mod. If bubbletea's latest major lives at a new import path (e.g. `charm.land/...` v2), use that path everywhere and note it in the commit message.
- [ ] Both `main.go` files: cobra root with a `version` subcommand, zerolog console logger for `lobo`, JSON logger for `lobo-agent`.
- [ ] Makefile targets: `build` (both binaries to `bin/`; agent with `GOOS=linux GOARCH=amd64 CGO_ENABLED=0`), `lint`, `test` (= `go test ./...`), and stubs `release up down status logs smoke` that call `bin/lobo <cmd>`. Note: `make test` runs unit tests; the live check is `make smoke` → `lobo test`. (Spec's `make test` row is renamed `make smoke`; update spec §CLI in this task.)
- [ ] `.golangci.yml`: govet, errcheck, staticcheck, unused, ineffassign.
- [ ] Verify: `make build && make lint && go test ./...` → exit 0.
- [ ] Commit `chore: go module skeleton`.

## P1 — infra + manual bring-up (all [ORCH])

### Task 1 — `.env` + `.env.example`

- [ ] Copy `RUNPOD_API_KEY` value from `~/dev/ai-image-studio/.env` into `./.env`.
- [ ] Copy R2 S3 creds (`yapper402-admin`: account id, access key, secret, endpoint) from the cloudflare skill into `./.env`.
- [ ] `LOBO_API_KEY=sk-$(openssl rand -hex 24)` into `./.env`. `LOBO_DOMAIN=lobo.example.com`.
- [ ] `.env.example`: same names, empty values, one comment each.
- [ ] Verify: `git check-ignore .env` prints `.env`. `git status` does not list `.env`.
- [ ] Commit `.env.example` only.

### Task 2 — R2 bucket `lobo`

- [ ] `wrangler r2 bucket create lobo` with `CF_WORKERS_TOKEN` + account `<cf-account-id>` passed inline (not exported to the shell profile).
- [ ] Do NOT enable a dev URL or custom domain.
- [ ] Verify: `wrangler r2 bucket info lobo` succeeds; `curl -sI https://pub-…r2.dev` is not applicable (no public URL exists).

### Task 3 — tunnel `lobo` + DNS

Scratch driver in the session scratchpad (Python, not committed).
- [ ] Confirm target: account `<cf-account-id>`, zone `example.com` (id `<cf-zone-id>`). Never carsan.com.
- [ ] Re-check `lobo.example.com` has 0 DNS records. If >0 → STOP.
- [ ] `POST /accounts/{acct}/cfd_tunnel` `{"name":"lobo","config_src":"cloudflare"}` with `CF_WORKERS_TOKEN`. On 403 → same call via Cloudflare MCP `execute` on the same account.
- [ ] `PUT /accounts/{acct}/cfd_tunnel/{id}/configurations` ingress:
  ```json
  [{"hostname":"lobo.example.com","path":"^/api/","service":"http_status:418"},
   {"hostname":"lobo.example.com","service":"http://127.0.0.1:8080"},
   {"service":"http_status:404"}]
  ```
  (`418` is a P1 probe for path routing. Task 5 switches it to `http://127.0.0.1:8081`.)
- [ ] `GET …/cfd_tunnel/{id}/token` → `CF_TUNNEL_TOKEN` in `.env`.
- [ ] Create CNAME `lobo` → `{id}.cfargotunnel.com`, proxied, with `CF_ZONE_TOKEN`. Record the record id in `results.md` (so later cleanup deletes only this record).
- [ ] Verify: `curl -s -o /dev/null -w '%{http_code}' https://lobo.example.com/` → `530`/`1033`-style error (tunnel has no connector yet).

### Task 4 — manual pod: verify unknowns, seed model

Scratch driver creates a pod through RunPod REST: image `ghcr.io/ggml-org/llama.cpp:server-cuda` (resolve the newest `server-cuda-bNNNN` tag via the ghcr tags API first and use that exact tag), 5090, COMMUNITY → SECURE fallback, `containerDiskInGb` 60, `ports ["22/tcp"]` (P1 only), start command that installs `openssh-server unzip`, adds `~/ssh/runpod2.pub`, runs sshd, then `sleep infinity`. `try/finally` terminates the pod.
- [ ] On the pod, record in `results.md`:
  - `command -v python3` (expect none), `ls /app`, `nvidia-smi`, driver + CUDA version.
  - `env | grep -i runpod` → is `RUNPOD_API_KEY` present? Try `DELETE /pods/$RUNPOD_POD_ID` semantics only at the end (it kills the pod).
  - `llama-server --version`.
- [ ] Seed model: download static `rclone` (or `aws` CLI v2 zip) on the pod. `hf` is unavailable without Python → download Q8_0 via `curl -L https://huggingface.co/HauhauCS/Qwen3.5-27B-Uncensored-HauhauCS-Aggressive/resolve/main/<file>`; record MB/s. `sha256sum` must equal `78ae0800…8dd5`. Upload to bucket `lobo`, key `models/<file>`, using R2 creds passed over SSH env (not written to pod disk).
- [ ] Measure pull: presign the object from the laptop (12 h), then on the pod `curl -o /dev/null -w '%{speed_download}' "$URL"` and a 16-way ranged pull (aria2c if apt has it). Record MB/s. Confirm presigned URLs honour `Range`.
- [ ] Run `llama-server -m … --alias qwen3.5-27b-uncensored-q8 --host 127.0.0.1 --port 8080 -ngl 99 -c 8192 --parallel 1 -fa on --cache-type-k q8_0 --cache-type-v q8_0 --jinja --chat-template-kwargs '{"enable_thinking":false}' --reasoning-budget 0 --api-key $LOBO_API_KEY --metrics --no-webui`. Record: compute capability 12.0 in log, layers offloaded, VRAM from `nvidia-smi`, `ss -ltnp` shows `127.0.0.1:8080` only (host flag beats `LLAMA_ARG_HOST`). If any flag is rejected, record and fix the flag list for Task 8.
- [ ] Start `cloudflared tunnel run --token $CF_TUNNEL_TOKEN` (static binary; record the pinned version used).
- [ ] From the laptop via the domain: `GET /health`, `GET /v1/models` (with key), chat test, tool-call test (spec §10 payload). `arguments` must be a JSON string. `GET /api/anything` → 418 proves path routing (else record → use agent-proxy fallback in Task 8).
- [ ] Save fixtures: `curl /metrics` → `internal/metrics/testdata/llama_metrics.txt`; `nvidia-smi --query-gpu=… --format=csv,noheader,nounits` → `internal/metrics/testdata/nvidia_smi.csv`; laptop `GET /pods/{id}` → `internal/runpod/testdata/pod.json` (strip ip/ports if sensitive; no keys are in it — check).
- [ ] PID-1 test last: `kill 1` over SSH, poll `GET /pods/{id}` 3 min, record whether the container restarts or the pod stops. Then terminate. Verify `GET /pods` has no `lobo` pod.
- [ ] Write `results.md` §P1 with every value above + the llama image tag + cloudflared version + decisions:
  - path routing works? (y → ingress; n → agent proxy)
  - pod key injected? (y → use it; n → pass ours as `RUNPOD_API_KEY` env)
  - PID-1 exit → restart or stop?
- [ ] If any decision diverges from the spec, update spec.md first, then continue. If the tool-call test fails on the newest tag: STOP P1, record, notify the user (spec says don't proceed to OpenCode, and there is no older-build fallback).
- [ ] Commit `results.md` + testdata. P1 gate: `/rival-fable` not needed (no code); orchestrator checks results against spec.

## P2 — release + agent + plain up/down

### Task 5 — tunnel ingress to agent [ORCH]
- [ ] If path routing worked: replace the `418` rule with `http://127.0.0.1:8081`. Else: single rule → `http://127.0.0.1:8080` (agent), agent proxies to llama on `:8082`.
- [ ] Record final ingress JSON in `results.md`.

### Task 6 — `internal/model` [IMPL]
- [ ] Test first: `Get("q8")` returns file/sha/size from spec; `Get("q6")` same for Q6_K; `Get("x")` errors with text containing `q8` and `q6`. `Key()` = `models/<File>`.
- [ ] Implement. `go test ./internal/model/... -v` passes.

### Task 7 — `internal/config` [IMPL]
- [ ] Tests (table): `LoadLaptop` on a temp `.env` with all vars → ok; missing `RUNPOD_API_KEY` → error mentions `RUNPOD_API_KEY`; with `t.Setenv("RUNPOD_API_KEY","from-os")` and no file value → still error (proves OS env is ignored). `LoadAgent` via `t.Setenv`: defaults `Ctx=8192, IdleMin=30, MaxHours=12` when unset; `LOBO_CTX=abc` → error naming `LOBO_CTX`.
- [ ] Implement with `godotenv.Read` + `validator/v10`. Pass.

### Task 8 — `internal/watchdog` [IMPL]
- [ ] Tests (table, fake times): busy (Processing>0) → no kill; idle 29 min → no, `KillIn`=1 min; idle 31 min → `Kill`, `"idle"`; PromptTokens increased → idle resets; uptime 12h01 while busy → `Kill`, `"max_life"`; `OK=false` samples count as idle; `KillIn` = min(idle remaining, life remaining).
- [ ] Implement. Pass.

### Task 9 — `internal/metrics` [IMPL]
- [ ] Tests on the P1 fixtures: `ParseLlama` extracts `llamacpp:requests_processing`, `llamacpp:prompt_tokens_total`, `llamacpp:tokens_predicted_total`, `llamacpp:prompt_tokens_seconds`, `llamacpp:predicted_tokens_seconds` (exact metric names = whatever the fixture contains; if a name differs, the fixture wins and this list is updated). Missing metric → zero, not error. Garbage input → error. `ParseNvidiaSMI("NVIDIA GeForce RTX 5090, 30112, 32607, 3")` → struct.
- [ ] Implement (plain line parser, no Prometheus lib). Pass.

### Task 10 — `internal/agent` download + logring [IMPL]
- [ ] Tests: `httptest` server with `Range` support serving 10 MB random bytes; `Download` with conns=4 writes identical bytes, progress callback reaches Total, wrong sha → error and the partial file is moved aside (not left as the target name). Server without Range → falls back to 1 connection. Context cancel → returns `ctx.Err()`.
- [ ] `LogRing`: keeps last N lines, splits on `\n`, concurrent writes safe (`-race`).
- [ ] Implement. `go test -race ./internal/agent/... -run 'Download|LogRing' -v` passes.

### Task 11 — `internal/agent` runner + status [IMPL]
- [ ] Tests with fake funcs + fake clock: happy path goes `boot→tunnel→download→load→ready`; `Status()` reflects each stage and download progress; download error → stage `failed`, `StageDetail` has the error, `Killer.KillSelf` called after the grace period (2 min, injected); tunnel start error → same.
- [ ] Watchdog loop inside runner (after `ready`): every 30 s sample metrics via injected func, `Decide`, kill on `Kill`. Test with fake clock: 31 idle minutes → `KillSelf` once.
- [ ] `proc.go`: `StartProcess(ctx, name, args, env, logs io.Writer) (*exec.Cmd, error)`; process exit after ready → stage `failed`, then kill (not unit-tested beyond a `/bin/sh -c 'exit 3'` case).
- [ ] Implement. Pass with `-race`.

### Task 12 — `internal/agent` API [IMPL]
- [ ] Tests (`httptest`): no/wrong bearer → 401 on all three routes; `/api/version` returns the exact bytes passed in; `/api/status` JSON field names match spec §Pod API (assert keys); `/api/logs?n=5` returns ≤5 lines; `n` > 1000 clamps to 1000.
- [ ] Implement. Pass.

### Task 13 — `cmd/lobo-agent` wiring [IMPL]
- [ ] Wire: `LoadAgent` → read `/lobo/release.json` → `LogRing` → `Runner` with real funcs:
  - tunnel: download pinned cloudflared (version from `results.md`) to `/lobo/bin/`, run `tunnel run --token`.
  - download: `cfg.ModelURL` → `/models/<model.Get(cfg.Model).File>`, sha from the catalog, 16 conns.
  - llama: `/app/llama-server` with the flag list verified in Task 4 + `-c cfg.Ctx`, stdout/stderr → LogRing.
  - health: poll `http://127.0.0.1:8080/health` until 200.
  - killer: RunPod `DELETE /pods/{RUNPOD_POD_ID}` using `RUNPOD_API_KEY` from env (P1 decides whose key).
  - API on `:8081` (or `:8080` + reverse proxy to llama `:8082` if P1 said no path routing; `httputil.ReverseProxy` with `FlushInterval: -1`).
- [ ] Verify: `make build` produces `bin/lobo-agent` (linux/amd64, static: `file bin/lobo-agent` says `statically linked`).

### Task 14 — `internal/release` [IMPL]
- [ ] Tests: `NextVersion([], 2026-09-23)` → `2026.09.23-1`; with keys `releases/lobo-2026.09.23-1.zip`,`-2.zip` → `-3`; keys from other days ignored. `BuildZip` → zip contains exactly `lobo-agent` (mode 0755) + `release.json`; returned sha matches file. Manifest JSON field names match spec `release.json`.
- [ ] `Store` (minio-go, bucket `lobo`, region `auto`): `Upload` puts zip then overwrites `releases/latest.json`; `Presign` 12 h (release zip and model). Not unit-tested against R2 (live in Task 17).
- [ ] Git info via `git rev-parse --short HEAD` and `git status --porcelain` (non-empty → dirty).
- [ ] Implement. Pass.

### Task 15 — `internal/runpod` [IMPL]
- [ ] Tests: `BuildCreatePayload` → `volumeInGb` 0, `ports` empty, `gpuTypeIds` = `["NVIDIA GeForce RTX 5090"]`, env has all keys from spec §Pod boot (incl. `LOBO_IDLE_MIN`, `LOBO_MAX_HOURS`), `dockerStartCmd` contains `$LOBO_RELEASE_URL` and `sha256sum -c` and `exec /lobo/lobo-agent`. `Client` against `httptest`: Create/List/Get/Delete paths + bearer header; decoding `testdata/pod.json`; a create response meaning "no instances available" (exact shape: record from P1 if seen, else from RunPod docs) → `ErrNoCapacity`.
- [ ] Bootstrap one-liner (in `payload.go`): `set -e; apt-get update -qq && apt-get install -y -qq unzip; curl -fsSL "$LOBO_RELEASE_URL" -o /tmp/r.zip; echo "$LOBO_RELEASE_SHA256  /tmp/r.zip" | sha256sum -c; unzip -o /tmp/r.zip -d /lobo; exec /lobo/lobo-agent`.
- [ ] Implement. Pass.

### Task 16 — `internal/control` + plain `lobo up/down/release` [IMPL]
- [ ] Tests with fake `RunPodAPI`/`ReleaseAPI`/`AgentAPI` + fake clock:
  - `Up`: presigns the release key and `model.Get(o.Model).Key()` with 12 h TTL and passes both URLs into `CreateOpts`;
  - `Up`: existing `lobo` pod → first event `Err` "already running"; COMMUNITY `ErrNoCapacity` → retries SECURE; both fail → `Err`; agent status sequence tunnel→download(50%)→ready → events in that order with `Progress`; no progress for `Timeout` → deletes pod, `Err` includes last log lines.
  - `Down`: deletes all pods named `lobo`, re-lists, returns spend = CostPerHr × hours since `LastStartedAt`.
  - `Snapshot`: pod absent → `Down=true`; agent unreachable while pod RUNNING → `Status=nil`, no error.
- [ ] `AgentAPI` real impl: `GET https://$LOBO_DOMAIN/api/{status,version,logs}` with bearer, 5 s timeout.
- [ ] `cmd/lobo`: `release` (build agent via `go build`, zip, upload, print version), `up --plain [--q6] [--ctx N] [--release V] [--idle-min N] [--max-hours N]`, `down`. `--plain` prints one zerolog line per event. Default `--release` = `latest.json`.
- [ ] Implement. `go test ./internal/control/... -v` passes. `make build` ok.

### Task 17 — P2 live [ORCH]
- [ ] `make release` → version printed; `latest.json` in bucket points to it.
- [ ] `bin/lobo up --plain` → reaches `ready` ≤6 min; record per-stage durations in `results.md`.
- [ ] `curl -H "Authorization: Bearer $LOBO_API_KEY" https://lobo.example.com/api/version` equals uploaded `release.json`. `/api/status` stage `ready`. `/v1/models` lists the model.
- [ ] Negative: `up --plain` with a release whose sha is corrupted (scratch: pass wrong sha via a hidden `--release-sha` debug flag) → times out and pod deleted. Model sha wrong (debug env override `LOBO_MODEL_SHA256_OVERRIDE`, agent-only, documented as test-only) → `stage: failed`, pod self-deletes within ~2 min.
- [ ] `bin/lobo down` → `GET /pods` empty.
- [ ] `/rival-fable` on P2 diff; fix HIGH/CRIT; commit.

## P3 — TUI

### Task 18 — `internal/tui` up view [IMPL]
- [ ] Model consumes `<-chan control.Event`. Renders a stage list (✓ done, spinner current, · pending), download progress bar (bubbles/progress) with MB/s + ETA, final box with URL, version, $/h, total time; error box with detail.
- [ ] Golden tests: feed fixture events, call `View()`, compare to `testdata/up_*.golden` (colors off via `lipgloss.SetColorProfile(termenv.Ascii)` or the v2 equivalent). Cases: mid-download 43%, ready, failed.
- [ ] Implement. Pass.

### Task 19 — `internal/tui` status dashboard [IMPL]
- [ ] Model polls `control.Snapshot` every 2 s (tea.Tick). Sections: Pod (id, GPU, cloud, $/h, uptime, spend), Release (version, sha, dirty ⚠), Stage, GPU (VRAM bar, util), LLM (requests, prompt/gen tokens, tok/s), Watchdog (idle, auto-kill in + reason). `q` quits. `Down=true` → prints "lobo: down" and quits.
- [ ] Golden tests: `Snap` fixtures → ready, downloading, down.
- [ ] Implement. Pass.

### Task 20 — `internal/checks` + `lobo test/logs/status`, TUI `up` [IMPL]
- [ ] `ValidateToolCall` tests: string args that parse to object → ok; object args → error; missing `tool_calls` → error; `finish_reason` not `tool_calls` → error.
- [ ] `cmd/lobo`: `up` defaults to TUI (`--plain` keeps Task 16 output); `status` (TUI; `--once` prints one plain snapshot); `logs [-n]`; `test` (streamed chat → prints first 200 chars; tool call → `ValidateToolCall`; exit 1 on failure).
- [ ] Makefile `up down status logs smoke release` call `bin/lobo`, each depending on `build`.
- [ ] Implement. `go test ./...` passes. `make build` ok.

### Task 21 — P3 live [ORCH]
- [ ] `make up` in a real terminal → TUI reaches ready. `make status` shows live numbers while `make smoke` runs in another terminal (requests/tokens move). `make logs` prints lines.
- [ ] Idle kill: `bin/lobo up --idle-min 3` → no traffic → pod gone from `GET /pods` within ~4 min; `make status` shows the countdown before that.
- [ ] `make down` path verified on a second `up`.
- [ ] `/rival-fable` on P3 diff; fix; commit.

## P4 — OpenCode + benchmark

### Task 22 — `opencode.json.example` + README [IMPL]
- [ ] `opencode.json.example`: provider `lobo`, `npm` `@ai-sdk/openai-compatible`, `baseURL` `https://lobo.example.com/v1`, `apiKey` `{env:LOBO_API_KEY}`, model `qwen3.5-27b-uncensored-q8`, `limit.context` = placeholder value 8192 (Task 23 sets the measured value).
- [ ] README: what it is, `.env` setup, `make release/up/status/smoke/down`, cost note, idle-kill note, key rotation, "laptop is control only".
- [ ] Commit.

### Task 23 — acceptance + benchmark [ORCH]
- [ ] `make up`. For ctx 8K/16K/24K/32K on Q8 (new `up --ctx N` per size): record VRAM after load and after a full-context prompt, prompt tok/s, gen tok/s, TTFT. OOM → record and stop increasing.
- [ ] Mirror Q6_K to bucket `lobo` `models/` (same method as Task 4, from the running pod). Repeat the table for Q6_K.
- [ ] OpenCode on the laptop (install if missing, after asking the user), config from the example, on a small Go repo: tests 1-6 from `docs/idea.md` §13. Record pass/fail + notes.
- [ ] Pick default model + ctx by rule: highest ctx with ≥1 GB VRAM headroom; Q8 unless Q6_K gives ≥2× ctx with tests still passing. Update `internal/model`/release defaults + `opencode.json.example` limit.
- [ ] `make release` + one final `make up` → `make smoke` → `make down`.
- [ ] `results.md` §P4 table. Commit.

## Exit gates [ORCH]
- [ ] `/rival-astra review` on the whole branch (fallback `/rival-fable`, then self-review with code-review plugins, stated plainly). Fix what holds.
- [ ] `/simplify` on the branch.
- [ ] Reconcile spec.md (`## As-built notes`), plan status `done`, move feature dir to `plans/done/`.
- [ ] Merge `feat/lobo-v1` → `master` same day (merge hygiene rule), push, `/notify`.
- [ ] Final check: `GET /pods` has no `lobo` pod.

## Type-consistency check
- `control.Event.Phase` values ↔ `agent.Stage` values: `tunnel/download/load/ready/failed` identical strings; control adds `create/image/running/terminated`.
- `Status` JSON names ↔ spec §Pod API ↔ Task 12 test ↔ `control.AgentAPI` decode.
- `Manifest` JSON ↔ spec `release.json` ↔ Task 14 test ↔ `/api/version`.
- Env names: `LOBO_RELEASE_URL, LOBO_RELEASE_SHA256, LOBO_MODEL_URL, LOBO_API_KEY, CF_TUNNEL_TOKEN, LOBO_MODEL, LOBO_CTX, LOBO_IDLE_MIN, LOBO_MAX_HOURS` — same in `runpod.BuildCreatePayload`, `config.LoadAgent`, spec.
- `model.Get` ids `q8|q6` ↔ `--q6` flag ↔ `LOBO_MODEL`.
