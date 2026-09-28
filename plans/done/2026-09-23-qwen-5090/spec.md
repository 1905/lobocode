# Lobo — Qwen3.5-27B uncensored on an on-demand RunPod 5090

**Date:** 2026-09-23
**Scope:** ~/dev/lobotomized-ai
**Status:** shipped 2026-09-23 (see As-built notes)

## TL;DR

**P1 — one-time infra + manual bring-up**
- What: Cloudflare tunnel `lobo` + DNS `lobo.example.com`; R2 bucket `lobo` with its public r2.dev dev URL, holding releases AND the Q8 GGUF (`models/…`) — `lobo.example.com` stays the project's only subdomain; one 5090 pod created by hand with a first release; raw chat + tool-call tests pass through `https://lobo.example.com/v1`.
- Why: proves 5090 + llama.cpp image + tunnel + release download work before the CLI exists.
- You do: nothing. I copy `RUNPOD_API_KEY` from ai-image-studio `.env`, and R2 keys from the cloudflare skill, into this repo's `.env`. I generate `LOBO_API_KEY`. Cost ~$1-2 GPU time.
- Does NOT: build the CLI/TUI, touch any existing DNS record, touch OpenCode.

**P2 — `make release` + pod agent + `/api/version`**
- What: `make release` builds the Go pod agent, zips it with `release.json` (version, git sha, dirty flag, build time, image tag, model), uploads to the `lobo` bucket. The pod downloads the zip at boot. The agent starts llama-server + tunnel, kills the pod after 30 min idle / 12 h, and serves `/api/version`, `/api/status`, `/api/logs`. Plain-text `lobo up --plain` / `lobo down` land here too, so P2 can be tested live before the TUI exists.
- Why: always know which code runs on the pod; the pod is self-sufficient.
- You do: nothing.
- Does NOT: auto-release on push (no CI), keep old releases pruned.

**P3 — `make up` / `make down` / `make status` (Go cobra + TUI)**
- What: Go CLI `lobo` (cobra + bubbletea). `make up` rents a 5090 and shows live boot progress (pod → image → release → model download % → model load → ready). `make down` terminates. `make status` is a live dashboard: pod, $/h, spend, release version, VRAM, tok/s, requests, idle timer.
- Why: control from the laptop only; see what is happening without SSH.
- You do: nothing.
- Does NOT: proxy any LLM traffic through the laptop. Laptop = control only.

**P4 — OpenCode hookup + Q8 vs Q6_K decision**
- What: `opencode.json.example` provider `lobo` → `https://lobo.example.com/v1`; the 6 OpenCode acceptance tests from `docs/idea.md` §13; measure VRAM/tok/s at 8K→32K; pick Q8 or Q6_K on numbers.
- Why: the OpenCode tool-call loop is the real acceptance test.
- You do: install OpenCode if missing; look at the numbers table.
- Does NOT: thinking mode, vision, parallel slots, speculative decoding.

## Problem(s)

1. No way to run the uncensored 27B model today. The only artifact is the write-up (`docs/idea.md:1-3`). The Mac can't host a 28.6 GB Q8 GGUF at useful speed.
2. The write-up reaches the server via an SSH tunnel (`docs/idea.md:520-535`). That needs the laptop in the request path and breaks on sleep. The user wants a self-sufficient pod on a stable `*.example.com` hostname.
3. RunPod bills per hour ($0.69/h community 5090, verified 2026-09-23). A forgotten pod costs ~$8/night; always-on ~$500/month. Nothing kills idle pods.
4. Each fresh pod must fetch 28.6 GB. HF speed is unmeasured. A network volume pins one datacenter, and 5090 stock is "Low".
5. Qwen3.5 tool calling broke in older llama.cpp builds (`docs/idea.md:432-438`). A wrong build silently breaks OpenCode.
6. With code shipped to a remote pod, nobody knows which version runs there. Debugging then means guessing.

## Goals

