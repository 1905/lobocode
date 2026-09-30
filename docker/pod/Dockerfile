# Complete public GPU image: runtime, agent, SSH and selected model weights.
# Built by pod-image.yml; LLAMA_IMAGE comes from lobo-proto DEFAULT_LLAMA_IMAGE.
ARG LLAMA_IMAGE
ARG SPLIT_IMAGE=ghcr.io/ggml-org/llama.cpp:full-cuda-b11118

FROM ${SPLIT_IMAGE} AS model
ARG MODEL=q8
COPY crates/lobo-proto/catalog.json /build/catalog.json
COPY docker/pod/build_model.py /build/build_model.py
# Retain verified upstream bytes across packaging fixes. This cache is build-only.
RUN --mount=type=cache,id=lobo-model-sources,target=/model-cache,sharing=locked \
    python3 /build/build_model.py "$MODEL" /out

FROM cloudflare/cloudflared:2026.9.1 AS tunnel

FROM rust:1.98.1-alpine AS build
ARG VERSION=dev
ARG GIT_SHA=none
ARG LLAMA_IMAGE
RUN apk add --no-cache musl-dev
WORKDIR /src
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY .cargo ./.cargo
COPY crates ./crates
RUN --mount=type=cache,target=/usr/local/cargo/registry --mount=type=cache,target=/src/target \
    LOBO_VERSION="$VERSION" cargo build -j 3 --release --locked -p lobo-agent --target x86_64-unknown-linux-musl && \
    mkdir -p /out && cp target/x86_64-unknown-linux-musl/release/lobo-agent /out/lobo-agent
RUN printf '{"version":"%s","git_sha":"%s","built_by":"github-actions","llama_image":"%s"}\n' \
      "$VERSION" "$GIT_SHA" "$LLAMA_IMAGE" > /out/release.json

FROM ${LLAMA_IMAGE}
USER root
RUN apt-get update && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends openssh-server ca-certificates curl jq \
    && rm -f /etc/ssh/ssh_host_* \
    && rm -rf /var/lib/apt/lists/* && mkdir -p /run/sshd
COPY --from=build /out/lobo-agent /lobo/lobo-agent
COPY --from=build /out/release.json /lobo/release.json
COPY --from=tunnel /usr/local/bin/cloudflared /lobo/bin/cloudflared
COPY --chmod=755 docker/pod/start.sh /lobo/start
COPY --from=model /out/model.json /lobo/model.json
COPY --from=model /out/licenses/ /lobo/licenses/
# One shard per layer keeps every layer below GHCR's 10 GB limit.
COPY --from=model /out/01/ /lobo/models/
COPY --from=model /out/02/ /lobo/models/
COPY --from=model /out/03/ /lobo/models/
COPY --from=model /out/04/ /lobo/models/
COPY --from=model /out/05/ /lobo/models/
COPY --from=model /out/06/ /lobo/models/
COPY --from=model /out/07/ /lobo/models/
COPY --from=model /out/08/ /lobo/models/
ARG MODEL=q8
RUN --network=none /lobo/lobo-agent check-image --model "$MODEL"
ENV LOBO_IMAGE_MODEL=1
EXPOSE 2222/tcp
ENTRYPOINT ["/lobo/start"]
