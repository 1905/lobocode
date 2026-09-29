# Local mode: run lobo on this Mac

**Date:** 2026-09-29
**Scope:** /Users/kass/dev/lobocode
**Status:** approved

## TL;DR

**P1 — `lobo up --provider local` (CLI).** What: runs the same model on this Mac with llama.cpp b11118 (Metal), the build the cloud pins. lobo downloads that llama.cpp build (11 MB) and the GGUF from Hugging Face into a weights folder you choose (`LOBO_WEIGHTS_DIR`). It serves `http://127.0.0.1:8931/v1` with your `LOBO_API_KEY` and stops after 30 min idle. `status`, `down`, `test` and `logs` work the same as in the cloud. Why: RunPod community 5090s were 5/5 broken today, the Mac has 64 GB, and Q6/Q8 fit (22.6/28.6 GiB). You do: nothing. Does NOT do: LAN access, Intel Macs, Linux/CUDA local, or running local and cloud at the same time.

**P2 — mac app UX.** What: the start panel gets `local | cloud`. Local shows which models are already in the weights folder, and a download size for the ones that aren't. The boot panel shows local steps (metal, model, load). The ready panel shows the local endpoint, tok/s, memory used and the idle-stop timer, with no money. Settings gets a `// local` section with a weights-folder picker and the port. Why: you asked for a UX to run it locally and set where weights live. You do: review PNG renders of every new state before merge. Does NOT do: any change to the cloud screens beyond the target switch.

**P3 — run it here.** What: I run `lobo up --provider local --q6` on this Mac against `/Volumes/Extreme/_lobocode`, run `lobo test`, and report tok/s. Then the same through the app. You do: nothing. Does NOT do: tune performance.

## Problem(s)

1. No way to serve without renting. `lobo up` only knows RunPod and Vast (`internal/control/up.go:60`, `d.Providers[o.Provider]`). Today every community 5090 was broken (VRAM held outside the container ×3, CUDA init failure ×2), while this Mac (M1 Max, 64 GB) can hold Q6 (~22.6 GiB) or Q8 (~28.6 GiB, measured 29316 MiB on the 5090 at 64K ctx).
2. Weights live wherever the pod puts them (`cmd/lobo-agent/main.go:37` `modelDir = "/models"`). The laptop has no notion of a weights folder, and the internal SSD has only 49 GiB free, so weights must be able to live on `/Volumes/Extreme`.
3. The home network DNS-blocks `*.r2.dev`, so the bucket is not a usable model source from this Mac. Hugging Face is: `huggingface.co` answers with a 302 to `us.aws.cdn.hf.co`, measured ~12 MB/s total.
4. The mac app assumes a rented pod everywhere: cost, provider pick, and the rent/container/tunnel steps (`macos/Sources/Lobocode/Store.swift:29-40`, `:168`).

## Goals

