# lobocode

On-demand Qwen3.5-27B Uncensored (HauhauCS Aggressive, GGUF) on a rented RTX 5090 (RunPod or Vast.ai), served as an OpenAI-compatible API at `https://lobo.example.com/v1`.

The laptop is control only. No LLM traffic goes through it. After `lobo up` the pod runs on its own:

- It downloads the release zip and the model from the R2 bucket `lobo`.
- It starts `cloudflared` (named tunnel `lobo`) and `llama-server` (official `ghcr.io/ggml-org/llama.cpp:server-cuda-*` image).
- It deletes itself after 30 min without requests, or at its fixed expiry (12 h by default).

## Install

```sh
make install     # builds lobo with its version and installs it to ~/.local/bin (PREFIX=… to change)
lobo config      # form for the API keys and the defaults for `lobo up`
lobo up
```

`lobo` alone prints the help.

Homebrew (prepared, not released yet): `brew install 1905/tap/lobo`. See [Homebrew](#homebrew).

## macOS app: lobocode

```sh
make install-mac   # builds bin/lobocode.app (bundles the lobo CLI from this commit) → ~/Applications/lobocode.app
```

A menu bar app over the same CLI and the same config file. No Dock icon. The menu bar item is a small square plus a status: grey `off`, cyan and filling up while it boots (`rent`, `42%`, `load`), green when ready (`27m` until idle-kill, or `45 t/s` while it generates), red `FAIL`. Click it for the panel: start (provider and model pick), live boot log with download speed, endpoint and API key copy, tok/s, VRAM, idle-kill countdown, spend, stop. The Settings window (`⌘,`) edits `~/.config/lobo/config.env` through `lobo config set`. Notifications on ready, boot failure, and when a pod stops by itself.

The app never talks to RunPod, Vast or the config file directly: it runs the bundled `lobo` (`up --json`, `status --json`, `down --json`, `config show --json|set|get`). `Lobocode --render DIR` writes PNGs of every panel state. Unsigned (ad-hoc): for this Mac, not for distribution.

## Config

One file: `~/.config/lobo/config.env` (`$XDG_CONFIG_HOME/lobo/config.env` if set; `lobo config path` prints it). Plain `KEY=value` lines, the same keys as `.env.example`, mode 600. Edit it by hand or with `lobo config`; the form keeps every line it does not show. `lobo config show` prints it with the keys masked. `--config FILE` uses another file.

Keys come only from that file. The OS environment is never read. `make install` copies the repo `.env` there once if no config exists yet.

Defaults for `lobo up` live in the same file. A flag always wins over the file, and the file wins over the built-in default:

| Key | Flag | Built-in |
|---|---|---|
| `LOBO_PROVIDER` (used when both keys are set) | `--provider` | runpod |
| `LOBO_MIN_MBPS` | `--min-mbps` | 100 |
| `LOBO_MODEL` (q8, q6) | `--q6` | q8 |
| `LOBO_CTX` | `--ctx` | release default (65536) |
| `LOBO_IDLE_MIN` | `--idle-min` | 30 |
| `LOBO_MAX_HOURS` | `--max-life` | 12 |
| `LOBO_CLOUD` (secure, community) | `--cloud` | secure |
| `LOBO_VAST_MAX_DPH` | | 1.20 |

One provider key is enough. With only `VASTAI_API_KEY`, `lobo up` uses Vast.

## Use

| Command | What it does |
|---|---|
| `lobo up` | Rent a 5090 and show boot progress until the API is ready. `lobo up --q6 --ctx 16384 --idle-min 10 --max-life 4h --release 2026.09.23-1 --plain` |
| `lobo status` | Live dashboard: pod, cost, release, stage, GPU load/VRAM, host load/RAM, tokens in/out, tok/s, auto-kill timer. `lobo status --once` prints one snapshot. |
| `lobo test` | Streamed chat + tool call against the live API. Fails if `tool_calls[].function.arguments` is not a JSON string. |
| `lobo logs` | Last agent, llama-server and cloudflared log lines (`/api/logs`, needs the key). |
| `lobo down` | Delete every pod named `lobo` on every configured provider. Other pods on the accounts are never touched. |

Dev targets in the repo (they use the repo `.env`):

| Command | What it does |
|---|---|
| `make release` | Build `lobo-agent`, zip it with `release.json`, refuse if any config secret is inside, upload to bucket `lobo`. |
| `make e2e` | End-to-end suite against the live API (needs `lobo up`): pod API, auth, chat, stop/max_tokens, JSON mode, streamed and non-streamed tool calls, tool-result round trip, compiling Go codegen, long-prompt needle, over-context error, concurrent queueing, metrics counters. |
| `make test` / `make lint` | Unit tests / golangci-lint. |

The 28.6 GB model download is most of the boot time. `up` replaces pods on bad hosts by itself (container not started in 6 min, broken CUDA, VRAM already taken, download slower than `LOBO_MIN_MBPS`), 4 pods at most. Debug a boot with `lobo up --ssh ~/ssh/runpod2.pub` (opens SSH on the pod).

**Vast.ai:** `lobo up --provider vast` rents the fastest-network 1× RTX 5090 among verified Vast offers (reliability ≥ 0.98, CUDA ≥ 12.8, ≤ `LOBO_VAST_MAX_DPH`, default $1.20/h). Needs `VASTAI_API_KEY`. The instance destroys itself with Vast's instance-scoped `CONTAINER_API_KEY`; the account key never goes to the host. `lobo down` and `lobo status` cover both providers.

Pod API through the domain:

- `GET /api/version`: the running `release.json`. Public.
- `GET /api/status`: stage, download, GPU, host, llama metrics, watchdog. Public.
- `GET /api/logs?n=200`: needs `Authorization: Bearer $LOBO_API_KEY`.
- Everything else goes to llama-server and needs the same bearer key.

## OpenCode

`lobo gen-api-key` creates `LOBO_API_KEY` in the config once (`lobo config` can too). The key stays the same across `lobo up`; `lobo gen-api-key --rotate` replaces it. It also writes `opencode.lobo.json` (gitignored, mode 600) with the `lobo` provider and the key inline.

To see the provider in every OpenCode session, merge its `provider.lobo` and `agent.lobo` blocks into the global config `~/.config/opencode/opencode.json`. Other models and agents are not changed.

Use the `lobo` agent (Tab in OpenCode, or `opencode run --agent lobo`). It turns off MCP tools, skills, webfetch, todo and task for this model only. Measured with OpenCode 1.18: the first request drops from 42,949 to 15,008 tokens (about 15 s → 5 s of prompt processing).

## Cost and safety

- RTX 5090 on RunPod: $0.69/h community, $0.99/h secure (checked 2026-09-23). `lobo up` tries secure first (`--cloud community` or `LOBO_CLOUD=community` to flip). Vast: whatever the chosen offer costs, ≤ `LOBO_VAST_MAX_DPH`.
- Idle kill: 30 min without a running or queued request and without token counters moving.
- Hard expiry: `LOBO_EXPIRES_AT` is fixed at create time and survives agent restarts. RunPod restarts the container when PID 1 exits.
- Self-delete uses the pod-scoped `RUNPOD_API_KEY` that RunPod injects (GraphQL `podTerminate`), or Vast's instance-scoped `CONTAINER_API_KEY`. The account keys never go to a pod.
- Bucket `lobo` is public (r2.dev dev URL). It holds the agent binary, release metadata and the two GGUF files. No secrets: `make release` scans every zip.
- Key rotation: `lobo gen-api-key --rotate`, then `lobo up` again. The key lives only in pod env.

## Homebrew

Prepared, not released. `.goreleaser.yaml` builds `lobo` for macOS and Linux (amd64, arm64) and writes the formula `lobo.rb` into `1905/homebrew-tap`; `.github/workflows/release.yml` runs it on a `v*` tag. `goreleaser release --snapshot --clean` builds everything locally without publishing.

Before the first `git tag v0.1.0 && git push --tags`:

- Make `1905/lobocode` public, or publish the archives from a public repo. Homebrew cannot download release assets of a private repo.
- Add the `HOMEBREW_TAP_TOKEN` secret (a token that can push to `1905/homebrew-tap`) to this repo.
- Make the domain configurable for other users (today every config points at `lobo.example.com` and one tunnel).

Agent releases (`make release`, versions like `2026.09.25-10`) go to the bucket, not to git tags, so they never trigger this.

## Layout

`cmd/lobo` laptop CLI · `cmd/lobo-agent` pod agent · `internal/*` packages · `plans/` spec, plans, measured results.
