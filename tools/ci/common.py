"""Shared helpers for the local scripts that replace the removed GitHub Actions workflows.

Python 3.9 compatible: the Air only has the system python3.
"""

from __future__ import annotations

import hashlib
import json
import os
import platform
import re
import shutil
import socket
import subprocess
import sys
import time
import urllib.error
import urllib.request
from pathlib import Path
from typing import Any, Optional

ROOT = Path(__file__).resolve().parents[2]
REPO = "1905/lobocode"
IMAGE = "ghcr.io/1905/lobocode"
RUST_TOOLCHAIN = "1.98.1"
RECEIPTS = ROOT / "bin" / "ci-receipts"
CHECK_JOBS = ("protocol", "agent-musl", "core-macos", "app")
SHA_RE = re.compile(r"[0-9a-f]{40}")
DIGEST_RE = re.compile(r"sha256:[0-9a-f]{64}")

MANIFEST_INDEX_TYPES = (
    "application/vnd.oci.image.index.v1+json",
    "application/vnd.docker.distribution.manifest.list.v2+json",
)
MANIFEST_IMAGE_TYPES = (
    "application/vnd.oci.image.manifest.v1+json",
    "application/vnd.docker.distribution.manifest.v2+json",
)
MAX_DOCUMENT = 4 * 1024 * 1024


class Failure(Exception):
    """A check failed. main() prints the message and exits 1."""


def log(message: str) -> None:
    print(f"==> {message}", file=sys.stderr, flush=True)


def require(condition: Any, message: str) -> None:
    if not condition:
        raise Failure(message)


def main_wrapper(main) -> None:
    try:
        main()
    except Failure as error:
        print(f"error: {error}", file=sys.stderr)
        sys.exit(1)
    except KeyboardInterrupt:
        sys.exit(130)


def run(cmd, *, env: Optional[dict] = None, cwd: Path = ROOT, capture: bool = False,
        check: bool = True) -> subprocess.CompletedProcess:
    """Run a list command, or a string through bash. Prints the command first."""
    shell = isinstance(cmd, str)
    log(cmd if shell else " ".join(str(part) for part in cmd))
    result = subprocess.run(
        ["bash", "-euo", "pipefail", "-c", cmd] if shell else [str(part) for part in cmd],
        cwd=cwd, env={**os.environ, **(env or {})}, text=True,
        stdout=subprocess.PIPE if capture else None,
        stderr=subprocess.PIPE if capture else None,
    )
    if check and result.returncode != 0:
        if capture and result.stderr:
            sys.stderr.write(result.stderr)
        raise Failure(f"command failed with exit code {result.returncode}")
    return result


def output(cmd, **kwargs) -> str:
    return run(cmd, capture=True, **kwargs).stdout.strip()


def require_tools(*names: str, hint: str = "") -> None:
    missing = [name for name in names if shutil.which(name) is None]
    require(not missing, f"missing tools: {', '.join(missing)}. {hint}".strip())


def is_platform(system: str, machine: str) -> bool:
    return platform.system() == system and platform.machine() in (
        (machine, "aarch64") if machine == "arm64" else (machine, "amd64") if machine == "x86_64" else (machine,))


def require_platform(system: str, machine: str, why: str) -> None:
    require(is_platform(system, machine),
            f"{why} needs {system} {machine}; this host is {platform.system()} {platform.machine()}")


# Git -------------------------------------------------------------------------

def git(*args: str) -> str:
    return subprocess.run(["git", *args], cwd=ROOT, text=True, check=True,
                          capture_output=True).stdout.strip()


def head_sha() -> str:
    return git("rev-parse", "HEAD")


def dirty_paths(*paths: str) -> list:
    status = git("status", "--porcelain", "--untracked-files=all", "--", *paths)
    return [line for line in status.splitlines() if line]


def require_clean(*paths: str, what: str = "working tree") -> None:
    dirty = dirty_paths(*paths)
    require(not dirty, f"{what} has uncommitted changes:\n" + "\n".join(dirty[:20]))


def require_generated_unchanged(*paths: str) -> None:
    """Same rule as the old CI: regenerated files must equal the committed ones."""
    run(["git", "--no-pager", "diff", "--exit-code", "--stat", "--", *paths])
    require_clean(*paths, what="generated files")


def remote_ref_sha(ref: str) -> Optional[str]:
    """SHA of a ref on origin. Peels annotated tags."""
    lines = git("ls-remote", "origin", ref, ref + "^{}").splitlines()
    shas = {name: sha for sha, name in (line.split("\t") for line in lines)}
    return shas.get(ref + "^{}") or shas.get(ref)


def exact_tag(pattern: str) -> Optional[str]:
    result = subprocess.run(["git", "describe", "--exact-match", "--tags", "--match", pattern, "HEAD"],
                            cwd=ROOT, text=True, capture_output=True)
    return result.stdout.strip() or None


# GitHub (through the gh CLI and its stored login) ------------------------------

def gh_api(path: str, *, method: str = "GET", body: Optional[dict] = None, paginate: bool = False) -> Any:
    cmd = ["gh", "api", "--method", method, path]
    if paginate:
        cmd += ["--paginate", "--jq", ".[]"]
    if body is not None:
        cmd += ["--input", "-"]
    result = subprocess.run(cmd, text=True, capture_output=True,
                            input=json.dumps(body) if body is not None else None)
    if result.returncode != 0:
        raise Failure(f"gh api {method} {path} failed: {result.stderr.strip() or result.stdout.strip()}")
    if paginate:
        return [json.loads(line) for line in result.stdout.splitlines() if line.strip()]
    return json.loads(result.stdout) if result.stdout.strip() else None


