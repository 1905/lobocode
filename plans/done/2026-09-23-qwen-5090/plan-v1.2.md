# Lobo Implementation Plan v1.2

**Date:** 2026-09-23
**Status:** done 2026-09-23 (as-built: see spec.md As-built notes, results.md)
**Spec:** ./spec.md (approved rev 2 + user changes 2026-09-23: r2.dev public URLs, bucket keys laptop-only, single-GET download, public `/api/status` + `/api/version`)
**Supersedes:** plan-v1.1.md (antislop 8/10, 3 cuts applied). v1.1 superseded plan-v1.0.md (Astra 5/10, 10 findings fixed; mapping at the end).

**Goal:** `make up` rents a RunPod 5090 that serves Qwen3.5-27B uncensored at `https://lobo.example.com/v1` on its own, kills itself when idle or expired, and reports its release via `/api/version`; `make status/down/release` control it from the laptop. MVP: make it work, no download optimisation.

**Architecture:** Two Go binaries in one module. `lobo` (laptop, cobra + bubbletea/lipgloss) talks only to the RunPod REST API, R2 (upload only) and the pod's `/api/*`. `lobo-agent` (pod, PID 1, shipped in a zip from R2 bucket `lobo` via its public r2.dev URL) runs cloudflared, downloads the GGUF with one GET, supervises `llama-server` from the official `ghcr.io/ggml-org/llama.cpp:server-cuda-*` image, serves `/api/*` and self-terminates.

**Tech Stack:** Go 1.26 · `spf13/cobra` · `rs/zerolog` · `joho/godotenv` · `go-playground/validator/v10` · `charmbracelet/bubbletea` + `bubbles` + `lipgloss` · `minio/minio-go/v7` (R2 upload, laptop only) · llama.cpp `server-cuda` image · cloudflared.

> For agentic workers: use superpowers:subagent-driven-development to implement task-by-task. Checkbox syntax for tracking.

## Exec rules (apply to every task)

- Branch `feat/lobo-v1` off `master` in the main checkout. No worktrees. Commit per task only after its review is clean.
- **[ORCH]** tasks are live/money/infra: orchestrator (Opus/Fable) only. **[IMPL]** tasks may go to any implementer model.
- Every Sonnet dispatch prompt includes verbatim: "You write only the code and unit tests this task names and run only this task's focused unit test. You never run e2e/integration/live/smoke tests, never rent a GPU or pod, never call RunPod/Cloudflare/R2/HF or any provider API, never publish, deploy or touch infra, never run anything that costs money."
- Reviews (user decision 2026-09-23): no per-task `/rival-fable`, no `/simplify`. ONE `/rival-astra review` on the whole branch when all code is done, fixes applied, then the final delivery gate.
- Keys come from this repo's `.env` only, never the OS environment. Never commit `.env`.
- **Bucket keys (R2_*) never leave the laptop**: not in any binary, zip, `release.json`, pod env, or SSH session.
- No `rm`: move to `/tmp/trash/`. No Docker pulls/runs on the laptop.
- Unit tests: `go test ./internal/<pkg>/... -run <Name> -v`. Build: `make build`. Lint: `make lint`.

## File map

**Create**
```
go.mod, go.sum, Makefile, .golangci.yml, .env.example, README.md, opencode.json.example
cmd/lobo/main.go                     cmd/lobo-agent/main.go
internal/config/{laptop.go,agent.go,config_test.go}
internal/model/{catalog.go,catalog_test.go}
internal/watchdog/{watchdog.go,watchdog_test.go}
internal/metrics/{llama.go,gpu.go,host.go,collect.go,metrics_test.go,testdata/{llama_metrics.txt,nvidia_smi.csv,loadavg.txt,meminfo.txt}}
internal/agent/{addr.go,download.go,download_test.go,logring.go,logring_test.go,proc.go,runner.go,runner_test.go,killer.go,killer_test.go,api.go,api_test.go,status.go}
internal/runpod/{client.go,payload.go,runpod_test.go,testdata/pod.json}
internal/release/{version.go,manifest.go,zip.go,scan.go,store.go,resolve.go,release_test.go}
internal/control/{events.go,up.go,down.go,status.go,control_test.go}
internal/checks/{checks.go,checks_test.go}
internal/tui/{up.go,status.go,styles.go,tui_test.go,testdata/*.golden}
plans/2026-09-23-qwen-5090/results.md
```

**Modify**
```
.gitignore                           add bin/, dist/
plans/2026-09-23-qwen-5090/spec.md   Task 0 edits (see there) + As-built notes at the end
```

**Scratch (session scratchpad, never committed):** P1 Python drivers (`.venv` + `boto3`/`requests`) for tunnel setup, the manual pod and the model seed.

**Out of scope:** CI, Dockerfiles, committed Python, anything under `docs/`, download optimisation (ranges, parallelism, retries, resume).

## Locked interfaces (all tasks must match these names)

