# Configure OpenCode and show actual Lobocode runtime state

**Date:** 2026-09-30
**Scope:** /Users/kass/dev/lobocode
**Status:** approved
**Approval:** User approved the expanded spec on 2026-09-30, including implementation and bounded scripted E2E.

## TL;DR

**What:** Add Configure/Repair OpenCode using the running endpoint, key and model.
**Why:** Fix stale settings and wrong model selection from the app.
**Your action:** Keep or clear “Use Lobocode by default” before saving.
**Limits:** Preserve other settings; setup sends no prompts. OpenCode may need restarting.

**What:** Show live model memory during loading and use, alongside Mac available memory.
**Why:** The existing display measures the whole Mac and hides memory during loading.
**Your action:** Nothing; readings refresh automatically.
**Limits:** Cloud shows device VRAM. Model memory excludes the app itself; no fake loading percentage.

**What:** Show Lobocode in the Dock and Cmd+Tab, with normal window reopening.
**Why:** Background-app settings currently hide it.
**Your action:** Nothing; keep the menu bar control too.
**Limits:** Closing a window keeps the app running.

**What:** Show Reading prompt, processed tokens, Generating and Idle instead of misleading 0/0.
**Why:** Reading a large input can produce no reply tokens for several minutes.
**Your action:** Nothing; unknown or stale activity is labeled unavailable.
**Limits:** No guessed percentages, response-time promises or captured prompt content.

**What:** Scope app Start, status and Stop to its target; fix the Vast error on Local Stop.
**Why:** Current cleanup scans all providers and can delete unrelated cloud instances.
**Your action:** Approve this expanded spec; then implement and test with scripts, without OpenCode.
**Limits:** The original short test remains available. The later same-size diagnostic below supersedes its input cap for one request only. No admission bypass, publication or GPU rental.

**What:** Run one direct local Q6 request with exactly 47,000 synthetic input tokens and at most 32 output tokens.
**Why:** Determine whether the model can finish an input as large as the stalled OpenCode request.
**Your action:** Nothing; the user explicitly requested this diagnostic on 2026-09-30.
**Limits:** Normal memory admission, 65,536 context, no OpenCode, no private prompts, no retries. Other E2E remains deferred.

**What:** Run cloud E2E on RunPod through the TUI and the native Mac app, with direct scripted inference.
**Why:** Local inference is blocked by Mac memory, and live cloud acceptance is still unverified.
**Your action:** Nothing for test preparation or GPU rental; the user explicitly requested cloud E2E.
**Limits:** Q6, one GPU at a time, bounded requests, exact cleanup. Local inference, OpenCode and app/CLI release remain deferred. Public image promotion still requires resolving the earlier publication hold.

## Problem(s)

1. OpenCode setup requires a CLI-generated file and manual merging. The generator fixes the model at Q8 and truncates its output file. It cannot safely repair a real user config unchanged. Evidence: `README.md:118`, `crates/lobo-core/src/genkey.rs:28`, `crates/lobo-core/src/genkey.rs:60`.
2. Local memory is mislabeled. The macOS collector sums whole-machine active, wired and compressed pages into `Gpu.vram_used_mb`; the Ready view calls that memory. No such indicator exists in the startup view. Evidence: `crates/lobo-core/src/local/deps.rs:182`, `crates/lobo-core/src/local/deps.rs:368`, `app/ui/src/lib/view.ts:128`, `app/ui/src/panel/BootLog.svelte:14`.
3. Display samples lack their collection time and share the watchdog's slow schedule. Both local and cloud runners use 30-second ticks; the UI cannot prove that a value belongs to the current model process. Evidence: `crates/lobo-agent/src/runner.rs:269`, `crates/lobo-core/src/local/supervise.rs:219`, `crates/lobo-agent/src/pod.rs:499`, `crates/lobo-proto/src/agent.rs:118`.
4. The app intentionally hides itself from normal macOS app switching. It declares `LSUIElement=true` and sets accessory activation, despite already creating a native main window. Evidence: `app/src-tauri/Info.plist:3`, `app/src-tauri/src/lib.rs:52`, `app/src-tauri/src/windows.rs:55`.
5. Ready displays generation and prompt rate gauges without a request activity state. Those gauges can be zero while the server processes input. Missing counters also default to zero in the current parser. Evidence: `app/ui/src/lib/view.ts:126`, `app/ui/src/panel/ReadyCard.svelte:45`, `crates/lobo-agent/src/metrics.rs:34`.
6. Local Stop and status use global provider enumeration. Stop records Vast errors while deleting every returned runtime, including Local; status rejects provider errors before selecting a valid Local instance. Start also scans every provider. Evidence: `app/src-tauri/src/backend.rs:57`, `crates/lobo-core/src/control/status.rs:3`, `crates/lobo-core/src/control/status.rs:40`, `crates/lobo-core/src/control/mod.rs:56`, `crates/lobo-core/src/control/up.rs:196`.

The session's read-only investigation observed a healthy Q6 runtime without swap activity. An 18-character user message expanded into an approximately 47,000-token request. A separate four-token title request completed at 9.86 tokens/s. Task 15 was reading input while `/metrics` reported zero rates and one processing request. See [investigation.md](investigation.md) for the observed counters and progress. These observations are not acceptance results for this feature. No prompt text, credentials or raw request bodies belong in these artifacts.

The session's OpenCode endpoint, Q6 model, key and `default_agent=lobo` were already corrected manually. That repair does not implement the app action.

## Goals

