#!/usr/bin/env python3
"""Local replacement for the removed `Pod image` workflow (.github/workflows/pod-image.yml).

Runs on Linux x86_64 with Docker buildx. Publishing needs a GHCR login with
write:packages, for example:
  gh auth refresh -s write:packages
  gh auth token | docker login ghcr.io -u <github-user> --password-stdin

  build      build complete Q6/Q8 model images; --publish pushes immutable tags,
             --promote also moves latest-<model> (stable vX.Y.Z tag at HEAD only)
  prebuilt   verify a prepared Q6 OCI export; --publish pushes it with oras,
             --promote-latest also moves latest-q6
"""

from __future__ import annotations

import argparse
import json
import re
import shutil
import tempfile
from pathlib import Path

from common import (DIGEST_RE, IMAGE, REPO, ROOT, SHA_RE, exact_tag, head_sha, log, main_wrapper, output,
                    remote_digest, require, require_clean, require_platform, require_tools, run)

MODELS = ("q6", "q8")
STABLE_TAG = re.compile(r"v[0-9]+\.[0-9]+\.[0-9]+")
DIGEST_DIR = ROOT / "bin" / "pod-image"


def free_gib(path: str) -> float:
    return shutil.disk_usage(path).free / 1024 ** 3


def llama_image() -> str:
    source = (ROOT / "crates/lobo-proto/src/release.rs").read_text()
    match = re.search(r'DEFAULT_LLAMA_IMAGE: &str = "(.*)";', source)
    require(match, "DEFAULT_LLAMA_IMAGE not found in crates/lobo-proto/src/release.rs")
    return match[1]


def require_anonymous(reference: str, digest: str) -> None:
    # An authenticated push alone does not prove this is a public package.
    actual = remote_digest(reference)
    require(actual == digest, f"anonymous {reference} resolves to {actual}, expected {digest}")
    log(f"{IMAGE}:{reference} -> {digest} (anonymous access verified)")


def build(args) -> None:
    require_platform("Linux", "x86_64", "a complete model image build")
    require_tools("docker")
    require(not args.promote or args.publish, "--promote requires --publish")
    require_clean(what="working tree (images bake in the commit SHA)")
    storage = args.storage or output(["docker", "info", "--format", "{{.DockerRootDir}}"])
    gib = free_gib(storage)
    require(gib >= args.min_free_gib,
            f"{storage} has {gib:.0f} GiB free; a complete model build needs {args.min_free_gib} GiB. "
            "Use --builder with a BuildKit builder on larger storage, then --storage <its data dir>.")

    sha = head_sha()[:12]
    tag = exact_tag("v*")
    version = tag or f"sha-{sha}"
    if args.promote:
        require(set(args.model) == set(MODELS), "--promote needs both models in one run")
        require(tag and STABLE_TAG.fullmatch(tag), f"--promote needs a stable vX.Y.Z tag at HEAD, got {tag}")
    llama = llama_image()
    digests = {}
    for model in args.model:
        tags = [f"{IMAGE}:sha-{sha}-{model}"] + ([f"{IMAGE}:{version}-{model}"] if tag else [])
        with tempfile.TemporaryDirectory(prefix="lobocode-build-") as scratch:
            metadata = Path(scratch) / "metadata.json"
            cmd = ["docker", "buildx", "build", "--file", "docker/pod/Dockerfile", "--platform", "linux/amd64",
                   "--provenance=false", "--metadata-file", metadata,
                   "--build-arg", f"VERSION={version}", "--build-arg", f"GIT_SHA={sha}",
                   "--build-arg", f"MODEL={model}", "--build-arg", f"LLAMA_IMAGE={llama}",
                   "--label", f"org.opencontainers.image.source=https://github.com/{REPO}"]
            if args.builder:
                cmd += ["--builder", args.builder]
            for name in tags:
                cmd += ["--tag", name]
            cmd += ["--push"] if args.publish else []
            run(cmd + ["."])
            if not args.publish:
                continue
            digest = json.loads(metadata.read_text()).get("containerimage.digest", "")
        require(DIGEST_RE.fullmatch(digest), f"buildx reported no valid digest for {model}")
        require_anonymous(digest, digest)
        DIGEST_DIR.mkdir(parents=True, exist_ok=True)
        (DIGEST_DIR / f"{model}.txt").write_text(digest + "\n")
        digests[model] = digest
    if args.promote:
        # Move latest only after both models were built and verified anonymously.
        for model in MODELS:
            run(["docker", "buildx", "imagetools", "create", "--prefer-index=false",
                 "--tag", f"{IMAGE}:latest-{model}", f"{IMAGE}@{digests[model]}"])
            require_anonymous(f"latest-{model}", digests[model])


