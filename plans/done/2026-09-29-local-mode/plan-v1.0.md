# Local Mode Implementation Plan v1.0

**Date:** 2026-09-29
**Status:** done
**Spec:** ./spec.md

**Goal:** `lobo up --provider local` serves the model from this Mac (llama.cpp b11118 Metal), with the weights folder set by the user, wired through the CLI and the mac app.
**Architecture:** A detached `lobo local run` supervisor runs the pod agent's `agent.Runner` with Mac hooks. It serves the same `/api/*` on 127.0.0.1:port+1. `internal/local` implements `provider.Provider`, so `control.Up/Snapshot/Down` stay one code path. The agent client is picked per instance provider.
**Tech Stack:** Go (cobra, zerolog, stdlib archive/tar+gzip, syscall), SwiftUI (macOS 13).

> For agentic workers: use superpowers:subagent-driven-development to implement task-by-task. Checkbox syntax for tracking.
> Implementer scope (verbatim, every dispatch): writes only the code and unit tests its task names, and runs only that task's focused unit test. Never runs e2e/integration/live/smoke tests, never starts llama-server or downloads models, never rents, publishes or deploys, never touches infra, never sets test-DB env vars.

## File map

**Create:**
- `internal/agent/llama.go`, `internal/agent/llama_test.go`
- `internal/local/state.go`, `runtime.go`, `models.go`, `deps.go`, `provider.go`, `platform.go` (+ `_test.go` each)
- `cmd/lobo/local.go` (`lobo local run`, `lobo models`)

**Modify:**
- `cmd/lobo-agent/main.go` (use `agent.LlamaArgs`)
- `internal/config/laptop.go`, `path.go`, `envfile.go`, `cmd/lobo/config.go`
- `internal/control/events.go`, `up.go`, `status.go` (+ tests, `controltest/fakes.go`)
- `cmd/lobo/main.go`, `cmd/lobo/genkey.go`
- `macos/Sources/Lobocode/Models.swift`, `Store.swift`, `PanelView.swift`, `SettingsView.swift`, `Renderer.swift` (+ `macos/Tests`)
- `README.md`, `.env.example`

**Out of scope:** Linux/Intel local, LAN bind, Metal tuning, moving weights between folders.

## Locked interfaces

```go
// internal/agent/llama.go
func LlamaArgs(m model.Model, host, port string, ctx int) []string // exact pod flags today (cmd/lobo-agent/main.go:153-160)

// internal/local/platform.go
func Supported() error                         // nil only on darwin/arm64
func UsableMiB() (int, error)                  // iogpu.wired_limit_mb if >0, else hw.memsize*3/4 (guess, commented)

// internal/local/state.go
type State struct { PID int `json:"pid"`; Port int `json:"port"`; APIPort int `json:"api_port"`; Model string `json:"model"`
  Weights string `json:"weights"`; StartedAt time.Time `json:"started_at"`; BootID string `json:"boot_id"` }
func StatePath() string                        // $XDG_STATE_HOME/lobo/local.json, default ~/.local/state/lobo/local.json
func ReadState() (State, bool, error)          // ok=false if missing OR pid dead (dead → state file removed)
func WriteState(State) error; func RemoveState() error

// internal/local/runtime.go
const RuntimeVersion = "b11118"
var RuntimeURL = "https://github.com/ggml-org/llama.cpp/releases/download/b11118/llama-b11118-bin-macos-arm64.tar.gz" // var: tests swap it
const RuntimeSize = 11205140
const RuntimeSHA256 = "ca0ea3156257b21eeb11d0628f2baecd3928013a3d060e2e192042276e5b1f35"
func EnsureRuntime(ctx context.Context, weights string, note func(string)) (llamaServer string, err error) // <weights>/runtime/llama-b11118/…/llama-server

// internal/local/models.go
const HFBase = "https://huggingface.co/HauhauCS/Qwen3.5-27B-Uncensored-HauhauCS-Aggressive/resolve/main/"
type ModelState struct { ID, File string; Size, OnDisk int64; Verified bool }   // json: id,file,size,on_disk,verified
type Listing struct { Weights string; FreeBytes uint64; Models []ModelState; Runtime struct{ Version string; Present bool } } // json per spec
func List(weights string) (Listing, error)
func MarkerPath(weights, file string) string  // <weights>/<file>.sha256-ok

// internal/local/deps.go
type RunConfig struct { Weights, LlamaServer, APIKey string; Port, Ctx int; Model model.Model }
func NewDeps(cfg RunConfig, logs io.Writer, stop func()) agent.Deps

// internal/local/provider.go
type Provider struct { Exe string; Weights string; Port int }                  // Exe = os.Executable()
func (Provider) Name() string                                                  // "local"
// Rent: Supported → weights writable → space for missing model → ports free → EnsureRuntime → spawn `Exe local run …` (Setsid, log $STATE/local.log) → wait ≤15 s for state file
// List/Get: from ReadState. Delete: SIGTERM, 10 s, SIGKILL, RemoveState. CostPerHr 0.

// internal/control/events.go
type Deps struct { …; LocalAgent AgentAPI; LocalURL string }                  // LocalURL = http://127.0.0.1:<port>/v1
func (d Deps) agentFor(provider string) AgentAPI; func (d Deps) apiURL(provider string) string
func Target(ctx context.Context, d Deps) (AgentAPI, string, error)             // running instance's agent + /v1 URL (cloud domain if none)

// internal/config
Laptop.WeightsDir `env:"LOBO_WEIGHTS_DIR"`, Laptop.LocalPort `env:"LOBO_LOCAL_PORT"`
func (l Laptop) RequireCloud() error          // CF_TUNNEL_TOKEN, LOBO_DOMAIN, LOBO_BUCKET_URL; no longer struct-required
func (l Laptop) Weights() string; func (l Laptop) Port() int                   // defaults: ~/Library/Application Support/lobo/weights, 8931
```

