# Pod image: llama.cpp server + lobo-agent baked in. `lobo up --image` (or LOBO_POD_IMAGE) rents this
# instead of the plain llama image, and the bootstrap skips apt + the release zip download.
# Built and pushed by .github/workflows/pod-image.yml only; it passes LLAMA_IMAGE = release.DefaultLlamaImage.
ARG LLAMA_IMAGE

FROM golang:1.26 AS build
ARG VERSION=dev
ARG GIT_SHA=none
ARG LLAMA_IMAGE
WORKDIR /src
COPY go.mod go.sum ./
RUN --mount=type=cache,target=/go/pkg/mod go mod download
COPY cmd/lobo-agent ./cmd/lobo-agent
COPY internal ./internal
RUN --mount=type=cache,target=/go/pkg/mod --mount=type=cache,target=/root/.cache/go-build \
    CGO_ENABLED=0 GOOS=linux GOARCH=amd64 go build -ldflags "-s -w -X main.version=${VERSION}" -o /out/lobo-agent ./cmd/lobo-agent
# /api/version body: the release.Manifest fields the CLI and the mac app read (version, git_sha) + the base image.
RUN printf '{"version":"%s","git_sha":"%s","built_by":"github-actions","llama_image":"%s"}\n' \
      "$VERSION" "$GIT_SHA" "$LLAMA_IMAGE" > /out/release.json

FROM ${LLAMA_IMAGE}
COPY --from=build /out/lobo-agent /lobo/lobo-agent
COPY --from=build /out/release.json /lobo/release.json
