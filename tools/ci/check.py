#!/usr/bin/env python3
"""Local replacement for the removed `Rust` CI workflow (.github/workflows/rust.yml).

Jobs and where they run:
  protocol     any host with cargo, go and python3 (normally the Dell)
  agent-musl   Linux x86_64 (the Dell)
  core-macos   macOS arm64 (the Air)
  app          macOS arm64 (the Air), includes the native startup/settings smoke

With no --job, every job that fits this host runs. A job that passes on a clean
tree writes bin/ci-receipts/<sha>-<job>.json. app_release.py requires all four.
"""

from __future__ import annotations

import argparse
import json
import subprocess
from pathlib import Path

from common import (CHECK_JOBS, RUST_TOOLCHAIN, ROOT, Failure, dirty_paths, head_sha, is_platform, log, main_wrapper,
                    output, require, require_clean, require_generated_unchanged, require_tools, run,
                    write_receipt)

MUSL = "x86_64-unknown-linux-musl"


def job_protocol(_args) -> dict:
    require_tools("cargo", "go", "python3")
    run("make rust-lint rust-test")
    run("make proto-ts")
    require_generated_unchanged("app/ui/src/proto")
    run("make proto-fixtures")
    require_generated_unchanged("crates/lobo-proto/fixtures", "crates/lobo-proto/catalog.json")
    run("make core-fixtures")
    require_generated_unchanged("crates/lobo-core/fixtures")
    run("cargo test --locked -p lobo-core --test go_interop", env={"LOBO_GO_INTEROP": "1"})
    run("make cli-fixtures")
    require_generated_unchanged("crates/lobo-cli/tests/fixtures", "crates/lobo-cli/tests/goldens")
    return {}


def job_agent_musl(_args) -> dict:
    require_tools("cargo", "rustup", "file")
    installed = output(["rustup", "target", "list", "--installed", "--toolchain", RUST_TOOLCHAIN])
    if MUSL not in installed.split():
        run(["rustup", "target", "add", MUSL, "--toolchain", RUST_TOOLCHAIN])
    # CI used musl-gcc from musl-tools. cargo-zigbuild needs no root to install.
    if subprocess.run(["which", "musl-gcc"], capture_output=True).returncode == 0:
        run(f"cargo build --release --locked -p lobo-agent --target {MUSL}")
    else:
        require_tools("cargo-zigbuild", "zig",
                      hint="Install musl-tools (needs root) or `cargo install cargo-zigbuild` plus zig.")
        run(f"cargo zigbuild --release --locked -p lobo-agent --target {MUSL}")
    # CARGO_TARGET_DIR or a cargo config can move target/ (the Dell uses /wrk/rust/target).
    target = json.loads(output(["cargo", "metadata", "--format-version", "1", "--no-deps"]))["target_directory"]
    binary = Path(target) / MUSL / "release" / "lobo-agent"
    description = output(["file", binary])
    log(description)
    require("statically linked" in description or "static-pie linked" in description,
            "lobo-agent is not statically linked")
    return {"agent_bytes": binary.stat().st_size}


def job_core_macos(_args) -> dict:
    require_tools("cargo")
    run("cargo test --locked -p lobo-core -p lobo-cli --features lobo-cli/test-fakes -j 2")
    return {}


def job_app(args) -> dict:
    require_tools("cargo", "pnpm", "node", "codesign", "ditto",
                  hint="pnpm 9.11.0: `npm install -g pnpm@9.11.0`.")
    run("pnpm -C app/ui install --frozen-lockfile")
    run("pnpm -C app/ui build")
    run("make app-fixtures")
    run("make app-lint app-test")
    require_generated_unchanged("app/ui/src/fixtures", "app/ui/src/gen", "app/ui/src/proto")
    run("make mac")
    app = ROOT / "bin/lobocode.app"
    binary = app / "Contents/MacOS/lobocode"
    require(binary.is_file() and binary.stat().st_mode & 0o111, f"{binary} is missing or not executable")
    require(not (app / "Contents/Resources/lobo").exists(), "the CLI is bundled in the app")
    run(["codesign", "--verify", "--deep", "--strict", app])
    smoke = subprocess.run([str(binary), "--lobo-local-run", "local", "run"],
                           capture_output=True, text=True, timeout=10)
    require(smoke.returncode == 2 and "cloud GPUs only" in smoke.stderr,
            f"local run was not refused: exit {smoke.returncode}, stderr {smoke.stderr[-500:]!r}")
    zipped = ROOT / "bin/lobocode-macos-arm64.zip"
    if zipped.exists():
        zipped.unlink()
    run(["ditto", "-c", "-k", "--keepParent", app, zipped])
    if args.no_native_e2e:
        log("native startup and settings smoke skipped (--no-native-e2e)")
        return {"native_e2e": False}
    # Evidence lands in bin/app-e2e.
    env = {"CARGO_BUILD_JOBS": "2"}
    run("make app-e2e-build", env=env)
    run("python3 tools/native_app_e2e.py --setup-only", env=env)
    run("python3 tools/native_app_e2e.py --legacy-config", env=env)
    return {"native_e2e": True}


JOBS = {
    "protocol": (job_protocol, None),
    "agent-musl": (job_agent_musl, ("Linux", "x86_64")),
    "core-macos": (job_core_macos, ("Darwin", "arm64")),
    "app": (job_app, ("Darwin", "arm64")),
}
assert tuple(JOBS) == CHECK_JOBS


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--job", action="append", choices=CHECK_JOBS,
                        help="job to run (repeatable). Default: every job that fits this host.")
    parser.add_argument("--no-native-e2e", action="store_true", help="app job: skip the native UI smoke")
    parser.add_argument("--allow-dirty", action="store_true",
                        help="run on uncommitted changes. No receipt is written.")
    args = parser.parse_args()

    jobs = args.job or [name for name, (_, host) in JOBS.items() if host is None or is_platform(*host)]
    skipped = [name for name in CHECK_JOBS if name not in jobs]
    for name in jobs:
        host = JOBS[name][1]
        if host is not None:
            require(is_platform(*host), f"job {name} needs {host[0]} {host[1]}")
    if not args.allow_dirty:
        require_clean(what="working tree (commit first, or pass --allow-dirty)")
    sha = head_sha()
    log(f"source {sha}, jobs: {', '.join(jobs)}" + (f"; not run here: {', '.join(skipped)}" if skipped else ""))

    failed = []
    for name in jobs:
        log(f"--- job {name}")
        try:
            extra = JOBS[name][0](args)
        except (Failure, subprocess.TimeoutExpired) as error:
            log(f"job {name} FAILED: {error}")
            failed.append(name)
            continue
        if args.allow_dirty:
            log(f"job {name} passed (dirty tree, no receipt)")
        elif head_sha() != sha or dirty_paths():
            log(f"job {name} passed, but HEAD moved or the tree changed during the run; no receipt")
        else:
            log(f"job {name} passed; receipt {write_receipt(sha, name, **extra).relative_to(ROOT)}")
    require(not failed, f"failed jobs: {', '.join(failed)}")


if __name__ == "__main__":
    main_wrapper(main)