1. Configure or repair the active Lobocode provider from verified runtime data without changing unrelated OpenCode settings. Addresses problem 1.
2. Offer a visible, initially checked default-selection checkbox before the write. Preserve custom agent behavior. Addresses problem 1.
3. Display correctly labeled, measured memory throughout startup and Ready without scrolling. Addresses problem 2.
4. Collect memory and activity every two seconds, track freshness and process identity, and leave the watchdog's 30-second policy intact. Addresses problem 3.
5. Restore Dock, Cmd+Tab, reopen and native window behavior while retaining the tray. Addresses problem 4.
6. Distinguish reading input, generating output, confirmed idle and unavailable activity without interpreting zero rates as idle. Addresses problem 5.
7. Keep app lifecycle and status actions scoped to the selected or owned runtime. Never contact unrelated providers during Local operations. Addresses problem 6.

## Non-goals

- Changing model selection, context size, startup admission estimates or memory limits.
- Reporting model memory as total Lobocode application memory, or adding Metal allocations to physical footprint.
- Estimating load percentage from model file size or memory growth.
- Installing OpenCode or the optional Lobocode CLI, launching chats, or rotating API keys.
- Disabling providers, MCP servers, skills or tools globally.
- Tuning model speed, changing prompt construction, or capturing prompt and response text.
- Publishing an app, image or release, restarting paused image jobs, or renting a GPU.

## Runtime ownership and local Stop

The app must capture its runtime identity before Stop: provider, instance identity and local boot/process identity where relevant. Keep this identity through cancellation, deletion, verification and the final status refresh. A changed UI selection or saved default must not redirect Stop. Local Stop must not list, authenticate with or delete resources through Vast or RunPod. Cloud Stop must touch only the recorded provider and instance. Missing ownership must fail safely, never fall back to global cleanup.

The current `control::down` intentionally performs global cleanup. Preserve that explicit CLI contract, but provide separate targeted operations for the app. Filtering provider maps alone is insufficient: `cleanup::pending` can reconcile an unrelated provider's operation, and `connection.stop_saved` can terminate an unrelated cloud connection. Preserve both unless the record belongs to this operation. See `crates/lobo-core/src/control/cleanup.rs:73` and `crates/lobo-core/src/control/status.rs:106`.

Local Start, status polling, verification and Stop must make zero cloud-provider requests. Scope existing-runtime checks to the chosen provider. If an unrelated pending operation owns the shared operation record, preserve it and report that it must be resolved separately; do not reconcile or overwrite it. Cloud image resolution still selects the latest public image per new cloud Start.

Keep startup cancellation owned: cancel its worker, await its completion, then verify only the recorded target. An already stopped target is success. A reused PID or replaced local boot must not be signalled. Preserve unrelated cloud instances and saved cloud connections throughout. A successful local stop reaches Off without an unrelated provider warning.

## OpenCode configuration

```text
OpenCode settings -> show target file, active endpoint, model and default checkbox
  -> Configure/Repair -> fresh runtime identity + authenticated model listing
  -> validate config -> preserve unrelated data -> private backup and key file
  -> atomic config replacement -> report saved target and restart/override caveat
```

Add a Clients tab to Settings and a compact OpenCode action in Ready. The Ready action opens Clients. Do not add another tall card to the fixed-size main window. Display `Use Lobocode by default` before Configure/Repair, checked initially; keep the user's choice while that view is open. Disable duplicate submissions.

The Rust backend resolves the current instance endpoint, running `Status.model`, running context and existing API key. Map the running Q6/Q8 ID to its catalog alias. Do not substitute the selected dropdown model or saved default. The existing endpoint preference starts with the active instance URL at `app/src-tauri/src/store.rs:134`; running model/context live at `crates/lobo-proto/src/agent.rs:137`.

Require a current Ready instance and a successful authenticated `GET /v1/models` matching that alias. Do not send a completion request. Reject empty keys, unavailable runtime state, wrong authentication or model mismatch before writing. Recheck boot ID, endpoint, model and key generation before commit; an intervening stop, restart or settings change invalidates the operation. A rotated saved key may differ from a running runtime's key, as documented in `crates/lobo-cli/src/cmd/genkey.rs:12`.

Use `lobo-local` for the local target and `lobo` for cloud, matching existing provider names. Repair that provider's OpenAI-compatible package, endpoint, private key-file reference and current model entry. Use the actual context limit; do not retain the generator's unconditional 65,536-token assumption. Cap the generated output limit at the smaller of 8,192 and the actual context. Preserve other provider entries, the other Lobocode target, unrelated models and custom options.

Always set `agent.lobo.model` to the verified provider/model while preserving any existing `agent.lobo` prompt, tools, permissions and other custom settings. If the agent is absent, create a primary lean agent with core coding tools and no MCP tools or skills. Scope those restrictions to the new agent. Do not reproduce the existing exporter's personal MCP-name list or change global permissions.

When the checkbox is checked, set top-level `model` to that provider/model and `default_agent` to `lobo`. When unchecked, preserve both keys exactly, including absence. Leave `small_model`, provider allowlists, policies and other agents unchanged. Report relevant provider restrictions and project overrides without silently relaxing them.

Detect the standard global config, including an inherited `XDG_CONFIG_HOME`. Prefer the existing `opencode.jsonc`, then `opencode.json`, then legacy `config.json`; create `opencode.jsonc` when none exists. Show the resolved path before writing and allow choosing an existing JSON/JSONC config for nonstandard setups. Do not infer a Terminal's environment from Finder-launched app state. Never write to project or managed settings automatically.

Use an established JSONC editor that preserves comments and unrelated text. Never parse and pretty-print the entire existing file. Invalid JSONC, duplicate ambiguous keys or non-object target containers fail without changes. Check for concurrent edits before replacement. Refuse unsupported or changed symlink targets without replacing the link.

Create a timestamped private backup of an existing config before changes. Write the validated key to a new, provider-specific private file in Lobocode's config directory and use OpenCode's `{file:...}` substitution. Use a fresh file for a changed key so backups retain valid references. Apply mode `0700` to new private directories and `0600` to key/config/backup files. Flush the prepared files, then atomically replace the config. On failure, preserve the original config and remove only new, unreferenced task-owned files. Repeated repair with unchanged values is a no-op.

