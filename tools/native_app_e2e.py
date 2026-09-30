#!/usr/bin/env python3
"""Run the actual app against task-owned local fixtures, then verify cleanup."""
import importlib.util
import argparse
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import sys
import tempfile
import time

REPO = Path(__file__).resolve().parents[1]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--setup-only", action="store_true", help="native startup/settings smoke without model memory requirements")
    parser.add_argument("--check-drag", action="store_true", help="wait for a native title-bar drag and verify the window position changes")
    args = parser.parse_args()
    root = Path(tempfile.mkdtemp(prefix="lobo-native-e2e-"))
    spec = importlib.util.spec_from_file_location("fixture", REPO / "app/e2e/fixture.py")
    fixture = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(fixture)
    info = fixture.prepare(root)
    config = Path(info["config"])
    config.rename(root / "template.env")  # The first test must create config through Settings.
    env = {key: value for key, value in os.environ.items() if key in {
        "HOME", "PATH", "USER", "LOGNAME", "SHELL", "TMPDIR", "LANG", "LC_ALL",
        "DISPLAY", "XAUTHORITY", "TERM", "CI",
    }}
    env.update(LOBO_E2E_ROOT=str(root), LOBO_APP_CONFIG=str(config), XDG_STATE_HOME=str(root / "state"))
    if args.setup_only:
        env["LOBO_E2E_SETUP_ONLY"] = "1"
    if args.check_drag:
        env["LOBO_E2E_CHECK_DRAG"] = "1"
    code = 1
    cleanup_error = None
    process = None
    try:
        process = subprocess.Popen(["pnpm", "-C", str(REPO / "app/e2e"), "test"], env=env, start_new_session=True)
        code = process.wait(timeout=600)
    finally:
        if process is not None:
            # Only this test's separate process group. The local supervisor has
            # its own session and is cleaned up through the CLI below.
            try:
                os.killpg(process.pid, signal.SIGTERM)
                deadline = time.monotonic() + 10
                while time.monotonic() < deadline:
                    try:
                        os.killpg(process.pid, 0)
                    except ProcessLookupError:
                        break
                    time.sleep(0.1)
                else:
                    os.killpg(process.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            process.wait()
        state = root / "state/lobo/local.json"
        if state.exists():
            # The isolated config has no cloud credentials. The ordinary CLI down
            # owns identity checks and process-group cleanup even after a test fails.
            cli = REPO / "bin/lobo-rs"
            result = subprocess.run([str(cli), "--config", str(config), "down", "--json"], env=env, capture_output=True, text=True, timeout=90)
            if result.returncode:
                cleanup_error = "Rust CLI fixture cleanup failed; inspect task logs"
                (root / "cleanup.log").write_text(result.stdout + result.stderr)
        if state.exists():
            cleanup_error = "task-owned supervisor state still exists"
        child_file = Path(info["weights"]) / ".e2e-llama.pid"
        if child_file.exists():
            pid = int(child_file.read_text())
            try:
                os.kill(pid, 0)
                cleanup_error = f"task-owned runtime process {pid} still exists"
            except ProcessLookupError:
                pass
        artifacts = REPO / "bin/app-e2e" / root.name
        artifacts.mkdir(parents=True, exist_ok=True)
        for p in root.glob("*.png"):
            shutil.copy2(p, artifacts / p.name)
        for p in root.glob("*-layout.json"):
            shutil.copy2(p, artifacts / p.name)
        for p in root.glob("drag-*.json"):
            shutil.copy2(p, artifacts / p.name)
        if (root / "logs").exists():
            shutil.copytree(root / "logs", artifacts / "logs")
        (artifacts / "result.json").write_text(json.dumps({"exit_code": code, "cleanup_error": cleanup_error, "fixture": str(root)}, indent=2) + "\n")
        print(f"Native E2E artifacts: {artifacts}", flush=True)
        if cleanup_error:
            print(f"CLEANUP FAILED: {cleanup_error}; retained {root}", file=sys.stderr)
        else:
            print("Task-owned local supervisor and runtime are stopped", flush=True)
    return 1 if cleanup_error else code


if __name__ == "__main__":
    raise SystemExit(main())
