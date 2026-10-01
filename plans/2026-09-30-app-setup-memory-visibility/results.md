# Implementation and validation results

Status: implementation in progress. Native E2E remains deferred. The separately authorized 47,000-token diagnostic was blocked by actual memory admission before model launch. Builds and unit checks remain remote.

Task20 source is prepared: two sequential fixed prompts, exact tokenizer arrays, hard token/request limits, owned existing-runtime checks and numeric-only private evidence. It starts nothing. Parent source review corrected a FIFO-read risk, fake identity-change case and a fragile privacy assertion; the child process receives a minimal environment. The original and same-size script syntax checks and HTTP fake self-tests now pass on Dell. No real generation request has run. An EOS-only result can validate protocol/counts while `content_seen=false`; it does not prove a visible reply. Acceptance remains pending in Task21.

- Base: master `ddd1d6a`; memory implementation `b9a1966`; docs `597fb1c`; isolated merge `e84e56f`.
- Earlier native memory run passed five cases, then its app exited normally at forced-denied Start. The WebDriver connection failed. The initiating exit cause remains unknown; this is not passing acceptance.
- Memory review found a queued-Start selection race. The approved feature plan includes its fix.
- Baselines pass: UI 11 tests, type check with zero warnings, Vite build; Dell core control module 52 tests. No new inference or provider requests.
- Public release, GPU images and updater remain held or paused. CLI, config and models are preserved.

## Deferred native fixture repair

Read-only diagnosis found that case 6 assigns a replacement to Tauri 2.12.0's non-writable, non-configurable `__TAURI_INTERNALS__.invoke`. The replacement cannot install, so its error variable stays null even after denied Start. Replace this interception with observed Failed state and a direct invoke/try-catch assertion.

The denial notification arrived at 09:54:32.073Z. The task app PID 46560 exited with status 0 at 09:54:35.566Z; WebDriver disconnected at 09:54:35.688Z. No crash report or recorded initiating exit command exists. Add test-only quit/ExitRequested and child-exit tracing before the deferred rerun. Do not report the normal exit as a crash or infer user action.

Cloud recovery limit: provider Instance metadata has no boot identity. After an uncertain create, the app must retain unresolved ownership unless an exact local boot or saved cloud connection proves the candidate. It must not delete an unproven instance or rent again. Live cloud acceptance remains held.

## Hosted baseline fb6e0bf

Run `36702647743` uses `native_e2e=false`. Protocol/Linux checks and static agent build pass. App lint, tests, fixture generation and native bundle pass; the native E2E step is skipped. Core macOS: 226 tests passed, one ignored, one failed. `native_snapshot_reads_without_loading_a_model` unconditionally unwraps the probe; the runner returns `Mac memory measurement unavailable: Metal device unavailable`. Production startup refusal is correct. Commit `fb0489b` independently checks Metal availability and requires that exact error when no device exists. Hosted retest is pending.

## Core ownership batch

Commits `d4b5c6c` and `ee60fbf` implement Tasks 1–4. The core app APIs use immutable runtime identity for discovery, status, sampling, Start and Stop. They preserve foreign pending records and saved connections. Local actions do not call cloud providers. CLI behavior is unchanged.

Dell validation passes: 74 control tests, 19 local-provider tests, one scoped connection fixture and Clippy with warnings denied. Final review corrections pass 24 app-scope tests. The native app build check passes on this Mac; no backend tests ran here.

Parent review found and corrected an unchecked adjacent saved port and unproven cloud discovery through a shared domain. Invalid ports preserve saved bytes; a domain reporting another runtime cannot establish instance ownership. Parent compliance and quality review pass for this batch. App wiring is next; E2E and final acceptance remain pending.

## Hosted ownership checkpoint 9ea8f6a

Run `36704616125` completed. The app, core-macos and static agent jobs pass. The missing-Metal memory-test correction is verified on hosted macOS. Native E2E remains disabled.

Linux lint passes, but the core library suite reports 249 passed, one ignored and one failed. `local::deps::tests::check_gpu_table` fails while executing a generated fixture with `ExecutableFileBusy` / `Text file busy`. This fixture failure is under investigation. It is not an inference run or evidence that the memory guard failed. The full workflow is not green yet.

The test helper now writes its executable through a child shell with literal positional arguments, then waits for exit before execution. This removes writable fixture descriptors from the shared test process. Concurrent fork inheritance is the likely failure mechanism; the hosted run did not capture descriptor traces. No production retries or sleeps were added. Dell `cargo test --locked -p lobo-core --lib` passes 250 tests with one opt-in test ignored. Hosted Linux retest remains pending.

## App ownership checkpoint 5d3817e

Task 5 source and regression tests are committed. Start captures the request and reserves its worker before memory admission. The app persists runtime identity privately, guards discovery before adoption, and uses the same owned target through Stop. Parent source review corrected stale discovery persistence and stale Ready status after Stop.

Rust formatting and whitespace checks pass. The new app tests have not run yet; compilation and tests will run on hosted macOS. No local build or test started after the user reported unavailable Mac memory. OpenCode repair, Dock behavior and telemetry remain in progress. No install, E2E or public release occurred.

Run `36706410556` at `70855c3` was cancelled after a fixture correction. Replacement run `36706552790` at `b79c973` passes Linux protocol/workspace checks, core-macos and static agent. App compilation fails with E0282 in the admission result. Commit `d26d5c2` adds the missing explicit result type; its hosted retest is pending. It also preserves legacy snapshot IDs and boot metadata while keeping the new ownership fields private. E2E remains disabled.

Run `36707086020` at `ff959e8` found app lint failures: public helpers exposed private StartSubmission, and the test preparation gate needed a type alias. Commit `aad828b` corrects both. Source formatting/whitespace pass; hosted app tests still await a passing build. No local compilation ran.

Run `36707493061` at `aad828b` passes Linux/core-macos/static-agent and app lint. App units: 52 pass, one fails. The old denied-Start assertion expected no display sample; the asynchronous worker now refreshes after admission failure. Its fake still returned sufficient memory, independently of its forced admission denial. The fixture now changes the displayed sample to insufficient before denial and waits for that fresh sample. It still requires zero runtime launches/rentals and two independent rejected admission attempts. Production admission logic is unchanged; hosted retest is pending.

Hosted run `36708630935` at `ca76f55`: app lint, Rust/UI unit tests and generated fixture/TypeScript checks pass. The corrected denied-Start fixture passes. Core macOS, Linux protocol/workspace and static-agent jobs pass. Native bundle is still building; native E2E remains disabled. Task 5 source review and unit acceptance are complete.

## Core OpenCode repair checkpoint