1. `make up` → working API at `https://lobo.example.com/v1` in ≤15 min, with live progress (r2.dev download dominates, see Goal 4). (Problem 1)
2. Laptop is never in the request path. Hostname is stable across pods. (Problem 2)
3. Pod terminates itself after 30 min idle and after 12 h regardless. `make down` terminates now. (Problem 3)
4. Model from R2 via the r2.dev dev URL, sha256 checked. Expected ~9-10 min for 28.6 GB (ai-image-studio measured r2.dev at 52 MB/s, `~/dev/ai-image-studio/CHANGELOG.md:44`). Faster options (custom domain, presigned S3) are deferred by user decision 2026-09-23. (Problem 4)
5. `lobo test` validates `tool_calls` with string `arguments` before OpenCode is touched. (Problem 5)
6. `GET /api/version` returns the running release's `release.json`. (Problem 6)
7. OpenCode passes the 6 acceptance tests in `docs/idea.md` §13. (Problem 1)

## Non-goals

- A proxy on the laptop, or any LLM traffic through the laptop.
- A custom Docker image or registry. Official llama.cpp image + release zip.
- Multi-user access, rate limiting, per-user keys.
- A network volume. SSH into the pod (logs come from `/api/logs`).
- CI-driven releases.

## Architecture

```text
Laptop (control only)          Cloudflare                        RunPod 5090 pod
┌─────────────────────┐                                   ┌───────────────────────────────────┐
│ make release ───────┼──▶ R2 bucket `lobo` (r2.dev)   ──▶│ bootstrap: curl zip, unzip, exec  │
│   (zip + release.json)       releases/<ver>.zip           │                                   │
│ make up/down/status │──▶ RunPod REST API                │ lobo-agent (Go, PID 1)            │
│   (cobra + TUI)     │                                   │  ├─ cloudflared (tunnel token)    │
└─────────────────────┘                                   │  ├─ llama-server :8080            │
                                                          │  ├─ /api/* on :8081               │
Any client (OpenCode…) ──https + Bearer──▶ lobo.example.com│  ├─ idle / max-life watchdog      │
                                          tunnel ingress: │  └─ model ← R2 `lobo` (r2.dev)    │
                                          /api/* → :8081  └───────────────────────────────────┘
                                          else   → :8080
```

The laptop only calls the RunPod API and R2. After create, the pod runs on its own: it fetches the release and the model, serves through the tunnel and kills itself when idle. A laptop that is off changes nothing. OpenCode is just one client. Any client with `LOBO_API_KEY` works from anywhere.

Why a named tunnel, not `{pod}-8080.proxy.runpod.net`: the pod id changes each boot, so the hostname would need a Worker + stored pod id. A named tunnel keeps one CNAME forever. Routing `/api/*` to the agent is done with a tunnel ingress path rule. So llama-server stays direct on the hot path. When no pod is up, the hostname returns Cloudflare error 1033 and `make status` says "down".

Verified 2026-09-23:
- `lobo.example.com` has no DNS record (free).
- `CF_WORKERS_TOKEN` can list tunnels on account `<cf-account-id>` (0 exist). Create permission not verified. Fallback: Cloudflare MCP on the same account.
- Official image `.devops/cuda.Dockerfile` builds with CUDA 12.8.1. ggml's CUDA CMake adds `120a-real` (5090) for CUDA ≥12.8.
- Image `server` stage (`.devops/cuda.Dockerfile`): only `llama-server` + libs, `libgomp1`, `curl`, `ffmpeg`. No Python (not needed: GGUF runs natively in C++). No `unzip` → bootstrap runs `apt-get install -y unzip`.
- Q8_0 sha256 `78ae0800c6062b0a2fbfb3649233fd152aaf0890d3cd871965795de0119e8dd5`, 28 595 762 272 bytes. Q6_K sha256 `9fc4e4768045cb187a86f42b19f3d74754406680c3688502020556a8f3b70c4b`, 22 082 528 352 bytes (HF API).

Not verified (P1 checks each):
- Tunnel ingress `path` rules on a remotely-managed tunnel. Fallback: the agent listens on :8080, handles `/api/*` itself and reverse-proxies the rest to llama-server on :8082 (streaming flush on).
- RunPod injects a pod-scoped `RUNPOD_API_KEY` for self-terminate. Fallback: pass our key in env (it then sits on a community host).
- `--host 127.0.0.1` overrides the image's `ENV LLAMA_ARG_HOST=0.0.0.0` (check `ss -ltnp` on the pod).
- What RunPod does when PID 1 exits (restart container vs stop). Decides the "agent crash" row below.

## Releases