```go
// internal/model
type Model struct{ ID, File, SHA256, Alias string; Size int64 }
func Get(id string) (Model, error)                 // "q8" | "q6"; unknown → error naming valid ids
func (m Model) URL(bucketURL string) string        // bucketURL + "/models/" + File

// internal/config
type Laptop struct{ RunPodAPIKey, LoboAPIKey, CFTunnelToken, Domain, BucketURL string; R2 R2Creds }
type R2Creds struct{ AccountID, AccessKey, SecretKey, Endpoint string }
func LoadLaptop(envPath string) (Laptop, error)    // godotenv.Read(envPath) ONLY; R2 optional here
func (l Laptop) RequireR2() error                  // called by `release` only; names missing var
func (l Laptop) SecretValues() map[string]string   // env name → value, for the release scan
type Agent struct{ LoboAPIKey, CFTunnelToken, Model, ModelURL, RunPodPodID, RunPodAPIKey string; Ctx, IdleMin int; ExpiresAt time.Time; BootTimeout time.Duration }
func LoadAgent() (Agent, error)                    // os env (pod side), validator; error names the env var

// internal/watchdog
type Sample struct{ At time.Time; OK bool; Processing, Deferred int; PromptTokens, GenTokens int64 }
type Config struct{ Idle time.Duration; ExpiresAt time.Time }
type Decision struct{ Kill bool; Reason string; KillIn time.Duration } // Reason: "idle" | "expired" | ""
type State struct{ /* unexported */ }
func NewState(start time.Time) *State
func (s *State) Observe(sm Sample)                 // OK=false: no-op (neither resets nor advances activity); counts consecutive failures
func (s *State) SetReady(at time.Time)             // idle clock starts here; before ready only expiry applies
func (s *State) Decide(now time.Time, cfg Config) Decision
func (s *State) IdleFor(now time.Time) time.Duration
func (s *State) FailedSamples() int

// internal/metrics
type Llama struct{ RequestsProcessing, RequestsDeferred int; PromptTokensTotal, GenTokensTotal int64; PromptTPS, GenTPS float64 }
func ParseLlama(r io.Reader) (Llama, error)        // error if requests_processing or prompt_tokens_total missing
type GPU struct{ Name string; VRAMUsedMB, VRAMTotalMB, UtilPct int }
func ParseNvidiaSMI(csv string) (GPU, error)       // --query-gpu=name,memory.used,memory.total,utilization.gpu --format=csv,noheader,nounits
type Collector struct{ LlamaURL, APIKey string; HTTP *http.Client; SMI func(ctx context.Context) (string, error) }
func (c Collector) Llama(ctx context.Context) (Llama, error)  // GET LlamaURL+"/metrics" with Bearer APIKey, 3 s timeout, non-200 → error
func (c Collector) GPU(ctx context.Context) (GPU, error)      // runs SMI with 3 s timeout
type Host struct{ Load1, Load5, Load15 float64; MemUsedMB, MemTotalMB int }
func ParseHost(loadavg, meminfo string) (Host, error)           // /proc/loadavg + /proc/meminfo (MemTotal − MemAvailable)
func (c Collector) Host(ctx context.Context) (Host, error)     // reads the two /proc files

// internal/agent
const LlamaAddr = "127.0.0.1:8080"; const AgentAddr = "127.0.0.1:8081"   // ONE routing mode; Task 5 may flip to proxy mode, see there
type Stage string // "boot","tunnel","download","load","ready","failed","terminating"
type DownloadProgress struct{ Bytes, Total int64; MBps float64 }
type Status struct{ Stage Stage; StageDetail string; Download DownloadProgress; UptimeS, IdleS, KillInS int64; KillReason string; ExpiresAt time.Time; GPU *metrics.GPU; Host *metrics.Host; Llama *metrics.Llama; MetricsFailures int; Model string; Ctx int } // nil = unavailable, rendered "n/a"; JSON names = spec §Pod API
func Download(ctx context.Context, url, dst, sha256 string, onProgress func(DownloadProgress)) error  // one GET, hash while writing, no retry
type LogRing struct{ /* unexported */ } ; func NewLogRing(n int) *LogRing ; func (r *LogRing) Write(p []byte) (int, error) ; func (r *LogRing) Tail(n int) []string
type Killer interface{ KillSelf(ctx context.Context) error }   // real impl never returns until the pod is confirmed gone or ctx ends
type PodAPI interface{ Delete(ctx context.Context, id string) error; Get(ctx context.Context, id string) (runpod.Pod, error) } // satisfied by *runpod.Client
type RunPodKiller struct{ PodID string; API PodAPI; Sleep func(time.Duration) } // retry/confirm only; HTTP lives in internal/runpod
type Runner struct{ /* deps injected: StartTunnel, Download, StartLlama, WaitHealthy, Sample funcs; Killer; Clock; Watchdog cfg; BootTimeout; FailGrace */ }
func (r *Runner) Run(ctx context.Context) error
func (r *Runner) Status() Status
func NewAPI(key string, version []byte, status func() Status, logs *LogRing) http.Handler // /api/version + /api/status public; /api/logs needs Bearer key

// internal/runpod
type Pod struct{ ID, Name, DesiredStatus, ImageName string; CostPerHr float64; LastStartedAt time.Time; /* + fields confirmed from testdata/pod.json */ }
type CreateOpts struct{ Image, ReleaseURL, ReleaseSHA256, ModelURL, LoboAPIKey, CFTunnelToken, Model string; Ctx, IdleMin int; ExpiresAt time.Time; CloudType string }
func BuildCreatePayload(o CreateOpts) map[string]any
type Client struct{ /* base URL, key, http.Client */ } ; func New(key string) *Client
func (c *Client) Create(ctx, CreateOpts) (Pod, error); List(ctx) ([]Pod, error); Get(ctx, id) (Pod, error); Delete(ctx, id) error  // Delete: 404 → nil; Get: 404 → ErrNotFound
var ErrNoCapacity = errors.New("runpod: no gpu capacity")
var ErrNotFound = errors.New("runpod: pod not found")

// internal/release
type Manifest struct{ Version, GitSHA string; GitDirty bool; BuiltAt time.Time; BuiltBy, LlamaImage string; Model struct{ ID, File, SHA256 string }; Defaults struct{ Ctx, IdleMin, MaxHours int } } // JSON names = spec release.json
type Resolved struct{ Manifest Manifest; ZipKey, ZipSHA256 string }   // == content of releases/lobo-<ver>.json and releases/latest.json
func NextVersion(existingKeys []string, today time.Time) string       // "YYYY.MM.DD-N"
func BuildZip(agentBin string, m Manifest, out string) (sha256 string, err error)
func ScanForSecrets(zipPath string, secrets map[string]string) error  // match → error naming the env var, never the value
type Store struct{ /* minio client, bucket "lobo" */ } ; func NewStore(config.R2Creds) (*Store, error)
func (s *Store) ListReleaseKeys(ctx) ([]string, error)
func (s *Store) Publish(ctx, zipPath string, r Resolved) error   // order: zip → releases/lobo-<ver>.json → releases/latest.json
func Resolve(ctx context.Context, h *http.Client, bucketURL, version string) (Resolved, error) // public GET; version "" → latest.json

// internal/control
type ReadyInfo struct{ URL, Version, GitSHA string; CostPerHr float64; Elapsed time.Duration }
type Event struct{ Phase string; Detail string; Download *agent.DownloadProgress; Ready *ReadyInfo; Err error; Done bool } // Phase: "create","image","running","tunnel","download","load","ready","failed","terminated"
func Up(ctx context.Context, d Deps, o UpOpts) <-chan Event
func Down(ctx context.Context, d Deps) (spentUSD float64, err error)
func Snapshot(ctx context.Context, d Deps) (Snap, error)        // Snap{Pod *runpod.Pod; Version *release.Manifest; Status *agent.Status; Down bool}
type Deps struct{ RunPod RunPodAPI; Releases ReleaseResolver; Agent AgentAPI; Cfg config.Laptop; Clock func() time.Time }
type UpOpts struct{ Model string; Ctx int; Release string; IdleMin, MaxHours int; Timeout time.Duration } // 0 values → release Defaults

// internal/checks
func ValidateToolCall(body []byte) error        // choices[0].finish_reason=="tool_calls"; tool_calls[0].function.arguments is a JSON string that parses to an object
func Chat(ctx, baseURL, key, model string) (string, error)          // streamed
func ToolCall(ctx, baseURL, key, model string) ([]byte, error)      // non-streamed, spec §10 get_weather payload
```

