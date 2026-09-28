# Pod image: llama.cpp server + lobo-agent baked in. `lobo up --image` (or LOBO_POD_IMAGE) rents this
# instead of the plain llama image, and the bootstrap skips apt + the release zip download.
# Built and pushed by .github/workflows/pod-image.yml only. Keep LLAMA_IMAGE = release.DefaultLlamaImage.
ARG LLAMA_IMAGE=ghcr.io/ggml-org/llama.cpp:server-cuda-b11118

FROM golang:1.26 AS build
ARG VERSION=dev
ARG GIT_SHA=none
ARG LLAMA_IMAGE
WORKDIR /src
COPY go.mod go.sum ./
RUN go mod download
COPY . .
RUN CGO_ENABLED=0 GOOS=linux GOARCH=amd64 go build -ldflags "-s -w -X main.version=${VERSION}" -o /out/lobo-agent ./cmd/lobo-agent
# /api/version body. Same shape as release.json in a release zip (release.Manifest).
RUN printf '{"version":"%s","git_sha":"%s","built_by":"github-actions","llama_image":"%s"}\n' \
      "$VERSION" "$GIT_SHA" "$LLAMA_IMAGE" > /out/release.json

FROM ${LLAMA_IMAGE}
COPY --from=build /out/lobo-agent /lobo/lobo-agent
COPY --from=build /out/release.json /lobo/release.json
