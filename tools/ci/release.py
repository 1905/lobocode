#!/usr/bin/env python3
"""Local replacement for the removed `Release` workflow (.github/workflows/release.yml).

The old workflow ran on a pushed vX.Y.Z tag. Check out that tag, then run the
three steps in order:

  images   Linux x86_64 (Dell or a big builder): build, publish and promote Q6 and Q8
  cli      macOS (the Air): GoReleaser builds and publishes the `lobo` CLI and the
           Homebrew formula. Needs HOMEBREW_TAP_KEY in the environment.
  dmg      macOS (the Air): build lobocode.dmg and attach it to the same release

`cli` refuses to run until both images for the tag are public.
App-only releases (app-vX.Y.Z) use app_release.py instead.
"""

from __future__ import annotations

import argparse
import os
import platform
import subprocess

from common import (IMAGE, REPO, ROOT, RUST_TOOLCHAIN, exact_tag, head_sha, log, main_wrapper, output,
                    remote_digest, remote_ref_sha, require, require_clean, require_platform, require_tools,
                    run)

GORELEASER = "2.13.3"
ZIG = "0.16.0"
ZIGBUILD = "0.23.4"
CLI_TARGETS = ("x86_64-apple-darwin", "aarch64-apple-darwin",
               "x86_64-unknown-linux-musl", "aarch64-unknown-linux-musl")


def release_tag() -> str:
    """The v* tag at HEAD. It must also exist on origin at the same commit."""
    tag = exact_tag("v*")
    require(tag, "HEAD has no v* tag. Check out the release tag first.")
    require(remote_ref_sha(f"refs/tags/{tag}") == head_sha(), f"tag {tag} is not pushed to origin at HEAD")
    require_clean(what="working tree (GoReleaser refuses dirty trees)")
    return tag


def step_images(args) -> None:
    tag = release_tag()
    cmd = ["python3", "tools/ci/pod_image.py", "build", "--publish"]
    # The old workflow moved latest-* only for stable tags (no '-' suffix).
    if "-" not in tag:
        cmd.append("--promote")
    if args.builder:
        cmd += ["--builder", args.builder]
    if args.storage:
        cmd += ["--storage", args.storage]
    run(cmd)


def require_version(tool: str, version_cmd: list, expected: str) -> None:
    found = output(version_cmd)
    if expected not in found:
        log(f"warning: {tool} {expected} expected (CI pin), found: {found.splitlines()[0] if found else '?'}")


def step_cli(args) -> None:
    require(platform.system() == "Darwin", "the CLI release needs macOS (Apple SDK for darwin targets)")
    tag = release_tag()
    require_tools("goreleaser", "zig", "cargo-zigbuild", "rustup", "gh",
                  hint=f"brew install goreleaser zig; cargo install cargo-zigbuild@{ZIGBUILD}")
    require(os.environ.get("HOMEBREW_TAP_KEY"), "HOMEBREW_TAP_KEY is not set (deploy key for 1905/homebrew-tap)")
    require_version("goreleaser", ["goreleaser", "--version"], GORELEASER)
    require_version("zig", ["zig", "version"], ZIG)
    require_version("cargo-zigbuild", ["cargo", "zigbuild", "--version"], ZIGBUILD)
    if not args.skip_image_check:
        # Replaces `needs: images`: the CLI must not ship before its images are public.
        for model in ("q6", "q8"):
            digest = remote_digest(f"{tag}-{model}")
            require(digest, f"{IMAGE}:{tag}-{model} is not public yet. Run the images step first.")
            log(f"{IMAGE}:{tag}-{model} -> {digest}")
    run(["rustup", "target", "add", "--toolchain", RUST_TOOLCHAIN, *CLI_TARGETS])
    token = output(["gh", "auth", "token"])
    run(["goreleaser", "release", "--clean"], env={"GITHUB_TOKEN": token})


def step_dmg(_args) -> None:
    require_platform("Darwin", "arm64", "the DMG build")
    tag = release_tag()
    require_tools("pnpm", "hdiutil", "gh")
    exists = subprocess.run(["gh", "release", "view", tag, "--repo", REPO], capture_output=True)
    require(exists.returncode == 0, f"release {tag} does not exist yet. Run the cli step first.")
    run("make dmg")
    run(["gh", "release", "upload", tag, ROOT / "bin/lobocode.dmg", "--clobber", "--repo", REPO])
    log(f"lobocode.dmg attached to {tag} (source {head_sha()})")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    commands = parser.add_subparsers(dest="command", required=True)
    images = commands.add_parser("images", help="build, publish and promote both model images")
    images.add_argument("--builder", help="docker buildx builder name")
    images.add_argument("--storage", help="path whose free space is checked")
    images.set_defaults(func=step_images)
    cli = commands.add_parser("cli", help="publish the CLI with GoReleaser")
    cli.add_argument("--skip-image-check", action="store_true", help="do not require public tag images")
    cli.set_defaults(func=step_cli)
    commands.add_parser("dmg", help="attach lobocode.dmg to the release").set_defaults(func=step_dmg)
    args = parser.parse_args()
    args.func(args)


if __name__ == "__main__":
    main_wrapper(main)