Pod env contract (set by `runpod.BuildCreatePayload`, read by `config.LoadAgent` / bootstrap):
`LOBO_RELEASE_URL, LOBO_RELEASE_SHA256, LOBO_MODEL_URL, LOBO_API_KEY, CF_TUNNEL_TOKEN, LOBO_MODEL, LOBO_CTX, LOBO_IDLE_MIN, LOBO_EXPIRES_AT (RFC3339, = create time + MaxHours), LOBO_BOOT_TIMEOUT (default 40m)` + RunPod's own `RUNPOD_POD_ID` and (P1 decides) `RUNPOD_API_KEY`.

`LOBO_EXPIRES_AT` is an absolute time in pod env. It survives an agent crash or a container restart, because RunPod re-applies the same env. So the expiry cannot reset.

---

## Task 0 — baseline [IMPL]

**Files:** Create `go.mod`, `Makefile`, `.golangci.yml`, `cmd/lobo/main.go`, `cmd/lobo-agent/main.go`. Modify `.gitignore`, `spec.md`.

- [ ] `git checkout -b feat/lobo-v1 master`
- [ ] `go mod init github.com/1905/lobotomized-ai`
- [ ] `go get` cobra, zerolog, godotenv, validator/v10, bubbletea, bubbles, lipgloss, minio-go/v7 at latest stable. If bubbletea/lipgloss latest major lives at a new import path (e.g. `charm.land/...` v2), use that path everywhere and note it in the commit message.
- [ ] Both `main.go`: cobra root + `version` subcommand; zerolog console logger for `lobo`, JSON for `lobo-agent`.
- [ ] Makefile: `build-lobo` (`bin/lobo`), `build-agent` (`bin/lobo-agent`, `GOOS=linux GOARCH=amd64 CGO_ENABLED=0`), `build` (both), `lint`, `test` (= `go test ./...`), and `release up down status logs smoke` → `bin/lobo <cmd>`, each depending on `build-lobo` only. `lobo release` does its own single agent build.
- [ ] `.golangci.yml`: govet, errcheck, staticcheck, unused, ineffassign.
- [ ] Spec edits: `make test` → `make smoke` in §CLI; §Pod API: `/api/status` + `/api/version` public, `/api/logs` needs key; `requests_total` → `requests_processing`, `requests_deferred`; nullable `gpu`/`llama`; watchdog row "failed sample = no-op"; env `LOBO_EXPIRES_AT`, `LOBO_BOOT_TIMEOUT` replace `LOBO_MAX_HOURS` in pod env.
- [ ] Verify: `make build && make lint && go test ./...` → exit 0.
- [ ] Commit `chore: go module skeleton`.

## P1 — infra + manual bring-up (all [ORCH])

### Task 1 — `.env` + `.env.example`
- [ ] Copy `RUNPOD_API_KEY` from `~/dev/ai-image-studio/.env`.
- [ ] Copy R2 S3 creds (`yapper402-admin`) from the cloudflare skill into `R2_ACCOUNT_ID, R2_ACCESS_KEY, R2_SECRET_KEY, R2_ENDPOINT`.
- [ ] `LOBO_API_KEY=sk-$(openssl rand -hex 24)`; `LOBO_DOMAIN=lobo.example.com`.
- [ ] `.env.example`: same names, empty values, one comment each (R2_* marked "laptop-only, upload").
- [ ] Verify: `git check-ignore .env` prints `.env`.
- [ ] Commit `.env.example`.

