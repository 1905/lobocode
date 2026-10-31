# PR #2 review and validation

**Date:** 2026-10-31
**Status:** merged and released as app 0.2.3

The reviewed branch merged into `master` at `13aa8d64bd3874dcf61464050cdc6150b87d1304`. [App 0.2.3](https://github.com/1905/lobocode/releases/tag/app-v0.2.3) was built from that exact source on Air. Mounted DMG contents, arm64 architecture, ad-hoc signature, source receipt and anonymous asset downloads passed. DMG SHA-256: `309f3581ca23b365e2954616ad3e12153e6766f6ca35c5003a8d3231401dcede`. The installed app and optional CLI were not replaced.

PR #2 adds the Qwen3.8 model catalog and verified chunk delivery. The final scope supports only `qwen38-q6`, on RunPod or Vast. The app has no model picker or local execution path.

## Findings and fixes

- China reviewed the contribution with Kimi K3.1 Pro. Findings covered the missing default image, mutable workflow actions, the CLI wizard dropping the new model, and retries rejecting a chunk that had already uploaded.
- Codex reviewed the contribution with GPT-6.1 Sol. Findings covered compatibility with the released Go CLI and incorrect slow-host measurements after a chunk retry.
- The maintainer fixes add Hugging Face fallback, catalog-backed CLI selection, verified acceptance of a retried upload, and a received-byte counter for the slow-host check. A second Codex review of those fixes found no issues.
- The final native test exposed another bug: the backend reported the effective model while the config still contained `q6`. The app now persists `qwen38-q6` at the save boundary. Native legacy migration passes.
- GitHub Actions is disabled and all workflow files are removed. Builds and tests run on Feesh and Air.

## Verified results

- Feesh: 548 workspace tests, 21 UI tests, Rust/UI lint, and 25 image-verifier checks passed. Two workspace integration tests remain ignored.
- Air: 97 app library tests, two binary tests, 21 UI tests, and six native fresh/legacy setup checks passed. Test-owned app and driver cleanup passed.
- The thin image contains the agent, llama.cpp runtime, SSH server and Cloudflare helper, with no baked model or generated SSH host keys. Offline smoke passed.
- Anonymous access passed for both `sha-b604359-thin` and `latest-qwen38-q6`. Digest: `sha256:0491a075ad2c7f2c3f29cd2dffb0d58a8139ed1963905ccbd4378fa3ae9a5637`. Image source: `b6043598f9ba88cacabfde574f5c691abc0b7322`; Linux AMD64.
- The contributor CDN returned HTTP 403 from Feesh. Fresh first/last byte-range downloads from Hugging Face matched the pinned chunk hashes. The earlier streamed whole-file verification checked all 84 hashes on October 26.

## Limits

No new GPU rental or inference test ran during this review. The contributor's earlier Vast measurements describe their build, not this release. The optional Homebrew Go CLI cannot use `qwen38-q6`; the Rust CLI remains unreleased. App signing is ad-hoc, without notarization. Legacy Q6/Q8 code remains for later cleanup.

The latest contributor update to PR #2 only changes README text. Its supported facts are incorporated into the maintained README, with the newer single-model and build-host requirements taking precedence. PR #4 is a separate SSH socket change and is outside this delivery.