Commit `739ca5b` completes Tasks 6–7. Parent source review passes strict parsing, leaf edits, unrelated-byte preservation, private key/backup files, no-op validation, rotation, and final fingerprint checks. Dell: 15 OpenCode fixtures, six unchanged CLI export fixtures and Clippy pass. Both lockfiles add only jsonc-parser 0.33.2. The final fingerprint check is optimistic; it cannot synchronize an external writer after that check. App authentication and controls remain in progress. No personal config was read or changed.

Run `36708630935` completed successfully at `ca76f55`, including the native bundle and bundle verification. The native E2E step was skipped. The core OpenCode checkpoint `86bdb68` is pushed for its own hosted checks; app setup wiring is in progress.

Run `36709440347` completed successfully at `86bdb68`. The new core OpenCode writer passes both Linux and macOS CI, and the app bundle passes. Native E2E was disabled. This verifies the core checkpoint, not the app setup commands still being implemented.

## App OpenCode setup checkpoint

Commit `bab6a73` adds Task8 commands and regression fixtures. Parent source review covers captured runtime/key validation, bounded no-redirect model discovery, final commit guards and selected-file metadata. Corrections keep commit guards alive across panic handling and parse the bounded captured config bytes instead of reopening the file. Owner reads are also bounded. App compilation and new units are pending hosted macOS. Task9 frontend and Task10 native activation are in progress; no personal config, installed app or runtime was changed.

Run `36712619930` at `18c010e`: Linux protocol/workspace, core-macos and static-agent jobs pass. App Rust Clippy and Svelte checks pass, with zero Svelte errors/warnings. Prettier rejects `app/ui/src/lib/api.ts`; the app unit/build steps did not run. The Clients delivery will include the formatting correction and a hosted rerun. Native E2E remains disabled.

## Clients and native activation checkpoint

Commit `fe54faf` implements Tasks9–10. Parent source review passes path/checkbox retention, runtime-change invalidation, duplicate prevention, safe errors and durable Settings-tab delivery. Review corrections retain useful sanitized authentication errors and preserve pending success when the user changes tabs. The native app uses Regular activation and no longer declares LSUIElement. Actual Dock, Command-Tab, drag, corner and picker acceptance remains deferred.

Dell UI checks pass: 19 tests, zero Svelte errors/warnings, Vite build with 189 modules, and the full UI formatting check. These checks include the Task8 API formatting correction. No Node process, build, app launch or test ran on the memory-constrained Mac. Hosted native compilation, app tests and bundle verification are next. README now describes the app-owned setup flow and marks the old screenshots and pending native acceptance.

## Hosted app artifact b5617ae

Run `36713757006` passed all jobs at `b5617ae6e4a44f7deada77dbeeff4f60f68c4aa1`: protocol/Linux, core-macos, agent-musl and app. App lint/tests, generated TypeScript/fixtures, bundle and signature checks pass. Native E2E was disabled. Artifact `11095757309` was downloaded into a private temporary directory; local `codesign --verify --deep --strict` passes. The actual bundle declares neither `LSUIElement` nor `LSBackgroundOnly`. The installed app remains unchanged.

## Direct same-size diagnostic — blocked before inference

The user requested a direct comparison without OpenCode. Plan v1.1 fixes one Q6 request at exactly 47,000 synthetic input tokens, context 65,536 and at most 32 output tokens. The script uses the model template and native completion endpoint, with a 30-minute deadline and no retry. This is a size comparison, not a replay of the private original request.

Dell validation: both Python files compile; the original HTTP fake suite and 12 new diagnostic scenarios pass. These cover exact input construction, large request bounds, changed model/context/identity, missing content, wrong counts, cached input, truncation, authentication, answer-match reporting and no retries. Parent source review passes.

The actual hosted app binary was run only as its normal headless supervisor, using isolated private config/state, a task-only key, existing Q6 weights and existing llama.cpp b11118. Production memory admission remained active. No GUI was launched or installed.

| Measurement | Result |
|---|---:|
| Physical Mac memory | 64 GiB |
| Available before startup | 13.36 GiB |
| Guard-required model/context budget | 26.7 GiB |
| Guard-available budget after reserves | 9.4 GiB |
| Model loads | 0 |
| Generation requests | 0 |
| Observed stages | tunnel → gpu → failed |
| Supervisor exit | 0 |
| Stop errors | 0 |
| Task processes remaining | 0 |
| Task state file removed | yes |
| Available after cleanup | 13.47 GiB |

The memory guard blocked startup correctly on the real Mac. This does **not** verify local generation, throughput or peak memory. The model-versus-OpenCode comparison remains pending, so the conditional OpenCode investigation has not resumed. At this snapshot, approximately 17.3 GiB more available memory is needed for the same configuration. Personal settings, installed app, Homebrew CLI and weights are unchanged.

## Cloud E2E preflight — 2026-09-30

The user explicitly requested cloud E2E for TUI and Mac, leaving local inference for later. Plan v1.4 is current. Direct authenticated RunPod REST v1/v2 and GraphQL inventory pass with zero pods. An initial urllib request returned 403; direct requests did not reproduce it. RunPod's live GPU catalog lists RTX 5090 Secure Cloud at USD 0.99/hour, availability LOW. No rental has started.

Anonymous `latest-q6` and `latest-q8` manifest requests return 404. The existing legacy `sha-914ac19` image is a runtime-only image without weights or `/lobo/start`; it is not a valid replacement. An isolated Q6 build is being prepared on Dell from committed `528322e`, using the existing model copied read-only from the Mac and independently hashed on Dell. Old paused Q8 workers/drivers remain preserved.

Real Mac cloud preflight: launched hosted production artifact `b5617ae` with task-only provider config and state. Used Computer Use to select Cloud/Q6 and press Start. Observed Boot then Fail with `cannot resolve latest q6 GPU image: HTTP 404 Not Found; no GPU was rented`. No scrolling; controls and full primary error fit. Provider inventory remained empty; no app owner or tunnel state was created. Quit the task app through its visible control and restored the unchanged original Local target preference. Installed app and personal provider config were preserved. Screenshot is retained in the private cloud-E2E task directory.

Real TUI preflight: compiled CLI `528322e468deeab23f8c0b74e804510d98cca071` on Dell, one job, 1 CPU / 2 GiB, 55.06 seconds. Binary SHA256 `d5091722248f3cba8f8fc549ed425098c90a20d09e666eb9e4cd1f103e1f0ea6`. Ran cloud Up under an 80×24 PTY with isolated config/state. The actual TUI displayed the same missing-image error and exited 1. This verifies failure handling, not successful cloud provisioning or inference.

Cloud harness source is prepared. Parent review and Dell Python compilation/fake checks pass: inherited bounded HTTP checks, 12 same-size scenarios, cloud state/owner/expiry/privacy cases, exactly one request and changed remote boot rejection. No live completion request has run.

The GitHub CLI token has `gist, read:org, repo` scopes. A zero-byte GHCR upload-session probe returned 403 `DENIED`; no upload session or image was created. A repository CI publication path is needed. Public image promotion is still held until the concrete candidate is ready and publication scope is resolved.