def releases_for_tag(tag: str) -> list:
    # The tag lookup endpoint returns 404 for drafts. Authenticated listing includes them.
    return [release for release in gh_api(f"repos/{REPO}/releases?per_page=100", paginate=True)
            if release["tag_name"] == tag]


# Anonymous GHCR access ---------------------------------------------------------

def anonymous_token() -> str:
    # Issued without credentials. Proves the package is public, not just pushable.
    url = f"https://ghcr.io/token?service=ghcr.io&scope=repository:{REPO}:pull"
    with urllib.request.urlopen(url, timeout=30) as response:
        token = json.load(response)["token"]
    require(isinstance(token, str) and token, "GHCR returned no anonymous token")
    return token


def registry_get(path: str, descriptor: Optional[dict] = None, token: Optional[str] = None):
    """Fetch a manifest or blob anonymously. Returns (document, sha256 digest), or None on 404."""
    request = urllib.request.Request(f"https://ghcr.io/v2/{REPO}/{path}", headers={
        "Authorization": "Bearer " + (token or anonymous_token()),
        "Accept": ", ".join(MANIFEST_INDEX_TYPES + MANIFEST_IMAGE_TYPES),
    })
    try:
        with urllib.request.urlopen(request, timeout=60) as response:
            raw = response.read(MAX_DOCUMENT + 1)
            header_digest = response.headers.get("Docker-Content-Digest")
    except urllib.error.HTTPError as error:
        if error.code == 404:
            return None
        raise Failure(f"anonymous registry request {path} failed: HTTP {error.code}") from None
    require(len(raw) <= MAX_DOCUMENT, "registry document exceeds size limit")
    digest = "sha256:" + hashlib.sha256(raw).hexdigest()
    if path.startswith("manifests/") and header_digest:
        require(header_digest == digest, "registry manifest digest mismatch")
    if descriptor:
        require(descriptor["digest"] == digest and descriptor["size"] == len(raw), "descriptor mismatch")
    return json.loads(raw), digest


def remote_digest(reference: str) -> Optional[str]:
    """Anonymous manifest digest of a tag or digest, or None when it does not exist."""
    found = registry_get("manifests/" + reference)
    return found[1] if found else None


def verify_public_image(reference: str, model: str) -> str:
    """Check a public linux/amd64 model image down to its config labels. Returns the root digest."""
    token = anonymous_token()
    found = registry_get("manifests/" + reference, token=token)
    require(found, f"{IMAGE}:{reference} does not exist or is not public")
    root, digest = found

    def check_descriptor(value: dict) -> None:
        require(DIGEST_RE.fullmatch(value.get("digest", "")), "invalid image digest")
        require(type(value.get("size")) is int and value["size"] > 0, "invalid image size")

    def image_config(document: dict, depth: int = 0) -> dict:
        require(depth <= 3 and document.get("schemaVersion") == 2, "invalid image schema")
        if document.get("mediaType") in MANIFEST_INDEX_TYPES:
            candidates = [child for child in document["manifests"]
                          if child.get("platform", {}).get("os") == "linux"
                          and child.get("platform", {}).get("architecture") == "amd64"]
            require(len(candidates) == 1, "expected exactly one linux/amd64 image")
            check_descriptor(candidates[0])
            child, _ = registry_get("manifests/" + candidates[0]["digest"], candidates[0], token)
            return image_config(child, depth + 1)
        require(document.get("mediaType") in MANIFEST_IMAGE_TYPES, "unsupported manifest type")
        require(isinstance(document.get("layers"), list) and document["layers"], "missing image layers")
        for layer in document["layers"]:
            check_descriptor(layer)
        config = document["config"]
        check_descriptor(config)
        require(config["mediaType"] in ("application/vnd.oci.image.config.v1+json",
                                        "application/vnd.docker.container.image.v1+json"), "invalid config type")
        return registry_get("blobs/" + config["digest"], config, token)[0]

    config = image_config(root)
    require(config["os"] == "linux" and config["architecture"] == "amd64", "wrong GPU platform")
    labels = config["config"]["Labels"]
    require(labels.get("io.lobocode.model") == model, "wrong image model")
    require(labels.get("org.opencontainers.image.source") == f"https://github.com/{REPO}", "wrong image source")
    require(re.fullmatch(r"[0-9a-f]{40}", labels.get("org.opencontainers.image.revision", "")),
            "missing image source revision")
    log(f"anonymous {reference} manifest/config verification passed: {digest}")
    return digest


# Check receipts ----------------------------------------------------------------

def receipt_path(sha: str, job: str, directory: Path = RECEIPTS) -> Path:
    return directory / f"{sha}-{job}.json"


def write_receipt(sha: str, job: str, **extra: Any) -> Path:
    RECEIPTS.mkdir(parents=True, exist_ok=True)
    path = receipt_path(sha, job)
    record = {"schema": 1, "job": job, "source_sha": sha, "host": socket.gethostname(),
              "platform": f"{platform.system()} {platform.machine()}",
              "finished_at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()), **extra}
    path.write_text(json.dumps(record, indent=2) + "\n")
    return path


def read_receipt(sha: str, job: str, directory: Path = RECEIPTS) -> Optional[dict]:
    path = receipt_path(sha, job, directory)
    if not path.is_file():
        return None
    record = json.loads(path.read_text())
    return record if record.get("source_sha") == sha and record.get("job") == job else None
