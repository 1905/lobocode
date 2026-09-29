# Pod image: llama.cpp server + lobo-agent (Rust, static musl) baked in.
# Built by pod-image.yml; LLAMA_IMAGE comes from lobo-proto DEFAULT_LLAMA_IMAGE.
ARG LLAMA_IMAGE

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
    LOBO_VERSION="$VERSION" cargo build --release --locked -p lobo-agent --target x86_64-unknown-linux-musl && \
    mkdir -p /out && cp target/x86_64-unknown-linux-musl/release/lobo-agent /out/lobo-agent
RUN printf '{"version":"%s","git_sha":"%s","built_by":"github-actions","llama_image":"%s"}\n' \
      "$VERSION" "$GIT_SHA" "$LLAMA_IMAGE" > /out/release.json

FROM ${LLAMA_IMAGE}
COPY --from=build /out/lobo-agent /lobo/lobo-agent
COPY --from=build /out/release.json /lobo/release.json
