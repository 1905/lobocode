#!/usr/bin/env python3
"""Capture the Go CLI baseline without provider access. Retired at Rust cutover."""
import argparse
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[2]
HELP_PATHS = [
    "up", "status", "logs", "test", "down", "config", "config_show",
    "config_set", "config_get", "config_path", "models", "gen-api-key",
    "version", "release", "local", "local_run", "completion",
]
KEY_ONLY = "LOBO_API_KEY=sk-test\n"
RUNPOD = KEY_ONLY + "RUNPOD_API_KEY=rp-test\n"
SAMPLE = RUNPOD + "LOBO_DOMAIN=lobo.test\nLOBO_CTX=8192\nFOO_TOKEN=private-example\n"


def normalize(text, root):
    text = re.sub(r"\x1b\[[0-9;]*m", "", text)
    text = text.replace(str(root), "$TMP")
    text = re.sub(r"sk-[0-9a-f]{48}", "sk-KEY", text)
    text = re.sub(r'"free_bytes":\d+', '"free_bytes":0', text)
    text = re.sub(r"\(\d+\.\d GB free\)", "(N GB free)", text)
    return re.sub(r"^\d\d:\d\d:\d\d ", "", text, flags=re.M)


def environment(root, help_mode=False):
    env = os.environ.copy()
    env.update(HOME=str(root / "home"), XDG_CONFIG_HOME="/home/u/.config" if help_mode else str(root / "config"),
               NO_COLOR="1", TERM="dumb", TZ="UTC", HTTP_PROXY="http://127.0.0.1:9",
               HTTPS_PROXY="http://127.0.0.1:9", ALL_PROXY="http://127.0.0.1:9", NO_PROXY="")
    return env


def run(binary, args, root, stdin="", help_mode=False):
    return subprocess.run([str(binary), *args], input=stdin, text=True,
                          stdout=subprocess.PIPE, stderr=subprocess.PIPE, cwd=root,
                          env=environment(root, help_mode), timeout=15)


def help_capture(binary, root, out):
    out.mkdir(parents=True, exist_ok=True)
    commands = {"root": [], "root_help": ["help"]}
    commands.update({name: [*name.split("_"), "--help"] for name in HELP_PATHS})
    for name, args in commands.items():
        result = run(binary, args, root, help_mode=True)
        if result.returncode or result.stderr:
            raise RuntimeError(f"help capture failed: {name}: {result.stderr}")
        (out / f"{name}.txt").write_text(result.stdout)


def cases():
    out = []

    def add(name, args, config=None, **extra):
        out.append(dict(name=name, args=args, config=config, config_mode="600",
                        cwd_tmp=True, stderr_match="exact", **extra))

    add("version", ["version"])
    add("unknown_command", ["nosuch"], parser=True)
    add("unknown_flag", ["up", "--bogus"], parser=True)
    add("config_path", ["config", "path"])
    for suffix, cfg in [("missing", None), ("present", SAMPLE)]:
        add(f"config_show_{suffix}", ["config", "show"], cfg)
        add(f"config_json_{suffix}", ["config", "show", "--json"], cfg)
    add("config_get", ["config", "get", "LOBO_DOMAIN"], SAMPLE)
    add("config_get_missing", ["config", "get", "NOPE"], SAMPLE)
    add("config_get_no_arg", ["config", "get"], parser=True)
    add("config_set_empty", ["config", "set"])
    add("config_set_mixed", ["config", "set", "--stdin", "X=1"])
    add("config_set_bad_key", ["config", "set", "lower=1"])
    add("config_set_values", ["config", "set", "LOBO_MIN_MBPS=150", "CF_TUNNEL_TOKEN=a=b", "LOBO_CTX="], SAMPLE)
    add("config_set_stdin", ["config", "set", "--stdin"], SAMPLE, stdin='{"LOBO_CTX":"8192"}')
    for name, text in [("empty", "{}"), ("array", "[1]"), ("number", '{"A":1}'),
                       ("syntax", "nope"), ("bad_key", '{"lower":"1"}')]:
        add(f"stdin_{name}", ["config", "set", "--stdin"], stdin=text,
            json_error=name in ("array", "number", "syntax"))
    add("config_non_tty", ["config"], SAMPLE)
    for command in ["up", "down", "status", "logs", "test", "models", "release"]:
        add(f"{command}_no_config", [command])
    add("bad_cloud", ["up", "--cloud", "bad"], RUNPOD)
    add("bad_provider", ["up", "--provider", "aws"], RUNPOD)
    add("vast_without_key", ["up", "--provider", "vast"], RUNPOD)
    add("bad_ctx", ["up"], RUNPOD + "LOBO_CTX=100\n")
    add("override_bad_ctx", ["up", "--ctx", "100000", "--provider", "runpod"], RUNPOD + "LOBO_CTX=100\n")
    add("release_no_r2", ["release"], RUNPOD)
    add("release_no_bucket", ["release"], RUNPOD + "R2_ACCOUNT_ID=a\nR2_ACCESS_KEY=k\nR2_SECRET_KEY=s\nR2_ENDPOINT=https://r2.test\n")
    for flag in [[], ["--json"]]:
        add("models_json" if flag else "models_text", ["models", *flag], KEY_ONLY, weights_dir=True)
    for name, args in [
        ("model", ["--model", "q2", "--ctx", "1", "--idle-min", "1"]),
        ("ctx", ["--idle-min", "1"]), ("idle", ["--ctx", "1"]),
        ("port", ["--ctx", "1", "--idle-min", "1", "--port", "9000", "--api-port", "9000"]),
        ("extra", ["--ctx", "1", "--idle-min", "1", "x"]),
    ]:
        add(f"local_invalid_{name}", ["local", "run", *args], parser=name == "extra")
    add("gen_key_new", ["gen-api-key"], "LOBO_DOMAIN=lobo.test\n")
    add("gen_key_keep", ["gen-api-key"], SAMPLE, twice=True)
    add("gen_key_missing", ["gen-api-key"], os_error=True)
    add("models_loose_mode", ["models"], KEY_ONLY, weights_dir=True)
    out[-1]["config_mode"] = "644"
    for args in [["up", "foo"], ["up", "foo", "--q6=false"], ["up", "--q6=false", "foo"], ["up", "--", "foo"],
                 ["version", "foo"], ["config", "path", "foo"]]:
        add(f"extra_{len(out)}", args)
    return out


