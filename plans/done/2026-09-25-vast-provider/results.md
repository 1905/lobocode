# Results — Vast provider

**Status:** done

## P1 — live probe (2026-09-25, instance 52607650, offer 51738273, 20,313 Mbps, $0.73/h, ~$0.01)

| Question | Result |
|---|---|
| Create | `PUT /api/v0/asks/{offer}/` 200 → `{"success":true,"new_contract":<instance id>,"instance_api_key":…}` |
| Image + `runtype: "ssh"` + onstart | `ghcr.io/ggml-org/llama.cpp:server-cuda-b11118`: loading at 20 s, **running at 36 s**, onstart ran at ~52 s |
| `env` as JSON object | ✓ every `LOBO_*` var visible in the container |
| Vast-injected env | `CONTAINER_API_KEY` (64 chars), `CONTAINER_ID`, `CONTAINER_LABEL`, `VAST_CONTAINERLABEL=C.<id>`, `PUBLIC_IPADDR`, `SSH_PUBLIC_KEY`, `LLAMA_ARG_HOST` (from the image) |
| Instance key: `GET /api/v0/instances/{own id}/` | 200 |
| Instance key: `DELETE /api/v0/instances/{own id}/` | 200 `{"success":true}`; laptop DELETE afterwards → 404 `no_such_instance`; laptop GET → 200 with `"instances": null` |
| GPU | RTX 5090, 0 MiB used, 32,607 MiB, driver 580.119.02 |
| Tools | curl ✓, python3 ✓, unzip ✗ (bootstrap installs it) |
| R2 presigned, 8 streams × 15 s from this host | 131 MB/s (R2 was slow at the source all day) |

Decision: self-destroy uses Vast's instance-scoped `CONTAINER_API_KEY` + `CONTAINER_ID` (REST DELETE). No account key on the host. "Gone" = DELETE 404, or GET 200 with `instances: null`.
Note: the agent is not PID 1 on Vast (onstart runs under Vast's init), so an agent crash does not restart the container; `die()` in the bootstrap and the fatal self-kill in the agent cover it.

## P4 — live run (2026-09-26 01:36–02:12, release 2026.09.25-10 @ 4b513b3, CLI from feat/macos)

| Step | Result |
|---|---|
| `up --provider vast` | ✓ ready in 348 s, 1st offer: 47302315, 18,047 Mbps, Switzerland, $0.68/h |
| `lobo test` | ✓ streamed chat 2.5 s, tool call 1.1 s |
| `make e2e` | ✓ (exit 0) |
| `/api/version` git_sha | ✓ 4b513b3 = release commit |
| `down` with a Vast instance up | ✓ 52625032 listed before, nothing after; $0.08 |
| idle kill `--idle-min 3` | ✓ 6 min of traffic, no kill; destroyed itself 170 s after the last request |
| forced failure `--min-mbps 100000` | ✓ 4 different offers, each: R2 → the model server fallback → "too slow" (111–117 MB/s) → deleted → next offer; `up`: "gave up: 4 pods in a row landed on bad hosts (all deleted)"; 0 Vast instances left |
| RunPod smoke | ✓ 569 s: no 5090 in SECURE → COMMUNITY; 1st pod container never started (6 min rule) → re-rent → ready; `lobo test` ✓; down $0.04 |
| Leftovers | RunPod 0 pods, Vast 0 instances (whole accounts, checked by API after the run) |

Note: RunPod 403s requests with Python's default urllib User-Agent (scripts need a UA header); the Go CLI is not affected.
