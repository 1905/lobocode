# Block local startup when Mac memory is insufficient

**Date:** 2026-09-30
**Scope:** /Users/kass/dev/lobocode
**Status:** pending review

## TL;DR

**What:** Check available Mac memory before local Start. Block startup and show required versus available memory when the selected model cannot fit.
**Why:** The current check runs after startup begins and ignores memory used by other apps and the selected context size.
**Your action:** Approve this memory-check spec. The cloud-image work continues under its existing approval.
**Limits:** No inference on this Mac, automatic model changes, memory-limit overrides, or release. Passing the check is an estimate, not a guarantee against later memory pressure.

## Problem(s)

1. The app shows Start for local models without a memory assessment. `app/ui/src/panel/StartCard.svelte:35` renders the button unconditionally. `app/ui/src/panel/LocalStart.svelte:12` reports disk state only. A model on disk is not proof that the Mac can load it.
2. The shared local provider downloads the runtime and launches the supervisor before the current memory check. See `crates/lobo-core/src/local/provider.rs:300` and `:347`. The check in `crates/lobo-core/src/local/deps.rs:87` first runs the runtime's device probe. It needs to reject insufficient memory before these actions.
3. `crates/lobo-core/src/local/platform.rs:24` uses a configured wired limit or 75% of installed RAM. It does not account for current usage. `crates/lobo-proto/src/catalog.rs:55` adds a fixed 2.5 GiB to model size and ignores context size.

## Goals

1. Show local memory eligibility before Start, for the selected model and context. Fixes problem 1.
2. Reject insufficient or unknown memory before runtime download, supervisor launch, model download, or inference. Fixes problem 2.
3. Use fresh physical-memory statistics and Metal's working-set limit. Include context-dependent model memory and explicit reserves. Fixes problem 3.
4. Repeat the check at Start and before loading the model. A previously displayed pass must not bypass a later failure. Fixes problems 1–3.

## Non-goals

- Guarantee future allocations after a passing snapshot.
- Change the selected model, context size, or target without user action.
- Change macOS wired limits, clear caches, terminate other apps, or count swap as RAM.
- Add a force-start bypass or change cloud GPU admission rules.
- Run model inference on this Mac or publish a release.

## Memory assessment

Read-only native calls collect installed RAM, VM page counts, page size and `MTLDevice.recommendedMaxWorkingSetSize`.
Do not invoke the inference runtime, download anything, or read model payloads for this check.
Do not use `os_proc_available_memory`: the installed Apple SDK marks it unavailable on macOS.

Keep the native probe separate from a pure calculation, so Dell tests can inject snapshots.
The snapshot must be internally valid. Missing fields, failed native calls, zero Metal capacity or arithmetic overflow fail closed.

Use this conservative physical-memory estimate:

```text
used = (anonymous pages - purgeable pages + wired pages + compressor pages) * page size
available now = installed RAM - used
system reserve = 4 GiB
budget = min(available now - system reserve, Metal recommended working set)
```

Subtractions saturate at zero. Do not add inactive pages, file cache, speculative pages or purgeable pages again.
In particular, `free_count` already includes speculative pages in Apple's VM statistics.
Compression and swap do not create additional capacity in this calculation.
This is an estimate of reclaimable physical memory, not a promise that all cache pages can be reclaimed immediately.

For the two built-in Qwen3.5-27B variants, calculate:

```text
weights = exact catalog model size
KV cache = padded context tokens * 16 attention layers * 2 caches * 4 KV heads * 256 dimensions * 34 / 32
runtime reserve = 4 GiB
required = weights + KV cache + runtime reserve
can start = required <= budget
```

The KV formula follows the current one-slot, q8_0 K/V, flash-attention runtime arguments.
Use the pinned runtime's context padding in the calculator and document its source beside the constant.
The runtime reserve covers recurrent state, compute buffers and runtime overhead. It is a conservative product policy, not a measured peak.
At 65,536 tokens, the KV allowance is 2.125 GiB. Q6 requires about 26.7 GiB; Q8 requires about 32.8 GiB before the separate system reserve.
Reject unknown model profiles or unsupported context values instead of reusing an unrelated estimate.
Keep this local policy separate from the existing cloud `min_free_mib` policy.

Source evidence:

