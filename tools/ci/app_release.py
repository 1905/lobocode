#!/usr/bin/env python3
"""Local replacement for the removed `App release` workflow (.github/workflows/app-release.yml).

Publishes the standalone Mac app for an existing app-vX.Y.Z tag. Run it on the
Air (macOS arm64) from a clean checkout of master at that tag.

Gates, all required:
  - the tag, local HEAD and origin/master are the same commit
  - check receipts for protocol, agent-musl, core-macos and app (with native
    smoke) exist for that commit in bin/ci-receipts (see check.py)
  - app versions in Cargo.toml, Cargo.lock, tauri.conf.json, package.json match
  - public latest-q6 passes the anonymous manifest/config check

Then it builds, signs ad-hoc, packs the DMG, validates it, uploads to an owned
draft, publishes without moving the CLI "latest" release, and re-downloads
anonymously. A retry after publication only re-validates the published assets.
--dry-run stops after the gates and runs on any host.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import plistlib
import re
import subprocess
import tempfile
import time
import urllib.request
from pathlib import Path

from common import (CHECK_JOBS, RECEIPTS, REPO, ROOT, head_sha, log, main_wrapper, read_receipt, releases_for_tag,
                    gh_api, remote_ref_sha, require, require_clean, require_platform, require_tools, run,
                    verify_public_image)

ASSETS = ("lobocode.dmg", "app-release.json", "SHA256SUMS")
BIN = ROOT / "bin"


def marker(sha: str) -> str:
    return f"<!-- lobocode-app-source:{sha} -->"


def owned(release: dict, sha: str, version: str) -> bool:
    return (release["target_commitish"] == sha and not release["prerelease"]
            and release["name"] == f"Lobocode app {version}" and marker(sha) in (release.get("body") or ""))


def package_version(cargo_toml: Path) -> str:
    """[package] version without tomllib (Python 3.9 on the Air)."""
    section = ""
    for line in cargo_toml.read_text().splitlines():
        if line.startswith("["):
            section = line.strip()
        elif section == "[package]":
            match = re.fullmatch(r'version\s*=\s*"([^"]+)"\s*', line)
            if match:
                return match[1]
    return ""


def gate(args) -> dict:
    tag = args.tag
    require(re.fullmatch(r"app-v[0-9]+\.[0-9]+\.[0-9]+", tag), "expected app-vX.Y.Z")
    version = tag[len("app-v"):]
    sha = head_sha()
    require_clean(what="working tree")
    tag_sha = remote_ref_sha(f"refs/tags/{tag}")
    master_sha = remote_ref_sha("refs/heads/master")
    require(tag_sha == sha == master_sha,
            f"tag ({tag_sha}), local HEAD ({sha}) and origin/master ({master_sha}) must be the same commit")

    receipts = []
    missing = []
    for job in CHECK_JOBS:
        record = read_receipt(sha, job, args.receipts)
        if record is None or (job == "app" and not record.get("native_e2e")):
            missing.append(job)
        else:
            receipts.append({key: record[key] for key in ("job", "host", "platform", "finished_at")})
    require(not missing, f"no passing check receipt for {sha} in {args.receipts}: {', '.join(missing)}. "
            "Run tools/ci/check.py on the Dell (protocol, agent-musl) and the Air (core-macos, app), "
            "then copy the Dell receipts into this directory.")

    releases = releases_for_tag(tag)
    require(len(releases) <= 1, "multiple releases use this app tag; refusing to continue")
    release = releases[0] if releases else None
    require(release is None or owned(release, sha, version),
            "existing release does not belong to this app source; refusing to change it")

    lock = (ROOT / "app/src-tauri/Cargo.lock").read_text()
    lock_versions = re.findall(r'\[\[package\]\]\nname = "lobocode-app"\nversion = "([^"]+)"', lock)
    require(package_version(ROOT / "app/src-tauri/Cargo.toml") == version, "app/src-tauri/Cargo.toml version mismatch")
    require(lock_versions == [version], "app/src-tauri/Cargo.lock lobocode-app version mismatch")
    for path in ("app/src-tauri/tauri.conf.json", "app/ui/package.json"):
        require(json.loads((ROOT / path).read_text())["version"] == version, f"{path} version mismatch")

    digest = verify_public_image("latest-q6", "q6")
    log(f"gates passed: {tag} at {sha}")
    return {"tag": tag, "version": version, "sha": sha, "receipts": receipts, "q6": digest,
            "published": bool(release and not release["draft"]), "draft": bool(release and release["draft"])}


def build(state: dict) -> None:
    run(["make", "mac", f"VERSION={state['version']}", f"APP_VERSION={state['version']}", f"COMMIT={state['sha']}"])
    contents = BIN / "lobocode.app/Contents"
    info_path = contents / "Info.plist"
    info = plistlib.loads(info_path.read_bytes())
    info["LobocodeSourceCommit"] = state["sha"]
    info_path.write_bytes(plistlib.dumps(info))
    receipt = {"schema": 2, "tag": state["tag"], "version": state["version"], "source_sha": state["sha"],
               "checks": state["receipts"], "platform": "macos-arm64", "signature": "ad-hoc",
               "notarized": False, "q6_preflight_digest": state["q6"]}
    raw = json.dumps(receipt, indent=2) + "\n"
    (contents / "Resources/app-release.json").write_text(raw)
    (BIN / "app-release.json").write_text(raw)
    run(["codesign", "--force", "--sign", "-", "--deep", BIN / "lobocode.app"])
    run(["codesign", "--verify", "--deep", "--strict", BIN / "lobocode.app"])
    # Reuse the DMG recipe without rebuilding the signed app.
    run(["make", "-o", "mac", "dmg"])
    sums = "".join(f"{hashlib.sha256((BIN / name).read_bytes()).hexdigest()}  {name}\n"
                   for name in ASSETS[:2])
    (BIN / "SHA256SUMS").write_text(sums)


def download_published(state: dict) -> None:
    BIN.mkdir(exist_ok=True)
    run(["gh", "release", "download", state["tag"], "--repo", REPO, "--dir", BIN, "--clobber",
         *sum((["--pattern", name] for name in ASSETS), [])])


def validate(state: dict) -> None:
    lines = (BIN / "SHA256SUMS").read_text().splitlines()
    require(len(lines) == 2, "expected DMG and receipt checksums only")
    expected = {}
    for line in lines:
        match = re.fullmatch(r"([0-9a-f]{64})  (lobocode\.dmg|app-release\.json)", line)
        require(match and match[2] not in expected, "invalid checksum file")
        expected[match[2]] = match[1]
    for name, digest in expected.items():
        require(hashlib.sha256((BIN / name).read_bytes()).hexdigest() == digest, f"{name} checksum mismatch")
    receipt = json.loads((BIN / "app-release.json").read_text())
    require(receipt["schema"] in (1, 2) and receipt["tag"] == state["tag"], "receipt tag mismatch")
    require(receipt["source_sha"] == state["sha"] and receipt["version"] == state["version"],
            "receipt source or version mismatch")
    require(receipt["platform"] == "macos-arm64" and receipt["signature"] == "ad-hoc"
            and receipt["notarized"] is False, "receipt platform or signature mismatch")
    require(re.fullmatch(r"sha256:[0-9a-f]{64}", receipt["q6_preflight_digest"]), "receipt Q6 digest invalid")
    with tempfile.TemporaryDirectory(prefix="lobocode-dmg-") as mount:
        run(["hdiutil", "attach", BIN / "lobocode.dmg", "-readonly", "-nobrowse", "-mountpoint", mount])
        try:
            root = Path(mount)
            require({p.name for p in root.iterdir()} == {"lobocode.app", "Applications"}, "unexpected DMG payload")
            require((root / "Applications").is_symlink() and os.readlink(root / "Applications") == "/Applications",
                    "Applications is not a /Applications symlink")
            app = root / "lobocode.app"
            contents = app / "Contents"
            info = plistlib.loads((contents / "Info.plist").read_bytes())
            require(info["CFBundleIdentifier"] == "io.github.1905.lobocode", "wrong bundle identifier")
            require(info["CFBundleShortVersionString"] == receipt["version"], "bundle version mismatch")
            require(info["LobocodeSourceCommit"] == receipt["source_sha"], "bundle source commit mismatch")
            require((contents / "Resources/app-release.json").read_bytes() == (BIN / "app-release.json").read_bytes(),
                    "embedded receipt differs from the release asset")
            require({p.name for p in (contents / "MacOS").iterdir()} == {"lobocode"}, "unexpected bundled executable")
            require(not any(p.name in ("lobo", "lobo-cli", "lobo-agent") for p in app.rglob("*")),
                    "CLI or agent bundled in app")
            binary = contents / "MacOS/lobocode"
            require(os.access(binary, os.X_OK), "app binary is not executable")
            archs = subprocess.check_output(["lipo", "-archs", str(binary)], text=True).strip()
            require(archs == "arm64", f"app binary architectures: {archs}")
            run(["codesign", "--verify", "--deep", "--strict", app])
            signature = subprocess.check_output(["codesign", "-d", "--verbose=4", str(app)],
                                                stderr=subprocess.STDOUT, text=True)
            require("Signature=adhoc" in signature, "expected an ad-hoc signature")
        finally:
            run(["hdiutil", "detach", mount])
    log("DMG, receipt, signature and standalone contents validated")


def prepare_draft(state: dict, notes: str) -> int:
    tag, sha, version = state["tag"], state["sha"], state["version"]
    releases = releases_for_tag(tag)
    require(len(releases) <= 1, "multiple releases use this app tag; refusing to continue")
    if releases:
        release = releases[0]
        require(release["draft"] and owned(release, sha, version),
                "refusing to overwrite a published or unrelated release")
        assets = gh_api(f"repos/{REPO}/releases/{release['id']}/assets?per_page=100", paginate=True)
        require(all(asset["name"] in ASSETS for asset in assets), "unexpected assets in draft; refusing to overwrite")
        return release["id"]
    checks = ", ".join(f"{r['job']} on {r['host']}" for r in state["receipts"])
    body = "\n".join([
        marker(sha), "", "Standalone cloud-only Mac app for Apple Silicon (macOS 13 or later).", "",
        notes.strip(), "",
        "Download `lobocode.dmg` and drag the app to Applications.",
        "The app has an ad-hoc signature. It is not Developer ID signed or notarized.",
        "Use macOS Privacy & Security to allow the app if Gatekeeper blocks it.", "",
        f"Source: {sha}", f"Required checks: {checks}",
        "Asset checksums: `SHA256SUMS`. Build receipt: `app-release.json`.",
    ])
    release = gh_api(f"repos/{REPO}/releases", method="POST", body={
        "tag_name": tag, "target_commitish": sha, "name": f"Lobocode app {version}", "body": body,
        "draft": True, "prerelease": False, "make_latest": "false"})
    log(f"created draft release {release['id']}")
    return release["id"]


def upload_and_verify(state: dict) -> None:
    # Only the verified, source-owned draft reaches this step. A retry may replace partial uploads.
    run(["gh", "release", "upload", state["tag"], "--repo", REPO, "--clobber", *(BIN / name for name in ASSETS)])
    with tempfile.TemporaryDirectory(prefix="lobocode-assets-") as folder:
        run(["gh", "release", "download", state["tag"], "--repo", REPO, "--dir", folder,
             *sum((["--pattern", name] for name in ASSETS), [])])
        for name in ASSETS:
            require((Path(folder) / name).read_bytes() == (BIN / name).read_bytes(), f"uploaded {name} differs")


def publish(state: dict, release_id: int) -> None:
    tag, sha = state["tag"], state["sha"]
    for ref in (f"refs/tags/{tag}", "refs/heads/master"):
        require(remote_ref_sha(ref) == sha, f"{ref} moved during the build; refusing publication")
    release = gh_api(f"repos/{REPO}/releases/{release_id}")
    require(release["id"] == release_id and release["tag_name"] == tag and release["draft"]
            and owned(release, sha, state["version"]), "draft identity changed; refusing publication")
    assets = gh_api(f"repos/{REPO}/releases/{release_id}/assets?per_page=100", paginate=True)
    require(len(assets) == len(ASSETS), "unexpected release asset count")
    for name in ASSETS:
        data = (BIN / name).read_bytes()
        asset = next((a for a in assets if a["name"] == name), None)
        require(asset and asset["state"] == "uploaded" and asset["size"] == len(data)
                and asset.get("digest") == "sha256:" + hashlib.sha256(data).hexdigest(),
                f"uploaded asset mismatch: {name}")
    published = gh_api(f"repos/{REPO}/releases/{release_id}", method="PATCH",
                       body={"draft": False, "make_latest": "false"})
    log(f"published {published['html_url']}")


def verify_anonymous_downloads(state: dict) -> None:
    for name in ASSETS:
        url = f"https://github.com/{REPO}/releases/download/{state['tag']}/{name}"
        for attempt in range(6):
            try:
                with urllib.request.urlopen(url, timeout=180) as response:
                    data = response.read()
                break
            except OSError as error:
                require(attempt < 5, f"anonymous download of {name} failed: {error}")
                time.sleep(5)
        require(data == (BIN / name).read_bytes(), f"anonymous {name} differs from the verified asset")
    log("anonymous published downloads match")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("tag", help="existing app-vX.Y.Z tag at the tested master commit")
    parser.add_argument("--notes", type=Path, help="markdown file with this release's change bullets")
    parser.add_argument("--receipts", type=Path, default=RECEIPTS, help=f"default: {RECEIPTS.relative_to(ROOT)}")
    parser.add_argument("--dry-run", action="store_true", help="run the gates only; works on any host")
    args = parser.parse_args()

    if not args.dry_run:
        require_platform("Darwin", "arm64", "the app release")
        require_tools("gh", "pnpm", "codesign", "hdiutil", "lipo", "shasum")
    state = gate(args)
    if args.dry_run:
        log("dry run: " + json.dumps(state))
        return
    if state["published"]:
        log("release already published; re-validating its assets")
        download_published(state)
        validate(state)
    else:
        require(state["draft"] or (args.notes and args.notes.is_file()),
                "--notes FILE is required when no draft release exists yet")
        build(state)
        validate(state)
        release_id = prepare_draft(state, args.notes.read_text() if args.notes else "")
        upload_and_verify(state)
        publish(state, release_id)
    verify_anonymous_downloads(state)


if __name__ == "__main__":
    main_wrapper(main)
