# Cloud access without buying a domain

Updated: 2026-09-30. Status: implemented in the unreleased Rust branch. Real SSH fixture tests pass on Dell. Live GPU-provider acceptance is still pending. Release remains on hold.

Distribution correction: the bucket requirement below describes the current implementation, not the intended public setup. The user rejected that dependency. It must be replaced with complete public Docker images containing the agent, runtime and selected model. See the [replacement plan](../plans/2026-09-30-public-pod-images.md) and [mistake record](implementation-mistakes.md).

Use a Lobocode-managed SSH tunnel as the default cloud connection. It needs no domain purchase, Cloudflare account or paid tunnel subscription. GPU rental and any provider bandwidth charges still apply.

## User flow

1. Enter a RunPod or Vast API key.
2. Select Cloud and start the model.
3. Lobocode creates the instance and connects its encrypted tunnel automatically.
4. OpenCode uses `http://127.0.0.1:8933/v1` by default.

The cloud control API uses port 8934. Local inference uses 8931 and 8932. `LOBO_CLOUD_PORT` changes the cloud pair; pairs cannot overlap. The computer forwards requests; inference and model memory stay on the cloud GPU. A small detached connection process stays running after the app closes or `lobo up` finishes. Other devices cannot use this loopback address.

## Why SSH

Vast documents local forwarding. Lobocode selects hosts with direct ports and requests container port 2222. [Vast SSH documentation](https://docs.vast.ai/guides/instances/connect/ssh)

RunPod supports full SSH through an exposed TCP port on pods with public IPs. Select compatible Community Cloud hosts; do not assume its basic SSH proxy supports forwarding. This constraint may reduce host availability. [RunPod SSH documentation](https://docs.runpod.io/pods/configuration/use-ssh)

SSH forwards HTTP bytes without translating responses. Dell tests verified progressive SSE and tool-call data through the real connection. A separate request waited 105 seconds before returning its first response.

## Alternatives checked

| Connection | Finding |
|---|---|
| RunPod HTTPS proxy | Provides a hostname without buying a domain. It can time out before the first response after roughly 100 seconds, which matters for large prompts. It is also provider-specific. |
| Cloudflare Quick Tunnel | No domain purchase, but Cloudflare states that SSE is unsupported. Unsuitable for the existing OpenAI-compatible chat stream. |
| Named Cloudflare tunnel | Keep as an optional advanced public endpoint for users who already have a domain. |
| Public HTTP port | Does not encrypt prompts or API keys. Do not use as the default. |

Sources: [RunPod proxy behavior](https://www.runpod.io/blog/runpod-proxy-guide), [RunPod exposed ports](https://docs.runpod.io/pods/configuration/expose-ports), [Cloudflare Quick Tunnel limits](https://developers.cloudflare.com/cloudflare-one/networks/connectors/cloudflare-tunnel/do-more-with-tunnels/trycloudflare/).

## Implementation

- `LOBO_CONNECTION=ssh` is the new default. A complete older domain/token configuration retains Cloudflare when this setting is absent. Cloud Settings can switch it to private SSH for the next start.
- The provider create request carries a fresh server key and the client's public key. The local private key never goes to the provider. The saved server public key pins its identity before the first connection. Personal SSH files are not used.
- Bootstrap starts a separate daemon on port 2222. It permits only key authentication and forwarding to loopback inference/control ports. It does not permit shell sessions. Bootstrap failures retain the instance-deletion trap.
- The helper binds only `127.0.0.1`. Inference and control requests still require the Lobocode API key. SSH mode does not download or start cloudflared.
- The helper uses a lock and a private control socket. It reconnects after a dropped connection and refreshes provider mappings. Stop removes its desired state, closes its owned SSH connection, and removes the run's keys.
- Connection state is stored beside the config, under `config.cloud/` for the default `config.env` filename. Private directories use mode 700. Private keys and state files use mode 600.

## Acceptance before release

Test on Dell or authorized cloud hosts, not with a model on the development Mac. No GPU rental or release was made for this change.

- Passed: new configuration validation for both providers, request/port parsing, progressive streaming, tool-call data, authentication, reconnect, and a 105-second first response.
- Passed: detached reopen without a duplicate connection, helper crash recovery, strict host-key rejection, unauthorized-client-key rejection, occupied-port checks, and owned connection/key cleanup.
- Passed: native UI-only setup and all 20 panel/three Settings browser layouts without scrolling. These checks do not prove GPU inference.
- Pending before release: actual RunPod/Vast provisioning with the updated agent, model inference, and real sleep/wake acceptance. Fixture tests do not prove live provider behavior.

Model and agent distribution remain a separate onboarding dependency. The bucket can use its provider's hostname. SSH mode requires the matching updated agent; no new agent image or release has been published. The implementation plan is [here](../plans/2026-09-30-domain-free-cloud.md).
