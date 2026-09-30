# Complete public GPU images

**Date:** 2026-09-30
**Scope:** /Users/kass/dev/lobocode
**Status:** approved

## TL;DR

**What:** Each new cloud start resolves the latest public Q8 or Q6 image and gives its exact digest to the GPU provider.
**Why:** Users must not need a domain, developer bucket, private host or separate agent download. The image contains all required software and weights.
**Your action:** Approved on 2026-09-30. Implementation continues under the existing release hold.
**Limits:** No release, image publication or GPU rental. Backend checks run on Dell. The DMG remains standalone; Homebrew owns the optional CLI.

## Problem(s)

1. Cloud distribution inherited private storage assumptions. The legacy bootstrap downloads an agent release, while the agent previously required a model URL. Removing the domain did not remove these dependencies. Evidence: `crates/lobo-core/src/bootstrap.rs:32`, `crates/lobo-agent/src/config.rs:63`, `docs/implementation-mistakes.md:34`.
2. A configured image can become stale. CLI help still advertises `LOBO_POD_IMAGE` or a release archive, and the image workflow does not publish model-specific latest tags. Every new app start needs a fresh public-registry resolution. Evidence: `crates/lobo-cli/src/cli.rs:169`, `.github/workflows/pod-image.yml:3`.
3. Old storage settings can still reject an otherwise usable configuration. Normal public-app startup must not validate or use obsolete model-source credentials. Evidence: `crates/lobo-core/src/config/mod.rs:273`.
4. Full images contain about 22 GB of Q6 weights or 29 GB of Q8 weights. The existing container deadline is six minutes. Packaging and startup limits must account for these sizes. Evidence: `crates/lobo-proto/catalog.json`, `crates/lobo-core/src/control/mod.rs:64`.

Line references describe the working tree when this spec was written. Some fixes already exist as unfinished changes.

## Goals

1. Give public users a standalone Mac install and cloud setup requiring only their GPU provider key. Generate the app API key automatically. Addresses problems 1 and 3.
2. Package agent, CUDA runtime, SSH daemon and selected weights together. Boot without installing packages or downloading agent/model files. Addresses problem 1.
3. Resolve the current public model tag on every new cloud start. Fail before rental when resolution fails. Addresses problem 2.
4. Verify model contents and retain cancellation, failed-boot cleanup, API authentication and automatic shutdown. Addresses problems 1 and 4.
5. Build and validate both image variants on a host with enough disk. Keep individual registry layers below supported limits. Addresses problem 4.
6. Update app setup, CLI wizard/help, README, assets, project memory and changelog to match the actual behavior. Addresses problems 1–3.

## Non-goals

- No public release, registry publication, branch push or paid GPU rental in this work session.
- No desktop auto-updater implementation. Its separate research remains in `docs/implementation-mistakes.md`.
- No change to the user's installed Homebrew or other CLI binary.
- No model inference or backend/lifecycle test suite on this Mac.
- No forced update or restart of an already running GPU.
- No removal of unrelated Go compatibility fixtures or legacy maintainer release tools.

## As-built notes