- [Apple Metal working-set documentation](https://developer.apple.com/documentation/metal/mtldevice/recommendedmaxworkingsetsize): GPU allocation guidance, not current free system memory.
- [Apple VM statistics definitions](https://github.com/apple-oss-distributions/xnu/blob/main/osfmk/mach/vm_statistics.h): anonymous, wired, compressed, purgeable and speculative page semantics.
- [sysinfo's Apple memory accounting](https://github.com/GuillaumeGomez/sysinfo/blob/master/src/unix/apple/system.rs): the used-memory calculation above. Do not use its available-memory value, which includes active anonymous pages.
- [Qwen3.5-27B configuration](https://huggingface.co/Qwen/Qwen3.5-27B/blob/main/config.json): 64 layers, full attention every fourth layer, four KV heads and 256-dimensional heads.
- [Pinned llama.cpp KV cache](https://github.com/ggml-org/llama.cpp/blob/b11118/src/llama-kv-cache.cpp) and `crates/lobo-agent/src/process.rs:39`: cache layout and actual app launch parameters.

These sources support the calculation inputs. They do not establish a measured end-to-end memory peak for this app.

## App and startup behavior

```text
select Local / model / context or refresh visible idle panel
  -> native memory snapshot -> assessment -> inline result

Start / Retry / tray Start
  -> fresh assessment -> reject with error OR proceed
  -> existing local setup and download
  -> fresh assessment immediately before model load
  -> reject with error OR launch inference
```

The app state carries a typed assessment with model, context, total bytes, available bytes, Metal limit, required bytes, budget and verdict.
An unavailable assessment carries a bounded user-facing reason instead of invented zero measurements.

Example display values, rounded only for presentation:

```json
{
  "model": "q8",
  "context": 65536,
  "verdict": "insufficient",
  "message": "Not enough memory for Q8. Needs about 32.8 GiB; 12.0 GiB is available for the model."
}
```

Show a compact inline error in the local start view and disable Start while checking, insufficient, or unavailable.
Keep model and target controls usable. Offer a smaller model only if that model's assessment passes; otherwise suggest closing apps or selecting Cloud.
Cloud remains an explicit user selection. Never rent a GPU automatically.
When the check passes, show “Memory check passed” with the required and available values. Do not claim guaranteed execution.
Refresh visible idle local assessments with the existing polling cycle, and refresh after model or context changes.
Discard stale responses that belong to a previous selection.
Start-time validation stays in the shared backend, so commands, tray actions and CLI cannot bypass the UI.

## File-level changes

| File | Change |
| --- | --- |
| `crates/lobo-core/src/local/memory.rs` (new) | Typed snapshots, pure model/context calculation, native probe and actionable rejection. |
| `crates/lobo-core/src/local/{mod,platform,provider,deps}.rs` | Expose assessment; validate before local side effects and again before inference; remove the weaker duplicate decision. |
| `crates/lobo-core/Cargo.toml`, `Cargo.lock`, `app/src-tauri/Cargo.lock` | Add only the native Metal bindings needed for the read-only query, if not already available. |
| `crates/lobo-core/tests/local_provider.rs`, `crates/lobo-core/src/local/deps/tests.rs` | Inject memory snapshots and prove denied starts do not download, spawn or load. |
| `app/src-tauri/src/{backend,controller,store,types,tray}.rs` and existing tests | Carry assessment, refresh it, reject stale results and preserve backend enforcement. |
| `app/ui/src/gen/*` | Regenerate affected app state types. |
| `app/ui/src/panel/{StartCard,LocalStart}.svelte`, `app/ui/src/lib/{view,view.test}.ts` | Display compact status and error; disable invalid Start without hiding target/model choices. |
| Existing app fixtures and `app/e2e/app.spec.js` | Add insufficient, unknown and passing-memory UI states; validate the blocked path without inference. |
| `README.md`, `CHANGELOG.md`, `AGENTS.md`, `docs/implementation-mistakes.md`, relevant UI assets | Describe the guard, record the late-check defect and keep validation limits explicit. |

## Tests

- Pure calculator on Dell: Q6/Q8; small/default/large context; boundary equality; one byte short; missing probe data; invalid counters; zero/overflow; no swap credit.
- Native-statistics adapter: correct page-size conversion, no speculative double-count and no duplicate purgeable credit. Test via injected data on Dell.
- Shared local provider on Dell: insufficient/unknown snapshots cause zero runtime downloads, zero supervisor launches and zero model downloads.
- Pre-load check on Dell: available memory falls after setup, so inference never launches. Use process fixtures, not a real model.
- App controller/UI: selection changes invalidate the assessment; old results cannot enable Start; tray and Retry cannot bypass the backend; Cloud selection still works.
- Mac UI only: inspect the actual read-only result and all fixture layouts without scrolling. Use an explicitly denied fixture to exercise Start rejection safely. Never click a production Start that could pass.
- Run focused smoke checks first. Then complete affected backend suites and clippy on Dell, plus frontend and native UI checks on the Mac.
- Build and install a new local app/DMG only after approval and checks. Preserve the separately installed CLI and the user's config/model files.

## Failure modes & decisions

| Failure | Behavior |
| --- | --- |
| Model exceeds hardware/Metal capacity | Block. Show model requirement and usable limit. |
| Other apps leave too little memory | Block. Suggest closing apps and recheck; leave Cloud selectable. |
| Snapshot query fails or profile is unknown | Block. Show “Cannot check available memory” or the unsupported-profile reason. |
| Memory falls between displayed pass and Start | Fresh backend check blocks without starting downloads or processes. |
| Memory falls while weights are being prepared | Pre-load check blocks inference and releases task-owned startup state. |
| Memory pressure changes after inference starts | Existing runtime errors and cleanup still apply. This feature does not promise continuous admission control. |
| Calculation is conservative | Reject rather than silently rely on swapping or change system limits. Revisit reserves only with recorded measurements. |

## Out of scope

- Runtime memory monitoring, forced eviction, system tuning, new models or automatic context reduction.
- Public image publication, release workflow execution, Homebrew installation or GPU rental.
- Desktop updater implementation.

## Rollout

P1: Implement and verify the shared memory guard plus compact app UI; commit locally after clean self-review.
P2: Update assets/docs and build/install the local app/DMG; record exact validation limits in the delivery commit.
Public release remains held. The separate approved public-image plan continues independently.
