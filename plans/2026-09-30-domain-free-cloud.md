# Domain-free cloud connections

Status: implementation authorized on 2026-09-30. Plan written before code changes.
Release remains on hold. Do not rent GPUs or publish builds for this work.

## Outcome

A new cloud setup needs no purchased domain, Cloudflare account, or tunnel token.
Lobocode manages an encrypted SSH connection. OpenCode uses
`http://127.0.0.1:8933/v1`; the cloud agent uses adjacent port 8934.
Inference stays on the rented GPU. The connection has no subscription fee.
GPU rental, provider bandwidth, and model storage are separate costs.

## Implementation order

1. Add explicit `LOBO_CONNECTION=ssh|cloudflare` and `LOBO_CLOUD_PORT` settings.
   New configurations default to SSH. Preserve older complete domain/token
   configurations when the connection setting is absent. The app and wizard
   save SSH explicitly for new cloud setups. Keep Cloudflare as file-configured
   legacy compatibility; remove its required fields from the normal UI.
2. Add a shared cloud connection module in `lobo-core`. Generate dedicated
   Ed25519 client and server keys for each boot. Store files with private
   permissions beside the configuration. Send the server private key and
   client public key through the authenticated provider create request.
   Pin the corresponding server public key before connecting. Do not use
   trust-on-first-use or disable host verification. Never use personal SSH keys.
3. Bootstrap a separate SSH daemon on container port 2222. Restrict it to
   public-key authentication and forwarding to loopback ports 8080 and 8081.
   Put its startup inside the existing failure/deletion trap. In SSH mode,
   the agent must not require, download, or start cloudflared.
4. Request a public TCP mapping on RunPod. Request a direct port on Vast.
   Preserve provider address and port mappings in internal instance data.
   Refresh mappings while booting and after disconnects. Fail without opening
   a public HTTP inference port. Compatible hosts may be less available.
5. Run a small detached connection helper from the CLI or app executable.
   Use OpenSSH with explicit options, strict host-key checks, loopback listeners,
   keepalives, and no user SSH configuration. A file lock prevents duplicate
   helpers. Persistent desired state identifies the owned instance and boot.
   The helper reconnects after network loss, survives closing the UI, and
   exits when stopped, expired, or the instance is deleted. Stop must not
   signal unrelated processes. Report connection failures and occupied ports.
6. Use the same connection in boot polling, status, logs, tests, and generated
   OpenCode configuration. Preserve cancellation and uncertain-create cleanup.
   A failed connection must not abandon a billed instance.
7. Update setup validation, CLI wizard, Settings, UI fixtures, README, assets,
   research notes, and changelog. Keep native window behavior and no scrolling.

## Validation

- Dell only: Rust backend/unit/integration tests, provider request fixtures,
  agent configuration, process ownership, cancellation, and cleanup.
- Dell only: real OpenSSH against a task-owned fake HTTP/SSE server; prove
  progressive streaming, tool-call bytes, long first-response delay, reconnect,
  strict host-key rejection, occupied ports, and detached-helper cleanup.
  This does not require a model or GPU rental.
- Mac: frontend checks, native UI-only setup smoke, and TUI layout checks.
  No inference, backend suites, or Docker on the Mac.
- Keep live RunPod/Vast rental, GPU inference, and actual sleep/wake acceptance
  marked pending until remote/manual testing is authorized. Do not release.

## Sources and limits

- [RunPod SSH](https://docs.runpod.io/pods/configuration/use-ssh)
- [RunPod TCP mappings](https://docs.runpod.io/pods/configuration/expose-ports)
- [RunPod create API](https://docs.runpod.io/api-reference/pods/POST/pods)
- [Vast SSH forwarding](https://docs.vast.ai/guides/instances/connect/ssh)
- [Vast create API](https://docs.vast.ai/api-reference/instances/create-instance)
- [Vast CLI port mapping implementation](https://github.com/vast-ai/vast-python/blob/master/vast.py)
- [Quick Tunnel SSE restriction](https://developers.cloudflare.com/cloudflare-one/networks/connectors/cloudflare-tunnel/do-more-with-tunnels/trycloudflare/)

The existing model/agent bucket is still a separate configuration dependency.
A bucket URL can use a provider hostname; it does not require buying a domain.
Cross-device public access is outside this change. Localhost is intentionally
available only to clients on the computer running the connection helper.
