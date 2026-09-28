# Vast provider — Implementation Plan v1.0

**Date:** 2026-09-25
**Status:** done
**Spec:** ./spec.md · **P1 results:** ./results.md

**Goal:** `bin/lobo up --provider vast` rents the fastest-network verified 1× RTX 5090 on Vast and boots lobo exactly like RunPod; `down`/`status` see both providers; RunPod stays default and unchanged.

**Architecture:** new `internal/provider` (neutral `Instance`, `CreateOpts`, `Provider`); `internal/runpod` and new `internal/vast` implement it; `internal/control` works only on `provider.Provider`. Bootstrap script + env builder shared (`internal/bootstrap`); only `die()` and the agent killer differ per provider (`LOBO_PROVIDER`).

**Tech:** Go 1.26, existing libs. No new deps.

## Locked interfaces

```go
// internal/provider
var ErrNoCapacity = errors.New("provider: no gpu capacity")
var ErrNotFound   = errors.New("provider: instance not found")
const Name = "lobo" // pod name / Vast label
type Instance struct {
    Provider, ID, Status, Detail string
    CostPerHr        float64
    StartedAt        time.Time
    HostDownloadMbps int
}
type CreateOpts struct { // today's runpod.CreateOpts minus CloudType/MinDownloadMbps, plus Cloud
    Image, ReleaseURL, ReleaseSHA256, ModelURL, ModelFallback, LoboAPIKey, CFTunnelToken, Model string
    Ctx, IdleMin, DLConns, MinMBps int
    ExpiresAt time.Time
    Cloud     string // runpod: "secure"|"community" preference; vast: ignored
    SSHPubKey, ModelSSHKey, ModelHostKey string
}
type Provider interface {
    Name() string                                                                  // "runpod" | "vast"
    Rent(ctx context.Context, o CreateOpts, note func(string)) (Instance, error)     // provider picks host (tiers/offers); ErrNoCapacity if none
    List(ctx context.Context) ([]Instance, error)                                  // lobo instances only
    Get(ctx context.Context, id string) (Instance, error)                          // ErrNotFound when gone
    Delete(ctx context.Context, id string) error                                   // gone = nil
}

// internal/bootstrap
func Env(o provider.CreateOpts, providerName string) map[string]string // all LOBO_* + CF_TUNNEL_TOKEN + LOBO_PROVIDER
func Script(providerName string) string                                  // bootstrap; die() terminates via runpod GraphQL or vast REST
```

Vast rent: `POST /bundles` with the spec filter, `order inet_down desc`, `limit 10`; try offers in order, skipping offer ids already tried in this process (a bad host is not retried); `PUT /asks/{id}/` `{client_id:"me", image, disk:80, label:"lobo", runtype:"ssh", onstart:Script("vast"), env:Env(o,"vast")}`; an offer error → next offer; all fail → `ErrNoCapacity`. `List` = `GET /instances` filtered `label=="lobo"`. `Get` 200 + `instances:null` or 404 → `ErrNotFound`. `Delete` 404 → nil.

Agent: `LOBO_PROVIDER` (default runpod). runpod → requires `RUNPOD_POD_ID`/`RUNPOD_API_KEY`, killer = runpod.Self. vast → requires `CONTAINER_ID`/`CONTAINER_API_KEY`, killer = vast.Self (DELETE own, confirm via GET). `cleanEnv` also drops `CONTAINER_API_KEY`.

## Tasks

- [ ] **T1 provider package + RunPod adapter** (`internal/provider/provider.go`, `internal/runpod/provider.go`). Move SECURE-first + network-tier loop from `control/up.go:193-215` into `runpod.Provider.Rent`. Move `runpod.CreateOpts` → `provider.CreateOpts`; payload builder takes `(provider.CreateOpts, cloud, mbps)`. Tests: existing tier/cloud tests move to `internal/runpod` and still pass.
- [ ] **T2 bootstrap package** (`internal/bootstrap/bootstrap.go`): `Env`, `Script`. RunPod payload uses them. Test: env keys (incl. `LOBO_PROVIDER`), no `R2_`/account keys, `die()` text per provider.
- [ ] **T3 control on provider** (`internal/control/*`, `controltest`): `Deps.Providers map[string]provider.Provider`, `UpOpts.Provider` (default "runpod"). `up`: "already running" checks all providers; rent via chosen one; Get/Delete via it. `down`/`status`: all providers. `Snap.Pod` → `*provider.Instance`. TUI/cmd adjust. All existing control/tui tests pass (renamed fakes).
- [ ] **T4 config** (`internal/config`): laptop `VASTAI_API_KEY` (optional), `LOBO_VAST_MAX_DPH` (default 1.20); agent `LOBO_PROVIDER` + per-provider required vars. Tests for both providers.
- [ ] **T5 vast package** (`internal/vast/{client,provider,self}.go`): as above; `httptest` tests with the live probe shapes (create response, `instances:null`, 404 body).
- [ ] **T6 agent** (`cmd/lobo-agent`): killer + fatal self-kill by provider; `cleanEnv` drops `CONTAINER_API_KEY`.
- [ ] **T7 CLI** (`cmd/lobo`): `up --provider runpod|vast`; deps build both providers when keys exist; rent line shows provider + detail.
- [ ] **T8 live P4** [orchestrator]: release; `up --provider vast` (fastest offer); `lobo test`; `make e2e`; `/api/version` sha = HEAD; `down` while up; idle kill `--idle-min 3`; forced failure (bad model URL via `--source public` is not a failure… use `--min-mbps 100000` → "too slow" → instance self-destroys and next offer) ; RunPod `up` smoke still works.
- [ ] **T9 exit**: one `/rival-astra review` on the branch → verify + fix → release → merge → push → notify → watchdog off.

## Self-review
- Every spec file-level row maps to T1–T7; P1 done (results.md).
- Names consistent: `provider.Provider/Instance/CreateOpts`, `bootstrap.Env/Script`, `LOBO_PROVIDER`.
- RunPod unchanged: T1/T3 keep every existing RunPod test green before Vast code lands.
