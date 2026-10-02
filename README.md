# lobocode

Run **Qwen3.5-27B Uncensored** (HauhauCS Aggressive) on a rented GPU from the standalone Mac app. Get an OpenAI-compatible API through a private SSH connection. The optional CLI also supports local Apple Silicon inference.

**Mac app 0.2.1:** cloud-only, standalone, Apple Silicon. [Download the DMG](https://github.com/1905/lobocode/releases/download/app-v0.2.1/lobocode.dmg). Homebrew installs the separately published CLI; the Rust CLI rewrite remains unreleased. Screenshots with performance numbers show sample data, not benchmark results.

The Rust cloud path resolves the latest complete public image on each new start. **Select Q6 for cloud use:** its image is public and passed RunPod TUI and native Mac Start → reply → Stop checks. New app cloud selections default to Q6. Q8 remains available as a selection, but its complete image is not published; choosing it fails before rental. Broader telemetry acceptance remains pending. See the [implementation record](docs/implementation-mistakes.md).

<p align="center">
  <img src="docs/img/panel_boot.png" width="340" alt="booting: rent, image pull, private connection, GPU check and bundled-model verification">
  <img src="docs/img/panel_ready.png" width="340" alt="ready: endpoint, api key, 45 tok/s, VRAM, idle-kill timer, stop">
</p>

## TL;DR

- **What:** start a 5090 on RunPod or Vast.ai from the standalone Mac app or optional CLI. The app connects through private SSH at `http://127.0.0.1:8933/v1`. No domain or bucket is required.
- **Measured Q6 cloud check:** 47,000 uncached input tokens at 2,186.7 prompt tok/s; first content after 24.04 seconds. Sustained output speed remains unmeasured.
- **Startup varies:** the TUI reached Ready in 230.178 seconds. Mac Ready occurred between about 21m11s and 22m30s after the Start capture. The provider still reported downloading at 15 minutes. No transfer-byte progress or cache-hit evidence was captured.
- **Observed test rates:** $0.69/h and $0.99/h. Available prices vary. Default shutdown limits are 30 minutes idle and 12 hours maximum life.
- **Safe to forget:** the pod kills itself. Your account keys never leave your laptop.

For the Rust cloud candidate, build from source using the instructions below, then run:

```sh
bin/lobo-rs config       # configure provider and API keys
bin/lobo-rs up --q6      # use the published complete Q6 image
bin/lobo-rs status       # inspect the running GPU
bin/lobo-rs down         # deletes all lobo instances on configured providers
```

The app's Stop button targets its recorded runtime. The CLI's `down` command retains its broader all-provider behavior.

## Install

The desktop app installs from its DMG and does not require the CLI. Install the optional CLI separately through Homebrew. [Download Mac app 0.2.1](https://github.com/1905/lobocode/releases/download/app-v0.2.1/lobocode.dmg), open the DMG, and drag **lobocode** to **Applications**. Requires Apple Silicon and macOS 13 or later. The app is ad-hoc signed and is not notarized.

**Published CLI through Homebrew** (macOS, Linux):

```sh
brew install 1905/tap/lobo
```

**Rust development build** (Rust from `rust-toolchain.toml`; Node.js and pnpm for the app):

```sh
git clone https://github.com/1905/lobocode && cd lobocode
make rust-build-lobo # → bin/lobo-rs; does not replace the installed CLI
make install-mac    # optional: the menu bar app → /Applications/lobocode.app
```

The source retains the legacy Go CLI and `make install` target until the Rust release. Use `bin/lobo-rs` to test the Rust CLI.

Open **Settings → Cloud**, enter a RunPod or Vast API key, save, and start Q6. An unavailable image fails before rental. Updating the app does not install or replace the optional CLI.

## What you need

**Mac app:** Apple Silicon and macOS 13 or later. Model weights and inference run on the cloud GPU. The app has no local model mode.

**In the cloud:**
- A **RunPod** or **Vast.ai** API key. One is enough.
- **OpenSSH** on the client computer. macOS includes it; Linux needs the `openssh-client` package.

`lobo config` asks for what your choice needs and writes `~/.config/lobo/config.env`.

New Rust configurations use a [private SSH connection](docs/cloud-without-domain.md). Lobocode creates its own keys, checks the server identity, and forwards requests automatically. No domain, Cloudflare tunnel account, or connection subscription is required. GPU and provider bandwidth charges still apply.

The connection survives closing the app or finishing `lobo up`. It reconnects after a network interruption. The endpoint works only on that computer. Stop deletes the instance and closes the connection.

Existing complete domain/token configurations keep their public connection. Select **use private connection** in Cloud Settings, or set `LOBO_CONNECTION=ssh`, before the next start. The SSH mode needs the matching cloud agent. The public Q6 image includes it and passed direct TUI and native Mac cloud lifecycle checks. The Rust CLI release remains on hold.

The GPU pulls a complete public Docker image. It includes the agent, inference runtime, SSH server and selected model weights. There is no separate agent install, private bucket or developer credential to configure.

Every new cloud start looks up the current `latest-q8` or `latest-q6` tag and sends its exact digest to the provider. The app version does not select the image version. If the registry lookup fails, startup fails before renting; it does not reuse an older image.

## Use

| Command | What it does |
|---|---|
| `lobo up` | Rent a 5090, show boot progress, print the endpoint when ready. |
| `lobo status` | Live dashboard: cost, GPU, tok/s, idle-kill timer. |
| `lobo test` | Streamed chat + tool call against the live API. |
| `lobo logs` | Last agent, llama-server and tunnel log lines. |
| `lobo down` | Delete every `lobo` pod on every configured provider. Nothing else. |

Useful flags for `lobo up`: `--provider vast`, `--q6`, `--ctx 16384`, `--idle-min 10`, `--max-life 4h`.

The terminal dashboard updates in place. Press `q` to leave status; press `q` or Ctrl-C during startup to cancel and wait for cleanup. Short terminals use a compact layout that retains metrics, shutdown timers and the quit hint. The real RunPod check confirmed Ready and status fit 80 columns. Some startup detail lines still clip at that width.

## Local inference through the optional CLI

The Mac app cannot run a local model. The separately installed CLI retains local execution. Existing model files, CLI settings and local processes are preserved when the app opens.

For the Rust CLI development build, use `bin/lobo-rs up --provider local`. Local inference requires Apple Silicon and enough available memory. The core checks memory at Start and immediately before model load. Installed RAM or downloaded weights alone do not establish that a model fits.

- Set `LOBO_WEIGHTS_DIR` to a folder with room for the GGUF (Q6 22 GB, Q8 29 GB).
- Local startup downloads llama.cpp and model weights with resume and SHA256 verification.
- The local endpoint defaults to `http://127.0.0.1:8931/v1` and uses `LOBO_API_KEY`.
- `LOBO_PROVIDER=local` remains a valid CLI default. The app ignores it when choosing a cloud provider and preserves it on disk.

## Menu bar app (macOS)

<img src="docs/img/menubar_ready.png" height="28" alt="menu bar: green, 45 t/s">

Start, watch the boot, copy the endpoint and key, see tok/s and spend, stop. The app uses the same Rust core and config file as the CLI. It manages only cloud GPUs; it does not download or load local model weights. Opening the app shows a native window, so it works when a full menu bar hides the item behind the notch.

Windows use native macOS title bars and rounded corners. Each view fits without scrolling. Settings groups controls into Cloud, Defaults and Clients tabs. The actual hosted app passed cloud Start, Ready, a direct reply, Stop and Off without scrolling; rounded corners were visible. Drag, Dock and Command-Tab behavior remain unverified.

The Mac cloud request used 25 uncached input tokens and returned the expected `4` in two output tokens. First content took 1.107 seconds. Ready displayed 22.8 / 31.8 GB of GPU memory. The idle gauges showed `0 / 0` after the reply; live rate updates were not verified. Owned pod/tunnel cleanup, native Quit and preference restoration passed. Loading-memory progression remains unfinished. Local Metal inference and live OpenCode integration remain unverified. See the [test results and remaining checks](plans/2026-09-30-app-setup-memory-visibility/results.md).

The [standalone DMG](https://github.com/1905/lobocode/releases/download/app-v0.2.1/lobocode.dmg) contains only the app. For development, `make install-mac` builds the app and `make dmg` creates `bin/lobocode.dmg`.

- The app is not notarized. If macOS blocks the first open, go to System Settings → Privacy & Security → **Open Anyway**. Or run `xattr -dr com.apple.quarantine /Applications/lobocode.app`.

<details><summary>Settings: Cloud, Defaults and Clients</summary>
<p><img src="docs/img/settings_cloud.png" width="520" alt="Cloud settings: provider keys, private SSH, API key and cloud port"></p>
<p><img src="docs/img/settings_defaults.png" width="520" alt="Defaults: model, context size, shutdown limits and provider options"></p>
</details>

## OpenCode

In the Mac app, wait for **Ready**, then select **OpenCode…** or open **Settings → Clients**. Confirm the displayed config file. Use **choose existing config…** for a different JSON or JSONC file. The app does not require the CLI or an OpenCode subprocess.

Leave **Use Lobocode by default** checked to set the default model and agent. Clear it to preserve existing defaults. Select **Configure/Repair**, then restart OpenCode. Project settings can override the selected file.

Setup authenticates the active endpoint and uses the running model and context limit. It preserves comments, unrelated providers and custom agent settings. Changed files receive a private backup; the API key stays in a private referenced file. An unchanged repair creates no extra files. Authentication or validation failure prevents replacement.

The app configures the `lobo` provider for its active cloud runtime. Existing provider restrictions remain in place and appear as warnings. A new `lobo` agent enables core coding tools and disables MCP tool and skill access. Existing custom agent controls remain unchanged.

The optional CLI retains its separate export command: `lobo gen-api-key` writes `opencode.lobo.json` for manual merging. It is not required for app setup. The screenshots above predate the Clients tab; native setup acceptance remains pending.

## Pod image

The Rust candidate uses separate complete images for Q6 and Q8. Each image includes the agent, inference runtime, SSH server and model weights. The Q6 tag, `ghcr.io/1905/lobocode:latest-q6`, is public and passed anonymous manifest verification on October 1, 2026. Q8 publication remains pending. Direct RunPod TUI inference and the native Mac cloud lifecycle passed. Broader telemetry acceptance and the Rust CLI release remain pending.

The app resolves the selected tag again before each new start. The provider receives `ghcr.io/1905/lobocode@sha256:<digest>`. It may reuse identical layers, but cannot substitute an older image digest.

A controlled Q6 check used a 65,536-token context and exactly 47,000 uncached synthetic input tokens. It returned the expected `4` in two output tokens. First content arrived after 24.040 seconds; the server reported 2,186.677 prompt tok/s. Its reported 45.271 output tok/s covers only two tokens and is not a sustained benchmark. Exact test-pod and tunnel cleanup passed. This does not validate local Metal inference or OpenCode.

Model weights use native GGUF shards in separate image layers. Startup verifies each shard before loading. Missing or corrupt files fail startup and trigger instance cleanup. Boot never downloads replacement weights, an agent archive or OS packages.

For CLI development only, an explicit complete-image override is available:

```sh
lobo up --image ghcr.io/1905/lobocode@sha256:<digest>
```

Normal starts ignore old `LOBO_POD_IMAGE`, bucket and model-source settings. The obsolete `--release`, `--source`, `--conns` and debug `--ssh` options return an error before rental.

Image pulls can transfer roughly 22–29 GB of weights plus the runtime. The default startup limit is 40 minutes. A host that cannot start the container within 30 minutes is removed. Cancellation also removes the owned instance.

A new host may need to download the full image. The complete image removes separate boot-time software and model downloads. It does not guarantee that a RunPod host already has the image cached.

## Config reference

The table describes shared CLI configuration. App settings exclude local weights and local ports. The app permits only RunPod or Vast and defaults to Q6 when no cloud model is selected.

`~/.config/lobo/config.env`, plain `KEY=value`, mode 600. Only this file is read. The shell environment is never used. A flag beats the file, and the file beats the built-in default.

| Key | Flag | Default |
|---|---|---|
| `LOBO_PROVIDER` (runpod, vast, local) | `--provider` | runpod |
| `LOBO_CONNECTION` (ssh, cloudflare) | | ssh for new configurations |
| `LOBO_CLOUD_PORT` (control API on port + 1) | | 8933 |
| `LOBO_MODEL` (q8, q6) | `--q6` | q8 |
| `LOBO_CTX` | `--ctx` | 65536 |
| `LOBO_IDLE_MIN` | `--idle-min` | 30 |
| `LOBO_MAX_HOURS` | `--max-life` | 12 |
| `LOBO_CLOUD` (community, secure) | `--cloud` | community |
| `LOBO_MIN_MBPS` (minimum advertised Vast host speed, MB/s) | `--min-mbps` | 100 |
| `LOBO_VAST_MAX_DPH` | | 1.20 |
| `LOBO_WEIGHTS_DIR` | | `~/Library/Application Support/lobo/weights` |
| `LOBO_LOCAL_PORT` (API on port + 1) | | 8931 |

Cloud and local port pairs must not overlap. SSH mode selects RunPod hosts with public TCP mappings and Vast hosts with direct ports. This can reduce host availability. `LOBO_DOMAIN` and `CF_TUNNEL_TOKEN` are only needed for the legacy `cloudflare` connection.

## Safety

- **Idle kill:** 30 min without requests. **Hard expiry:** fixed at create time, 12 h by default.
- The pod deletes itself with a pod-scoped key that the provider injects. Your account keys stay on the laptop.
- Bad hosts can be replaced automatically: container never started, broken CUDA or unavailable VRAM. At most 4 tries, within the startup limit.

## Development

| Area | Source | Checks |
|---|---|---|
| Shared protocol | `crates/lobo-proto` | `make proto-ts` |
| Pod agent and shared core | `crates/lobo-agent`, `crates/lobo-core` | `make rust-test rust-lint` on CI or a test host |
| Rust CLI and TUI | `crates/lobo-cli` | `cargo test -p lobo-cli --test tui_parity` |
| Native app | `app/src-tauri`, `app/ui` | `make app-lint`, `pnpm -C app/ui test` |
| Native UI smoke | `app/e2e` | `make app-e2e` (cloud setup, legacy migration and removed IPC) |

The app calls the shared Rust core directly and does not bundle the CLI. Go source and frozen compatibility fixtures remain until cutover. Plans and validation limits are recorded in [execution.md](plans/2026-09-29-rust-rewrite/execution.md).

On the development Mac, run UI checks only. Run model inference and backend/lifecycle suites on an authorized remote host. Live checks can rent GPUs. `make release` publishes an agent; it is not a local build command.

Complete-image builds need substantial disk space. The image workflow checks for 200 GiB free Docker storage per concurrent build. Set `POD_IMAGE_RUNNER` to a suitable Linux AMD64 runner. Standard hosted runners do not meet that capacity check. This is a maintainer build requirement, not a user installation requirement.

The app-only `App release` workflow accepts an `app-vX.Y.Z` tag at the exact tested `master` commit. It requires successful Rust CI and native UI smoke, checks anonymous public Q6 access, and publishes a DMG, checksums and source receipt. It does not rebuild GPU images or release the CLI.

The legacy `v*` release workflow still builds Q6 and Q8 before publishing CLI assets. Image publication is separate from normal app startup. End users always resolve the current public image at each new start.

Maintainers must make the `1905/lobocode` container package public before release acceptance. New GHCR packages default to private. The anonymous-access check deliberately fails until visibility is public. End users do not need a GitHub account or registry credentials. See [GitHub's container registry documentation](https://docs.github.com/en/packages/working-with-a-github-packages-registry/working-with-the-container-registry).
