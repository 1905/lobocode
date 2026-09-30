"""Build-time only: verify upstream GGUF, split it, and prepare image layers."""
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import urllib.request

MODEL_REPO = "HauhauCS/Qwen3.5-27B-Uncensored-HauhauCS-Aggressive"
BASE = f"https://huggingface.co/{MODEL_REPO}/resolve/main/"


def sha256(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def build(model_id, output):
    catalog = json.loads(Path("/build/catalog.json").read_text())
    model = next(m for m in catalog if m["id"] == model_id)
    output.mkdir(parents=True)
    cache = Path("/model-cache")
    cache.mkdir(parents=True, exist_ok=True)
    source = cache / model["file"]
    if not source.exists():
        # Rename only complete downloads; an interrupted build leaves no reusable source.
        partial = source.with_suffix(".partial")
        subprocess.run(["curl", "--fail", "--location", "--proto", "=https", "--proto-redir", "=https", "--retry", "5", "--retry-all-errors",
                        "--output", str(partial), BASE + model["file"]], check=True)
        partial.replace(source)
    else:
        print(f"Rechecking cached upstream source: {source.name}", flush=True)
    measured_size = source.stat().st_size
    measured_sha256 = sha256(source)
    if measured_size != model["size"] or measured_sha256 != model["sha256"]:
        raise ValueError("upstream model differs from the pinned catalog")
    print(f"Verified upstream {model_id}: size={measured_size} sha256={measured_sha256}", flush=True)
    shards = Path("/tmp/model-shards")
    shards.mkdir()
    subprocess.run(["/app/llama-gguf-split", "--split-max-size", "4G", str(source), str(shards / "model")],
                   check=True, env={**os.environ, "LD_LIBRARY_PATH": "/app"})
    files = sorted(shards.glob("model-*.gguf"))
    if not 1 <= len(files) <= 8:
        raise ValueError(f"expected 1..8 GGUF shards, got {len(files)}")
    manifest = {"model": model_id, "source_sha256": model["sha256"], "shards": []}
    for i in range(1, 9):
        (output / f"{i:02}").mkdir()
    for i, shard in enumerate(files, 1):
        if shard.is_symlink() or not shard.is_file() or shard.name != f"model-{i:05}-of-{len(files):05}.gguf" or not 24 <= shard.stat().st_size < 5_000_000_000:
            raise ValueError("unexpected shard name or size")
        manifest["shards"].append({"file": shard.name, "size": shard.stat().st_size, "sha256": sha256(shard)})
        shutil.move(shard, output / f"{i:02}" / shard.name)
    (output / "model.json").write_text(json.dumps(manifest, indent=2) + "\n")
    licenses = output / "licenses"
    licenses.mkdir()
    (licenses / "MODEL.txt").write_text(
        f"Model: {MODEL_REPO}\nSource: https://huggingface.co/{MODEL_REPO}\n"
        f"Original GGUF SHA-256: {model['sha256']}\nLicense: Apache-2.0\n"
        "Packaging modification: split into GGUF shards with llama.cpp. Model tensors are unchanged.\n"
    )
    with urllib.request.urlopen("https://www.apache.org/licenses/LICENSE-2.0.txt", timeout=30) as response:
        (licenses / "Apache-2.0.txt").write_bytes(response.read())


if __name__ == "__main__":
    build(sys.argv[1], Path(sys.argv[2]))