1. `lobo up --provider local` boots llama-server on this Mac and prints a ready endpoint (P1).
2. One weights folder, set by `LOBO_WEIGHTS_DIR`, holding the GGUFs and the llama.cpp runtime. Missing files are downloaded, resumable and sha256-checked (P1).
3. Same llama-server build and flags as the cloud (P1). Model quality and behaviour match.
4. Idle stop after `LOBO_IDLE_MIN` (default 30) frees the memory (P1, from the user's answer).
5. `status/down/test/logs/gen-api-key` cover local (P1).
6. The mac app can start, watch and stop local runs, and set the weights folder (P2).

## Non-goals

- LAN or remote access to the local server. It binds 127.0.0.1 only (user answer).
- Intel Macs, and Linux with a local GPU.
- Local and cloud running at the same time. `up` keeps refusing while anything runs.
- A separate local API key. The same `LOBO_API_KEY` is used.
- A Metal performance tuning pass.

## Architecture

The local run reuses the pod agent's `Runner` (`internal/agent/runner.go:18` `Deps` are all hooks) inside a detached `lobo local run` supervisor process. The CLI talks to it the same way it talks to a pod: `/api/status`, `/api/logs` and `/api/version`.

```
lobo up --provider local                     (exits when ready, like today)
  └─ local.Provider.Rent
       ├─ ensure runtime  <weights>/runtime/llama-b11118/   (GitHub asset, sha256 pinned)
       └─ spawn detached: lobo local run  (setsid, log → $STATE/local.log, state → $STATE/local.json)
             agent.Runner with local Deps:
               StartTunnel  no-op (never exits)
               CheckGPU     llama-server --list-devices has "MTL0"; memory need ≤ usable (see below)
               Download     <weights>/<file>: missing/short → HF download (resume); then sha256 once → <file>.sha256-ok
               StartLlama   same args as the pod, host 127.0.0.1, port LOBO_LOCAL_PORT (8931)
               Llama        /metrics (unchanged)
               GPU          name "Apple M1 Max" (sysctl), used = llama RSS, total = hw.memsize
               Killer       stop llama, remove state file, exit
             agent API on 127.0.0.1:8932   (/api/status, /api/logs, /api/version)
control.Up polls the local agent API → same events → ReadyInfo.URL http://127.0.0.1:8931/v1
```

State file `$XDG_STATE_HOME/lobo/local.json` (default `~/.local/state/lobo/local.json`):

```json
{"pid": 41234, "port": 8931, "api_port": 8932, "model": "q6", "weights": "/Volumes/Extreme/_lobocode",
 "started_at": "2026-09-29T12:00:00Z", "boot_id": "…"}
```

`local.Provider` implements `provider.Provider`:
- `List` returns the state file entry if its pid is alive, else nothing. A stale file is moved aside.
- `Delete` sends SIGTERM to the supervisor, waits 10 s, then SIGKILL.
- `CostPerHr` is 0.

Usable memory check: `hw.memsize × 0.75` must be ≥ `minFreeMiB(model)`. 0.75 is the macOS default GPU wired limit, a guess marked in code. If `iogpu.wired_limit_mb` > 0, use that instead. On failure the stage error reads "q8 needs 29.2 GB, this Mac allows ~48 GB to the GPU". There is no pod here, so there is nothing to delete, but the error is reported the same way.

Model source for local: `https://huggingface.co/HauhauCS/Qwen3.5-27B-Uncensored-HauhauCS-Aggressive/resolve/main/<file>`. It's byte-identical to our copies, checked against the catalog sha256 (`internal/model/catalog.go:22,29`).

## Config

| Key | Default | Meaning |
|---|---|---|
| `LOBO_WEIGHTS_DIR` | `~/Library/Application Support/lobo/weights` | GGUFs + `runtime/` |
| `LOBO_LOCAL_PORT` | `8931` | llama-server. The agent API uses port + 1 |
| `LOBO_PROVIDER=local` | — | makes local the default target |

The cloud keys (`CF_TUNNEL_TOKEN`, `LOBO_DOMAIN`, `LOBO_BUCKET_URL`) stop being required when only local is used. Validation moves to "required by the cloud providers".

New command `lobo models --json` lists the catalog models with their state in the weights folder. The app reads it:

```json
{"weights": "/Volumes/Extreme/_lobocode", "free_bytes": 1009000000000,
 "models": [{"id": "q6", "file": "…Q6_K.gguf", "size": 22082528352, "on_disk": 22082528352, "verified": true},
            {"id": "q8", "file": "…Q8_0.gguf", "size": 28595762272, "on_disk": 0, "verified": false}],
 "runtime": {"version": "b11118", "present": true}}
```

## App UX (P2)

States, all rendered by `Lobocode --render` for review:

| Surface | Local | Cloud |
|---|---|---|
| Off panel | target `[local] cloud`, model rows `q6 22.1 GB ✓ on disk` / `q8 28.6 GB ↓ download`, weights path dim, START | unchanged + target switch |
| Boot | steps `start · metal · model · load · ready`. Model shows `verify` or download bar + MB/s + ETA | unchanged |
| Ready | endpoint `http://127.0.0.1:8931/v1` copy, key copy, gen/prompt tok/s, memory bar `23.1/64 GB`, `idle-stop 27:14`, `local · $0`, STOP | unchanged |
| Fail | message + retry, e.g. not enough memory, port busy, download failed | unchanged |
| Settings | new `// local` section: weights folder + `[choose…]` (NSOpenPanel) + free space, port | unchanged |
| Menu bar | same square and word. Ready shows `45 t/s` / idle timer | unchanged |

The rules from `~/.claude/docs/ux-design-guide.md` apply: one primary action, no explaining captions, and money shown only where money is spent. Local shows `$0`, dim.

## File-level changes

| File | Change |
|---|---|
| `internal/local/runtime.go` (new) | Pinned llama.cpp macOS asset (URL, size 11205140, sha256 `ca0ea3156257b21eeb11d0628f2baecd3928013a3d060e2e192042276e5b1f35`). Ensure = download + verify + untar into `<weights>/runtime/llama-b11118/`. |
| `internal/local/provider.go` (new) | `provider.Provider` for local: Rent spawns the supervisor, List/Get read the state file, Delete signals it. |
| `internal/local/state.go` (new) | State file read/write, pid-alive check, stale cleanup. |
| `internal/local/deps.go` (new) | `agent.Deps` for the Mac: Metal check, memory check, HF download + one-time sha marker, llama start, RSS/sysctl metrics, killer. |
| `internal/local/models.go` (new) | `lobo models` data: on-disk size, verified marker, free space. |
| `internal/agent/llama.go` (new) | `LlamaArgs(m, host, port, ctx)` shared by the pod agent and local, so flags can't drift. |
| `cmd/lobo-agent/main.go` | Use `agent.LlamaArgs`. |
| `cmd/lobo/main.go` | `--provider local`; hidden `lobo local run`; `lobo models [--json]`; register the local provider in deps. |
| `internal/control/status.go`, `events.go`, `up.go` | Agent client chosen per instance provider (local = `http://127.0.0.1:<api_port>`). ReadyInfo URL for local. Skip the release manifest for local (built-in defaults, same as `--image`). |
| `internal/config/laptop.go`, `path.go`, `envfile.go` | `LOBO_WEIGHTS_DIR`, `LOBO_LOCAL_PORT`. Cloud keys required only when a cloud provider is used. `local` is a valid `LOBO_PROVIDER`. |
| `cmd/lobo/genkey.go` | `opencode.lobo.json` also gets a `lobo-local` provider at `http://127.0.0.1:8931/v1`. |
| `cmd/lobo/test.go` (or wherever `lobo test` lives) | Target the running instance's URL (local or domain). |
| `macos/Sources/Lobocode/Store.swift`, `Models.swift` | Target (local/cloud), `models --json`, local step mapping, local ready fields. |
| `macos/Sources/Lobocode/PanelView.swift` | Target switch, local off/boot/ready/fail views. |
| `macos/Sources/Lobocode/SettingsView.swift` | `// local` section with folder picker and port. |
| `macos/Sources/Lobocode/Renderer.swift` | Sample states for every local surface. |
| `README.md`, `.env.example` | Local section, new keys. |

## Tests

- `internal/local`:
  - Runtime ensure with a fake HTTP server: sha mismatch is rejected, a present runtime is skipped.
  - State file round trip, and a dead pid reads as not running.
  - Models listing with fake files and markers.
  - Memory check at the boundaries.
- `internal/agent`: `LlamaArgs` golden. The pod agent's args are unchanged byte for byte.
- `internal/control`: fake local provider + fake agent. Up emits the local phases, ReadyInfo URL is `http://127.0.0.1:8931/v1`, and status picks the local agent.
- `internal/config`: a local-only config (no CF/domain/bucket) validates. A cloud provider without them errors.
- `cmd/lobo`: `models --json` shape, `--provider local` accepted.
- Swift: `Store` maps local events and `models --json`. Renders of all new states.
- Manual (P3): real `lobo up --provider local --q6` on this Mac, then `lobo test`, `lobo status`, `lobo down`, and the idle stop with `--idle-min 1`.

## Failure modes & decisions

| Failure | Behaviour |
|---|---|
| Not Apple Silicon / not macOS | `--provider local` errors before doing anything: "local mode needs macOS on Apple Silicon" |
| Weights folder missing or not writable (SSD unplugged) | Fail at start: "weights folder /Volumes/Extreme/_lobocode is not writable". Nothing is created elsewhere |
| Not enough space for a missing model | Fail before downloading, with needed vs free bytes |
| HF download drops | Resume (`agent.Download`). After retries, fail with the source named |
| sha256 mismatch | Move the bad file to `<weights>/.bad/`, fail. Never serve an unverified file |
| Port 8931/8932 busy | Fail: "port 8931 in use (LOBO_LOCAL_PORT)" |
| Model too big for usable memory | CheckGPU fails with needed vs usable GB |
| Supervisor crashes | Next `status` finds a dead pid, reports `down`, cleans the state file |
| Laptop sleeps | llama-server pauses with the machine. The idle timer counts wall time, so it may stop right after wake. Accepted |
| `up` while local runs | Refused, same as the cloud ("lobo already running: local …") |

## Out of scope

- LAN exposure, TLS, a separate local key.
- Linux/CUDA or Intel local.
- Auto-choosing local vs cloud.
- Moving an existing weights folder when the setting changes. The new folder starts empty unless files are already there.
- Metal tuning (batch sizes, `--cache-type` changes).

## Rollout

- **P1** CLI local mode + config + `lobo models` + opencode local provider. Commit gated on tests.
- **P2** mac app UX + renders. Commit after you review the renders.
- **P3** live run on this Mac (Q6, then Q8 if both downloads verify). Results go into this spec. Merge to master.