CLI: `lobo up --provider local` (also `LOBO_PROVIDER=local`). `lobo local run --model q6 --ctx N --idle-min N --boot-id X` is hidden. `lobo models [--json]`.

## Self-test sanity check (before Task 1)

- [ ] On clean `master`: `git checkout -b feat/local-mode master`
- [ ] `go build ./... && go test ./...` → all ok. `cd macos && swift build -c release && swift test` → 9 tests pass.

## Tasks

### Task 1 — shared llama args
Files: Create `internal/agent/llama.go`, `llama_test.go`. Modify `cmd/lobo-agent/main.go`.
- [ ] Test: `LlamaArgs(q8, "127.0.0.1", "8080", 65536)` equals today's slice (copy the literal from main.go:153-160 into the test).
- [ ] Implement. Replace the inline slice in main.go.
- [ ] `go test ./internal/agent ./cmd/lobo-agent` ok. Commit.

### Task 2 — config keys, cloud keys optional
Files: `internal/config/laptop.go`, `path.go`, `envfile.go`, `config_test.go`. Modify `cmd/lobo/config.go` (plainKeys).
- [ ] Tests:
  - A config with only `LOBO_API_KEY` + `LOBO_PROVIDER=local` loads.
  - `RequireCloud()` errors naming the missing keys.
  - `Weights()`/`Port()` defaults.
  - `LOBO_PROVIDER=local` is valid in `Defaults()`.
- [ ] Implement:
  - Drop `validate:"required"` from CF/domain/bucket and add `RequireCloud`.
  - Add the keys to the Layout "defaults" group and to plainKeys.
- [ ] `go test ./internal/config ./cmd/lobo` ok. Commit.

### Task 3 — local state + platform
Files: Create `internal/local/state.go`, `platform.go` + tests.
- [ ] Tests:
  - State round trip under `t.Setenv("XDG_STATE_HOME", tmp)`.
  - A dead pid (spawn and wait on `true`, use its pid) → `ok=false` and the file is gone.
  - `Supported()` on the test host: nil on darwin/arm64, else an error.
- [ ] Implement. Liveness via `syscall.Kill(pid, 0)`. Sysctl via `syscall.Sysctl`/`SysctlUint64`.
- [ ] `go test ./internal/local` ok. Commit.

