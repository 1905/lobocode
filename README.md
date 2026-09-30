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
make install-mac    # optional: the menu bar app → /Applications/lobocode.app
```

## What you need

**On this Mac** (Apple Silicon, 32 GB+): nothing else. See [Run on this Mac](#run-on-this-mac).

**In the cloud:**
- A **RunPod** or **Vast.ai** API key. One is enough.
- A **Cloudflare named tunnel** token and a hostname routed to it. That hostname is your endpoint.
- A public **bucket URL** that holds the model GGUF.

`lobo config` asks for what your choice needs and writes `~/.config/lobo/config.env`.

## Use

| Command | What it does |
|---|---|
| `lobo up` | Rent a 5090, show boot progress, print the endpoint when ready. |
| `lobo status` | Live dashboard: cost, GPU, tok/s, idle-kill timer. |
| `lobo test` | Streamed chat + tool call against the live API. |
| `lobo logs` | Last agent, llama-server and tunnel log lines. |
| `lobo down` | Delete every `lobo` pod on every configured provider. Nothing else. |

Useful flags for `lobo up`: `--provider vast`, `--q6`, `--ctx 16384`, `--idle-min 10`, `--max-life 4h`.

## Run on this Mac

Apple Silicon only. Same llama.cpp build and flags as the pod, Metal instead of CUDA, no rent.

- Set `LOBO_WEIGHTS_DIR` to a folder with room for the GGUF (Q6 22 GB, Q8 29 GB). `lobo models` shows what is there.
- `lobo up --provider local` downloads llama.cpp and the model from Hugging Face (resumable, sha256-checked), then serves `http://127.0.0.1:8931/v1` with your `LOBO_API_KEY`.
- It stops after `LOBO_IDLE_MIN` (30) minutes without requests. `lobo status`, `test`, `logs` and `down` work as in the cloud.
- A local-only config needs just `LOBO_API_KEY`. `LOBO_PROVIDER=local` makes it the default.

## Menu bar app (macOS)

<img src="docs/img/menubar_ready.png" height="28" alt="menu bar: green, 45 t/s">

Start, watch the boot, copy the endpoint and key, see tok/s and spend, stop. The app uses the same Rust core and config file as the CLI. It runs local models directly. Opening the app shows a native window, so it works when a full menu bar hides the item behind the notch.

Windows fit their content without scrolling. Settings groups controls into Local, Cloud and Defaults tabs.

Install: download `lobocode.dmg` from the release, open it, drag **lobocode** to **Applications**. Or build it with `make install-mac` (`make dmg` builds the image).

- The app is not notarized. The first open says it can't be opened: go to System Settings → Privacy & Security → **Open Anyway**. Or run `xattr -dr com.apple.quarantine /Applications/lobocode.app`.
- With the weights on an external drive, macOS asks once to allow access to a removable volume. Allow it, or the app can't see your models.

<details><summary>Settings window</summary>
<img src="docs/img/settings.png" width="420" alt="settings: provider keys, access, defaults for lobo up">
</details>

## OpenCode

`lobo gen-api-key` writes `opencode.lobo.json` with the `lobo` provider (your domain), the `lobo-local` provider (this Mac) and the agent. Merge its `provider.lobo` and `agent.lobo` blocks into `~/.config/opencode/opencode.json`, then pick the `lobo` agent (Tab). The agent turns off MCP tools and skills for this model: the first request drops from 43K to 15K tokens.

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
| `LOBO_PROVIDER` (runpod, vast, local) | `--provider` | runpod |
| `LOBO_MODEL` (q8, q6) | `--q6` | q8 |
| `LOBO_CTX` | `--ctx` | 65536 |
| `LOBO_IDLE_MIN` | `--idle-min` | 30 |
| `LOBO_MAX_HOURS` | `--max-life` | 12 |
| `LOBO_CLOUD` (community, secure) | `--cloud` | community |
| `LOBO_MIN_MBPS` | `--min-mbps` | 100 |
| `LOBO_POD_IMAGE` | `--image` | none |
| `LOBO_VAST_MAX_DPH` | | 1.20 |
| `LOBO_WEIGHTS_DIR` | | `~/Library/Application Support/lobo/weights` |
| `LOBO_LOCAL_PORT` (API on port + 1) | | 8931 |

## Safety

- **Idle kill:** 30 min without requests. **Hard expiry:** fixed at create time, 12 h by default.
- The pod deletes itself with a pod-scoped key that the provider injects. Your account keys stay on the laptop.
- Bad hosts are replaced automatically: container never started, broken CUDA, VRAM taken, slow download. At most 4 tries.

## Development

`make test` · `make lint` · `make e2e` (live API suite, needs `lobo up`) · `make release` (agent zip to the bucket). Layout: `cmd/lobo` CLI, `cmd/lobo-agent` pod agent, `internal/*`, `macos/` app, `plans/` specs and measured results.