`make release`:
1. `GOOS=linux GOARCH=amd64 CGO_ENABLED=0 go build ./cmd/lobo-agent`.
2. Write `release.json`, zip `lobo-agent` + `release.json` into `lobo-<version>.zip`.
3. Upload to bucket `lobo`: `releases/lobo-<version>.zip`, then overwrite `releases/latest.json` → `{"version": "…", "key": "releases/lobo-<version>.zip", "sha256": "…"}`.

Version = `YYYY.MM.DD-N` (N = count of releases that day, read from the bucket). Dirty tree is allowed but flagged.

`release.json`:

```json
{
  "version": "2026.09.23-1",
  "git_sha": "a71d072",
  "git_dirty": false,
  "built_at": "2026-09-23T10:12:00Z",
  "built_by": "dev@laptop",
  "llama_image": "ghcr.io/ggml-org/llama.cpp:server-cuda-<pinned build>",
  "model": {"file": "Qwen3.5-27B-Uncensored-HauhauCS-Aggressive-Q8_0.gguf", "sha256": "78ae08…8dd5"},
  "defaults": {"ctx": 8192, "idle_min": 30, "max_hours": 12}
}
```

Bucket `lobo` has its r2.dev dev URL enabled (user decision 2026-09-23: "just use public dev url"). Base URL goes in `.env` as `LOBO_BUCKET_URL`. `make up` passes plain public URLs for the zip and the model. No custom domain (1 project = 1 subdomain: only `lobo.example.com`; `models.example.com` belongs to ai-image-studio).

**Bucket keys never leave the laptop.** R2 keys live only in the laptop `.env` and are used only for uploads (`make release`, model upload). They are never in the agent binary, the zip, `release.json` or pod env. `make release` scans the built zip for every secret value in `.env` and refuses to upload on a match.

Consequence: everything in the bucket is public. The release zip holds the agent binary + `release.json` (no secrets, enforced above). Anyone who guesses the URL can download the agent. Accepted for now.

## Pod boot

`make up` → RunPod `POST /pods`:

```json
{
  "name": "lobo",
  "imageName": "<llama_image from latest release.json>",
  "gpuTypeIds": ["NVIDIA GeForce RTX 5090"],
  "gpuCount": 1,
  "cloudType": "COMMUNITY",
  "containerDiskInGb": 60,
  "volumeInGb": 0,
  "ports": [],
  "dockerEntrypoint": ["bash", "-c"],
  "dockerStartCmd": ["<bootstrap one-liner: apt-get install unzip, curl $LOBO_RELEASE_URL, check $LOBO_RELEASE_SHA256, unzip to /lobo, exec /lobo/lobo-agent>"],
  "env": {
    "LOBO_RELEASE_URL": "<LOBO_BUCKET_URL>/releases/lobo-<ver>.zip",
    "LOBO_RELEASE_SHA256": "…",
    "LOBO_MODEL_URL": "<LOBO_BUCKET_URL>/models/<file>",
    "LOBO_API_KEY": "<from .env>",
    "CF_TUNNEL_TOKEN": "<from .env>",
    "LOBO_MODEL": "q8",
    "LOBO_CTX": "8192",
    "LOBO_IDLE_MIN": "30",
    "LOBO_EXPIRES_AT": "<RFC3339, create time + max life (default 12 h)>",
    "LOBO_BOOT_TIMEOUT": "40m"
  }
}
```

The bootstrap fails → its `ERR` trap deletes the pod via RunPod API. `make up` sees no progress for 20 min and terminates the pod as a second line of defence.