### Task 4 — runtime fetch
Files: Create `internal/local/runtime.go` + test.
- [ ] Tests (httptest server serving a tiny tar.gz with `build/bin/llama-server`, and RuntimeURL/size/sha swapped via package vars):
  - It extracts and returns an executable path.
  - A second call makes no HTTP request.
  - A sha mismatch errors and leaves no runtime dir.
  - A path-traversal tar entry is rejected.
- [ ] Implement:
  - Download to a temp file in `<weights>/runtime/`, verify size + sha, untar.
  - Mode bits kept. Symlinks inside the tree allowed only if relative and inside.
  - Atomic rename of the dir.
  - `note()` gets "llama.cpp b11118 11 MB".
- [ ] `go test ./internal/local -run Runtime` ok. Commit.

### Task 5 — models listing
Files: Create `internal/local/models.go` + test.
- [ ] Tests:
  - Temp weights dir with a full-size sparse file + marker → verified.
  - A short file → on_disk < size, not verified.
  - Missing → 0.
  - JSON field names exactly as the spec.
- [ ] Implement. Free space via `syscall.Statfs`.
- [ ] Test ok. Commit.

### Task 6 — Mac agent deps
Files: Create `internal/local/deps.go` + test.
- [ ] Tests (fake llama-server script in a temp dir, fake HTTP model source):
  - CheckGPU passes when the output has `MTL0` and the memory fits.
  - CheckGPU fails with needed vs usable GB (inject `usableMiB` func).
  - Download: complete+marker → no request; missing → download + sha + marker; bad sha → file moved to `<weights>/.bad/`, error.
  - Metrics GPU name + RSS parse from `ps -o rss= -p` output.
- [ ] Implement with `agent.Download` (`internal/agent/download.go:28`) and `agent.HTTPSource`. StartLlama uses `agent.LlamaArgs` and `agent.StartProcess`, env `LLAMA_API_KEY`. Killer calls `stop()`.
- [ ] Test ok. Commit.

### Task 7 — `lobo local run` supervisor + `lobo models`
Files: Create `cmd/lobo/local.go` (+ `local_test.go` for flag parsing and `models --json` output via a temp weights dir).
- [ ] Implement `local run`:
  - Load config, `local.NewDeps`, `agent.NewRunner` (ExpiresAt = now + 100y, Idle from `--idle-min`).
  - `agent.NewAPI` on 127.0.0.1:APIPort.
  - `WriteState` on start, `RemoveState` on exit.
  - SIGTERM → stop llama, exit 0.
- [ ] Implement `models [--json]`: plain table and JSON.
- [ ] `go test ./cmd/lobo` ok. Commit.

### Task 8 — local provider
Files: Create `internal/local/provider.go` + test.
- [ ] Tests (Exe = a test helper binary built via `os.Args[0]` + env switch, which writes a state file and sleeps):
  - Rent returns `Instance{Provider:"local", ID:pid, CostPerHr:0}`.
  - List sees it.
  - Delete kills it and removes the state.
  - Busy port → error naming `LOBO_LOCAL_PORT`.
  - Unwritable weights → error.
  - Not enough space → error with bytes.
- [ ] Implement per the locked interface. `Rent` pre-checks run before `EnsureRuntime`.
- [ ] Test ok. Commit.

### Task 9 — control: agent per provider, local up
Files: Modify `internal/control/events.go`, `up.go`, `status.go`, `controltest/fakes.go`, `control_test.go`.
- [ ] Tests:
  - Up with a fake local provider + fake LocalAgent → phases include gpu/download/load/ready, `Ready.URL == "http://127.0.0.1:8931/v1"`, no release Resolve called (use `noReleases`), cloud Agent never called.
  - Snapshot with a local instance uses LocalAgent.
  - `Target` returns the local URL when local runs, the domain otherwise.
- [ ] Implement:
  - `agentFor`/`apiURL`.
  - Local skips the presign and model-source switch. ModelURL is unused locally, and local deps build the HF URL.
  - Built-in defaults like `--image`.
- [ ] `go test ./internal/control` ok. Commit.

### Task 10 — CLI wiring
Files: Modify `cmd/lobo/main.go`, `genkey.go` (+ tests).
- [ ] Tests: `providers(cfg)` includes local on darwin/arm64. `up --provider local` passes the cloud-key check. genkey output has a `lobo-local` provider with baseURL `http://127.0.0.1:8931/v1`.
- [ ] Implement:
  - `deps()` sets `LocalAgent = NewHTTPAgentURL("http://127.0.0.1:<port+1>", key)` and `LocalURL`.
  - `RequireCloud()` is called only for runpod/vast.
  - `test`, `logs` and the status version use `control.Target`.