### Task 2 — R2 bucket `lobo` + dev URL
- [ ] `wrangler r2 bucket create lobo` then `wrangler r2 bucket dev-url enable lobo`, with `CF_WORKERS_TOKEN` + account `<cf-account-id>` passed inline.
- [ ] Put the printed `https://pub-<hash>.r2.dev` into `.env` as `LOBO_BUCKET_URL`.
- [ ] Verify: upload a 1-byte `probe.txt` via S3 creds; `curl -s $LOBO_BUCKET_URL/probe.txt` returns it. Move probe away (delete object) after.

### Task 3 — tunnel `lobo` + DNS
Scratch Python driver.
- [ ] Target check: account `<cf-account-id>`, zone `example.com` id `<cf-zone-id>`. Never carsan.com.
- [ ] Re-check `lobo.example.com` has 0 records (assert `success==true` AND length 0). Else STOP.
- [ ] `POST /accounts/{acct}/cfd_tunnel` `{"name":"lobo","config_src":"cloudflare"}` with `CF_WORKERS_TOKEN`. On 403 → same call via Cloudflare MCP on the same account.
- [ ] `PUT …/cfd_tunnel/{id}/configurations` ingress:
  ```json
  [{"hostname":"lobo.example.com","path":"^/api/","service":"http_status:418"},
   {"hostname":"lobo.example.com","service":"http://127.0.0.1:8080"},
   {"service":"http_status:404"}]
  ```
- [ ] `GET …/cfd_tunnel/{id}/token` → `CF_TUNNEL_TOKEN` in `.env`.
- [ ] CNAME `lobo` → `{id}.cfargotunnel.com`, proxied, with `CF_ZONE_TOKEN`. Record the record id + tunnel id in `results.md`.
- [ ] Verify: `https://lobo.example.com/` returns a Cloudflare tunnel error (no connector yet).

### Task 4 — manual pod: unknowns, both models seeded, fixtures
Scratch driver creates a pod via RunPod REST. Image: newest `server-cuda-bNNNN` tag from the ghcr tags API (exact tag). 5090, COMMUNITY → SECURE. `containerDiskInGb` 80 (room for Q8 + Q6 during seeding). `ports ["22/tcp"]` (P1 only). Env: `PUBLIC_KEY` = `~/ssh/runpod2.pub`, `LOBO_API_KEY`, `CF_TUNNEL_TOKEN`. No R2 keys.

Start command (controlled PID 1, for finding 10):
```
bash -c 'echo "boot $(date +%s%N)" >> /boot-ids; apt-get update -qq && apt-get install -y -qq openssh-server unzip >/dev/null; mkdir -p ~/.ssh && echo "$PUBLIC_KEY" > ~/.ssh/authorized_keys; service ssh start; trap "echo exit-by-usr1 >> /boot-ids; exit 7" USR1; while true; do sleep 1; done'
```
`try/finally` terminates the pod.

- [ ] Record in `results.md`: `command -v python3` (expect none), `ls /app`, `nvidia-smi`, driver/CUDA, `llama-server --version`, `env | grep -i runpod | sed 's/=.*/=<set>/'` → is `RUNPOD_API_KEY` present?
- [ ] Seed BOTH models (finding 4). Keys stay on the laptop:
  - Pod: `curl -L -o /models/<file> https://huggingface.co/HauhauCS/Qwen3.5-27B-Uncensored-HauhauCS-Aggressive/resolve/main/<file>` for Q8_0 and Q6_K; `sha256sum` must match spec values. Record HF MB/s.
  - Laptop driver: `create_multipart_upload` for `models/<file>` in bucket `lobo`; presign `upload_part` URLs (1 GiB parts, 12 h); send URLs over SSH; pod runs `curl -sS -T <part> "<url>" -D -` per part (split with `dd`), returns ETags; laptop `complete_multipart_upload`.
  - Verify from the pod: `curl -s $LOBO_BUCKET_URL/models/<file> | sha256sum` matches, for both. Record r2.dev MB/s (one connection).