def prebuilt(args) -> None:
    require(SHA_RE.fullmatch(args.source_sha), "--source-sha must be a full 40-character commit")
    require(DIGEST_RE.fullmatch(args.image_digest), "--image-digest must be sha256:<64 hex>")
    require(not args.promote_latest or args.publish, "--promote-latest requires --publish")
    run(["python3", "tools/verify_cloud_candidate.py", "--layout", args.layout, "--metadata", args.metadata,
         "--source-sha", args.source_sha, "--image-digest", args.image_digest])
    log("prepared Q6 image verification passed. No model was started.")
    if not args.publish:
        return
    require_tools("oras", hint="Install oras 1.3.4: https://github.com/oras-project/oras/releases")
    tag = f"sha-{args.source_sha[:12]}-q6"
    existing = remote_digest(tag)
    require(existing in (None, args.image_digest), f"{tag} already points to {existing}. Preserving it.")
    # oras reads the docker credential store, so `docker login ghcr.io` covers it.
    run(["oras", "cp", "--force", "--concurrency", "3", "--from-oci-layout",
         f"{args.layout}@{args.image_digest}", f"{IMAGE}:{tag}"])
    require_anonymous(tag, args.image_digest)
    if args.promote_latest:
        existing = remote_digest("latest-q6")
        require(existing in (None, args.image_digest), f"latest-q6 now points to {existing}. Preserving it.")
        run(["oras", "tag", f"{IMAGE}@{args.image_digest}", "latest-q6"])
        require_anonymous("latest-q6", args.image_digest)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    commands = parser.add_subparsers(dest="command", required=True)

    b = commands.add_parser("build", help="build complete model images from this checkout")
    b.add_argument("--model", action="append", choices=MODELS, help="repeatable; default: q6 and q8")
    b.add_argument("--publish", action="store_true", help="push immutable sha-/version- tags to GHCR")
    b.add_argument("--promote", action="store_true", help="also move latest-q6/latest-q8 (stable tag only)")
    b.add_argument("--builder", help="docker buildx builder name")
    b.add_argument("--storage", help="path whose free space is checked (default: Docker root dir)")
    b.add_argument("--min-free-gib", type=int, default=200)
    b.set_defaults(func=build)

    p = commands.add_parser("prebuilt", help="verify and optionally publish a prepared Q6 OCI export")
    p.add_argument("--layout", type=Path, required=True, help="OCI layout directory, e.g. .../q6-oci")
    p.add_argument("--metadata", type=Path, required=True, help="q6-metadata.json next to the layout")
    p.add_argument("--source-sha", required=True)
    p.add_argument("--image-digest", required=True)
    p.add_argument("--publish", action="store_true", help="push sha-<sha12>-q6 to GHCR with oras")
    p.add_argument("--promote-latest", action="store_true", help="also move latest-q6")
    p.set_defaults(func=prebuilt)

    args = parser.parse_args()
    if args.command == "build" and not args.model:
        args.model = list(MODELS)
    args.func(args)


if __name__ == "__main__":
    main_wrapper(main)