def cases_capture(binary, root, out):
    catalog = json.loads((ROOT / "crates/lobo-proto/catalog.json").read_text())
    captured = []
    for case in cases():
        work = root / case["name"]
        work.mkdir()
        config = work / "config.env"
        body = case["config"]
        if case.get("weights_dir"):
            weights = work / "weights"
            weights.mkdir()
            with (weights / catalog[0]["file"]).open("wb") as f:
                f.truncate(catalog[0]["size"] // 2)
            body += f"LOBO_WEIGHTS_DIR={weights}\n"
        if body is not None:
            config.write_text(body)
            config.chmod(int(case["config_mode"], 8))
        args = ["--config", str(config), *case["args"]]
        if case.get("twice"):
            first = run(binary, args, work)
            if first.returncode:
                raise RuntimeError(first.stderr)
        result = run(binary, args, work, case.get("stdin", ""))
        if case["args"][0] in ("up", "down", "status", "logs", "test", "release") and result.returncode == 0:
            raise RuntimeError(f"unsafe successful provider command: {case['name']}")
        case.update(exit=result.returncode, stdout=normalize(result.stdout, work), stderr=normalize(result.stderr, work), files_after={})
        if case.get("parser"):
            case["stderr_match"] = "prefix"
        elif case.get("json_error"):
            case["stderr_match"] = "json_error"
            # Go versions differ in decoder diagnostics. The CLI-owned prefix
            # is stable and is what the Rust parity contract compares.
            prefix = "error: config set --stdin: want a JSON object of strings:"
            if not case["stderr"].startswith(prefix):
                raise RuntimeError(f"missing CLI error prefix: {case['name']}")
            case["stderr"] = prefix + " [JSON decoder error]\n"
        elif case.get("os_error"):
            case["stderr_match"] = "os_error"
        elif re.search(r"^\d\d:\d\d:\d\d ", result.stderr):
            case["stderr_match"] = "log"
        for name in ["config.env", "opencode.lobo.json"]:
            path = work / name
            if path.exists():
                case["files_after"][name] = normalize(path.read_text(), work)
        captured.append(case)
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(captured, indent=2, ensure_ascii=False) + "\n")
    print(f"captured {len(captured)} no-network Go CLI cases")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("kind", choices=["help", "cases", "all"])
    parser.add_argument("out", type=Path)
    args = parser.parse_args()
    out = args.out.resolve()
    with tempfile.TemporaryDirectory(prefix="lobo-cli-fixtures-") as tmp:
        root = Path(tmp).resolve()
        binary = root / "lobo"
        subprocess.run(["go", "build", "-o", str(binary), "./cmd/lobo"], cwd=ROOT, check=True)
        if args.kind in ("help", "all"):
            help_capture(binary, root, out / "help" if args.kind == "all" else out)
        if args.kind in ("cases", "all"):
            cases_capture(binary, root, out / "cases.json" if args.kind == "all" else out)
        if args.kind == "all":
            goldens = out.parent.parent / "goldens"
            goldens.mkdir(parents=True, exist_ok=True)
            for source in ["cmd/lobo/testdata", "internal/tui/testdata"]:
                for path in (ROOT / source).glob("*.golden"):
                    shutil.copyfile(path, goldens / path.name)


if __name__ == "__main__":
    main()
