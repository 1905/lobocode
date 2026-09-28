# Vast.ai provider for lobo

**Date:** 2026-09-25
**Scope:** ~/dev/lobotomized-ai
**Status:** done (approved 2026-09-25)

## TL;DR

**P1 — live probe on the real Vast account (~$0.10)**
- What: rent one Vast 5090 by hand with the llama.cpp image and a probe onstart; record which env vars the container gets, whether the container can destroy itself, and container start time; then destroy it.
- Why: Vast self-destroy and "our image + onstart" are unproven; everything else depends on them.
- You do: nothing. I copy `VASTAI_API_KEY` into this repo's `.env`.
- Does NOT: change any code.

**P2 — provider abstraction (no behaviour change)**
- What: `up/down/status` talk to a `Provider` interface; RunPod becomes the first implementation. Agent self-kill and bootstrap `die` become provider-specific.
- Why: control code is hard-wired to RunPod (`internal/control/events.go:40`, `up.go:202`, `config/agent.go:18`).
- You do: nothing. `make up` behaves exactly as today.
- Does NOT: add Vast yet.

**P3 — Vast provider**
- What: `bin/lobo up --provider vast` searches 1× RTX 5090 offers (verified, reliability ≥ 0.98, disk ≥ 80 GB, CUDA ≥ 12.8, ≤ $1.20/h), sorted by network speed, rents the fastest; same agent, model sources, GPU/speed checks, 6 min container re-rent, idle/expiry self-destroy. `down` and `status` check both providers.
- Why: Vast lets us pick the fastest host up front (18 Gbps offers today); RunPod can't sort.
- You do: nothing. Default stays RunPod.
- Does NOT: fall back between providers automatically, or make Vast the default.

**P4 — live test + one Astra review**
- What: full `up --provider vast` → smoke → e2e → idle kill on the real account; then one `/rival-astra` review of the whole branch, fixes, merge.
- Why: prove it works end to end before you try it.
- You do: nothing until the final report.
- Does NOT: run any other reviews.

## Problem(s)

1. `lobo up` can only rent RunPod. RunPod repeatedly gave pods whose container never started (`plans/done/2026-09-23-qwen-5090/results.md`, 2026-09-24/25 rows), hosts at 11–19 MB/s, and "no 5090 in SECURE".
2. RunPod's API filters host network speed (`minDownloadMbps`) but cannot sort by it (`internal/control/up.go:168` steps tiers down blind). Vast's offer search sorts by `inet_down` (probe 2026-09-25: top offers 16–19 Gbps).
3. The control layer is RunPod-only: `control.RunPodAPI` (`internal/control/events.go:40`), create/delete calls (`internal/control/up.go:202,288,307,314`, `down.go:20`), pod type `runpod.Pod` (`up.go:18`).
4. Self-termination is RunPod-only: agent config requires `RUNPOD_POD_ID`/`RUNPOD_API_KEY` (`internal/config/agent.go:18-19`), killer uses GraphQL `podTerminate` (`cmd/lobo-agent/main.go:182`, `internal/runpod/self.go:61`), bootstrap `die()` too (`internal/runpod/payload.go:44-45`). On Vast the agent would fail config and never self-destroy → unbounded billing.

## Goals

1. `bin/lobo up --provider vast` boots a working lobo on Vast; API at `https://lobo.example.com/v1` exactly like RunPod. (Problem 1)
2. Vast host = fastest `inet_down` among offers passing the filter. (Problem 2)
3. RunPod behaviour and defaults unchanged. (Problem 3)
4. A Vast instance always destroys itself on idle, expiry, boot failure, or bootstrap failure — without our account key on the host if Vast offers an instance-scoped key. (Problem 4)
5. `down` and `status` never miss a lobo instance on either provider. (Problems 3, 4)

## Non-goals

- Automatic fallback RunPod ↔ Vast.
- Vast as default.
- Multi-GPU, other GPU types, Vast "interruptible" (bid) instances.
- A generic plugin system — two providers, one small interface.

## Provider interface

```go
// internal/provider
type Instance struct {
    Provider  string    // "runpod" | "vast"
    ID        string
    Name      string    // always "lobo"
    Status    string    // provider status, e.g. RUNNING / running
    CostPerHr float64
    CreatedAt time.Time
    Detail    string    // e.g. "SECURE, host ≥5000 Mbps" or "offer 51401937, 18877 Mbps, California"
}
type Provider interface {
    Name() string
    Create(ctx context.Context, o CreateOpts) (Instance, error) // o = today's runpod.CreateOpts minus RunPod-only fields
    List(ctx context.Context) ([]Instance, error)              // lobo instances only
    Delete(ctx context.Context, id string) error               // gone = nil
}
```

`up` rents through one provider (`--provider runpod|vast`, default `runpod`). Re-rent rules (bad host, container not started, too slow, no CUDA/VRAM) stay provider-neutral. RunPod-specific choices (SECURE-first, `minDownloadMbps` tiers) move into the RunPod provider's `Create`. `down`/`status` list both providers (a provider without a key in `.env` is skipped).

## Vast rental

Search (`POST https://console.vast.ai/api/v0/bundles`, Bearer key):

```json
{"gpu_name":{"in":["RTX 5090"]}, "num_gpus":{"eq":1}, "rentable":{"eq":true},
 "verified":{"eq":true}, "reliability2":{"gte":0.98}, "disk_space":{"gte":80},
 "cuda_max_good":{"gte":12.8}, "dph_total":{"lte":1.2},
 "order":[["inet_down","desc"]], "limit":10}
```