`lobo-agent` boot stages. Each stage is visible in `/api/status` → the TUI shows it:
1. `tunnel` — start `cloudflared tunnel run --token $CF_TUNNEL_TOKEN` (binary downloaded from Cloudflare's GitHub releases). Started first, so progress is visible over the domain during the model download.
2. `download` — GET of the model from `$LOBO_MODEL_URL` (public r2.dev URL). MVP: one plain streaming GET, sha256 computed while writing, no ranges, no parallelism, no retries (user decision 2026-09-23). Reports bytes/total/MB/s. Then sha256.
3. `load` — start llama-server with `docs/idea.md` §17 flags plus `--metrics --no-webui --host 127.0.0.1 --port 8080 -c $LOBO_CTX`. Stage ends when `/health` is OK.
4. `ready`.

Any stage fails → agent logs the error, keeps `/api/status` up with `stage: failed` for 2 min (so the TUI can show why), then deletes its own pod.

Boot stages run under `LOBO_BOOT_TIMEOUT`; timeout = failed.

Watchdog (inside the agent), every 30 s, from agent start:
- `now ≥ LOBO_EXPIRES_AT` → delete own pod (any stage). The expiry is an absolute time in pod env, so a restart can't reset it.
- After `ready`: scrape llama-server `/metrics` (with the API key). Idle = now − last OK sample showing activity (`requests_processing>0`, `requests_deferred>0`, or token counters moved). Failed samples never count as activity.
- Idle ≥ `idle_min` → delete own pod.
- Delete retries with backoff until `GET /pods/{id}` says gone.

## Pod API

`/api/version` and `/api/status` are public (user decision 2026-09-23). `/api/logs` needs `Authorization: Bearer $LOBO_API_KEY`.

`GET /api/version` → the `release.json` of the running release.

`GET /api/status`:

```json
{
  "stage": "ready",
  "stage_detail": "",
  "download": {"bytes": 28595762272, "total": 28595762272, "mbps": 441.2},
  "uptime_s": 1834,
  "idle_s": 312,
  "kill_in_s": 1488,
  "kill_reason": "idle",
  "gpu": {"name": "NVIDIA GeForce RTX 5090", "vram_used_mb": 30112, "vram_total_mb": 32607, "util_pct": 0},
  "host": {"load1": 1.2, "load5": 0.9, "load15": 0.7, "mem_used_mb": 9120, "mem_total_mb": 64000},
  "llama": {"requests_processing": 1, "requests_deferred": 0, "prompt_tokens_total": 182340, "gen_tokens_total": 21044, "prompt_tps": 2410.5, "gen_tps": 48.3},
  "metrics_failures": 0,
  "expires_at": "2026-09-23T22:10:00Z",
  "model": "q8", "ctx": 8192
}
```

`GET /api/logs?n=200` → last N lines of agent + llama-server + cloudflared logs.

GPU numbers come from `nvidia-smi --query-gpu`. Host numbers come from `/proc/loadavg` + `/proc/meminfo`. llama numbers come from its `/metrics` (tokens in = prompt total, tokens out = generated total, since llama-server start). `make status` shows only what this endpoint returns. `gpu`, `host`, `llama` are `null` when unavailable.

## CLI (laptop)

Go, cobra + bubbletea/lipgloss, zerolog, godotenv, validator (per `~/.claude/docs/go-project-standards.md`). Keys from this repo's `.env` only, never the OS env.

| Make target | Command | Behaviour |
|---|---|---|
| `make release` | `lobo release` | Build + zip + upload (above). Prints version. |
| `make up` | `lobo up [--q6] [--ctx N] [--release V]` | Refuse if a pod named `lobo` exists. Create pod (COMMUNITY, then SECURE if no stock). TUI: RunPod stages (created → image pulling → running) from `GET /pods/{id}`, then agent stages from `/api/status` (download bar with MB/s + ETA, load, ready). Ends with endpoint URL, release version, $/h, total boot time. 10 min without progress → terminate + show last `/api/logs`. |
| `make down` | `lobo down` | Terminate every pod named `lobo`. Confirm `GET /pods` has none. Print session spend. |
| `make status` | `lobo status` | Live dashboard, refresh 2 s: pod id, GPU, cloud type, $/h, uptime, spend since start; release version + git sha (+ dirty); stage; VRAM bar, GPU util; requests, prompt/gen tokens, tok/s; idle timer and "auto-kill in". Pod down → one line "down" + exit. |
| `make logs` | `lobo logs [-n]` | Print `/api/logs`. |
| `make smoke` | `lobo test` | Streamed chat + non-streamed tool-call checks (`docs/idea.md` §9-10) via the domain. Fails if `arguments` is not a JSON string. |

`.env` (gitignored), names only in `.env.example`:

```text
RUNPOD_API_KEY=        # copied from ~/dev/ai-image-studio/.env
LOBO_API_KEY=          # sk-<48 hex>, generated once
CF_TUNNEL_TOKEN=       # from tunnel create
R2_ACCOUNT_ID= R2_ACCESS_KEY= R2_SECRET_KEY= R2_ENDPOINT=   # laptop-only: uploads
LOBO_BUCKET_URL=       # https://pub-<hash>.r2.dev
```

## Model mirror

One-time, in P1. Upload from a pod, not the laptop (28.6 GB over the home uplink is slow):
1. On the first pod: download Q8_0 from HF → check sha256 → upload to bucket `lobo`, key `models/<file>`. Keys stay on the laptop here too: the laptop starts an S3 multipart upload and presigns one PUT URL per 1 GB part. The pod `curl -T`s each part to its URL and returns the ETags. The laptop completes the upload.
2. Measure the r2.dev pull (one connection). Record MB/s in `results.md`. Prior data point: 52 MB/s on r2.dev in ai-image-studio.
3. Q6_K is uploaded only if P4 picks it.

No public URL for anything in this project except the API itself.

## File-level changes

| File | Change |
|---|---|
| `go.mod`, `Makefile`, `.golangci.yml` | Module `github.com/1905/lobotomized-ai`. Targets: `release up down status logs test build lint`. |
| `cmd/lobo/main.go` | Laptop CLI root (cobra). |
| `cmd/lobo-agent/main.go` | Pod agent entry. |
| `internal/release/` | Version numbering, `release.json`, zip, secret scan, R2 upload, `latest.json`. |
| `internal/runpod/` | REST client: create, list, get, delete. Create payload builder. |
| `internal/tui/` | bubbletea models for `up` progress and `status` dashboard. |
| `internal/agent/` | Stage runner, downloader, process supervisor, `/api/*` server. |
| `internal/watchdog/` | Pure idle/max-life decision: `State.Observe(Sample)` + `State.Decide(now, Config) Decision`. |
| `internal/model/` | Model catalog: `q8`, `q6` → file, sha256, size, R2 URL. |
| `internal/control/` | Laptop-side orchestration for `up/down/status`: emits an `Event` stream consumed by both the TUI and `--plain` log output. |
| `internal/metrics/` | Parse llama-server Prometheus text + `nvidia-smi` CSV. |
| `internal/checks/` | Chat + tool-call validation for `lobo test`. |
| `internal/config/` | `.env` load + validator, errors name the missing var. |
| `opencode.json.example` | Provider `lobo`, baseURL `https://lobo.example.com/v1`, key `{env:LOBO_API_KEY}`, context from P4 numbers. |
| `.env.example`, `README.md` | Var names, how to run. |
| `plans/2026-09-23-qwen-5090/results.md` | Measured numbers from P1/P4. |

## Tests

Unit (Go, table-driven, no network):
- `watchdog.Decide`: busy → no; idle 29 min → no; 31 min → kill "idle"; tokens moved → idle resets; now ≥ expires_at → kill "expired"; failed samples never count as activity.
- `metrics`: parse real llama-server `/metrics` sample + `nvidia-smi` CSV sample (captured in P1 into `testdata/`).
- `release`: version N increments per day; `release.json` fields; zip contains exactly `lobo-agent` + `release.json`.
- `runpod`: payload has env keys, `volumeInGb` 0, `--q6` swaps model.
- `checks`: `arguments` string → pass; object → fail; no `tool_calls` → fail.
- `config`: missing var → error names it.
- `tui`: status model renders "down", "download 43%", "ready" from fixture JSON (golden strings).

Live (orchestrator only, costs money):
- P1: `nvidia-smi` shows RTX 5090; llama-server log shows compute capability 12.0, all layers on GPU; `lobo test`-equivalent curl passes via domain; the 4 unverified items above.
- P2: `/api/version` matches the uploaded `release.json`; bad release sha → pod never starts agent, `up` times out and terminates; bad model sha → `stage: failed`, pod deletes itself.
- P3: full `make up` → `make status` → `make down` with TUI; idle kill with `idle_min=3`.
- P4: OpenCode tests 1-6; VRAM + tok/s at 8K/16K/24K/32K for Q8, then Q6_K.

## Failure modes & decisions

| Failure | Behaviour |
|---|---|
| No 5090 in COMMUNITY | Retry SECURE ($0.99/h). Still none → exit with a clear message. No other GPU types. |
| Release download / sha fails | Bootstrap exits, agent never starts. `up` sees no progress in 10 min → terminates. |
| Agent stage fails | `stage: failed` + reason for 2 min, then self-delete. |
| Laptop off after `up` | Nothing changes. Watchdog kills the pod when idle. |
| Agent process crashes | Depends on RunPod PID-1 behaviour (verify in P1). If the container restarts → full reboot from release. If it stops → the pod bills with no watchdog alive. Mitigation: `up` and `status` warn about any `lobo` pod older than 12 h. |
| Pod-scoped RunPod key absent | Pass our key via env. README warns. |
| Q8 OOMs at 8K | `--ctx 4096`; still OOM → default to Q6_K. |
| Tool-call test fails | Bump pinned llama.cpp image, new release, retest. No OpenCode debugging before it passes. |
| Cloudflare 100 s timeout on long non-streamed replies | OpenCode streams. `lobo test` tool call is short (<100 s). Documented. |
| API key leaks | New `LOBO_API_KEY` in `.env`, next `up` uses it. |
| r2.dev throttles / download fails | No retry (MVP). Stage `failed` → pod self-deletes; run `make up` again. Download optimisation is out of scope. |
| Secret found in release zip | `make release` refuses to upload, names the `.env` key that matched. |
| Community pod reclaimed mid-session | `status` shows down; run `up` again. |

## Out of scope

- Thinking mode, vision (mmproj), parallel slots, speculative decoding, 262K context.
- Cloudflare Access or auth beyond the API key.
- Serving other models. Pruning old releases.
- Download optimisation: parallel ranges, retries, resume, custom domain, presigned downloads.
- CI for this repo. `llama-cpp-python` (`docs/idea.md` §16).

## Rollout

- **P1** — tunnel + DNS + `lobo` bucket, model to R2, manual pod, raw chat/tool-call via domain, resolve the 4 unverified items. Commit `results.md` + findings.
- **P2** — `make release`, `lobo-agent` (stages, watchdog, `/api/*`), unit tests; live release + failure tests. Commit.
- **P3** — `lobo up/down/status/logs/test` with TUI, unit + golden tests; live full cycle + idle kill. Commit.
- **P4** — `opencode.json.example`, acceptance tests 1-6, quant/context benchmark, set defaults. Commit.

## As-built notes (2026-09-23)

What shipped differs from the spec above in these points. Evidence: `results.md`.

- **Self-delete key:** RunPod injects a pod-scoped `RUNPOD_API_KEY`. It gets 403 on REST but may `podTerminate` its own pod over GraphQL. Agent and bootstrap use that. Our account key never goes to a pod. `RUNPOD_API_KEY` is not in the create payload.
- **Model source:** default is SSH from the model server (user request): restricted user `lobo`, forced command streaming one of the two GGUFs from an offset; pod pulls 54–57 MB/s, resumes by offset, host key pinned. Fallback (remove `LOBO_MODEL_*` from `.env`): bucket `lobo` public r2.dev URL, 1–52 MB/s depending on host. Bucket seeded from the model server via laptop-presigned multipart PUTs.
- **Download:** single connection, but resumes with `Range` on a dropped connection (r2.dev dropped at 92.8% live). Hosts under 15 MB/s after 2 min fail with `host: download too slow`.
- **Host checks** (all seen live): GPU preflight `llama-server --list-devices` must show `CUDA0` (retries 3 min: CUDA init failed with `unknown error` right after boot on several pods) and free VRAM ≥ model + 1.5 GiB (one host had ~6 GB held outside our container → OOM on load). `gpu:`/`host:` failures → `up` deletes the pod and rents another (3 tries).
- **Agent fails fast** when llama-server exits during load, or cloudflared exits in any stage. Fatal config errors still self-terminate (PID-1 exit only restarts the container).
- **llama flags:** `--reasoning off` replaces the deprecated `--chat-template-kwargs enable_thinking` + `--reasoning-budget 0`. API key via `LLAMA_API_KEY` env, not argv. `LD_LIBRARY_PATH=/app:/usr/local/cuda/lib64` is required when not started from the image entrypoint.
- **Boot time:** ~10–13 min (download dominates), not ≤6 min. Goal 1 updated by the r2.dev decision.
- **`make test`** = unit tests; live checks are `make smoke` (`lobo test`) and `make e2e` (23-test suite against the live API). OpenCode acceptance is left to the user.
- **Debug:** `lobo up --ssh <pubkey>` opens 22/tcp and runs sshd before the bootstrap.
- **Not done:** Q8 vs Q6_K / context benchmark (P4) beyond Q8 @ 8K (VRAM 27.1/32.6 GB, 48–50 tok/s gen, ~2400 tok/s prompt). Interactive TUI checked by golden tests only, not in a real terminal.