- [ ] Run llama-server by hand: `-m /models/<q8> --alias qwen3.5-27b-uncensored-q8 --host 127.0.0.1 --port 8080 -ngl 99 -c 8192 --parallel 1 -fa on --cache-type-k q8_0 --cache-type-v q8_0 --jinja --chat-template-kwargs '{"enable_thinking":false}' --reasoning-budget 0 --api-key $LOBO_API_KEY --metrics --no-webui`. Record: compute capability 12.0, layers offloaded, VRAM, `ss -ltnp` shows only `127.0.0.1:8080`. Any rejected flag → record + fix flag list (it becomes Task 13's list).
- [ ] Start pinned-version `cloudflared tunnel run --token $CF_TUNNEL_TOKEN` (record version).
- [ ] Laptop via domain: `/health`, `/v1/models` (key), chat, tool-call (spec §10) → `arguments` is a JSON string. `GET /api/x` → 418 ⇒ path routing works.
- [ ] Fixtures (finding 3): `curl -sf -H "Authorization: Bearer $LOBO_API_KEY" http://127.0.0.1:8080/metrics` → must be HTTP 200 → `internal/metrics/testdata/llama_metrics.txt` (record the exact metric names present; also record status code without the key). `nvidia-smi --query-gpu=… --format=csv,noheader,nounits` → `nvidia_smi.csv`. `cat /proc/loadavg`, `/proc/meminfo` → `loadavg.txt`, `meminfo.txt`. Laptop `GET /pods/{id}` → `internal/runpod/testdata/pod.json` (check: no secrets inside; env values redacted if present).
- [ ] PID-1 test last (finding 10): `kill -USR1 1` over SSH; confirm SSH drops and `exit-by-usr1` was written (seen on next boot if restarted). Poll `GET /pods/{id}` for 3 min. Classify: new `boot` line in `/boot-ids` + `lastStartedAt` changed ⇒ restart; `desiredStatus` EXITED/no SSH ⇒ stop. Record the raw evidence.
- [ ] Terminate. Verify `GET /pods` has no `lobo` pod.
- [ ] `results.md` §P1 decisions: path routing y/n; pod key injected y/n; PID-1 exit ⇒ restart/stop; metrics need key y/n; metric names.
- [ ] Divergence from spec → update spec.md first. Tool-call test fails on newest tag → STOP, record, notify the user.
- [ ] Commit `results.md` + testdata.

## P2 — release + agent + plain up/down

### Task 5 — tunnel ingress + routing mode [ORCH]
- [ ] Path routing worked → rule `^/api/` → `http://127.0.0.1:8081`. Constants stay `LlamaAddr=127.0.0.1:8080`, `AgentAddr=127.0.0.1:8081`.
- [ ] Path routing failed → single rule → `http://127.0.0.1:8080`; cut plan v1.2 that flips constants to `AgentAddr=127.0.0.1:8080`, `LlamaAddr=127.0.0.1:8082` and adds a reverse proxy task. Only ONE mode is ever implemented (finding 7). All code uses `LlamaAddr` for `--host/--port`, health, metrics.
- [ ] Record final ingress JSON in `results.md`.

### Task 6 — `internal/model` [IMPL]
- [ ] Tests: `Get("q8")`/`Get("q6")` → file/sha/size/alias from spec (`qwen3.5-27b-uncensored-q8` / `-q6`); `Get("x")` errors naming `q8`, `q6`; `URL("https://pub-x.r2.dev")` = `https://pub-x.r2.dev/models/<File>`.
- [ ] Implement. Pass.

### Task 7 — `internal/config` [IMPL]
- [ ] Tests: `LoadLaptop` with full temp `.env` → ok; missing `RUNPOD_API_KEY` → error names it; `t.Setenv("RUNPOD_API_KEY","os")` + no file value → still error (OS env ignored); no R2 vars → `LoadLaptop` ok, `RequireR2()` error names `R2_ACCESS_KEY`; `SecretValues()` contains `LOBO_API_KEY, CF_TUNNEL_TOKEN, RUNPOD_API_KEY, R2_ACCESS_KEY, R2_SECRET_KEY`. `LoadAgent`: defaults `Ctx=8192, IdleMin=30, BootTimeout=40m`; `LOBO_EXPIRES_AT` missing or not RFC3339 → error naming it; `LOBO_CTX=abc` → error naming it.
- [ ] Implement. Pass.

### Task 8 — `internal/watchdog` [IMPL]
- [ ] Rule: **idle = now − last time an OK sample showed activity (or the ready time)**. Activity = `Processing>0` or `Deferred>0` or token counters moved. Failed samples (`OK=false`) add nothing: they never reset the clock. So a dead scraper cannot keep a pod alive.
- [ ] Tests (fake times): before `SetReady` idle never kills, expiry does; after ready: busy → no; idle 29 min → no, `KillIn`=1 min; 31 min → `"idle"`; tokens moved → reset; 5 min idle then 26 min of failed samples → `"idle"`; `now ≥ ExpiresAt` busy or not → `"expired"`; `NewState` + `ExpiresAt` already past → immediate `"expired"`; `FailedSamples` counts consecutive failures, resets on OK; `KillIn` = min(idle remaining, expiry remaining).
- [ ] Implement. Pass.

(Note on finding 3: a busy pod is only killed if the scraper fails for ≥ idle window while traffic flows. The live test in Task 17 proves the real scraper works under load, so that path needs a scraper bug to trigger. Expiry bounds cost either way.)

### Task 9 — `internal/metrics` [IMPL]
- [ ] Tests on P1 fixtures: `ParseLlama` reads `llamacpp:requests_processing`, `llamacpp:requests_deferred`, `llamacpp:prompt_tokens_total`, `llamacpp:tokens_predicted_total`, `llamacpp:prompt_tokens_seconds`, `llamacpp:predicted_tokens_seconds` (fixture names win; list updated if they differ). Missing `requests_processing` or `prompt_tokens_total` → error; other missing → 0. `ParseNvidiaSMI("NVIDIA GeForce RTX 5090, 30112, 32607, 3")` → struct; malformed → error.
- [ ] `Collector` tests (`httptest`): sends `Authorization: Bearer <key>`; 401 → error; slow server > 3 s → error; `SMI` fake returning fixture → GPU.
- [ ] `ParseHost` tests on fixtures `testdata/loadavg.txt` + `testdata/meminfo.txt` (captured in Task 4): load 1/5/15, mem used = MemTotal − MemAvailable; malformed → error.
- [ ] Implement. Pass.

### Task 10 — `internal/agent` download + logring [IMPL]
- [ ] Tests (`httptest`): one GET writes identical bytes, progress reaches Total (from Content-Length), wrong sha → error and the file is moved to `<dst>.bad`; non-200 → error; ctx cancel/deadline → `ctx.Err()`; server that stalls forever + ctx deadline → returns (finding 1).
- [ ] `LogRing`: last N lines, splits on `\n`, safe under `-race`.
- [ ] Implement. `go test -race ./internal/agent/... -run 'Download|LogRing' -v` passes.

### Task 11 — `internal/agent` killer [IMPL]
- [ ] Depends on Task 16 (`runpod.Client`, `Delete` 404 → nil, `Get` 404 → `runpod.ErrNotFound`). Do Task 16 before this one.
- [ ] `RunPodKiller.KillSelf` (finding 6): loop { `API.Delete` with 10 s timeout; ok → `API.Get`: `ErrNotFound` or `DesiredStatus` TERMINATED → return nil; else sleep backoff 2s,4s,…,60s cap } until ctx done. Called with a fresh `context.Background()`-derived ctx. Logs each attempt.
- [ ] Tests (fake `PodAPI` + fake Sleep): Delete errs twice then ok, Get NotFound → nil after 3 Deletes; Delete ok but Get RUNNING twice → keeps going until NotFound; ctx cancel → returns ctx.Err().
- [ ] Implement. Pass.

### Task 12 — `internal/agent` runner + status [IMPL]
- [ ] Runner loop starts at `Run` entry, not at ready (finding 1): every 30 s → `Sample` (only after ready; before ready it records no samples) → `Decide` with `ExpiresAt` → kill on `Kill`.
- [ ] Boot stages `tunnel→download→load→ready` run under `context.WithTimeout(ctx, BootTimeout)`. Timeout or error → stage `failed`, `StageDetail` set, wait `FailGrace` (2 min, injected), then `KillSelf`. Stage `terminating` while killing.
- [ ] Tests (fake funcs + fake clock): happy path stage order and Status; download error → failed → KillSelf after grace; download hangs → BootTimeout → failed → KillSelf; WaitHealthy never true → same; `ExpiresAt` reached during download → `"expired"` KillSelf without waiting for boot; `ExpiresAt` already past at start → KillSelf immediately; after ready, 31 idle min → KillSelf once; llama process exits after ready → failed → KillSelf. `-race`.
- [ ] `proc.go`: `StartProcess(ctx, name string, args, env []string, logs io.Writer) (*exec.Cmd, error)` + exit notification channel; one test with `/bin/sh -c 'exit 3'`.
- [ ] Implement. Pass.

### Task 13 — `internal/agent` API [IMPL]
- [ ] Tests: `/api/version` and `/api/status` → 200 without auth; `/api/logs` no/wrong bearer → 401, right → 200; `/api/version` returns exact bytes; `/api/status` JSON keys match spec §Pod API (as edited in Task 0), `gpu`/`llama` are `null` when unavailable; `/api/logs?n=5` ≤5 lines; `n>1000` clamps.
- [ ] Implement. Pass.

### Task 14 — `cmd/lobo-agent` wiring [IMPL]
- [ ] `LoadAgent` → read `/lobo/release.json` → `LogRing` → `Runner` with real deps:
  - tunnel: download pinned cloudflared (version from `results.md`) to `/lobo/bin/cloudflared`, run `tunnel run --token`.
  - download: `Download(ctx, cfg.ModelURL, "/models/"+m.File, m.SHA256, …)` where `m = model.Get(cfg.Model)`.
  - llama: `/app/llama-server` with the Task 4 flag list, `--host`/`--port` from `LlamaAddr`, `-m /models/<file>`, `--alias m.Alias`, `-c cfg.Ctx`, output → LogRing.
  - health: poll `http://`+LlamaAddr+`/health` until 200.
  - sample: `metrics.Collector{LlamaURL: "http://"+LlamaAddr, APIKey: cfg.LoboAPIKey, SMI: nvidia-smi exec}` (finding 3). Status refresh every 5 s fills `Llama`, `GPU`, `Host` (each nil on its own failure); watchdog uses the llama sample.
  - killer: `RunPodKiller{PodID: RUNPOD_POD_ID, API: runpod.New(RUNPOD_API_KEY)}`.
  - API on `AgentAddr`.
- [ ] Verify: `make build`; `file bin/lobo-agent` → ELF x86-64, statically linked.

### Task 15 — `internal/release` [IMPL]
- [ ] Tests: `NextVersion` (none → `-1`; `-1`,`-2` → `-3`; other days ignored). `BuildZip` → exactly `lobo-agent` (0755) + `release.json`; sha matches. `ScanForSecrets`: zip containing a secret value → error naming the env var and NOT containing the value; clean zip → nil; values shorter than 8 chars skipped (avoid false hits). `Resolve` (`httptest` as bucket): `""` → latest.json; `"2026.09.22-1"` → that version's json with a different image + ctx (finding 2); 404 → error naming the version.
- [ ] `Store.Publish` order: zip → `releases/lobo-<ver>.json` → `releases/latest.json` (immutable per-version metadata first). Not unit-tested against R2.
- [ ] Git info: `git rev-parse --short HEAD`, `git status --porcelain` non-empty → dirty.
- [ ] Implement. Pass.

### Task 16 — `internal/runpod` [IMPL]
- [ ] Tests: `BuildCreatePayload` → `volumeInGb` 0, `ports` empty, `gpuTypeIds` `["NVIDIA GeForce RTX 5090"]`, env keys = pod env contract exactly, `LOBO_EXPIRES_AT` RFC3339, no `R2_` key anywhere in the payload. `dockerStartCmd` bootstrap contains `sha256sum -c`, `exec /lobo/lobo-agent`, and the failure trap below. Client vs `httptest`: paths, bearer, decode `testdata/pod.json`, `Delete` 404 → nil, no-capacity response → `ErrNoCapacity` (shape from P1 if seen, else RunPod docs).
- [ ] Bootstrap (in `payload.go`), bounded and self-deleting on failure (finding 1):
  ```
  set -e
  die(){ curl -s -m 10 -X DELETE "https://rest.runpod.io/v1/pods/$RUNPOD_POD_ID" -H "Authorization: Bearer $RUNPOD_API_KEY"; sleep 600; exit 1; }
  trap die ERR
  timeout 300 bash -c 'apt-get update -qq && apt-get install -y -qq unzip'
  timeout 300 curl -fsSL "$LOBO_RELEASE_URL" -o /tmp/r.zip
  echo "$LOBO_RELEASE_SHA256  /tmp/r.zip" | sha256sum -c
  unzip -o /tmp/r.zip -d /lobo
  exec /lobo/lobo-agent
  ```
  (`die` retries are best-effort; the laptop `up` timeout is the second line of defence. If P1 says `RUNPOD_API_KEY` is not injected, our key is passed in env and README says so.)
- [ ] Implement. Pass.

### Task 17 — `internal/control` + plain `lobo release/up/down` [IMPL]
- [ ] Tests with fakes + fake clock:
  - `Up`: `Resolve(o.Release)` → image, zip URL/sha, defaults; zero `UpOpts` fields take release `Defaults`; `ExpiresAt` = now + MaxHours; model URL = `model.URL(Cfg.BucketURL)`; existing `lobo` pod → `Err` "already running"; COMMUNITY `ErrNoCapacity` → SECURE; both fail → `Err`; agent statuses tunnel→download(50%)→ready → events in order, download events carry `*DownloadProgress`, final event carries `*ReadyInfo` (finding 8); no progress for `Timeout` (default 20 min; download bytes moving counts as progress) → `Delete`, `Err` with last `/api/logs` lines.
  - `Down`: deletes all `lobo` pods, re-lists until none (3 tries), spend = CostPerHr × hours since `LastStartedAt`.
  - `Snapshot`: no pod → `Down=true`; pod RUNNING + agent unreachable → `Status=nil`, no error.
- [ ] `AgentAPI` real: `GET https://$LOBO_DOMAIN/api/{status,version}` (public), `/api/logs` with bearer; 5 s timeout.
- [ ] `cmd/lobo`: `release` (`RequireR2`, build agent once via `go build` with linux/amd64 env, zip, `ScanForSecrets(zip, cfg.SecretValues())`, `Publish`, print version); `up --plain [--q6] [--ctx N] [--release V] [--idle-min N] [--max-life D]` (`--max-life` is a Go duration, default from release `Defaults.MaxHours`); `down`. `--plain` = one zerolog line per event.
- [ ] Implement. `go test ./internal/control/... -v` passes; `make build` ok.

### Task 18 — P2 live [ORCH]
- [ ] `make release` → version; `releases/lobo-<ver>.json` and `latest.json` public and equal.
- [ ] `bin/lobo up --plain` → `ready`; record per-stage durations.
- [ ] `/api/version` (no key) equals published manifest; `/api/status` stage `ready`, `llama` non-null (proves authenticated scraper works); `/v1/models` lists the alias.
- [ ] Busy-survives test (finding 3): `up --idle-min 3`; run `lobo`-equivalent chat requests in a loop for 6 min; pod stays; `/api/status` `metrics_failures` 0; stop the loop → pod gone within ~4 min.
- [ ] Expiry: `up --max-life 12m` → pod deleted with reason `expired` (check `/api/status` `kill_reason` before, `GET /pods` after). Checksum failures are covered by unit tests only (Tasks 10, 12, 16): no test-only flags or env ship (antislop cut 1). Crash behaviour relies on the P1 PID-1 result (Task 4).
- [ ] `bin/lobo down` → `GET /pods` empty.
- [ ] Commit.

## P3 — TUI

### Task 19 — `internal/tui` up view [IMPL]
- [ ] bubbletea model over `<-chan control.Event`: stage list (✓ done, spinner current, · pending), progress bar (bubbles/progress) from `Event.Download` with MB/s + ETA = (Total−Bytes)/MBps, final lipgloss box from `Event.Ready` (URL, version+sha, $/h, elapsed), error box.
- [ ] Golden tests fed by events produced by `control.Up` running on the Task 17 fakes (finding 8), not hand-made events. Colours off (Ascii profile / v2 equivalent). Cases: download 43%, ready, failed.
- [ ] Implement. Pass.

### Task 20 — `internal/tui` status dashboard [IMPL]
- [ ] Polls `control.Snapshot` every 2 s. Sections: Pod (id, GPU, cloud, $/h, uptime, spend), Release (version, sha, dirty ⚠), Stage, GPU (util % bar, VRAM bar), Host (load 1/5/15, RAM bar), LLM (processing, queued, tokens in = prompt total, tokens out = gen total, prompt tok/s, gen tok/s), all totals since the llama-server start, Watchdog (idle, kill in + reason, expires at, metrics failures ⚠ if >0). `nil` sections → "n/a". `q` quits. `Down` → "lobo: down" + exit.
- [ ] Golden tests: Snap fixtures ready / downloading / down / metrics-unavailable.
- [ ] Implement. Pass.

### Task 21 — `internal/checks` + `lobo test/logs/status`, TUI `up` [IMPL]
- [ ] `ValidateToolCall` tests: string args → object ok; object args → error; missing `tool_calls` → error; `finish_reason` ≠ `tool_calls` → error.
- [ ] `cmd/lobo`: `up` = TUI by default (`--plain` kept); `status` (TUI; `--once` = one plain snapshot); `logs [-n]`; `test` (streamed chat → first 200 chars; tool call → validate; exit 1 on failure).
- [ ] Implement. `go test ./...` passes; `make build` ok.

### Task 22 — P3 live [ORCH]
- [ ] `make up` in a real terminal → TUI to ready. `make status` while `make smoke` runs in another terminal: tokens and processing move. `make logs` prints.
- [ ] `make down`.
- [ ] Commit.

## P4 — OpenCode + benchmark

### Task 23 — `opencode.json.example` + README [IMPL]
- [ ] `opencode.json.example`: provider `lobo`, `npm` `@ai-sdk/openai-compatible`, `baseURL` `https://lobo.example.com/v1`, `apiKey` `{env:LOBO_API_KEY}`, models `qwen3.5-27b-uncensored-q8` and `-q6`, `limit.context` 8192 (Task 24 sets the measured value).
- [ ] README: what it is; `.env`; `make release/up/status/smoke/logs/down`; cost + idle/expiry notes; public bucket note; key rotation; "laptop is control only".
- [ ] Commit.

### Task 24 — acceptance + benchmark [ORCH]
- [ ] Q6_K is already in the bucket (Task 4, finding 4); confirm `curl -sI $LOBO_BUCKET_URL/models/<q6>` Content-Length = 22 082 528 352 before any Q6 pod.
- [ ] Q8 then Q6: `up --ctx N` for 8K/16K/24K/32K: VRAM after load and after a full-context prompt, prompt tok/s, gen tok/s, TTFT. OOM → record, stop increasing.
- [ ] OpenCode on the laptop (ask the user before installing), config from the example, small Go repo: tests 1-6 from `docs/idea.md` §13; pass/fail + notes.
- [ ] Defaults rule: highest ctx with ≥1 GB VRAM headroom; Q8 unless Q6_K gives ≥2× ctx with tests passing. Update release defaults + `opencode.json.example`.
- [ ] `results.md` §P4. Commit.

## Exit gates [ORCH]
- [ ] One `/rival-astra review` on the whole branch (all code done). Fix what holds.
- [ ] **Final delivery gate (finding 5)**, after the last code change: `go test ./... && make lint`; `make release`; `make up` with that version; `/api/version` git_sha == `git rev-parse --short HEAD` and `git_dirty` false; `make smoke`; `make down`; `GET /pods` empty.
- [ ] Reconcile spec.md (`## As-built notes`), plan status `done`, move feature dir to `plans/done/`.
- [ ] Merge `feat/lobo-v1` → `master` same day, push, `/notify`.

## Type-consistency check
- `control.Event.Phase` ↔ `agent.Stage`: `tunnel/download/load/ready/failed` identical; control adds `create/image/running/terminated`; agent adds `boot/terminating` (control maps both to their own phase names).
- `agent.Status` JSON ↔ spec §Pod API (Task 0 edit) ↔ Task 13 test ↔ `control.AgentAPI` decode.
- `release.Resolved` = content of `releases/lobo-<ver>.json` and `latest.json` ↔ `Resolve` ↔ `Publish`.
- Pod env contract ↔ `BuildCreatePayload` ↔ `LoadAgent` ↔ bootstrap ↔ spec.
- `model.Get` ids `q8|q6` ↔ `--q6` ↔ `LOBO_MODEL`; `Model.Alias` ↔ llama `--alias` ↔ `opencode.json.example`.
- `LlamaAddr`/`AgentAddr` used for llama flags, health, metrics, API bind, tunnel ingress.

## Review findings → fixes (Astra, v1.0, 5/10)
| # | Finding | Fixed in |
|---|---|---|
| 1 | Max-life only after ready, no boot deadline | Absolute `LOBO_EXPIRES_AT` in pod env; watchdog from `Run` start; `BootTimeout`; bootstrap `timeout` + `die` self-delete. Tasks 8, 10, 12, 16, 18 |
| 2 | Release resolution lacks boot metadata | `Resolved` + immutable `releases/lobo-<ver>.json` before `latest.json`; `Resolve(version)`. Tasks 15, 17 |
| 3 | Real metrics not wired; `/metrics` needs auth | `metrics.Collector` with bearer + timeouts; fixture must be 200; failed samples add nothing; busy-survives live test. Tasks 4, 9, 14, 18 |
| 4 | Q6 mirror impossible from prod pods | Both models seeded in P1 via laptop-presigned multipart PUTs; Q6 size check before Q6 pods. Tasks 4, 24 |
| 5 | Final release predates exit-gate fixes | Final delivery gate after the Astra review fixes. Exit gates |
| 6 | KillSelf no retry contract | `RunPodKiller`: timeout, backoff, 404 = done, confirm via GET, fresh ctx. Task 11 |
| 7 | Proxy fallback port conflict | `LlamaAddr`/`AgentAddr` defined once; only one mode implemented; proxy mode = plan v1.2. Tasks 5, 14 |
| 8 | Event can't feed up view | `Event.Download`, `Event.Ready`; TUI goldens fed by real `control.Up` on fakes. Tasks 17, 19 |
| 9 | No `requests_total` source | Show `requests_processing` + `requests_deferred`; nullable sections; required metrics error. Tasks 0, 9, 20 |
| 10 | `kill 1` doesn't prove PID-1 exit | bash PID 1 with `trap … USR1` + `/boot-ids`. Task 4 (real-agent repeat dropped with antislop cut 1) |

User changes folded in (2026-09-23): r2.dev public bucket URL (Task 2); R2 keys laptop-only + `ScanForSecrets` (Tasks 7, 15, 16, 17); presigned multipart seeding (Task 4); single-GET download (Task 10); `/api/status` + `/api/version` public (Task 13).

## Antislop cuts (v1.1 → v1.2)
1. No test-only overrides (`--release-sha`, `LOBO_MODEL_SHA256_OVERRIDE`, `LOBO_DEBUG_EXIT_AFTER`); checksum failures unit-tested.
2. Makefile: ops targets depend on `build-lobo` only; `lobo release` owns the single agent build.
3. `RunPodKiller` uses `runpod.Client` via `PodAPI`; no second HTTP implementation.
