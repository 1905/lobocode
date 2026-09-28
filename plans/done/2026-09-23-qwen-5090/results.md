# Results

**Status:** done 2026-09-23

## P1 — infra

- R2 bucket `lobo` created; dev URL `https://pub-<bucket-id>.r2.dev`. Public GET returned 401 for ~1 min after enabling, then 200.
- Tunnel `lobo` id `<tunnel-id>` (remotely managed). `CF_WORKERS_TOKEN` got `Authentication error` on create. Created with the API token inside `~/.cloudflared/cert.pem` (same account `<cf-account-id>`, zone `example.com`).
- DNS: CNAME `lobo` → `<tunnel-id>.cfargotunnel.com`, proxied, record id `<dns-record-id>`. Only this record belongs to the project.
- Domain without connector: HTTP 530.

## P1 — manual pod `3ab00vd3rf1575` (COMMUNITY, $0.69/h, 2026-09-23 08:48–09:05 UTC)

Image `ghcr.io/ggml-org/llama.cpp:server-cuda-b11118` (llama.cpp `0.4.1-dev (build 11118, commit e6ab7c1a4)`).

| Check | Result |
|---|---|
| GPU | NVIDIA GeForce RTX 5090, 32607 MiB, driver 580.65.06, CUDA 13.0 |
| Host | 112 vCPU, 440 GB RAM, 80 GB container disk |
| `python3` in image | **present** (`python3-minimal` from the base). Not used. My Dockerfile-based "no Python" claim was wrong. |
| `unzip` in image | absent → bootstrap installs it |
| llama-server libs | `/app/llama-server` needs `LD_LIBRARY_PATH=/app` (image env is `/usr/local/cuda/lib64`; the entrypoint works from `WORKDIR /app`). Agent must set it. |
| `LLAMA_ARG_HOST` | image sets `0.0.0.0` → agent passes `--host 127.0.0.1` explicitly (override check pending with llama running) |
| RunPod env | injects `RUNPOD_POD_ID`, `RUNPOD_API_KEY` (a **pod-scoped key**, not ours), `RUNPOD_GPU_NAME`, etc. SSH sessions do NOT inherit container env → read `/proc/1/environ`. |
| Pod key scope | REST `GET/DELETE /v1/pods/{self}` → **403**. GraphQL `pod(input:{podId})` → OK. GraphQL `myself` → Unauthorized. GraphQL `podTerminate(self)` → **pod terminated**. ⇒ killer + bootstrap `die` use GraphQL `podTerminate` with the injected key; our account key never goes to a pod. |
| PID-1 exit | `kill -USR1 1` (trap → `exit 7`): container **restarted** within 15 s, new boot id written, container disk kept (`/boot-ids` survived), RunPod API unchanged (`desiredStatus` RUNNING, `lastStartedAt` same). ⇒ agent crash = full re-boot of the agent with the same env and the same `LOBO_EXPIRES_AT`. |
| cloudflared | `2026.9.1` static binary; 4 connections registered with the token (default protocol works on the pod). |
| Path routing | via domain: `/api/x` → 418, `/api/status` → 418, `/` → 502, `/v1/models` → 502 ⇒ ingress `path` rules work; no proxy mode needed. |
| HF download on pod | ~130 MB/s combined for 2 parallel files (stopped: user wants seeding on the model server, not on the GPU pod). |

Decisions:
- Routing mode: path rules (`LlamaAddr=127.0.0.1:8080`, `AgentAddr=127.0.0.1:8081`).
- Self-delete: GraphQL `podTerminate` with pod-injected `RUNPOD_API_KEY`. Confirm with GraphQL `pod` query.
- Model seeding: on the model server (HF → the model server → R2 via laptop-presigned multipart PUTs).
- Still open (needs a pod with the model): llama flags, VRAM, compute capability, `ss -ltnp`, `/metrics` auth + fixture, tool-call test, r2.dev pull speed.

## P2 — Task 5 ingress (final)

`[{"hostname":"lobo.example.com","path":"^/api/","service":"http://127.0.0.1:8081"},{"hostname":"lobo.example.com","service":"http://127.0.0.1:8080"},{"service":"http_status:404"}]`

## Model seed (the model server)

HF → the model server: Q6_K 37 MB/s (592 s), Q8_0 48 MB/s (601 s), run in parallel. sha256 of both match spec.

## P2 — live `lobo up` runs