Return only the changed file path, provider/model and bounded result message. Do not return keys, full config or raw parser excerpts to the frontend or logs. Successful authenticated setup proves the recorded endpoint/key/model combination at that moment. It does not prove a generation or override project/managed policy. Display: `Saved. Restart OpenCode to reload. Project settings can override this file.`

[OpenCode's config documentation](https://opencode.ai/docs/config/) documents JSONC, config precedence and file substitutions. Its [current config writer](https://github.com/anomalyco/opencode/blob/dev/packages/opencode/src/config/config.ts#L129) selects existing JSONC first and edits individual values. Recheck the supported version during implementation; do not depend on an installed CLI for parsing or writing.

## Measured memory and sample freshness

Use a new optional status object for measured display data. Keep existing fields readable for older clients. An older agent without the new fields yields `Unavailable` in the new indicator; do not relabel legacy whole-Mac usage as model memory.

Illustrative local payload; numbers are fixtures, not measurements from this work:

```json
{
  "telemetry": {
    "version": 1,
    "boot_id": "fixture-boot",
    "memory": {
      "collected_at": "2026-09-30T12:00:00Z",
      "sample_age_ms": 120,
      "sequence": 1,
      "state": "available",
      "scope": "model_process",
      "used_bytes": 24500000000,
      "mac_available_bytes": 8400000000,
      "total_bytes": null
    },
    "activity": {
      "collected_at": "2026-09-30T12:00:00Z",
      "sample_age_ms": 120,
      "sequence": 1,
      "state": "reading_prompt",
      "processed_tokens": 16384,
      "total_tokens": null,
      "tokens_per_second": 58.0
    }
  }
}
```

`used_bytes`, availability and rates are nullable. Zero means a successful measurement of zero. Missing, unsupported, invalid and failed readings are not zero. Keep process identity internal; bind each sample to the runtime boot ID and actual child's PID/start identity. Never expose prompt text or raw slots data through status.

For local models, measure `ri_phys_footprint` through `proc_pid_rusage` for the actual `llama-server` child. `MacDeps.pid` records that child at `crates/lobo-core/src/local/deps.rs:294`. `LocalState.pid` is the supervisor at `crates/lobo-core/src/local/supervise.rs:211`; it is not the model process. Capture the child start identity and reject PID reuse, exit or identity mismatch. Clear that identity immediately when the child exits.

Apple's [resource usage definitions](https://github.com/apple-oss-distributions/xnu/blob/main/bsd/sys/resource.h) provide physical footprint and process start time. Treat the measured value as that process's OS-accounted footprint. Do not call it total application usage or exact model-weight allocation. Do not add separate Metal or resident-memory counters to it.

Read Mac available memory from the merged memory-guard probe. Keep its existing available-memory estimate and reserve semantics; label the displayed field `Mac available`, not model allowance. Collect it independently, including before the model child exists. Cloud uses measured `nvidia-smi` device memory used/total, labeled `Device VRAM`, not process memory. Cloud collection remains available during loading after GPU access succeeds.

One reusable compact indicator appears in startup and Ready. Local examples: `Model 22.8 GiB · Mac available 7.8 GiB`, `Model not started · Mac available 12.0 GiB`, or `Model unavailable · Mac available 7.8 GiB`. Cloud example: `Device VRAM 25.4 / 31.4 GiB`. Render units as GiB consistently. Only a measured cloud used/total pair may produce a capacity bar; clamp its displayed ratio to 0–100%. Memory growth must never become a loading percentage.

Run the bounded memory/activity display sampler every two seconds, independently of watchdog evaluation. The fast sampler reads only `/slots` and memory; `/metrics` scrapes reset throughput buckets and must keep exactly one existing 30-second reader. The runner owns the fast task, allows one sample in flight and skips missed ticks. Fetch activity and memory independently so one failed source cannot suppress the other. Stop sampling when the runtime exits or cancellation completes. New samples must reach visible native windows on a two-second polling schedule without increasing provider-listing or rental calls. Use a cached active agent connection for this display path; retain normal provider reconciliation separately.

Record actual collection time, not the time the UI fetched status. The agent also reports monotonic sample age, so cloud clock differences cannot make old samples appear fresh. The UI adds elapsed time since receipt. At an effective age above six seconds, show `Stale` for memory and `Unavailable` for activity; remove live rate/progress presentation. If no valid sample exists, show `Unavailable`. A repeated status response cannot refresh a sample's age. Reject samples for a previous boot or decreasing collection sequence; refresh immediately on reopen.

Keep watchdog observations, missing-metrics handling, idle timeout and termination policy on their existing 30-second cadence. Display sampling failures must not increment watchdog failure counts or extend runtime lifetime. Do not add extra watchdog observations from the display loop. Its status writes must not erase fresher two-second display samples. Existing processing/deferred counts already protect busy requests from idle termination; preserve that policy.

## Request activity

Read authenticated `/slots` from the existing runtime's internal endpoint. Parse only request identity, processing state and numeric counters needed by the display. The [pinned llama.cpp server documentation](https://github.com/ggml-org/llama.cpp/blob/b11118/tools/server/README.md#slots) describes slot processing state and the separate metrics gauges. Slots formats vary across runtime versions; use sanitized fixtures for the pinned cloud runtime and supported local versions.

Check `is_processing` first because inactive slots retain old counters. Recognize measured `n_prompt_tokens_processed` and `next_token.n_decoded`; normalize `next_token` as an object or a one-element array. `n_prompt_tokens` is a buffer length, not processed progress. Track boot ID, slot ID and `id_task`. Clear counters when identity changes, a request completes or a counter resets. Do not combine one request's counters with another's rate.

| Evidence | Display |
|---|---|
| Fresh active request with prompt work and zero decoded output | `Reading prompt · 16,384 tokens` using the measured processed count. |
| Fresh active request with decoded output | `Generating`, plus valid measured output count/rate. |
| Fresh valid slots response with no active or queued work | `Idle`; omit live throughput. |
| Active work is known but its phase/counters are unsupported | `Processing request`; never `Idle`. |
| No valid sample, request failure or unsupported response | `Unavailable`, with a bounded reason. |
| Previously valid activity sample older than six seconds | `Unavailable`; do not show its old values as live. |

Queued work is not idle. Display a compact waiting count if the source reports one. If multiple requests are active, show their count and a combined phase only when unambiguous; do not invent a single-request total.

The supported `/slots` response lacks a total prompt-token count. Show measured processed tokens without percentage or ETA. `n_ctx`, prompt characters, buffer length, cached tokens and model context capacity cannot supply that denominator. Diagnostic log percentages in the investigation do not become a UI source.

Calculate live tokens/s only from valid counter deltas and positive monotonic time for the same request, or a documented request-scoped measurement. Reset the rate after identity changes, regressions, long gaps or stale samples. Reject negative, non-finite and overflowing numbers. A first sample has no derived rate. Zero completed-request `/metrics` gauges must not hide fresh slot activity.

Replace the two large rate tiles with a compact activity block so memory, endpoint/key actions and Stop remain visible. The tray must also show reading/generating/processing activity instead of falling back to an idle timer when rates are zero. Prompt content and token IDs are discarded, not logged, persisted or returned to the app. No log parsing or request-body capture is required.

## Native app behavior

Remove `LSUIElement=true` from the Tauri bundle and use regular activation. Keep the tray popup's separate skip-taskbar behavior. Main and settings remain normal titled windows with native corners and dragging. The current reopen handler already shows main at `app/src-tauri/src/lib.rs:92`; preserve and verify it.

Normal launch appears in the Dock and Cmd+Tab. `--background` may suppress initial windows but must not revert the app to accessory activation. Closing main or settings hides that window and keeps the app and tray running. Dock reopen brings main forward. Switching back through Cmd+Tab restores focus to a visible app window; when none is visible, main becomes available through reopen. Do not force a popup on every focus change.

Keep the existing Quit implementation and cleanup ownership. These UI changes must not start, stop or restart a model. Apple documents the agent-app exclusion in its [Launch Services reference](https://developer.apple.com/library/archive/documentation/General/Reference/InfoPlistKeyReference/Articles/LaunchServicesKeys.html#//apple_ref/doc/uid/20001431-108256).

## Same-size local diagnostic — authorized 2026-09-30

The latest user instruction explicitly resumes only a direct real-model comparison. The observed original request had approximately 47,000 input tokens and reached 28,672 processed tokens (61%) before cancellation. Its exact total and output limit are unknown. Use exactly 47,000 synthetic input IDs, Q6, context 65,536 and a 32-token output cap. This proves only whether a synthetic input of this size can finish on the same model and host.

Use the successful hosted macOS artifact from revision `b5617ae` without installing it. Run its normal headless supervisor with isolated private configuration/state and the existing weights/runtime. Both production memory checks remain active. If admission fails, record it and stop. Do not terminate unrelated applications, download weights, rent a GPU, or bypass admission.

Apply the model's chat template to a fixed synthetic message with thinking disabled. Tokenize its template prefix, neutral repeated filler and final question, then send exactly 47,000 token IDs to the native completion endpoint. Use one generation, no retry, a 30-minute deadline and a 32-token output cap. Require exact evaluated input count, no truncation, the expected model/boot identity, and nonempty output. Preserve numerical counters, timing and outcome only. Stop only the task-owned supervisor and verify its child exits.

A successful result permits the next OpenCode investigation. It does not establish full compatibility or justify restarting OpenCode. Native GUI E2E, broad local tests, app installation and release remain deferred.

## Cloud E2E — authorized 2026-09-30

The latest instruction is “local test later. do cloud e2e”. It authorizes RunPod test rentals and the required cloud-client checks. Local inference stays deferred. Test Q6 with context 65,536. Use one GPU at a time and at most two sequential client lifecycles. Only one request uses exactly 47,000 synthetic input tokens with at most 32 output tokens; the other client uses a short bounded smoke request. OpenCode stays closed.

Normal starts must resolve the current public model tag. Read-only checks found no complete candidate image and HTTP 404 for both public latest tags. Finish a complete Q6 image on Dell, verify its model manifest and offline startup requirements, and record its immutable digest. Do not rent while the image cannot be pulled. The earlier public-publication hold remains separate from the newly authorized rental: prepare the actual image before resolving publication/promotion approval. Do not substitute a runtime-only image, private developer bucket or boot-time model download.

Use the actual CLI under a PTY and the verified hosted Mac app artifact. Preserve installed app/CLI and personal provider settings. Keep test config, API key, cloud connection and operation state private and isolated. A shared target preference may be backed up and restored only without concurrent writes; do not add a production feature solely to make this test easier.

Capture the created RunPod ID, boot ID, image digest, hourly rate and creation time. Require those identities before direct inference and cleanup. Record streaming, exact token counts, first-content latency, prompt/generation speeds, agent readiness and visible client state. Delete only the test pod and verify provider absence plus tunnel cleanup. CLI global Down is unsuitable if any unrelated runtime exists. Use the app's scoped Stop or exact owned provider deletion for recovery.

Cloud harness reuses the bounded HTTP and synthetic-token helpers. Its CloudRuntime adapter validates the private saved connection against the expected instance/boot receipt and verifies the remote agent/model/context. It does not inspect local model files. Numeric evidence must identify a cloud runtime, never a local supervisor.

### Candidate publication through repository CI

The saved GitHub CLI token cannot open a GHCR upload session (HTTP403). Use a narrowly scoped manual GitHub Actions workflow with `packages: write`, running on a task-owned ephemeral Dell runner. Mount only the verified OCI candidate read-only; expose no host Docker socket, personal home or old build storage. Pin the official runner image and ORAS release. The runner handles one job and is removed afterward.

Verification is the default. Validate the requested source revision, image digest, OCI content hashes and complete-image configuration before any registry write. Publication and `latest-q6` promotion are separate explicit workflow inputs, both false by default. Publish the immutable source tag first, verify anonymous digest access, then allow the separately approved latest tag promotion. Record the exact resulting digest. Do not create an app/CLI release or modify latest-q8. The existing public package can be reused; do not change unrelated permissions.

## File-level changes

| File path | Change |
|---|---|
| `crates/lobo-core/src/opencode.rs` and module registration | Add JSONC-preserving config discovery, targeted patches, private key/backup writes and atomic replacement. Keep the existing CLI export contract separate. |
| `crates/lobo-core/src/opencode/tests.rs` | Add preservation, idempotence, failure and concurrent-edit fixtures. Never use personal config. |
| `crates/lobo-proto/src/agent.rs` | Add optional versioned memory/activity samples with collection time, age, sequence and boot identity. Preserve older status decoding. |
| `crates/lobo-core/src/local/deps.rs` | Measure the actual child footprint, retain start identity and collect Mac available memory without overloading GPU fields. |
| `crates/lobo-core/src/local/memory.rs` | Reuse the merged native availability probe; preserve startup admission behavior. |
| `crates/lobo-core/src/local/supervise.rs` | Wire the display sampler independently of the existing watchdog interval. |
| `crates/lobo-agent/src/metrics.rs` | Add bounded authenticated slots parsing and typed activity collection. Retain device VRAM collection with explicit scope. |
| `crates/lobo-agent/src/runner.rs` and `crates/lobo-agent/src/pod.rs` | Run independent display sampling during startup/Ready, publish fresh samples and clear identity on exit. Preserve watchdog behavior. |
| `crates/lobo-agent/src/api.rs` | Return sanitized optional telemetry and accurate sample age. Do not expose raw slots. |
| `crates/lobo-core/src/control/status.rs`, `up.rs`, `cleanup.rs` and tests | Add app-targeted lifecycle/status operations with immutable runtime ownership; preserve explicit CLI global operations and unrelated operation/connection records. |
| `crates/lobo-core/src/control/agent_http.rs` and `crates/lobo-core/src/control/mod.rs` | Support direct status sampling through the cached active agent connection without provider discovery per sample. |
| `app/src-tauri/src/backend.rs`, `commands.rs` and `controller.rs` | Add current-instance OpenCode setup, authenticated model verification, operation invalidation and two-second visible telemetry polling. Keep secrets in Rust. |
| `app/src-tauri/src/types.rs`, `store.rs` and `tray.rs` | Carry setup result and sanitized display data; reject stale or mismatched boot samples. Show active work in the tray even when rates are zero. |
| `app/src-tauri/Info.plist`, `src/lib.rs` and `src/windows.rs` | Enable regular app activation, register commands and retain native reopen/tray behavior. |
| `app/ui/src/settings/Settings.svelte` and new `Clients.svelte` | Add compact Clients settings with target path, checkbox, Configure/Repair and bounded status. |
| `app/ui/src/lib/api.ts`, `view.ts`, `fmt.ts` and their tests | Add typed commands and pure memory/activity formatting with honest unknown and stale states. |
| `app/ui/src/panel/BootLog.svelte`, `ReadyCard.svelte`, `Panel.svelte` and new `RuntimeMemory.svelte` | Reuse one compact memory indicator in startup/Ready and show activity without scrolling. |
| `app/ui/src/proto/`, `app/ui/src/gen/` and `app/ui/src/fixtures/` | Regenerate types and add loading, processing, unavailable, stale and old-agent fixtures. |
| `app/e2e/` and existing core/agent test modules | Cover config setup, zero-rate active processing, process identity, protocol compatibility and native app behavior. |
| `README.md`, `CHANGELOG.md` and `docs/implementation-mistakes.md` | Document in-app setup, exact memory meanings and validation limits. Record unfinished acceptance until it passes. |

Cloud CI file-map addition: extend the existing `.github/workflows/pod-image.yml` with a prebuilt-candidate dispatch mode for verification and gated publication; preserve its normal image/release paths. Create `tools/verify_cloud_candidate.py` for the small offline verifier. Keep temporary runner provisioning scripts, credentials and image files outside tracked source.

Cloud diagnostic file-map addition: create `tools/cloud_runtime_e2e.py` with a private saved-connection identity adapter and cloud-specific numeric evidence. Update this feature’s `plan-v1.2.md`, evidence, spec, changelog and project memory. Build artifacts and task credentials stay outside tracked source.

Diagnostic file-map addition: create `tools/same_prompt_runtime_e2e.py`; extend only the transport seams in `tools/bounded_runtime_e2e.py` needed for a larger bounded request and `/apply-template`. Update this spec, `plan-v1.1.md`, `results.md`, `investigation.md`, project memory and `CHANGELOG.md` with measured results and limits.

Implementation file-map detail from the approved plan: create `crates/lobo-core/src/control/app_scope.rs` and `app_scope/tests.rs`, `crates/lobo-core/src/local/process_memory.rs` and its tests, `crates/lobo-proto/src/telemetry.rs`, `crates/lobo-agent/src/telemetry.rs`, `app/e2e/runtime.spec.js`, `tools/bounded_runtime_e2e.py`, and this feature's `results.md`. Modify their module registrations, Cargo manifests/locks, `connection.rs`, local provider identity checks, existing control/connection test modules, `app/src-tauri/examples/generate_ui.rs`, `app/src-tauri/tests/fixtures.rs`, `e2e_memory.rs`, `app/e2e/{fixture.py,wdio.conf.js,app.spec.js}`, `tools/native_app_e2e.py`, `Makefile`, and `.github/workflows/rust.yml`. The workflow adds an explicit dispatch switch to defer native E2E without skipping unit/build checks. These files implement the approved contracts; they add no user-facing scope.

## Tests

Backend, full and lifecycle suites run on Dell or an authorized hosted environment. Native UI fixtures use isolated config/app identities and deny real Start. The user's later instruction permits only bounded real-model feature E2E on this Mac after fixes: two sequential direct scripted requests, each capped at 64 input and 32 output tokens. Do not use OpenCode. Record request success, time to first streamed content, generation time, actual prompt/output token counts and measured tokens/s. Count tokens with the runtime tokenizer before sending; do not infer the input count from character length. Check actual native visuals during loading, generation and Stop. Short prompt processing may finish between UI samples; verify that state with deterministic fixtures instead of inflating the live prompt. Use only an existing model; do not download weights, rent GPUs or bypass memory admission. If the model is absent or Start is denied, record the E2E as blocked rather than relaxing those limits. This exception does not authorize backend suites here.

**OpenCode unit and integration checks:**

- JSON/JSONC with comments, trailing commas, unrelated providers, custom agents, models, plugins and permissions remain intact outside intended edits.
- Q6/Q8 and local/cloud use the actual running alias, endpoint and context. A different selected/saved model cannot override runtime identity.
- Checkbox checked updates model/default agent; unchecked preserves exact prior defaults. New lean agents restrict only themselves; custom agents retain their controls.
- Missing file, JSONC precedence, custom selected path, malformed data, ambiguous keys, unsupported symlinks, read-only target and concurrent modification have defined non-destructive results.
- Authentication failure, missing model, changed boot, rotated key and stop/restart races leave the config unchanged.
- Private backup/key permissions, file substitution, atomic-write failure, recovery and idempotence are exercised with temporary fixtures. Errors and frontend payloads contain no secrets.
- A fixture OpenCode config parser resolves the written file without model calls. Exercise documented override behavior and report it without modifying unrelated settings.

**Target isolation checks:**

- Configure fake failing Vast/RunPod providers beside a valid local runtime. Local Start, status, Stop and final refresh make zero calls to those providers.
- Local Stop preserves coexisting cloud instances, unrelated pending operations and saved cloud connections. Scoped cloud Stop preserves other instances.
- Cover cancellation during startup, already stopped targets, stale PID/boot identities, changed UI selection, repeated Stop and worker cleanup failure.
- Verify clean local shutdown reaches Off without a Vast warning. Do not contact real providers to reproduce the error.

**Memory and activity unit/integration checks:**

- Inject native readings for model child, supervisor mismatch, missing PID, PID reuse, changed start identity, process exit and native probe failure.
- Verify startup visibility before child creation, during model loading and after Ready. Local available memory and cloud device VRAM keep their separate meanings.
- Exercise null versus zero, invalid timestamps, clock skew, collection sequence, six-second staleness, repeated cached responses and boot replacement.
- Use pinned and alternate supported `/slots` fixtures: object/array forms, missing fields, malformed numbers, queued work, unknown phase and multiple requests.
- Reproduce zero prompt/generation gauges with active prompt processing. Show measured processed tokens, no guessed denominator, then generation and confirmed idle.
- Verify rate reset across tasks, slot reuse, counter rollback, gaps and failures. No prompt content, token IDs or raw slots reach status/logs.
- Use a fake clock to prove actual two-second slots/memory sampling, bounded work, no overlapping polls and cancellation. Provider listing cadence must not increase; fast reads must never scrape `/metrics`.
- Prove the independent 30-second watchdog receives the same observations and termination decisions before/after the change.
- New app/old agent and old client/new agent decode safely. Unsupported display data becomes unavailable without breaking Stop or status.
- Local Stop with broken or configured cloud credentials completes without any provider list/delete/auth calls. A target-change race must not stop a different runtime.

**UI and native checks:**

- Render startup, Ready, stale, unavailable, long status messages, Clients and both targets without overflow at the real native window sizes.
- Check successful Configure/Repair, disabled duplicate action, visible checkbox, failure feedback and restart/override text using isolated fixtures.
- Verify the actual `.app` in Dock and Cmd+Tab; test main/settings drag, native corners, close/reopen, tray toggling, keyboard focus and Quit.
- Exercise visible-window two-second updates and reopening after a sample becomes stale. Keep all controls visible without scrolling.
- Hosted macOS may test the native process-memory probe with a harmless task-owned fixture process. This is not model inference or proof of real-model memory attribution.

Deliver focused build and smoke evidence first after implementation is authorized. Keep full regression, failure and compatibility checks pending until completed. Real cloud telemetry acceptance remains explicitly unverified while publication and rental are held.

## Failure modes & decisions

| Failure | Behavior |
|---|---|
| No Ready model, stale identity or failed `/v1/models` authentication | Disable or fail Configure/Repair with no config write. Never guess Q8 or rotate a key. |
| User changes model, target, key or runtime during setup | Invalidate the operation and require retry against fresh state. |
| Invalid config, concurrent edit or unsupported file target | Preserve original bytes; show the file path and bounded actionable error. |
| Key/backup write or final replacement fails | Keep original config and its key references; remove only new unreferenced files. |
| Project, custom or managed config overrides global settings | Report the limitation; do not claim effective session configuration or rewrite those sources. |
| Model child does not exist yet | Show `Model not started` and available Mac memory if measured. |
| Child exited, PID was reused or boot changed | Discard footprint and request counters immediately. |
| Memory or slots source fails independently | Mark that field unavailable; retain unrelated fresh fields. Stop remains usable. |
| Sample age exceeds six seconds | Show `Stale` memory and `Unavailable` activity; stop displaying old values as live. |
| Slots say active while gauges report zero | Show the supported active phase, or `Processing request`; never infer idle from gauges. |
| Total prompt tokens are unknown | Show measured processed tokens without percentage. |
| Old agent or unsupported slots shape | Keep status and lifecycle usable; show unavailable telemetry without automatic runtime upgrades. |
| UI sampling slows or fails | Skip overlapping samples; preserve watchdog cadence and existing termination behavior. |
| Unrelated cloud provider fails during a Local action | Local actions make no cloud calls; they complete from the owned local runtime state. |
| Stopped runtime identity differs from the captured identity | Preserve the replacement runtime and report the changed target; never fall back to global cleanup. |
| Window closes or the app receives reopen | Keep tray/runtime behavior; Dock reopen restores main. |

## Out of scope

- Resuming unrelated image builds or the updater. Completing the memory baseline is an authorized prerequisite for these fixes.
- Public release tags, image publication or GPU rental. Normal branch pushes, CI, local QA installation and direct merges follow the existing full-auto authorization.
- Changes to the independent local memory-admission feature or GitHub updater design.
- Mac inference beyond the bounded two-request E2E exception, broad host process inspection, or user prompt/config capture.
- CLI packaging, global OpenCode policy changes, automatic model selection or performance tuning.

## Rollout

P1 — Create the isolated feature worktree from clean master and merge the existing memory branch (`b9a1966` at drafting) into that feature branch. Preserve its pending documentation. Do not merge unverified memory code to master merely to create a baseline.

P2 — On that combined feature baseline, fix app-owned Start/status/Stop scope and add in-app OpenCode configuration plus regular native activation; one commit gated on clean review.

P3 — Add measured memory, request activity, independent sampling and compact views; one commit gated on clean review and focused remote/native fixture checks.

P4 — After implementation, complete the deferred E2E, failure, compatibility and regression acceptance; document actual limits and update the changelog in one commit gated on clean review.

This is one feature and one implementation plan. The approved work includes implementation, normal CI/direct merges, local QA delivery and bounded scripted E2E. Image builds and the updater remain paused. Public release, image publication and rental remain held. Show usable smoke-tested native results before completing the broader suites; do not claim full acceptance early.

## Execution clarification

After approving this spec, the user said “e2e test later.” Implement fixes and run unit/build checks first. Defer native fixture E2E, OpenCode integration E2E and the bounded model benchmark until the later validation stage. Keep all unfinished acceptance visible. Do not resume the image builds or updater.

A previously launched memory fixture E2E had already ended: five cases passed; the app disconnected during the denied-Start case and the final case could not connect. Cleanup passed and no model started. Preserve the evidence at `bin/app-e2e/lobo-native-e2e-zlqngx7s` in the memory worktree. Investigate the existing logs; do not rerun E2E during implementation.

Implementation detail (ownership batch): extend `crates/lobo-core/src/provider/mod.rs` with default owned-identity operations, overridden by LocalProvider, so PID/boot checks occur at the final signal boundary. Keep the CLI delete API unchanged.

Implementation research correction: OpenCode v1.18.33 supports agent-scoped `tools` wildcard restrictions, but has no built-in `list`. New lean agents allow `read`, `edit`, `write`, `bash`, `glob`, and `grep`; write/edit map to its shared edit permission, including `apply_patch`. Existing agent controls stay unchanged. JSONC parsing must disable loose keys, missing commas, single quotes, hexadecimal numbers, and unary plus. Reject duplicate decoded keys before CST lookup. Preserve unrelated text even when adding properties to compact objects.

New-agent permission detail: set agent-scoped `permission.read` to `{ "*": "allow", "mcp:*": "deny" }` after the lean tools map. OpenCode v1.18.33 resource tools use `read` permission patterns `mcp:<server>:...`; this denies their use while keeping file reads. Some resource tool schemas remain visible to OpenCode. Do not promise zero MCP schema overhead. Preserve all existing custom-agent permissions and never change global MCP settings.

App ownership implementation detail: add Rust-only `PreparedUp` without Debug/Serialize. `Backend::prepare_up(request: UpRequest) -> Result<PreparedUp>` captures config/options and performs blocking admission without provider calls or worker creation. `Backend::up(prepared: PreparedUp, previous: Option<RuntimeTarget>, cancel: CancellationToken, owner: OwnerSink) -> Result<UpOperation>` launches from that captured config after the final generation check. Keep the core provider's independent admission guard. Track reservation/preparation/operation as one owned job so Stop/Quit can cancel before launch without blocking on Active or Store.

Add private `snapshot_owned(provider, captured: Option<RuntimeTarget>) -> Result<(Option<RuntimeTarget>, Snap)>` and `load_owner() -> Result<Option<RuntimeTarget>>`. Keep `snapshot(provider) -> Snap` as a wrapper. Pair owner and snapshot in one result, persist owner before delivery, and discard stale replies. Remove an owner record only after scoped absence and only when its identity still matches. These helpers stay inside Rust and do not change CLI or frontend contracts.

Telemetry source correction: pinned llama.cpp b11118 `/slots` has no queued-request count. Its high-priority slot query can overtake pending work. Set `ActivitySample.queued_requests: Option<u64>` and use null for this adapter; do not infer zero or scrape `/metrics` on the fast path. Add `ActivityState::NoActiveRequest` (`no_active_request`) and display `No active request` when a valid slots response reports no active slots and queue state is unknown. `Idle` is reserved for a future documented source that measures both counts as zero; the current adapter never emits Idle. Keep parse_slots/observe signatures unchanged. Active prompt/generation phases work with unknown queue count. The current shared local/pod flags leave `/slots` enabled by its documented default.

The runner owns sample sequence per two-second batch. It saves independent monotonic observation instants for memory/activity and recomputes sample_age_ms on every status response. ActivityTracker does not own a competing sequence. Local footprint identity uses rusage ri_proc_start_abstime from the same native read as footprint; it is separate from the supervisor ownership start identifier. Identity-probe failure yields unavailable telemetry and must not drop the child exit monitor.

App uncertain-create recovery must verify ownership of a discovered candidate. A single ID absent from the pre-create list is not proof: another client may have created it. Require matching local boot identity or an exact saved cloud connection record for provider/instance/boot. Without proof, keep the unresolved journal, do not delete the candidate, and do not rent again. Current cloud provider Instance data has no boot metadata; app cloud uncertain-create recovery can remain unresolved. Existing CLI adoption behavior remains unchanged.

Cloud app discovery also requires an exact saved provider/instance/boot connection record. A shared legacy domain response cannot prove which provider instance answered. Keep the existing CLI legacy path; app discovery fails safely when ownership cannot be proved.

Latest host constraint: the user reported no available Mac memory. Continue code edits here, but run builds and unit checks remotely. Local app install and native/model E2E stay pending until the host constraint is lifted. Do not infer GPU-rental authorization.

Discovery commit clarification: `snapshot_owned` returns the candidate owner and snapshot without adopting it. Add private Rust-only `Backend::adopt_owner(expected: Option<RuntimeTarget>, target: RuntimeTarget) -> Result<()>` for an atomic on-disk identity comparison and local write. Controller validates captured generations under Active, releases Store, persists the candidate, then applies the paired owner/snapshot. Keep Active only through this short local commit, never across network requests. This prevents a discarded stale discovery reply from changing persistent ownership. Persistence still precedes Store/IPC delivery.

Ownership projection clarification: keep new RuntimeTarget and model-child PID/start fields out of IPC. Preserve existing Snap instance IDs and Status.boot_id; they are existing protocol metadata and boot correlation is needed for telemetry. Do not add generic identifier redaction to legacy progress text. This narrows the plan wording to the spec's internal process measurements and preserves protocol compatibility.

Setup file-map detail: `app/src-tauri/Cargo.toml` may declare the shared reqwest version/features or an HTTP fixture dependency needed for authenticated model discovery. Prefer the existing core HTTP helper where it fits; never call completion-based core checks for setup. This adds no generation request or new user-facing scope.

Native sizing clarification: `windows.rs` provides initial sizes only. Existing `App.svelte` measures content with ResizeObserver and adds the native title-bar inset. Preserve fixed widths (main/panel340, Settings520) and that established height behavior; do not force content into the initial340/540 heights or add scrolling. Keep Clients and telemetry compact within their existing views.


OpenCode metadata contract (Tasks 6–9):

```rust
ConfigRestrictions { provider_disabled: bool, provider_not_enabled: bool }
inspect_restrictions(source: &str, provider: &str) -> Result<ConfigRestrictions>
OpenCodeInfo { path: String, endpoint: Option<String>, provider: Option<String>, model_alias: Option<String>, context: Option<u64>, can_configure: bool, reason: Option<String>, warnings: Vec<String> }
OpenCodeResult { path: String, provider: String, model_alias: String, changed: bool, message: String, warnings: Vec<String> }
```

The shared restriction inspector uses the same strict JSONC parser and validates string arrays. It reports existing provider restrictions without changing them. IPC warnings/reasons are bounded static messages. ConfigOutcome retains its four fields. `opencode_info` still reports the discovered config path when no Ready runtime exists, with configure disabled and a clear reason.

Use a setup-specific bounded HTTP client with redirects disabled. Capture the private raw Lobocode config generation/fingerprint as well as Store generations; masked ConfigShow cannot detect external key rotation. Hold Active and Store through the bounded local configure/replace call, including its validation callback; never hold either across network awaits. Releasing guards after validate but before replacement would allow Stop to race the write. No runtime start or generation request is permitted.

Setup module file-map addition: create `app/src-tauri/src/opencode.rs` (and its `opencode/tests.rs` if useful) for private binding/revision/HTTP helpers. Keep backend wiring, command registration and controller generation gates in their existing files. This avoids adding all parsing/HTTP/revision logic to backend.rs; it adds no new public API beyond the locked metadata. Read `/tmp/lobocode-setup-binding-design.md` as source guidance; the two-flag ConfigRestrictions contract above is authoritative.

Ready shortcut delivery clarification: an existing Settings webview may not have subscribed yet. Retain the requested tab until Settings consumes it after subscribing; events prompt consumption rather than carry the only copy. A small safe native consume-settings-tab command/state is allowed in windows.rs/commands.rs/lib.rs. This preserves unsaved fields and covers rapid ordinary-open followed by Ready shortcut without navigation or window recreation. Add App.svelte to Task9 files for initial tab/event wiring.

Selected-file metadata clarification: `opencode_info(path: Option<String>)` accepts an optional explicit config path. Omitted path discovers the global file; supplied path validates and inspects that selected JSON/JSONC file. Task9 sends the user-selected path on refresh, so an invalid default file cannot block a valid chosen file or produce warnings for the wrong file. Info/result shapes and the other commands remain unchanged. Add the invalid-default/valid-explicit-path case to app units.

Native fixture file-map clarification (Task11): create `app/src-tauri/src/e2e_runtime.rs`, `e2e_trace.rs`, `app/e2e/runtime_fixture.py`, and `trace-service.js` alongside the planned runtime.spec.js. Use validated E2E-only dependency injection in CoreBackend; keep real scoped Stop and real OpenCode authentication/config writing. Deny both prepare_up and up before real dependency construction. Synthetic process identities never reach LocalProvider or local.json. Isolate OpenCode config, state and app preferences under the task root. The HTTP fixture never serves generation.

Split hosted `make app-e2e-build` into an ungated compilation-only step. Keep all Python native launches, including runtime-ui-only, under the existing native_e2e guard. This compiles prepared fixture code remotely without executing E2E. Add E2E-only native/driver exit diagnostics and remove the invalid memory-case invoke replacement. Native drag/corners/Dock/Cmd+Tab remain pending actual acceptance; webview screenshots or plist inspection cannot establish them.
