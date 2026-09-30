#!/usr/bin/env python3
"""Prepare isolated config and sparse model fixtures; no downloads or providers."""
import json
from pathlib import Path
import socket
import sys

REPO = Path(__file__).resolve().parents[2]


def ports():
    # Keep the app's adjacent port pair outside the usual ephemeral range.
    for port in range(17000, 19000, 2):
        first, second, driver = socket.socket(), socket.socket(), socket.socket()
        try:
            first.bind(("127.0.0.1", port))
            second.bind(("127.0.0.1", port + 1))
            driver.bind(("127.0.0.1", port + 1000))
            return port
        except OSError:
            pass
        finally:
            first.close()
            second.close()
            driver.close()
    raise RuntimeError("no isolated E2E port pair available")


def prepare(root, memory_only=False):
    root.mkdir(parents=True, exist_ok=True)
    if list(root.iterdir()):
        raise RuntimeError("E2E fixture directory must be empty")
    weights = root / "weights"
    weights.mkdir()
    model = next(m for m in json.loads((REPO / "crates/lobo-proto/catalog.json").read_text()) if m["id"] == "q6")
    file = weights / model["file"]
    with file.open("wb") as stream:
        stream.truncate(model["size"])
    meta = file.stat()
    assert meta.st_blocks * 512 < 1_048_576, "fixture must remain sparse"
    (weights / (file.name + ".sha256-ok")).write_text(f'{model["sha256"]} {meta.st_size} {meta.st_mtime_ns}\n')
    runtime = weights / "runtime/llama-b11118"
    if not memory_only:
        runtime.mkdir(parents=True)
        script = (Path(__file__).parent / "fake_llama.py").read_text().split("\n", 1)[1]
        server = runtime / "llama-server"
        server.write_text(f"#!{sys.executable}\n{script}")
        server.chmod(0o755)
    else:
        (root / "memory.json").write_text(json.dumps({"mode": "insufficient"}) + "\n")
    port = ports()
    config = root / "config.env"
    config.write_text(
        "# Isolated native app E2E fixture. No provider credentials.\n"
        "LOBO_API_KEY=sk-isolated-native-app-e2e\n"
        "LOBO_PROVIDER=local\nLOBO_MODEL=q6\nLOBO_CTX=8192\n"
        "LOBO_IDLE_MIN=30\nLOBO_MAX_HOURS=1\n"
        f"LOBO_WEIGHTS_DIR={weights}\nLOBO_LOCAL_PORT={port}\n"
    )
    config.chmod(0o600)
    info = {"root": str(root), "config": str(config), "weights": str(weights), "port": port}
    (root / "fixture.json").write_text(json.dumps(info, indent=2) + "\n")
    return info


if __name__ == "__main__":
    print(json.dumps(prepare(Path(sys.argv[1]).resolve())))
