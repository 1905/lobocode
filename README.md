# lobocode

Your own uncensored coding model, on demand. `lobo up` rents one RTX 5090, serves **Qwen3.5-27B Uncensored** (HauhauCS Aggressive, Q8 GGUF) as an OpenAI-compatible API, and deletes the GPU when you stop using it.

<p align="center">
  <img src="docs/img/panel_boot.png" width="340" alt="booting: rent, container, tunnel, gpu, model download at 713 MB/s">
  <img src="docs/img/panel_ready.png" width="340" alt="ready: endpoint, api key, 45 tok/s, VRAM, idle-kill timer, stop">
</p>

## TL;DR

- **What:** one command rents a 5090 on RunPod or Vast.ai and gives you `https://<your-domain>/v1`. Point OpenCode or any OpenAI client at it.
- **Speed:** about 45 tok/s generation, 500+ tok/s prompt, 64K context.
- **Cost:** $0.69–0.99/h while it runs. It deletes itself after 30 min idle, and after 12 h in any case.
- **Safe to forget:** the pod kills itself. Your account keys never leave your laptop.

```sh
brew install 1905/tap/lobo
lobo config     # paste keys once
lobo up         # ~5 min later: ready
lobo down       # or just walk away
```

## Install

**Homebrew** (macOS, Linux):

```sh
brew install 1905/tap/lobo
```

**From source** (Go 1.26+):

```sh
git clone https://github.com/1905/lobocode && cd lobocode
make install        # → ~/.local/bin/lobo
make install-mac    # optional: the menu bar app → ~/Applications/lobocode.app
```

## What you need

- A **RunPod** or **Vast.ai** API key. One is enough.
- A **Cloudflare named tunnel** token and a hostname routed to it. That hostname is your endpoint.
- A public **bucket URL** that holds the model GGUF.

`lobo config` asks for all of it and writes `~/.config/lobo/config.env`.

## Use

| Command | What it does |
|---|---|
| `lobo up` | Rent a 5090, show boot progress, print the endpoint when ready. |
| `lobo status` | Live dashboard: cost, GPU, tok/s, idle-kill timer. |
| `lobo test` | Streamed chat + tool call against the live API. |
| `lobo logs` | Last agent, llama-server and tunnel log lines. |
| `lobo down` | Delete every `lobo` pod on every configured provider. Nothing else. |

Useful flags for `lobo up`: `--provider vast`, `--q6`, `--ctx 16384`, `--idle-min 10`, `--max-life 4h`.

## Menu bar app (macOS)

<img src="docs/img/menubar_ready.png" height="28" alt="menu bar: green, 45 t/s">

Start, watch the boot, copy the endpoint and key, see tok/s and spend, stop. It runs the same `lobo` CLI and uses the same config file. Build it with `make install-mac`. It's unsigned, so it's for your own Mac.

<details><summary>Settings window</summary>
<img src="docs/img/settings.png" width="420" alt="settings: provider keys, access, defaults for lobo up">
</details>

## OpenCode

`lobo gen-api-key` writes `opencode.lobo.json` with the `lobo` provider and agent. Merge its `provider.lobo` and `agent.lobo` blocks into `~/.config/opencode/opencode.json`, then pick the `lobo` agent (Tab). The agent turns off MCP tools and skills for this model: the first request drops from 43K to 15K tokens.

## Pod image

`ghcr.io/1905/lobocode` = the official llama.cpp CUDA server + `lobo-agent` baked in. Each `v*` tag publishes a matching image. Use it with:

```sh
lobo up --image ghcr.io/1905/lobocode@sha256:<digest>
# or once, in the config:  LOBO_POD_IMAGE=ghcr.io/1905/lobocode@sha256:<digest>
```

Without it, the pod starts from the plain llama.cpp image and downloads the agent at boot.

## Config reference

`~/.config/lobo/config.env`, plain `KEY=value`, mode 600. Only this file is read. The shell environment is never used. A flag beats the file, and the file beats the built-in default.

| Key | Flag | Default |
|---|---|---|
| `LOBO_PROVIDER` | `--provider` | runpod |
| `LOBO_MODEL` (q8, q6) | `--q6` | q8 |
| `LOBO_CTX` | `--ctx` | 65536 |
| `LOBO_IDLE_MIN` | `--idle-min` | 30 |
| `LOBO_MAX_HOURS` | `--max-life` | 12 |
| `LOBO_CLOUD` (community, secure) | `--cloud` | community |
| `LOBO_MIN_MBPS` | `--min-mbps` | 100 |
| `LOBO_POD_IMAGE` | `--image` | none |
| `LOBO_VAST_MAX_DPH` | | 1.20 |

## Safety

- **Idle kill:** 30 min without requests. **Hard expiry:** fixed at create time, 12 h by default.
- The pod deletes itself with a pod-scoped key that the provider injects. Your account keys stay on the laptop.
- Bad hosts are replaced automatically: container never started, broken CUDA, VRAM taken, slow download. At most 4 tries.

## Development

`make test` · `make lint` · `make e2e` (live API suite, needs `lobo up`) · `make release` (agent zip to the bucket). Layout: `cmd/lobo` CLI, `cmd/lobo-agent` pod agent, `internal/*`, `macos/` app, `plans/` specs and measured results.