The existing `Pod image` workflow now has a prepared candidate mode. Its default verifies a read-only OCI export without publishing. The verifier checks blob hashes, graph references, source/model labels and Linux AMD64 runtime configuration. Dell validation passed: 18 fake cases, Python compilation, actionlint and shell syntax. Actual candidate verification and CI execution remain pending. Optional publication uses pinned ORAS 1.3.4. Existing conflicting tags cause failure; the registry offers no atomic compare-and-swap guarantee against an unrelated simultaneous publisher.

Cloud preparation was committed and pushed as `bfeb9e4a406e077598845c4c2e6d0ceb5194295d`. The candidate image uses source `528322e468deeab23f8c0b74e804510d98cca071`. Task-only small-request and tunnel-cleanup scripts passed four smoke scenarios and three cleanup scenarios on Dell. All transport and lifecycle effects were mocked. These checks do not establish live inference or successful client Stop.

Image preparation checkpoint at 13:53 UTC: the existing 22,082,528,352-byte Q6 model was copied to Dell. Its SHA-256 matched the catalog in 249 seconds, then matched again inside the network-disabled model build stage. The build is writing six GGUF shards. CUDA base layers were reused through verified read-only copies from the paused builder. Original Q8 containers and drivers remain paused. Candidate export, offline acceptance and verification-only CI are still pending; no GPU has been rented.

A second source review found that OCI index platform declarations were not compared with the runnable image config. The verifier now rejects mismatches throughout the descriptor path while retaining plain manifests and unknown-platform attestations. All 23 focused fixtures, Python compilation and actionlint pass on Dell. This correction changes verification only; the candidate agent source remains `528322e`.

## Complete Q6 candidate — built and checked, not published

The complete Linux AMD64 image built successfully from `528322e468deeab23f8c0b74e804510d98cca071`. Its digest is `sha256:83db6998106ca53b67b2bcec9cba445f91924f942b664958eebb742a8539a2d5`. The local OCI export contains 24,691,613,469 bytes. Build and export took 4,116 seconds on the bounded Dell worker.

The image contains the Rust agent, llama.cpp CUDA runtime, SSH server, startup script, six verified Q6 shards, model manifest and licenses. The largest stored layer is 3,993,657,861 bytes. The built-in `check-image --model q6` passed without networking in 128.7 seconds. A separate 236-second scan verified every OCI blob, all shard hashes, required files, source labels, embedded release metadata, Linux AMD64 platform, entrypoint and absence of shared SSH host keys. These are image checks; no GPU inference ran.

Provenance limit: the unchanged production Dockerfile writes `built_by=github-actions` into `release.json`. This candidate was built on Dell. Its receipts record the actual builder; that inherited field does not establish a GitHub-hosted build.

Evidence is under `/storage/lobocode-cloud-e2e-q6-528322e-20260930/artifacts` on Dell: `q6-oci/`, `q6-metadata.json`, `q6-image-evidence.json`, `q6-build.log` and `q6-validation.log`. The exported files are read-only. Original Q8 work remains paused. The temporary limit on `lobo-public-image-check` was restored to its original 2 CPUs / 4 GiB after confirming it was idle.