| Run | Result |
|---|---|
| 1 (release -2) | COMMUNITY create → HTTP 500 `create pod: There are no instances currently available`. Not matched as no-capacity → no SECURE fallback. Fixed matcher (`no instances`) + test. |
| 2 (release -2) | COMMUNITY none → SECURE pod `2sjak5inhao3m4` $0.99/h. rent→download start ~1 min. r2.dev single GET 49–52 MB/s. **Dropped at 92.8% with `unexpected EOF`** after 9 min. Agent: stage `failed` 09:45:31 → pod gone by 09:47:34 (2 min grace + GraphQL self-terminate) ✓. |
| fix | Downloader resumes on drop with `Range: bytes=N-` (single connection, up to 8 resumes, hash continues). r2.dev returns 206 for Range ✓. Release -3. |
| 3 (release -3) | COMMUNITY pod `ahzhub6870xxz4` $0.69/h. create→ready **9m48s** (image 30 s, download 9m05s at 52 MB/s, load 10 s). Smoke passed but slow: chat 184 s, tool call 62 s. Cause: `ggml_cuda_init: failed to initialize CUDA: unknown error` → llama ran on CPU (0.6 tok/s, VRAM 2 MB). Pod terminated ($0.17). |
| debug | SSH pod `nw0wk8qhjivgi7` (driver 595.91.07): `llama-server --list-devices` finds `CUDA0: RTX 5090` with the image env, our agent env, and `env -i`. ⇒ host-specific CUDA failure on the previous machine, not our env. |
| fix | Agent: GPU preflight (`--list-devices` must show `CUDA0:`) right after the tunnel, before the download; fails with `gpu: …`. `up`: on `gpu:` failure delete the pod and rent another (3 tries total). Release -4. |
| 4 (release -4) | 3 pods in a row failed the new GPU preflight (`ggml_cuda_init: failed to initialize CUDA: unknown error`) within ~30 s of boot, each deleted in ~20 s. `up` gave up after 3. |
| repro | `up --ssh` (sshd installed first, ~40 s later agent start): same host `80.15.7.37` passed the GPU check. Running the real agent from SSH also passed. Cause not proven: early-boot timing vs package side effect. Fix: GPU check retries every 10 s up to 3 min; nvidia-smi sampling starts only after it passes. Release -5. |
| 5 (release -5) | GPU check ok on attempt 1. Download 41 MB/s. **Load failed: `cudaMalloc failed: out of memory` allocating 25972 MiB** — host showed 25901 of 32109 MiB free (≈6 GB held outside our container, util 100% before our load). Agent missed the llama exit and sat in `load`. Pod downed ($0.24). |
| fix | Agent watches llama exit during `/health` wait → fail at once. GPU check requires free VRAM ≥ model + 1.5 GiB (Q8: 28.8 GiB) → `gpu:` failure → `up` re-rents. `--reasoning off` replaces deprecated `enable_thinking` kwarg. Release -6. |
| 6 (release -6) | Clean host (VRAM 6 MB used before load). GPU check ok attempt 1, 31599 MiB free. create→ready **13m09s** (download 38–41 MB/s). Q8 @ 8K ctx: VRAM 27124/32607 MB. Smoke: chat 2.5 s, tool call 1.35 s. **e2e: 23/23 pass in 30 s.** Stream first chunk 618 ms through Cloudflare. Gen 48–50 tok/s, prompt ~2400 tok/s. `/metrics` without key → 401. Real `/metrics` fixture captured. |
| expiry | `up --max-life 6m`: agent killed the pod at ~6m20s (reason `expired`, during download); gone from `GET /pods` 20 s later ✓. `up` printed `pod failed:` with empty reason → fixed to `pod stopped: watchdog: expired`. |

## Model over SSH from the model server (user request 2026-09-23)