- `.github/workflows/release.yml` must depend on the complete image workflow before publishing CLI or DMG assets. Independent tag-triggered workflows could otherwise release the app before its required images exist. Use the same-commit reusable workflow mechanism documented in [GitHub Actions](https://docs.github.com/en/actions/how-tos/reuse-automations/reuse-workflows).
- Cloud `--conns` and debug `--ssh` are also obsolete. Reject them before rental; normal cloud SSH remains automatic.
- Shared dependency cleanup also touches `app/src-tauri/src/backend.rs` and `crates/lobo-cli/src/cmd/up.rs`. Neither normal path initializes private storage signing.
- TUI progress and app fixtures distinguish image verification from model downloads. Frozen Go fixtures remain unchanged; Rust expectations document the intentional behavior changes.
- Full image builds require 200 GiB free Docker storage per concurrent CI job. Standard hosted runners do not satisfy this prerequisite; configure `POD_IMAGE_RUNNER` before release.

## Image selection

The public repository is `ghcr.io/1905/lobocode`. Normal starts select `latest-q8` or `latest-q6` from the chosen model.

```text
Start
  -> check configuration and existing instances
  -> resolve selected model's current public tag
  -> verify registry manifest digest
  -> rent with ghcr.io/1905/lobocode@sha256:<digest>
  -> provider pulls that exact image
  -> image starts agent and SSH
  -> Mac opens its authenticated private connection
```

Query the registry again for each new start operation. Do not select an image from the installed DMG version or `LOBO_POD_IMAGE`.
Do not retain a last-known image as a fallback. If resolution fails, show the error before any provider create request.

Resolve once for an operation. Retries within that operation use the same digest. A later Start resolves again.
The provider may reuse layers whose content matches that digest. Reusing identical layers does not select an older image.

An explicit CLI development `--image` override remains separate from normal app starts. It must reference a complete image.
Reject obsolete cloud `--release` and `--source` requests with clear migration errors. Hide or correct their old help text.

## Image contents and boot

Build one Linux AMD64 image per model. Include:

- The Rust `lobo-agent` and its release metadata.
- The pinned llama.cpp CUDA runtime.
- OpenSSH server and the existing restricted forwarding configuration.
- The selected model, split into native GGUF shards.
- A manifest containing model identity, source hash and each shard's size and hash.
- Model attribution and license text.
- The existing tunnel binary only for compatibility with explicitly configured legacy Cloudflare connections.

Download upstream weights only during the image build. Check the catalog size and SHA-256 before splitting.
Use llama.cpp's GGUF split tool. Put shards in separate layers below GHCR's documented 10 GB per-layer limit.
Use shards below 5 GB to leave margin. Do not merge the shards at startup; load through the first shard.

Manifest shape:

```json
{
  "model": "q8",
  "source_sha256": "<catalog SHA-256>",
  "shards": [
    {"file": "model-00001-of-00008.gguf", "size": 4000000000, "sha256": "<shard SHA-256>"}
  ]
}
```

The example shows one entry. The actual manifest contains every shard with its measured size and hash.

At boot, validate the selected model, manifest and every shard. Reject missing files, symlinks, wrong sizes and wrong hashes.
Respect cancellation while reading weights. Report verification progress without describing it as a network model download.
Missing binaries or invalid weights must fail through the owned cleanup path. Never substitute a network download.

Retain per-instance client/server keys, pinned server identity and API authentication. Provider credentials remain the user's credentials.
Only build-time registry publication needs maintainer credentials. Public image pulls require no maintainer account access.
New GHCR packages default to private. Before release acceptance, the maintainer
must set the container package visibility to public. The workflow's anonymous
manifest check rejects a private package. This setup is held with publication;
it is not an end-user configuration step.

Allow up to 40 minutes for startup by default, including up to 30 minutes before the agent becomes reachable during image pull.
Retain explicit CLI timeout overrides and the maximum-lifetime limit. Timeout or cancellation must delete owned instances and report cleanup failures.

## Setup and compatibility

Normal Cloud Settings and the CLI wizard ask for provider credentials and ordinary runtime settings. They show no bucket, image or model-source field.
Existing obsolete storage settings may remain saved, but must not influence normal startup or make it invalid.
Legacy maintainer release commands validate their own storage settings when explicitly invoked.

The Mac app calls the shared core directly. The DMG contains no CLI and requires no Homebrew installation.
A local app update preserves configuration and model files. Do not install or replace a CLI binary.
All native views must fit without scrolling. Verify actual window dragging, rounded corners and visible controls.

## File-level changes

| Files | Change |
| --- | --- |
| `crates/lobo-core/src/images.rs`, `src/lib.rs` | Add anonymous registry resolution and digest verification. No cached fallback. |
| `crates/lobo-core/src/control/{mod,up,wiring,testkit}.rs` | Inject image resolution, replace normal bucket lookup and adjust image-pull/startup deadlines. |
| `crates/lobo-core/src/config/{mod,readiness,show}.rs` | Remove obsolete storage requirements from normal startup and readiness. Keep explicit legacy-tool validation. |
| `crates/lobo-core/src/provider/{mod,runpod,vast}.rs`, `src/bootstrap.rs` | Pass the selected image digest and boot only the image's entrypoint. Omit private download settings. |
| `crates/lobo-agent/src/{image_model,config,pod,main,lib}.rs` | Validate bundled weights, start the bundled runtime and add a network-free image-content check. |
| `docker/pod/{Dockerfile,build_model.py,start.sh}` | Build complete images, verify upstream bytes, split model layers and retain failure cleanup. |
| `.github/workflows/pod-image.yml`, `.dockerignore` | Build both variants with sufficient disk; publish immutable model tags and promote latest tags only after both accepted builds. Exclude unrelated build output. |
| `app/ui/src/{lib/settings.ts,panel/SetupCard.svelte,settings/Settings.svelte}`, `app/e2e/app.spec.js` | Remove obsolete setup fields and check compact native layouts. |
| `crates/lobo-cli/src/{cli.rs,wizard/flow.rs,wizard/state.rs}` | Update setup and help for complete public images. |
| Affected tests under `crates/lobo-core`, `crates/lobo-agent`, `crates/lobo-cli`, and app fixtures | Cover changed contracts. Preserve frozen Go fixtures; document intentional Rust differences. |
| `README.md`, `CHANGELOG.md`, `AGENTS.md`, `docs/cloud-without-domain.md`, `docs/implementation-mistakes.md`, existing UI assets | Describe actual behavior and keep unpublished/live-test limits explicit. |

## Tests

### Registry and startup — Dell

- Serve tag A, then tag B. Two new starts must send different resolved digests to the provider.
- Return a registry error after a successful lookup. Verify no stale image and no provider create request.
- Reject malformed manifests, wrong digests and unsupported model names before rental.
- Verify old bucket, source and configured-image values cannot affect normal public-image startup.
- Check Q6/Q8 selection, explicit CLI override and obsolete-flag errors.
- Verify both provider requests use the exact digest and omit private download credentials.
- Check image-pull timeout, cancellation and failed-boot deletion with fixtures. No paid GPU rental.

### Bundled model and container — Dell

- Verify good shards, missing/corrupt shards, invalid paths, model mismatch and cancellation.
- Run the baked `check-image` command without network access or a GPU.
- Exercise entrypoint failure cleanup with fake provider APIs, including correctly quoted RunPod requests.
- Build both complete images, inspect layer sizes and record image digests. Use Dell storage with enough free space.
- Run focused tests and required workspace checks. Repeat only for changed code or failures.

### UI and packaging — Mac

- Check frontend tests, generated fixtures and production app build without model inference.
- Inspect the actual native app with fresh setup. Verify dragging, rounded corners and all controls without scrolling.
- Check Settings and TUI layouts after removing storage fields. Run non-UI CLI tests on Dell.
- Verify the local DMG contains only the standalone app and that the installed CLI is unchanged.

### Release acceptance — held

- Verify anonymous pulls of both published images.
- Start Q6 and Q8 on RunPod and Vast. Verify inference, streaming, tool calls, reconnect and lifecycle cleanup.
- Verify a published latest-tag change affects the next Start and a registry outage fails before rental.
- Publish compatible images before a public DMG/Homebrew release uses them.

These checks require later release and rental authorization. Local fixtures do not establish live-provider acceptance.

## Failure modes & decisions

| Failure | Behavior |
| --- | --- |
| Registry unavailable, denied or invalid | Fail before renting. No cached image fallback. |
| Model tag changes after resolution | Start the resolved digest. The next operation resolves again. |
| Provider has an older cached image | Request the new digest. Do not send only a mutable tag. |
| Model content or required binary missing | Fail boot and delete the owned instance. No boot-time download fallback. |
| Long image pull | Wait within the explicit startup budget. Cancel or timeout triggers owned cleanup. |
| Delete request fails | Return a cleanup error and retain recovery state. Do not claim the instance is stopped. |
| Old private-source configuration remains | Ignore it for normal startup. Do not read or forward its credentials. |
| One image build fails | Do not promote latest tags. Keep the prior public release available. |
| Latest image breaks the protocol | Publish a compatible corrective image. Keep protocol compatibility as a release requirement. |
| User has an older DMG | Resolve latest image independently of the DMG version. Preserve the existing agent protocol. |

## Out of scope

- Publishing images, tags, DMGs or Homebrew formulas during the current release hold.
- Paid GPU acceptance before authorization.
- Desktop updater, Apple signing identity setup or notarization account changes.
- Inference and backend tests on this Mac.
- A repository-wide migration of old plan files.

## Rollout

P1: Finish image packaging, latest resolution and focused Dell checks in one reviewed local implementation commit.
P2: Finish native setup, documentation, assets and local app/DMG verification in one reviewed local delivery commit.
P3: After the hold is lifted, verify published images and live providers before a public app/CLI release.

The earlier loose plan remains at `../2026-09-30-public-pod-images.md`. It is not silently moved or deleted.
After spec approval, create the versioned implementation plan in this directory.
Existing uncommitted implementation is preserved. The user approved this spec on 2026-09-30.

Sources: [Docker multi-stage builds](https://docs.docker.com/build/building/multi-stage/), [GHCR limits and public pulls](https://docs.github.com/en/packages/working-with-a-github-packages-registry/working-with-the-container-registry), [pinned CUDA image](https://github.com/ggml-org/llama.cpp/blob/b11118/.devops/cuda.Dockerfile), [GGUF split](https://github.com/ggml-org/llama.cpp/blob/b11118/tools/gguf-split/README.md), [model license](https://huggingface.co/HauhauCS/Qwen3.5-27B-Uncensored-HauhauCS-Aggressive).
