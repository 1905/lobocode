# Public Docker distribution

Status: planned before implementation on 2026-09-30. Supersedes the bucket
dependency in the domain-free cloud plan. No push, publication or GPU rental.

## Required behavior

Lobocode is a public application. Its users must not depend on the developer's
bucket, storage credentials, private hosts or personal configuration.

- The DMG installs the standalone Mac app. The optional CLI installs separately
  through Homebrew. Do not replace the user's CLI with a manual install.
- Cloud setup needs a GPU provider API key. The app generates its own client
  API key, selects the public image and starts the encrypted SSH connection.
- Each public Docker image contains the Linux agent, CUDA inference runtime,
  SSH daemon and selected model weights. Q8 and Q6 use separate image tags.
- Cloud boot must not fetch agent zips, install OS packages or download models.
  Missing or mismatched image contents fail through the existing cleanup path.
- The image version follows the app release. Explicit image overrides remain
  available for development. Existing private storage settings must not become
  an automatic download source.

## Implementation order

1. Build model-specific Docker images. Verify upstream model bytes against the
   catalog, then use llama.cpp's GGUF split tool. Store shards in separate image
   layers below GHCR's 10 GB layer limit. Include model attribution and license.
2. Verify and load the model already in the image. Preserve GPU checks, API
   authentication, shutdown timers and failed-boot deletion. Do not fall back to
   network downloads when bundled content is missing or invalid.
3. Default cloud launches to the matching public image. Remove bucket/release
   lookup and private model-source selection from normal startup and readiness.
4. Remove bucket, image and download-source details from normal app setup. Update
   the CLI wizard, fixtures, native smoke, README, assets and changelog.
5. Run backend and container fixture checks on Dell only. Run native UI checks
   and builds on the Mac, without inference. Rebuild the local app and DMG.

## Release acceptance

Before publication, build both complete images on a host with enough disk space.
Verify anonymous pulls, image contents, and live RunPod/Vast startup for each
model. Publish the images before distributing a DMG/Homebrew release which uses
them. No new image is currently published; keep that limit explicit.

## Sources

- [Docker multi-stage builds](https://docs.docker.com/build/building/multi-stage/)
- [GHCR public pulls and layer limits](https://docs.github.com/en/packages/working-with-a-github-packages-registry/working-with-the-container-registry)
- [Pinned CUDA image contents](https://github.com/ggml-org/llama.cpp/blob/b11118/.devops/cuda.Dockerfile)
- [GGUF split options](https://github.com/ggml-org/llama.cpp/blob/b11118/tools/gguf-split/README.md)
- [Upstream model and Apache-2.0 license](https://huggingface.co/HauhauCS/Qwen3.5-27B-Uncensored-HauhauCS-Aggressive)

Documentation establishes the supported approach. It does not establish that the
new complete images have built or passed live inference.
