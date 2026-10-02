#!/usr/bin/env python3
"""Run cloud-only native UI checks with private config and no inference."""
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
    parser.add_argument("--legacy-config", action="store_true", help="verify old local preferences and CLI config remain cloud-only")
    parser.add_argument("--check-drag", action="store_true", help="wait for a native title-bar drag and verify the window position changes")
    args = parser.parse_args()
    root = Path(tempfile.mkdtemp(prefix="lobo-native-e2e-"))
    spec = importlib.util.spec_from_file_location("fixture", REPO / "app/e2e/fixture.py")
    fixture = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(fixture)
    info = fixture.prepare(root, legacy_config=args.legacy_config)
    config = Path(info["config"])
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
            # Only this test's app and driver process group.
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
        if (root / "state/lobo/local.json").exists():
            cleanup_error = "cloud-only UI test unexpectedly created local runtime state"
        if Path(info["prefs"]).read_text() != '{"target":"local"}\n':
            cleanup_error = "legacy preferences changed"
        if (Path(info["weights"]) / "preserve.txt").read_text() != "existing CLI model data\n":
            cleanup_error = "existing CLI data changed"
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
            print("Task-owned app and driver are stopped; CLI data preserved", flush=True)
    return 1 if cleanup_error else code


if __name__ == "__main__":
    raise SystemExit(main())
