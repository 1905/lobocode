# Cloud access without buying a domain

Research date: 2026-09-30. Status: proposed; not implemented or live-tested. Release remains on hold.

Use a Lobocode-managed SSH tunnel as the default cloud connection. It needs no domain purchase, Cloudflare account or paid tunnel subscription. GPU rental and any provider bandwidth charges still apply.

## User flow

1. Enter a RunPod or Vast API key.
2. Select Cloud and start the model.
3. Lobocode creates the instance and connects its encrypted tunnel automatically.
4. OpenCode uses a stable loopback endpoint, for example `http://127.0.0.1:8933/v1`.

The example port is a proposal. Keep it separate from local inference and its adjacent control port. The computer forwards requests; inference and model memory stay on the cloud GPU. A background connection process must stay running while clients use the endpoint. Other devices cannot use this loopback address.

## Why SSH

Vast explicitly documents local port forwarding and offers direct and proxy SSH connections. Direct connections need open ports. [Vast SSH documentation](https://docs.vast.ai/guides/instances/connect/ssh)

RunPod supports full SSH through an exposed TCP port on pods with public IPs. Select compatible Community Cloud hosts; do not assume its basic SSH proxy supports forwarding. This constraint may reduce host availability. [RunPod SSH documentation](https://docs.runpod.io/pods/configuration/use-ssh)

SSH forwards the existing HTTP bytes, so the proposed design does not translate streaming chat or tool-call responses. This is a design inference, not a completed Lobocode integration test.

## Alternatives checked

| Connection | Finding |
|---|---|
| RunPod HTTPS proxy | Provides a hostname without buying a domain. It can time out before the first response after roughly 100 seconds, which matters for large prompts. It is also provider-specific. |
| Cloudflare Quick Tunnel | No domain purchase, but Cloudflare states that SSE is unsupported. Unsuitable for the existing OpenAI-compatible chat stream. |
| Named Cloudflare tunnel | Keep as an optional advanced public endpoint for users who already have a domain. |
| Public HTTP port | Does not encrypt prompts or API keys. Do not use as the default. |

Sources: [RunPod proxy behavior](https://www.runpod.io/blog/runpod-proxy-guide), [RunPod exposed ports](https://docs.runpod.io/pods/configuration/expose-ports), [Cloudflare Quick Tunnel limits](https://developers.cloudflare.com/cloudflare-one/networks/connectors/cloudflare-tunnel/do-more-with-tunnels/trycloudflare/).

## Required implementation

- Make domain and Cloudflare token optional for SSH mode in config validation, the setup wizard and app Settings. Preserve existing named-tunnel configurations.
- Capture provider SSH address and port mappings. The current provider models do not retain all required fields.
- Generate a dedicated connection key and install only its public half on the task-owned instance. Do not change the user's normal SSH keys. Establish and pin the instance host key through an authenticated provider path.
- Forward inference and agent control ports through one owned connection. Bind listeners to `127.0.0.1`, retain API-key authentication, and never expose an unauthenticated public inference port.
- Keep the tunnel in a small detached connection process. Quitting the window or finishing `lobo up` must not break OpenCode. Share state and reconnect logic between app and CLI.
- Detect port conflicts, sleep/wake, changed provider mappings, disconnects and instance replacement. Keep the GPU watchdog independent of the laptop connection. Stop cleans up only owned resources.
- Remove the mandatory cloudflared startup stage in SSH mode. Route status, logs, ready checks, test calls and generated OpenCode config through the discovered endpoint.

## Acceptance before release

Test on Dell or authorized cloud hosts, not with a model on the development Mac. No GPU rental or release was made for this research.

- Fresh configuration with no `LOBO_DOMAIN` or `CF_TUNNEL_TOKEN` works on both providers.
- Streaming begins progressively; a slow first token does not hit an HTTP proxy timeout.
- Tool calls, status, logs, cancellation and Stop work through the tunnel.
- App/CLI reopen, sleep/wake and reconnect preserve the endpoint without duplicate GPU creation.
- Wrong keys, host-key mismatch, occupied local ports and dropped connections fail visibly.
- Existing domain-based configurations still work.

Model and agent distribution are a separate onboarding dependency. The current configuration still requires a model bucket URL. Removing the domain requirement alone does not remove that requirement.
