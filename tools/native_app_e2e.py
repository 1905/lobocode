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
    modes = parser.add_mutually_exclusive_group()
    modes.add_argument("--setup-only", action="store_true", help="native startup/settings smoke without model memory requirements")
    modes.add_argument("--memory-only", action="store_true", help="UI memory verdicts and forced-denied Start; never start a runtime")
    modes.add_argument("--runtime-ui-only", action="store_true", help="owned HTTP runtime UI/config/Stop; never launch inference")
    parser.add_argument("--check-drag", action="store_true", help="wait for a native title-bar drag and verify the window position changes")
    args = parser.parse_args()
    if sys.platform == "darwin" and not (args.setup_only or args.memory_only or args.runtime_ui_only):
        parser.error("UI-only tests on this Mac: choose a UI-only mode; model and lifecycle tests require an authorized remote host")
    root = Path(tempfile.mkdtemp(prefix="lobo-native-e2e-")).resolve()
    spec = importlib.util.spec_from_file_location("fixture", REPO / "app/e2e/fixture.py")
    fixture = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(fixture)
    info = fixture.prepare(root, memory_only=args.memory_only, runtime_ui_only=args.runtime_ui_only)
    config = Path(info["config"])
    if not (args.memory_only or args.runtime_ui_only):
        config.rename(root / "template.env")  # The setup test creates config through Settings.
    env = {key: value for key, value in os.environ.items() if key in {
        "HOME", "PATH", "USER", "LOGNAME", "SHELL", "TMPDIR", "LANG", "LC_ALL",
        "DISPLAY", "XAUTHORITY", "TERM", "CI",
    }}
    env.update(LOBO_E2E_ROOT=str(root), LOBO_APP_CONFIG=str(config),
               XDG_STATE_HOME=str(root / "state"), XDG_CONFIG_HOME=str(root / "config-home"))
    if args.setup_only:
        env["LOBO_E2E_SETUP_ONLY"] = "1"
    if args.memory_only:
        env["LOBO_E2E_MEMORY_ONLY"] = "1"
    if args.runtime_ui_only:
        env["LOBO_E2E_RUNTIME_UI_ONLY"] = "1"
    if args.check_drag:
        env["LOBO_E2E_CHECK_DRAG"] = "1"
    code = 1
    cleanup_error = None
    process = None
    runtime_service = None
    try:
        if args.runtime_ui_only:
            runtime_spec = importlib.util.spec_from_file_location("runtime_fixture", REPO / "app/e2e/runtime_fixture.py")
            runtime_module = importlib.util.module_from_spec(runtime_spec)
            runtime_spec.loader.exec_module(runtime_module)
            runtime_service = runtime_module.start(root, info)
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
        if runtime_service is not None:
            runtime_service.close()
        state = root / "state/lobo/local.json"
        if state.exists() and not (args.memory_only or args.runtime_ui_only):
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
        if args.runtime_ui_only and (state.exists() or child_file.exists() or (Path(info["weights"]) / "runtime").exists()):
            # Never pass a synthetic fixture record to real-provider CLI cleanup.
            cleanup_error = "forbidden runtime artifact in UI-only fixture; no real cleanup attempted"
        elif child_file.exists():
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
        for p in root.glob("memory-*.json"):
            shutil.copy2(p, artifacts / p.name)
        for name in ("runtime-evidence.json", "native-trace.jsonl", "driver-trace.jsonl", "click-trace.jsonl", "native-acceptance.json"):
            if (root / name).exists():
                shutil.copy2(root / name, artifacts / name)
        if (root / "logs").exists():
            shutil.copytree(root / "logs", artifacts / "logs")
        (artifacts / "result.json").write_text(json.dumps({"exit_code": code, "runner_exit_code": code,
            "app_exit_evidence": "driver-trace.jsonl", "cleanup_error": cleanup_error, "fixture": str(root)}, indent=2) + "\n")
        print(f"Native E2E artifacts: {artifacts}", flush=True)
        if cleanup_error:
            print(f"CLEANUP FAILED: {cleanup_error}; retained {root}", file=sys.stderr)
        else:
            print("Task-owned fixture cleanup completed", flush=True)
    return 1 if cleanup_error else code


if __name__ == "__main__":
    raise SystemExit(main())