The exported image reports CUDA 12.8.1. Its exact `NVIDIA_REQUIRE_CUDA` constraint is preserved in the evidence. NVIDIA documents compatibility between newer drivers and older CUDA toolkits in its [CUDA compatibility guide](https://docs.nvidia.com/deploy/cuda-compatibility/latest/why-cuda-compatibility.html). Actual RunPod host compatibility and inference remain unverified.

Verification-only [CI run 36730862442](https://github.com/1905/lobocode/actions/runs/36730862442) passed at workflow revision `505c255441e57f832ccc0b802a21ebb1d1b78f29`. Input validation, checkout and every-blob/image-identity verification passed. ORAS installation and publication were skipped. Normal image and promotion jobs were also skipped. Independent anonymous requests still return HTTP 404 for `latest-q6` and `sha-528322e468de-q6`.

CI checked 29 blobs totaling 24,691,612,965 bytes in 136 seconds. Runner `21` deregistered automatically, its container exited 0 and was removed, and the temporary artifact-directory ACL was restored. The new build worker is stopped with cache and candidate preserved. Small review receipts are copied to `bin/cloud-e2e-20260930/`; private CI evidence is in `/tmp/lobocode-cloud-ci-6b028f/result.json` on the Mac.

The image is not published, and no GPU has been rented. Public image publication/promotion still requires resolving the earlier release hold before normal TUI and Mac cloud starts can run. Successful cloud inference, throughput, Ready/Stop and full native acceptance remain pending.

## RunPod E2E execution — 2026-10-01

The user explicitly approved image publication and RunPod E2E. The existing Q6 candidate remains unchanged. Fresh account inventory returns HTTP 200 with zero pods. The current GPU catalog reports RTX 5090 with 32 GB, low stock, USD 0.99/hour Secure Cloud and USD 0.69/hour Community Cloud. Actual rental price will be recorded from the created pod. The hosted Mac artifact still passes strict signature verification. Publication and real-client acceptance are in progress; no inference result is claimed yet.

Publication run `36799985478` reverified the candidate at `8ff808e` and started ORAS upload. Most base layers already exist in the public registry. The model upload measured about 0.8–1.6 MB/s. ORAS v1.3.4 uses a monolithic PUT for each 3.7–4 GB model layer, which is incompatible with GitHub's documented 10-minute upload limit at that speed. Increasing the job timeout alone is not a sufficient fix. Plan v1.5 changes only the uploader to checksum-pinned regctl with 32 MiB chunks and three concurrent requests. Incomplete layers cannot be assumed resumable across job restarts; completed blobs can be reused. The candidate image digest remains unchanged.

The task-only exact-pod provider helper passed seven mocked checks on Dell: capture/status/delete, refusal for changed image/boot/creation time, one DELETE followed by polling after an uncertain response, and refusal when config or desired state changes. No real provider mutation occurred. Both isolated client configs and free loopback port pairs are ready.

Startup research: the bundled entrypoint matches the normal RunPod payload. The image has CUDA 12.8.1; pinned llama.cpp b11118 includes RTX 5090 architecture support. Actual host driver compatibility and extracted disk footprint remain live checks. RunPod Pod image caching can improve later starts on a cached host, but host selection does not guarantee reuse. FlashBoot is a separate Serverless endpoint feature, not implemented in the current Pod backend. Sources: [RunPod Pod caching guidance](https://www.runpod.io/articles/guides/docker-setup-pytorch-cuda-12-8-python-3-11), [Serverless FlashBoot](https://docs.runpod.io/serverless/endpoints/endpoint-configurations#flashboot), [GHCR limits](https://docs.github.com/en/packages/working-with-a-github-packages-registry/working-with-the-container-registry), [regctl chunk settings](https://regclient.org/cli/regctl/registry/set/).

Candidate-publisher focused validation passes on Dell: regctl v0.11.6 binary SHA-256 matches its release metadata; exact flags are supported; a network-disabled UID1001 check reads the real readonly candidate at its expected digest; a small OCI-copy fixture preserves its digest. Actionlint 1.7.12, shell syntax for all eight workflow script blocks and whitespace checks pass. Live GHCR chunked upload remains pending.

Actual Mac cloud UI preflight on 2026-10-01: hosted artifact b5617ae shows Cloud/RunPod/Q6, idle10m/max1h and Off. All visible controls fit without scrolling; native window corners are rounded. No Start was pressed and no runtime state was created. The task app exited through its visible Quit control. Original preferences were restored only after matching the task-written bytes. Screenshot: `/tmp/lobo-cloud-mac-e2e-20261001.a542eX/mac-cloud-off.png`. This does not establish native movement, app switcher behavior, cloud Ready or inference.

Publication transport correction is committed and pushed as `7dc04ccc45193fb32850052c4a9f7b0d61a5348f`. Previous run `36799985478` was cancelled; runner22 deregistered, its container was removed, and candidate-directory ACL restored. Its logs remain private. Replacement run `36801249737` uses runner23 and the same source/image digests. It is performing fresh verification before chunked publication. No public tag or GPU rental is established at this checkpoint.

Chunked publication run `36801249737` failed with HTTP416 from GHCR, 23 seconds into publication. Verification and pinned-tool installation passed. The logs do not contain Range headers or establish which PATCH failed; runner traffic totaled18.5MB. No manifest or tag was published. Runner23 was removed and the temporary candidate ACL restored. Regctl chunking is not a verified GHCR fix on this path. Further retries are held during source research.

Plan v1.6 moves registry publication to a dependent hosted job through a temporary CI artifact. Official pinned client source confirms8MiB block uploads, bounded retries, ZIP64 and streaming extraction without a second full archive on disk. Candidate files total24,691,613,469bytes (22.996GiB), about2,944blocks. Hosted download requires30GiB free and is followed by a full OCI verification. Only public image files and metadata are staged; no logs, config or credentials. Backend quota and actual transfer speed remain live checks.

Parent review found a known timeout/retry defect in download-artifact v4.3.0. The final workflow pins v6.0.0 at `018cc2cf5baa6db3ef3c5f8a56943fffe632ef53`; its bundled source rejects timed-out downloads correctly and remains compatible with v4 uploads. Source: [fixed download implementation](https://github.com/actions/download-artifact/blob/018cc2cf5baa6db3ef3c5f8a56943fffe632ef53/dist/index.js#L2232). The verifier remains the publication gate.

Artifact staging run `36802508956` passed full OCI verification at revision `539de38`. Upload began at 01:46:47 UTC. At 01:51:16, buffered diagnostics acknowledged 318,767,104 bytes, about 1.18 MB/s. Network transmission continued without a quota error or OOM; transmitted bytes are not counted as accepted bytes. This rate leaves too little margin within the 360-minute job. Its runtime token expires at 07:54:18 UTC; only timestamp claims were inspected. The current run remains active while a 720-minute replacement is prepared. Replacement credential lifetime, full transfer, publication and live inference remain unverified.

The narrow staging-limit change passed Dell actionlint 1.7.12, bash syntax for all 11 multiline scripts and whitespace checks. It was committed and pushed as `6c4dddfd4bbc9659b6fb76d9ef8c35b1ac94ebc6`. A later acknowledgement reached 947.9 MB at 01:59:52 UTC, about 1.22 MB/s since the earlier sample. This still leaves less than 45 minutes of margin. Exact cancellation and replacement are in progress; no GPU has been rented.

Additional native checks on hosted app `b5617ae` passed for rounded corners and controls fitting without scrolling. Checked Off Local, Off Cloud and all four Settings tabs. The current memory reading showed 26.7 GiB required versus 21.4 GiB budget, an insufficient-memory error and disabled Local Start. Zero Start, inference or OpenCode actions occurred. A title-bar drag returned no tool error, but absolute movement remains unverified; the tool lacks global Cmd+Tab and Dock inspection timed out. Task PID77870 exited through Quit, unrelated installed PID31287 stayed running, and preferences were restored byte-for-byte after a guarded comparison. Private result and seven screenshots: `/tmp/lobo-cloud-mac-e2e-20261001.a542eX/native-window-6y26gqh0/result.json`. This is Off-state UI evidence, not cloud runtime acceptance.

Run `36802508956` is cancelled. Its runner24 and container are removed, the candidate ACL was restored and logs remain private. Replacement run `36804047216` resolves to exact workflow revision `6c4dddfd4bbc9659b6fb76d9ef8c35b1ac94ebc6`. Runner25 uses the same readonly mount and 1 CPU / 2 GiB limits, with a twelve-hour-twenty-minute external watchdog. The runtime-token expiry gate and image transfer are still in progress.

Replacement runtime-token gate passed: issued 02:03:57 UTC, expires 14:13:57 UTC, lifetime twelve hours ten minutes. Only timestamp claims were inspected. The complete OCI verification passed again and artifact upload is running. Signed upload URL lifetime remains undocumented and independently unverified. This checkpoint establishes the longer runtime credential, not successful transfer or publication.

Startup measurement interpretation: CLI `boots.jsonl` preserves full ReadyInfo. Its `rent_to_container_s` uses RunPod `lastStartedAt`, falling back to `createdAt`, through the agent entrypoint timestamp. This combines allocation, image pull, unpacking and container startup; it is not a pure download measurement. Preserve provider `createdAt` for a separate creation-to-entrypoint calculation. Agent `verify_s` hashes bundled weights; `load_s` covers llama process startup through healthy readiness. Total `elapsed_ns` also includes image resolution and replacement attempts. Missing provider timestamps can yield zero; report that as unknown. Advertised `host_download_mbps` and faster second-start timings do not prove measured bandwidth or cache reuse. The current API evidence has no cache-hit or physical-host proof.

Run `36804047216` failed during artifact staging. At 03:06:32 UTC, after 3,609 seconds, the log reported: `Server failed to authenticate the request. Make sure the value of Authorization header is formed correctly including the signature.` Final acknowledged bytes were 3,867,148,288. The runtime token remained valid until 14:13:57 UTC. The timing is consistent with a one-hour signed upload URL, but no SAS expiry or HTTP/error code was logged; do not present that cause as proved. No artifact was finalized, the hosted publisher was skipped, and both approved Q6 tags still return anonymous HTTP 404. Runner25/container were removed, ACL restored, and monitor/watchdog exited. Private receipt: `/tmp/lobocode-cloud-publish-9c915c/result.json`.

Multipart preparation: the pinned runner bundles Node24.19.0/npm under `/home/runner/externals/node24/bin`, but neither is on PATH. Its Docker root has 13.742 GiB free, insufficient for a full part copy. Dell `/storage` has 2,240.210 GiB free; use an isolated scratch mount there. The probe container was removed. The official artifact SDK still uses one signed URL per artifact; independent smaller artifacts provide fresh authorization and durable completion. Plan v1.8 is implementation work, not a successful transfer result.

Multipart Python checks passed on Dell: 19 tests plus compilation. Tests cover deterministic roundtrip, files spanning parts, empty files, exact/short boundaries, the live disk bound at each consumed part, flush failure, independent file/part corruption, invalid manifests, symlink/FIFO replacement and sanitized errors. Parent source review passed. Restore removes a part only after its hash passes and pending output writes flush. A failed restore removes its new output; consumed parts must be downloaded again before retry.

The hosted download explicitly uses the current repository token, repository and workflow run ID. The pinned download action selects public REST listing only when given `github-token`; this allows exact IDs from earlier attempts of the same run. The action's default internal listing carries a job backend ID. Source: [pinned download action](https://github.com/actions/download-artifact/blob/018cc2cf5baa6db3ef3c5f8a56943fffe632ef53/src/download-artifact.ts). Complete-set and raw-content validation remain mandatory after download.

RunPod startup research remains separate from measured acceptance. Its [optimization guide](https://docs.runpod.io/serverless/development/optimization) distinguishes image initialization from GPU model loading. Its [Model Store description](https://www.runpod.io/blog/building-runpods-model-store) describes host-cached models for Serverless, including cold-host downloads. These Serverless features do not establish cache placement or fast startup for Lobocode's current Pod API. The complete image removes installation and separate weight downloads after container start. A new host may still need the image's roughly 23 GiB transfer, followed by verification and GPU loading. Measure the authorized Pod tests before promising startup speed.

Multipart action checks passed on Dell: 17 tests, Node24.19.0, network disabled. Official SDK6.2.1 imports with install scripts disabled. Checks cover reuse, full-set outputs, malformed inputs, ZIP bounds, token lifetime and actual child-process success/failure/late logs. Peak memory for the whole test container was 134,606,848 bytes; this is fixture memory, not measured live upload memory. Test containers and workers exited. Parent and independent integration review passed. Dell actionlint and all 14 multiline shell blocks passed. Real publication and resume proof remain pending.

Multipart commit `14f9f8c4afe1fba7a6c7e8fd3e461a6bd4823ae8` is pushed. An additional Dell check used its exact files: Python packed five files (32,879 bytes) into nine parts, Node accepted the manifest, and Python restored identical bytes while consuming parts. The fixture and container were removed. A separate 64-byte scratch permission check passed with the exact planned isolation and cleanup. Dell does not allow passwordless Python sudo; the private runner helper uses a task-only scratch bind and CHOWN capability for ownership setup, then no capabilities for reads and cleanup.

Live multipart attempt1 is [run 36812163531](https://github.com/1905/lobocode/actions/runs/36812163531), staging job110209380891, runner26. It uses exact workflow revision `14f9f8c`, source `528322e` and the unchanged approved image digest. Its private receipt root is `/tmp/lobocode-cloud-transfer-helpers-3bf43610/attempt-8c195413a22f38c5`. Initial input/checkout checks passed; full image verification is running. The monitor will cancel only this run after two data parts finalize, preserve those artifacts and perform exact cleanup before a same-run retry. No GPU has been rented.

That run failed before upload. Full OCI verification passed in 139 seconds; locked SDK installation passed. Packing failed immediately on `Unexpected or missing OCI layout entries.` Read-only inspection found `blobs/`, `index.json`, empty real `ingest/`, and `oci-layout`. The strict fixture omitted BuildKit's empty work directory. Plan v1.9 accepts only that optional empty directory; no image bytes change. At 03:52:11 UTC, runner26/container and scratch were removed, and the candidate ACL was restored. No artifact finalized. Receipt: `attempt-8c195413a22f38c5/cleanup.json` under the private helper root.

The empty-ingest correction passed parent review and all 22 Dell fixtures. Its accepted fixture produces identical transfer bytes and preserves the source. Nonempty ingest, file/symlink/dangling-symlink/FIFO replacements and unrelated directories fail before output. Read-only selection of the actual candidate also passed with file opening blocked. No real packing or upload occurred in this check.

Size accounting correction: the OCI tree has 31 files totaling 24,691,613,469 bytes. The transfer also includes the 648-byte `q6-metadata.json`, making 32 files and 24,691,614,117 bytes. The earlier total described the OCI tree alone. No content changed.

Correction `aa32c246d8cbbbc7659c25a6c88f9f9b2f805072` is pushed. Fresh [run 36812995172](https://github.com/1905/lobocode/actions/runs/36812995172) started at 03:59 UTC with staging job110211932581 and runner27. Source and image digests are unchanged. Private state: `/tmp/lobocode-cloud-transfer-helpers-3bf43610/attempt-6e176160124a7d93/state.json`. The same controlled two-part checkpoint and cleanup apply. No GPU is rented.

That run passed full image verification, SDK installation and packing. It produced 46 data parts; manifest SHA256 is `1fdf93a34500da5da59a69215de74d95dfe3ac56afc52b5e3da6d1fc2924d369`. The manifest artifact finalized, and the first data part is uploading. Live transmission counters are not acknowledged payload or finalization evidence.

Live action receipts are buffered by the runner. Its current page/block files did not yet contain gate or upload receipt lines, and the in-progress job-log endpoint returned 404. The [pinned runner logger](https://github.com/actions/runner/blob/v2.337.0/src/Runner.Common/Logging.cs) uses buffered writers and flushes them at page/block end. Keep the controlled two-part cancellation unchanged. After exact cleanup, collect actual numeric gate events from that exact terminal job log and bind its hash to the checkpoint before accepting it. The action enforces those gates before uploads; external receipt collection happens after cancellation. Do not claim unseen live receipt lines. A separate reviewed private finalizer validates the run/job/SHA, cleanup, surviving artifact IDs and lifetime formula before updating acceptance.

The checkpoint passed. Manifest artifact11140903248 and data artifacts11141190713 (part0) /11141022672 (part1) finalized. Each data payload is536,870,912bytes; upload durations were438.29 and480.95seconds. The monitor cancelled only run36812995172 at04:30:27UTC. Runner27, its container and scratch were removed; candidate ACL restoration passed at04:31:09UTC. All three artifacts survive. Part2 began but did not finalize.

Four actual terminal-log token gates passed. Their issued/expiry timestamps are1790827150/1790870950; each has300seconds beyond the required remaining-job budget. The saved terminal log SHA256 is `e8f7bee7a5a224a57d9571e0425f121c2fd1fce8ff3d5c8aa842dc8ff0a403c0`. The reviewed finalizer accepted this evidence at04:35:17UTC after checking exact run/job/SHA, cleanup and surviving IDs. This proves completed parts survive cancellation; cross-attempt reuse remains pending.

At04:36UTC, the original staging job was rerun in the same workflow run at unchanged revision `aa32c246`. Attempt2 is job110220721178, runner28, with the original requested label and a fresh isolated scratch/container. Private state: `/tmp/lobocode-cloud-transfer-helpers-3bf43610/attempt-156058fb1b3bf124/state.json`. The retry must reuse those exact three artifact IDs, finish remaining parts, reconstruct and verify the image, then publish. No GPU has been rented.

Attempt2 regenerated the identical manifest hash. At04:53:26UTC, its first SDK upload worker was reading part-000002; the original manifest/part0/part1 artifacts remained present and unexpired. This observes skipping completed uploads. It does not replace terminal exact-ID reuse receipts, which remain pending. Private safe receipt: `attempt-156058fb1b3bf124/resume-skip-observation.json` under the helper root.

Eight data artifacts are now finalized across the two attempts, totaling4,294,967,296raw bytes (4GiB). This exceeds the earlier single-artifact failure's3,867,148,288acknowledged bytes. The new transfer remains active without failure. This is partial-transfer evidence, not complete publication or live inference acceptance.

By05:54UTC, nine data parts were finalized (4.5GiB). Fresh uploads had continued for more than one hour since the04:53 observation without the previous authentication failure. The original failure's signed-URL-expiry cause remains unproved; this only records the new path crossing that elapsed-time boundary successfully.

Cloud execution readiness review found two operational corrections. Use Dell idle30minutes, not the stale10-minute example. Stop unattended startup before25minutes, and require at least34minutes of the one-hour lifetime remaining before the32-minute large-request harness. Run the TUI from a fresh private evidence directory because `boots.jsonl` uses its working directory. The Mac tiny request keeps idle10minutes. These are execution bounds, not completed live checks.

RunPod's [Pod templates](https://docs.runpod.io/pods/templates/overview) save image and launch settings; creating one does not establish a faster cold pull. The [RunPod Docker guide](https://www.runpod.io/articles/guides/docker-setup-pytorch-cuda-12-8-python-3-11) explicitly says image caching is possible but warm-host placement is not guaranteed. No ordinary-Pod cache-pinning control was found in the reviewed documentation. Separate Serverless features and paid network volumes are not part of this image or acceptance claim.


## Feesh host correction — 2026-10-01

The user stopped Dell execution and explicitly selected feesh for the remaining image/publication/backend and TUI work. Run36812995172 attempt2 was cancelled; exact runner28/container/scratch cleanup and candidate ACL restoration passed at06:01:44UTC. Original image/cache and paused Q8 remain untouched. Eleven artifacts survive: the manifest plus ten512MiB data parts (5GiB). Terminal logs prove reuse of original IDs11140903248,11141190713,11141022672, and all nine actual token gates passed. No image was published. Private receipts: `attempt-156058fb1b3bf124/{cleanup.json,artifacts-before-cleanup.json,cancelled-retry-receipts.json}` under `/tmp/lobocode-cloud-transfer-helpers-3bf43610`.

Feesh resolves to the configured feesh9 SSH host. Read-only checks found24CPUs,118GiB available RAM,397GiB free disk and Docker24.0.2/buildx0.10.5. Existing services are outside this task. A bounded32MiB range of the public Q6 file returned HTTP206 in1.599seconds (~21MB/s). This is download evidence, not measured registry upload. The existing22,082,528,352-byte Q6 seed passed the separate capped hash check below. Planv1.10 rebuilds from exact runtime source528322e and requires fresh full image verification before publication.

Feesh Q6 seed verification passed:22,082,528,352bytes, SHA256 `9fc4e4768045cb187a86f42b19f3d74754406680c3688502020556a8f3b70c4b`,219.370seconds at a96MiB/s cap. It remained a regular nonsymlink file with stable device/inode/size/mtime/ctime. The local0600receipt is `/tmp/lobo-feesh-q6-sha256-20261001.json`. This permits a private seed copy; it does not establish the rebuilt image yet.

Feesh source packet contains307 tracked files from528322e. The task Dockerfile hash is `0629aaff7ac95dfcf980ccc91f0d0889c4b078395ea67cc5a664246adbafaf92`; its sole change exactly matches the earlier readonly model-seed substitution. The private seed copy verified22,082,528,352bytes in36.083seconds without changing the original.

A logging-key collision in the private build launcher stopped the worker before compilation. The corrected driver and narrow resume rechecked source, seed, stopped container, cache volume, resource caps and mounts. Actual image build started at06:19:34UTC in container `131af52c36b96df5aaf9ea35fbe4969d40168dd4ed0c0d3491576efdc8f9f962`, capped at4CPUs/8GiB with no extra swap and a180-minute deadline. Cache is `lobo-feesh-q6-528322e-20261001-cache`; task root is `/srv/lobo-cloud-e2e-q6-528322e-20261001`.

High host load average was investigated before adding CLI compilation. Measured CPU idle was52.4%, I/O wait13.4% and available RAM about111GiB; unrelated mysqld work stayed untouched. The public full-cuda base pulled in102seconds. Local seed-context transfer reached16.18GB in101seconds, about160MB/s. These are build preparation measurements, not registry upload or GPU startup results.

## Feesh Q6 build and independent validation — passed

The complete Q6 image built and exported in 734.401 seconds (12m14s) from `528322e468deeab23f8c0b74e804510d98cca071`. Its new digest is `sha256:aa590424f7862b5664c943cc5753bb72d447cbdaec5ed03a66642e138e3857f4`. The earlier Dell digest does not identify this image. Model packaging passed in 93.3 seconds, agent compilation in 2m49s, and the built-in offline image check in 38.4 seconds.

Independent validation passed in 168.798 seconds. The canonical verifier hashed all 28 OCI blobs, totaling 24,623,685,411 bytes, and checked source, model and Linux AMD64 identity. A separate stream checked all 29 decoded layers against their uncompressed hashes. Final filesystem checks passed for nine required files, executable architecture, the exact startup script, licenses and embedded release metadata. All six shard hashes passed, with 851 tensors matching the verified source. No shared SSH host keys remain. These checks do not establish GPU inference or startup speed.

The exact 4 CPU / 8 GiB worker stopped at 06:31:49 UTC without an OOM event. Its named BuildKit cache remains intact. The production Dockerfile still writes `built_by=github-actions`; the build receipts identify Feesh as the actual builder.

The first transfer-space projection exceeded the 160 GiB task budget. After validation, only the verified private seed copy was removed. Its path, regular-file identity, size and timestamps matched the copy receipt. The original `/srv/lobo/models/` seed kept the same device, inode, size, mtime and ctime. Source files, candidate, evidence and BuildKit cache were preserved. Projected task use, including the complete transfer and an extra 512 MiB reserve, is now 150,659,781,539 bytes (140.31 GiB). Projected host free space is 272,008,719,453 bytes, above the 200 GiB floor.

The transfer input has 31 files totaling 24,623,686,563 bytes, including `q6-metadata.json`. Publishable files are mode `0444`; OCI directories are `0555`. The artifact root remains private, with temporary access delegated to the reviewed CI helper. Logs and evidence remain private.

Safe receipts are under `/private/tmp/lobo-feesh-build-2036z5kf/receipts/`: `build-result.json`, `validation-result.json`, `canonical-verification.json`, `q6-image-evidence.json`, `resume-worker-stop.json`, `private-seed-cleanup.json` and `publication-permissions.json`. Original receipts and image files remain under `/srv/lobo-cloud-e2e-q6-528322e-20261001/artifacts/` on Feesh. Only small JSON receipts were copied to the Mac.

The isolated Feesh CLI is also ready from the same full source SHA. Binary: `/srv/lobo-cloud-client-e2e-20261001/bin/lobo`; SHA256: `07150da1ebfcec9b1d7e23d36810a51b47624cffd512dd17eaf0096aa3f60591`. Its bounded build wrapper took 544.795 seconds. Compilation passed, but the wrapper returned 1 because it used unsupported `--version`. Corrected `version` and help checks both returned 0 without rebuilding. Exact container `ecb458f920db60e8d6ae0e6bf5d4df41ee205a8af453e069349a8f4ab7b5dc16` was absent at 06:37:23 UTC; unrelated containers were preserved.

CLI preparation passed syntax checks for six Python files and the shell script. Its private `0600` config selects RunPod Secure Cloud, SSH, Q6, context 65,536, idle 30 minutes and a one-hour maximum life. No provider call or inference request ran during preparation. Safe receipts: `/tmp/lobo-feesh-client-prep-20261001.3b5rZE/{cli-readiness.json,build-result.json,cli-container-cleanup.json,client-preflight.json,rust-image-pin.json}`. These are preparation and smoke results; broad fixtures and live cloud E2E are not claimed.

## Feesh Q6 publication — passed

[Run 36826266337](https://github.com/1905/lobocode/actions/runs/36826266337), attempt 1, passed at unchanged workflow revision `aa32c246d8cbbbc7659c25a6c88f9f9b2f805072`. Staging job `110252539483` ran from 06:43:09 to 06:56:08 UTC. Hosted publisher `110256224372` completed successfully at 07:04:56 UTC.

Feesh helper validation passed before launch. Python 3.10 checked seven modules and eight embedded scripts. Focused fixtures covered state mismatches, token claims, complete upload counts, cache-inclusive capacity and exact owned stops. The dummy entrypoint passed with networking disabled and UID 1001. Its first fixture required unavailable `memory.peak`; the corrected fixture used `memory.current`. The production entrypoint stayed unchanged.

All 46 data parts and the manifest finalized. The payload was 24,623,686,563 bytes. Upload took 663.292 seconds, from the first gate at 06:45:01.378958 to finalization at 06:56:04.671282 UTC. Average raw-payload throughput was 37.123 MB/s; this is not registry wire speed. All 47 actual terminal-log gates passed numeric claim, ordering, lifetime-formula and exact-count checks. The fresh run reused no artifacts.

The actual runner stayed capped at 1 CPU / 2 GiB. Maximum sampled working memory was 445,054,976 bytes, with no observed OOM event. All sampled capacity checks passed, including the task root and exact named BuildKit cache. Maximum observed task usage was 150,123,134,976 bytes.

Hosted download passed in 116 seconds, reconstruction in 171 seconds and full OCI verification in 74 seconds. Registry publication and latest promotion passed in 130 seconds. These measurements do not establish GPU startup or inference speed.

At 07:06:19 UTC, independent anonymous reads verified both `ghcr.io/1905/lobocode:sha-528322e468de-q6` and `ghcr.io/1905/lobocode:latest-q6`. Response-body SHA256 and `Docker-Content-Digest` matched `sha256:aa590424f7862b5664c943cc5753bb72d447cbdaec5ed03a66642e138e3857f4`. Each manifest had 29 layers. The runtime source remains `528322e468deeab23f8c0b74e804510d98cca071`.

Runner 29, exact container `7335798fe73a8910e7a81b4cf1abda32bea27795688f1258f2430ca566cedcc1` and its transfer scratch were removed at 06:57:33 UTC. Candidate ACL restoration passed. The hosted job deleted its 47 exact artifact IDs; the monitor independently confirmed all were absent. Previous artifacts, the candidate, original model and BuildKit cache were preserved.

Private receipts are under `/private/tmp/lobocode-cloud-feesh-helpers-8jcvwm2h/attempt-283de63452d3005c/`: `staging-proof.json`, `runtime-token-gate-source.json`, `transfer-performance.json`, `cleanup.json`, `publication-ci-result.json` and `public-verification-feesh.json`. The last file is a 499-byte copy of Feesh `artifacts/public-verification.json`; SHA256 is `730ccc1816243cf34e927868113cb0015349528e6a4cafcef866024436c1eb13`.

Live TUI and Mac cloud E2E remain pending. No GPU was rented during publication. App/CLI release, Q8 and broader feature acceptance remain held.


## RunPod response schema check before rental — 2026-10-01

The private provider receipt fixtures covered legacy response fields. Current
[GET Pod documentation](https://docs.runpod.io/api-reference/pods/GET/pods/podId)
and the public [v1 schema](https://rest.runpod.io/v1/openapi.json) instead describe
`image`, `gpu.count` and `lastStartedAt`. The documented create request uses
`imageName` and `gpuCount`; the response schema does not promise `createdAt`.
Empty inventory checks could not establish the live populated response format.

The first live Pod response at 07:12–07:14 UTC returned legacy `imageName`,
`gpuCount` and a string `createdAt`. Its `lastStartedAt` used the Go-style date
format. Therefore, the original helper was not proved incompatible with the
actual API. The correction supports both documented and live response shapes,
with stricter boot ownership. Do not report this as a proved production API bug.

The scoped helper correction requires the exact provider-reported `LOBO_BOOT_ID`,
pod ID, name, image digest, one GPU, and unchanged private config/connection state.
It accepts current response fields and rejects conflicting legacy aliases.
`lastStartedAt` is supplemental evidence when present, with exact later matching.
An absent timestamp is recorded as absent rather than invented. All 13 focused
helper fixtures passed on Feesh with fake responses. These checks do not prove
acceptance of a populated live provider response. Live cloud E2E remains pending.

The schema declares `costPerHr` as a number but shows a string example. That
inconsistency does not establish a live production parsing failure. The app/CLI
parser remains unchanged pending actual evidence.

The [RunPod API overview](https://docs.runpod.io/api-reference/overview) announces
REST v1 retirement on November 15, 2026. The production provider uses v1. A v2
migration is a separate follow-up; this cloud test does not implement or verify it.

## Live TUI startup and diagnostic correction — 2026-10-01

The production CLI reached Ready through an actual 80×24 PTY on Feesh. Its process
completed in 230.178 seconds with exit 0. The saved boot record reports 174.911
seconds from rental to container start, 19.500 seconds for weight verification
and 28.318 seconds for model load. The model source was `Docker image`.
The CLI observed COMMUNITY capacity at $0.69/hour. This differs from the prepared
Secure Cloud setting; it is the observed rental, not a Secure Cloud acceptance claim.

The startup detail line clipped at 80 columns. Ready and status views fit.
These are real terminal observations, not fixture renders. Provider allocation,
image pull and container scheduling are combined in the rental-to-container
measurement. There is no cache-hit receipt or separate image-pull duration, so
these timings do not prove image or host cache reuse.

Startup evidence is under `/srv/lobo-cloud-client-e2e-20261001/evidence/`:
`pty-up-001/result.json`, `pty-up-001/boots.jsonl`, the private raw/frame logs and
`pty-status-001/`. The synthetic PTY check had already passed fragmented cursor
queries and the exact 80×24 screen. It did not call the CLI or a provider.

The first real diagnostic did generate tokens, but its acceptance receipt failed.
`large-request-001/summary.json` records one attempt, error
`response_settings_invalid` and an empty measurements list. Read-only recovery
at 07:24:14 UTC found 47,000 processed input tokens, two generated tokens and
21.4864 seconds of server prompt processing. It found no active slot or request.
The recovery made zero generation requests and kept the same runtime identity.
Receipt: `post-request-numeric-recovery.json` in the same evidence root.

The validator discarded the final response before saving a measurement. Therefore,
the first receipt cannot prove time to first content, returned content, answer
correctness, cache-count acceptance or complete request timing. Aggregate server
metrics confirm processing and generation; they do not replace the missing receipt.
The failed receipt remains intact.

Two test-contract mistakes caused the rejection. Pinned llama.cpp b11118 returns
the requested nested response path as a literal flat key, and its generation
settings do not serialize `n_cmpl`. The fix reads
`generation_settings/n_predict` and removes the unsupported echoed `n_cmpl`
request/check. Request `n_cmpl=1`, one prompt, one final event and all input/output
limits remain unchanged. Positive fakes now use the real flat shape. A negative
fixture rejects nested-only settings. Sources:
[flat-field selection](https://github.com/ggml-org/llama.cpp/blob/b11118/tools/server/server-common.cpp#L791-L808),
[serialized settings](https://github.com/ggml-org/llama.cpp/blob/b11118/tools/server/server-task.cpp#L89-L139)
and [native final response](https://github.com/ggml-org/llama.cpp/blob/b11118/tools/server/server-task.cpp#L323-L345).

Python 3.10 also required parser-only normalization of Rust's nine-digit fractional
expiry timestamp. The saved ownership bytes remain unchanged. That correction and
its nine-digit positive fixture passed the full fake-only cloud self-test in
1.917 seconds. Receipt: `cloud-self-test-6hlfp2vt/receipt.json`.

Parent review passed for the response-contract diff. The full cloud self-test then
passed on Feesh Python 3.10.12 in 1.968 seconds at 07:27:18 UTC. Output confirmed
the HTTP fake suite, all 12 same-size diagnostic scenarios, and cloud saved-state
identity, privacy, expiry and file-change checks. It used a separate network
namespace with only loopback and no external routes, one CPU, 512 MiB address
space and nice level 19. No model, CLI or provider call ran in these fake tests.

The exact tested source hashes are:

| File | SHA256 |
| --- | --- |
| `tools/bounded_runtime_e2e.py` | `2309b9998fb1815a8d084b36af1471b902d8bad875fcc551a0f457b57dbc83ae` |
| `tools/same_prompt_runtime_e2e.py` | `23387b35954b510da29afbcfe929aa725c5061a19f871d24bf4f41c1d75fa004` |
| `tools/cloud_runtime_e2e.py` | `57ef1f521ebe5a2c9b232e47741b535bde7ac7964d0152b7e1930bd6660c842c` |

Test and staging receipts are in
`/srv/lobo-cloud-client-e2e-20261001/evidence/response-settings-self-test-vvu0x8w6/`.
The three scripts were copied to the private Feesh and Mac task roots only after
the tests passed. Preimage hashes, regular-file identities, backups and final
hashes were checked. The Mac step copied files; it ran no tests or inference.
Its receipt is
`/tmp/lobo-cloud-mac-e2e-20261001.a542eX/response-settings-preimage-728680au/staging-receipt.json`.

## Controlled cloud diagnostic repeat — passed

Plan v1.11 H4 passed with one controlled request at 07:28:41–07:29:11 UTC.
The pre-request gate recorded 2,473 seconds of remaining lifetime, above the
34-minute minimum. It required unchanged provider/config identity, one request,
`cache_prompt=false` and zero automatic retries. The same pod `1uc4kemsb5093y`
and boot `3522f4b7f11dca98` used runtime source
`528322e468deeab23f8c0b74e804510d98cca071` and the verified public Q6 image digest
`sha256:aa590424f7862b5664c943cc5753bb72d447cbdaec5ed03a66642e138e3857f4`.

The complete receipt reports one generation attempt, 47,000 processed input
tokens, zero cached tokens and two generated tokens. Nonempty content matched
the expected answer `4`. All final-response, token-count and timing checks passed.

| Measurement | Recorded result |
| --- | --- |
| Time to first content | 24,040.013 ms |
| Total request time | 24,210.052 ms |
| Server prompt processing | 21,493.801 ms |
| Server prompt rate | 2,186.677 tokens/s |
| Server generation time | 22.089 ms |
| Server-reported generation rate | 45.2714 tokens/s |

Only two tokens were generated. The generation rate is not a sustained benchmark.
This synthetic size comparison shows that the cloud runtime can process the
47,000-token input and return the expected content. It does not test OpenCode or
replay the original private prompt.

The provider check at 07:29:30 UTC confirmed the same running pod/boot, matching
provider identity, one GPU and unchanged private config. Receipts are
`/srv/lobo-cloud-client-e2e-20261001/evidence/large-request-002/summary.json`,
`large-request-002-gates.json` and `large-request-002-provider-after.json` in the
same evidence root. The first failed request and its numeric recovery remain
preserved separately.

Plan v1.11 H1–H4 are complete. TUI cleanup is underway. H5/F7, the native Mac
cloud lifecycle and broader feature acceptance remain pending.