- [ ] `go build ./... && go test ./...` ok. Commit.

### Task 11 — docs
Files: `README.md` (Local section: 5 lines + key table rows), `.env.example`.
- [ ] Commit.

### Task 12 — Swift data layer
Files: `macos/Sources/Lobocode/Models.swift`, `Store.swift`, `macos/Tests/LobocodeTests/LobocodeTests.swift` (+ fixture `models.json`).
- [ ] Tests:
  - Decode the `models --json` fixture.
  - `Step(phase:)` for local.
  - `Store.upArgs` for local = `["up","--json","--provider","local","--q6=…"]`.
  - Ready local URL shown as given.
- [ ] Implement:
  - `target: Target (.local/.cloud)`, persisted in UserDefaults.
  - `loadModels()` via `lobo models --json`.
  - Local steps: start, metal (gpu), model (download/verify), load, ready.
- [ ] `swift test` ok. Commit.

### Task 13 — Swift panels
Files: `PanelView.swift`.
- [ ] Off panel:
  - `[local] cloud` switch.
  - Local: model rows (`q6 22.1 GB ✓` / `q8 28.6 GB ↓ download`), weights path dim, START.
  - Cloud: unchanged.
- [ ] Boot local: step list per Task 12, download bar with MB/s + ETA, verify line.
- [ ] Ready local: endpoint + copy, key + copy, gen/prompt tok/s, memory bar (`gpu.vram_used_mb/total`, labelled "memory"), `idle-stop mm:ss`, `local · $0` dim, STOP.
- [ ] Fail local: message + retry.
- [ ] `swift build` ok. Commit.

### Task 14 — Swift settings
Files: `SettingsView.swift`.
- [ ] `// local` section: `weights` row (path text + `[choose…]` NSOpenPanel, directories only, create allowed) + free space, `port` field. Add both keys to `plainKeys` and to `validate` (port 1024–65535).
- [ ] `swift test` ok. Commit.

### Task 15 — renders (review gate)
Files: `Renderer.swift`.
- [ ] Samples: `panel_off_local`, `panel_boot_local` (downloading, verifying), `panel_ready_local`, `panel_fail_local`, `settings` (with the local section).
- [ ] Orchestrator: render, look at every PNG, fix clipping, send the PNGs to the user as an album on Telegram. **Wait for the user's OK on the renders.**

### Task 16 — live run on this Mac (orchestrator only)
- [ ] `lobo config set LOBO_WEIGHTS_DIR=/Volumes/Extreme/_lobocode` (config file, not env).
- [ ] With the Q6 download verified: `lobo up --provider local --q6`, then `lobo status --once`, `lobo test`, record tok/s.
- [ ] `lobo down`. Confirm the pid is gone and the state file removed.
- [ ] Idle stop: `lobo up --provider local --q6 --idle-min 1`, wait ~2 min, confirm it stopped by itself.
- [ ] If Q8 verified: one Q8 `up` + `test`.
- [ ] App: start local from the panel, reach ready, stop.

### Task 17 — exit gate
- [ ] `/rival-codex review` on `master...feat/local-mode`. Verify, fix.
- [ ] `/simplify`.
- [ ] Reconcile spec (`## As-built notes`). Merge to master and push over the SSH alias. Move the feature dir to `plans/done/`.
- [ ] Telegram notify.

## Type-consistency check

- Provider name `"local"` is used the same way in Tasks 2, 8, 9, 10 and 12.
- Ports: llama `Port()` = 8931, agent API = Port()+1 = 8932, in Tasks 7, 8, 10 and 16.
- Marker `<file>.sha256-ok` (`MarkerPath`) is shared by Tasks 5 and 6.
- The `models --json` field names in Task 5 match Task 12's Swift decode (`id,file,size,on_disk,verified`, `weights`, `free_bytes`, `runtime.version/present`).
- `LlamaArgs` is used by Task 1 (pod) and Task 6 (local).