Create: `PUT /api/v0/asks/{offer_id}` with `label: "lobo"`, `image` (release llama image), `disk: 80`, `runtype: "ssh"` (not `args`: `args` never runs the onstart — ai-image-studio `vast.rs:249`), `onstart` = our bootstrap, `env` = the same `LOBO_*` / `CF_TUNNEL_TOKEN` vars as RunPod plus `LOBO_PROVIDER=vast`. An offer that is gone when we PUT → try the next offer. Bad host (same rules as RunPod) → destroy, try the next offer; 4 tries total.

Destroy: `DELETE /api/v0/instances/{id}`. List: `GET /api/v0/instances` filtered to `label == "lobo"`.

Self-destroy on the host: decided by P1.

| P1 finding | Design |
|---|---|
| Container has an instance-scoped key + own id (expected names `CONTAINER_API_KEY`, `CONTAINER_ID`; unverified) that can destroy only this instance | Agent killer + bootstrap `die` use it. No account key on the host. |
| No such key / it can't destroy | Create a restricted Vast API key per `up` if the API allows scoping; else pass the account key in env and say so in README (host sees it). Decision recorded in results before P3. |

## File-level changes

| File | Change |
|---|---|
| `internal/provider/provider.go` (new) | `Instance`, `Provider`, `CreateOpts` (moved from `runpod.CreateOpts`), `ErrNoCapacity`. |
| `internal/runpod/*` | Implement `provider.Provider`; keep SECURE-first + network tiers inside `Create`. Payload builder takes `provider.CreateOpts`. |
| `internal/vast/client.go` (new) | REST client: `SearchOffers`, `Create`, `List`, `Get`, `Destroy`; typed offer/instance. |
| `internal/vast/provider.go` (new) | `provider.Provider`: search → try offers in order → create. |
| `internal/vast/self.go` (new) | Instance self-destroy with the P1-chosen key. |
| `internal/bootstrap/bootstrap.go` (new) | Shared bootstrap script; `die()` calls a provider-specific terminate (RunPod GraphQL / Vast REST). |
| `internal/control/*` | `Deps.Providers map[string]provider.Provider` + chosen provider; `up/down/status` via the interface; events show provider + offer detail. |
| `internal/config/{laptop,agent}.go` | Laptop: optional `VASTAI_API_KEY`, `LOBO_VAST_MAX_DPH` (default 1.20). Agent: `LOBO_PROVIDER`; RunPod vars required only when provider = runpod; Vast vars when vast. |
| `cmd/lobo-agent/main.go` | Killer + fatal self-kill chosen by `LOBO_PROVIDER`. |
| `cmd/lobo/main.go` | `up --provider runpod|vast`; `down`/`status` across providers. |
| `internal/tui/*` | Show provider in rent line and dashboard. |
| `README.md`, `.env.example` | Vast usage, key, cost note. |

## Tests

Unit (fakes / httptest, no network):
- vast: search query body exactly as above; offer parsing (fixture from the live probe); create body (`runtype: "ssh"`, `label`, env, onstart); offer-taken error → next offer; destroy 404 → nil; list filters label.
- provider: RunPod implementation passes today's control tests unchanged (SECURE-first, tiers).
- control: `--provider vast` uses the Vast provider; bad-host re-rent destroys and tries the next offer; `down` deletes lobo on both providers; `status` finds a Vast instance when RunPod has none; provider without key skipped.
- agent config: runpod vars not required when `LOBO_PROVIDER=vast` and vice versa.
- bootstrap: `die()` text per provider.

Live (orchestrator only, real account):
- P1 probe (below). P4: `up --provider vast` → ready; `lobo test`; `make e2e`; `/api/version` git sha = HEAD; idle kill with `--idle-min 3`; `down` with a Vast instance up; forced boot failure → instance self-destroys.

## Failure modes & decisions

| Failure | Behaviour |
|---|---|
| No offer passes the filter | Error naming the filter; suggest `LOBO_VAST_MAX_DPH` or RunPod. No automatic fallback (non-goal). |
| Offer taken between search and PUT | Try the next offer (no delay). |
| Container never starts / no CUDA / VRAM taken / slow download | Same as RunPod: destroy, next offer, 4 tries. |
| Vast API 401/403 | Clear "check VASTAI_API_KEY" error. |
| Self-destroy fails | Killer retries with backoff until gone (same contract as RunPod). |
| Instance keeps billing storage after stop | We only ever destroy, never stop. |
| Both providers have a lobo instance | `up` refuses ("already running on X"); `down` deletes both. |

## Out of scope

- Vast bid/interruptible pricing, templates, volumes.
- Auto-choosing the provider by price or stock.
- Changing model sources: Vast uses the same presigned R2 + the model server fallback.

## Rollout

- **P1** — live probe on Vast (orchestrator, ~$0.10); results appended to results.md; spec updated if the self-destroy path differs.
- **P2** — provider interface + RunPod implementation; all existing tests green; no behaviour change.
- **P3** — Vast client, provider, self-destroy, bootstrap, CLI/TUI; unit tests.
- **P4** — live Vast run (up, smoke, e2e, idle kill, forced failure); one `/rival-astra` review of the whole branch; fixes; merge; push.

## As-built notes

- `Provider` interface grew `Rent` (provider picks the host) and `Get`; `Instance` has `StartedAt`/`HostDownloadMbps`, no `Name`.
- Astra review fixes: boot id ties `/api/status` to the rented pod (shared tunnel); uncertain Vast creates are adopted, never retried on another offer; `down` keeps going when one provider fails; bootstrap terminate retried until accepted and exec failure terminates; download errors show scheme://host only.
- RunPod key is optional on the laptop (Vast-only configs work).
