#!/usr/bin/env python3
"""Prepare private cloud-only UI fixtures. No runtime, model or provider calls."""
import json
from pathlib import Path
import socket
import sys


def ports():
    for port in range(17000, 19000, 2):
        with socket.socket() as driver:
            try:
                driver.bind(("127.0.0.1", port + 1000))
                return port
            except OSError:
                pass
    raise RuntimeError("no isolated E2E driver port available")


def prepare(root, legacy_config=False):
    root.mkdir(parents=True, exist_ok=True)
    if list(root.iterdir()):
        raise RuntimeError("E2E fixture directory must be empty")
    root.chmod(0o700)
    config = root / "config.env"
    prefs = root / "preferences/prefs.json"
    prefs.parent.mkdir()
    prefs.write_text('{"target":"local"}\n')
    weights = root / "existing-weights"
    weights.mkdir()
    (weights / "preserve.txt").write_text("existing CLI model data\n")
    if legacy_config:
        config.write_text(
            "# Legacy CLI configuration. No provider credentials.\n"
            "LOBO_API_KEY=sk-isolated-native-app-e2e\n"
            "LOBO_PROVIDER=local\nLOBO_MODEL=q6\nLOBO_CTX=8192\n"
            "LOBO_IDLE_MIN=30\nLOBO_MAX_HOURS=1\n"
            f"LOBO_WEIGHTS_DIR={weights}\nLOBO_LOCAL_PORT=17891\n"
        )
        config.chmod(0o600)
        owner = config.with_suffix(".app-runtime.json")
        owner.write_text(json.dumps({"provider": "local", "boot_id": "legacy",
            "instance_id": "4242", "local_pid": 4242, "local_start_id": 100,
            "agent_url": None, "api_url": None}) + "\n")
        owner.chmod(0o600)
    info = {"root": str(root), "config": str(config), "weights": str(weights),
            "prefs": str(prefs), "port": ports(), "legacy": legacy_config}
    (root / "fixture.json").write_text(json.dumps(info, indent=2) + "\n")
    return info


if __name__ == "__main__":
    print(json.dumps(prepare(Path(sys.argv[1]).resolve())))