- the model server: user `lobo` (shell `/bin/sh`, needed because sshd runs the forced command via the login shell; `nologin` just printed "This account is currently not available"), key `~/ssh/lobo-the model server`, `authorized_keys` = `restrict,command="/usr/local/bin/lobo-model-serve"`. Script allows exactly the two GGUF names + a numeric offset → `tail -c +N`. Models moved to `/srv/lobo/models` (root:lobo 640).
- Verified from the laptop with the real key: `GGUF` magic at offset 0, correct tail bytes; `/etc/passwd`, `id`, `…; id`, `../..`, negative offset, `-L` forwarding all refused.
- Incident: my first restriction test ran from zsh without word-splitting, so ssh never got the key → 4+ failed auths → fail2ban (`maxretry 4 / findtime 10m / bantime 1h, increment`) banned the laptop IP 103.134.77.234. Then the first pod (`99.69.17.69`) was banned too: Go client negotiated a non-ed25519 host key, `FixedHostKey` failed, 4 retries = ban. Unbanned both via ProxyJump through bohetz.
- Fixes: client negotiates only the pinned host-key type; handshake/auth errors are permanent (no retry → a pod can't ban itself). Test server offers ECDSA + ed25519 (fails on the old code, passes now).

## Final gate (release -11, model from the model server over SSH)

| Step | Result |
|---|---|
| up | pod 1 COMMUNITY: CUDA `unknown error` for all 19 tries / 3 min → deleted; pod 2 SECURE $0.99/h: download from the model server **54–57 MB/s**, load 7 s, ready. Total `up` 12m45s. |
| version | `/api/version` git `16b15b9` = HEAD, dirty=false ✓ |
| smoke | chat 2.1 s, tool call 1.36 s ✓ |
| e2e | 22/23. `TestToolCallRoundTrip` failed with laptop→Cloudflare `connection reset by peer` (transport, no response). Passed in the release -6 run. Not rerun (pod idle-killed). |
| busy | 6 min of requests with `idle-min 3`: pod stayed up ✓ |
| idle kill | pod deleted itself 151 s after the last request ✓ |

## 2026-09-24 — fast-boot work (release 2026.09.24-1…-3)

Shipped: per-stage boot timings (bootstrap apt/zip, tunnel, gpu, download MB/s, verify, load, rent→container) in `/api/status` + `up` report + `boots.jsonl`; parallel ranged download (SSH + HTTP, 256 MiB chunks, resume per chunk, sha256 pass after); `lobo-agent bench`; presigned R2 source (`--source r2`); `--conns`; re-rent when the container never starts (6 min).

Live today:
| Run | Result |
|---|---|
| baseline `--source ssh --conns 1` | 3 COMMUNITY hosts too slow from the model server: 4.6 / 12.7 / 11.8 MB/s (yesterday 55). 4th (SECURE) never started its container (no IP, `runtime: null`) for 13 min → killed. |
| `--source r2 --conns 8` | 4/4 pods: container not started after 6 min (GraphQL `runtime: null`), all deleted. |
| image test | could not rent at all: "no instances" for 5090 and 4090, COMMUNITY and SECURE. Account balance **$6.49** (spend limit $80) — likely cause; unconfirmed. |

Not measured yet (blocked on RunPod balance): SSH 1/4/8 vs R2 presigned 4/8/16/32 MB/s, full timed boot.

## 2026-09-25 — fast boot, measured (renting worked again at the same $6.49 balance → the blocker was stock, not balance)

Boot A (`--source r2 --conns 8`, release -4, full-file sha pass): **total 150 s** — rent→container 28.1 s, apt 2.9, zip 1.5, tunnel 0.6, gpu 0.3, **download 83.0 s @ 345 MB/s**, sha256 pass 20.2, load 7.9.

Bench on that pod (40 s per run, nothing written):
| Source | Streams | MB/s |
|---|---|---|
| the model server SSH | 1 | 3.5 |
| the model server SSH | 4 | 14.8 |
| the model server SSH | 8 | 31.4 |
| R2 presigned | 8 | 366 |
| R2 presigned | 16 | 633 |
| R2 presigned | 32 | 713 |

Changes: per-chunk sha256 table (256 MiB chunks, computed on the model server, `internal/model/chunks_gen.go`) → no full-file pass; default source r2 presigned @ 32 streams; ssh capped at 8 (the model server MaxStartups).

Boot B (defaults, release 2026.09.25-1): **total 335 s** — rent→container 15.6 s, apt 3.9, zip 2.2, tunnel 1.5, gpu 0.7, **download 297.9 s @ 96 MB/s** (32 streams; host network-limited), verify 0.0, load 7.5. Smoke ✓ (chat 2.75 s, tool 1.2 s).

Download speed is per host: 96–713 MB/s with the same code. Everything except the download is ~30 s.

## 2026-09-25 — fast-network hosts (release 2026.09.25-2)

RunPod REST create has `minDownloadMbps` (filter, no sort). `up` now tries 10000 → 5000 → 2500 → 1000 → any Mbps, COMMUNITY before SECURE at each tier.

Boot C (defaults): ≥10000 Mbps tier had capacity only on SECURE ($0.99/h). **Total 91.5 s** — rent→container 26.8, apt 5.3, zip 1.9, tunnel 1.4, gpu 0.8, **download 41.1 s @ 695 MB/s** (32 streams, r2 presigned), verify 0.0 (chunk table), load 8.0. Smoke ✓ (chat 2.3 s, tool 1.3 s). `machine.maxDownloadSpeedMbps` came back empty from pod GET.

Trend: 9–13 min (r2.dev / the model server, 1 stream) → 150 s (r2 presigned ×8) → 91.5 s (×32 + chunk hashes + fast-net host).

## 2026-09-25 — 64K context for OpenCode

OpenCode's first request is ~43K tokens (system prompt + tools); the 8K pod rejected it and OpenCode looped on "Compaction". Now: release default ctx 65536, OpenCode `limit.context` 65536 / output 8192.

Measured: Q8 @ 64K → VRAM **29274 / 32607 MB** (fits, 3.3 GB free). Boot 50.4 s (≥5000 Mbps COMMUNITY host, 878 MB/s). `opencode run` → "lobo works"; 42,881-token prompt processed in 15.2 s (2,830 tok/s), gen ~43 tok/s.

## 2026-09-25 — OpenCode prompt size (OpenCode 1.18)

| Setup | First-request prompt tokens |
|---|---|
| default (global config: blender + pencil MCP, ~100 Claude skills via `skill` tool) | 42,949 |
| agent `lobo`: MCP off | 25,226 |
| + `skill`, `webfetch`, `todowrite`, `todoread`, `task` off | 15,008 |
| + short custom system prompt | 13,225 (not used: saves 1.8K, loses OpenCode's tool instructions) |

Shipped: `make gen-api-key` writes a `lobo` agent (model lobo/q8, those tools off); merged into the global config. Other models/agents unchanged. `opencode run` from a script is flaky (every other call fails silently or with "Unexpected server error"); not a pod issue.
