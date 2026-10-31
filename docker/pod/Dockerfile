# Public GPU image with runtime, agent, and SSH. The default target is thin.
# Model chunks download at startup and are checked against the pinned catalog.
# Build and verify images on the authorized Linux build host.
# LLAMA_IMAGE comes from lobo-proto DEFAULT_LLAMA_IMAGE.
ARG LLAMA_IMAGE

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
RUN printf '{"version":"%s","git_sha":"%s","built_by":"manual","llama_image":"%s"}\n' \
      "$VERSION" "$GIT_SHA" "$LLAMA_IMAGE" > /out/release.json

FROM ${LLAMA_IMAGE} AS thin
USER root
RUN apt-get update && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends openssh-server ca-certificates curl jq \
    && rm -f /etc/ssh/ssh_host_* \
    && rm -rf /var/lib/apt/lists/* && mkdir -p /run/sshd
COPY --from=build /out/lobo-agent /lobo/lobo-agent
COPY --from=build /out/release.json /lobo/release.json
COPY --from=tunnel /usr/local/bin/cloudflared /lobo/bin/cloudflared
COPY --chmod=755 docker/pod/start.sh /lobo/start
EXPOSE 2222/tcp
ENTRYPOINT ["/lobo/start"]
