#!/usr/bin/env python3
"""Measure qwen38-q6 output speed with llama.cpp speculative decoding modes.

sweep     Rent one RTX 5090 bench pod (or attach with --pod), stage verified
          weights, run each llama-server flag set over every workload, and
          terminate the pod unless --keep is set.
endpoint  Run the workloads against a running OpenAI endpoint, for example
          the private `lobo up` connection at http://127.0.0.1:8933.
report    Summarize JSONL rows into a Markdown report.
pods      List bench pods.  terminate  Delete one bench pod.

RUNPOD_API_KEY comes from the environment only. Rows go to --out as JSONL.
"""
import argparse
import hashlib
import json
import os
import shlex
import statistics
import subprocess
import sys
import time
import urllib.error
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
PIN = "7f1ccf4"  # prompts come from this commit, so reruns send identical text
IMAGE = "ghcr.io/1905/lobocode@sha256:3d64617400c59420f0aea21a0bb6c0ac83376a8ee377dd2710ef63d5fc41fc0a"
GPU = "NVIDIA GeForce RTX 5090"
POD_NAME = "lobo-spec-bench"  # not the lobo pod name, so `lobo down` never touches it
RUNPOD = "https://rest.runpod.io/v1"
SSH_KEY = Path.home() / "ssh/runpod2"
DRAFT_REPO = "z-lab/Qwen3.8-27B-DFlash2-GGUF"
DRAFT_FILE = "Qwen3.8-27B-DFlash2-Q8_0.gguf"
LOCAL_PORT = 18080
CTX = 65536

CATALOG = {m["id"]: m for m in json.loads((ROOT / "crates/lobo-proto/catalog.json").read_text())}
MODEL = CATALOG["qwen38-q6"]

# Mirrors LlamaArgs::to_args() for Spec::Off (crates/lobo-agent/src/process.rs).
BASE_ARGS = ["-m", f"/models/{MODEL['file']}", "--alias", MODEL["alias"], "--host", "127.0.0.1",
             "--port", "8080", "-ngl", "99", "-c", str(CTX), "--parallel", "1", "-fa", "on",
             "--cache-type-k", "q8_0", "--cache-type-v", "q8_0", "--jinja", "--reasoning", "off",
             "--metrics", "--no-webui"]


def mtp(n):
    return ["--spec-type", "draft-mtp", "--spec-draft-n-max", str(n)]


def ngram(match=None, lo=None, hi=None):
    args = ["--spec-type", "ngram-mod"]
    if match:
        args += ["--spec-ngram-mod-n-match", str(match), "--spec-ngram-mod-n-min", str(lo),
                 "--spec-ngram-mod-n-max", str(hi)]
    return args


def dflash(n):
    return ["-md", f"/models/{DRAFT_FILE}", "-ngld", "99", "--spec-type", "draft-dflash",
            "--spec-draft-n-max", str(n), "-ctkd", "q8_0", "-ctvd", "q8_0"]


CONFIGS = {
    "off": [],
    "mtp1": mtp(1), "mtp2": mtp(2), "mtp3": mtp(3),
    "ngram": ngram(), "ngram-small": ngram(24, 16, 32),
    "dflash7": dflash(7), "dflash15": dflash(15),
    "cms1024": ["--checkpoint-min-step", "1024"], "cms0": ["--checkpoint-min-step", "0"],
}

GREEDY = {"temperature": 0.0}
# Qwen non-thinking recommendation; the GGUF carries no generation config.
SAMPLED = {"temperature": 0.7, "top_p": 0.8, "top_k": 20, "seed": 42}
CHAT = ("W1", "W2")


def pinned(path):
    return subprocess.run(["git", "show", f"{PIN}:{path}"], cwd=ROOT, check=True,
                          capture_output=True, text=True).stdout


def workloads():
    doc = "\n\n".join(f"=== {p} ===\n{pinned(p)}" for p in (
        "README.md", "AGENTS.md", "CHANGELOG.md", "crates/lobo-core/src/control/up.rs"))
    code = pinned("crates/lobo-agent/src/process.rs")
    tool = {"type": "function", "function": {
        "name": "read_file", "description": "Read a file from the workspace",
        "parameters": {"type": "object", "properties": {"path": {"type": "string"}},
                       "required": ["path"]}}}
    return [
        {"id": "W1", "max_tokens": 512, "messages": [{"role": "user", "content":
            "Explain in plain prose, without lists or code, how a bicycle derailleur shifts "
            "gears and why chains wear out. Write about 400 words."}]},
        {"id": "W2", "max_tokens": 512, "messages": [{"role": "user", "content":
            "Write a Go function `func ParseDuration(s string) (time.Duration, error)` that "
            "parses strings like \"1h30m\", \"45s\" and \"2d\" (days allowed). Add doc comments "
            "and a table-driven test. Output only Go code."}]},
        {"id": "W3", "max_tokens": 3072, "messages": [{"role": "user", "content":
            f"Here is a Rust source file.\n\n```rust\n{code}\n```\n\nRename the function "
            "`last_line` to `last_error_line` everywhere in this file, including tests. Change "
            "nothing else. Output the complete updated file in one ```rust block and no other text."}]},
        {"id": "W4", "max_tokens": 512, "messages": [{"role": "user", "content":
            f"{doc}\n\nQuote verbatim, in document order, every paragraph above that mentions "
            "RunPod. Output only the quotes, separated by blank lines."}]},
        {"id": "W5", "max_tokens": 256, "messages": [{"role": "user", "content":
            f"{doc}\n\nQuote verbatim every line of up.rs above that contains the word `image`. "
            "Output only those lines."}]},
        {"id": "W6", "max_tokens": 256, "tools": [tool], "messages": [{"role": "user", "content":
            "Open the file src/main.rs with the read_file tool and tell me what it does."}]},
    ]


# ---------- pure functions ----------

def median(xs):
    xs = [x for x in xs if x is not None]
    if not xs:
        raise ValueError("median of no values")
    return float(statistics.median(xs))


def summarize(rows):
    groups = {}
    for r in rows:
        if r.get("kind") == "row" and not r.get("warmup") and not r.get("error"):
            groups.setdefault((r["label"], r["config"], r["workload"], r["mode"]), []).append(r)
    out = {}
    for key, rs in groups.items():
        drafted = sum(r.get("draft_n") or 0 for r in rs)
        accepted = sum(r.get("draft_n_accepted") or 0 for r in rs)
        cold = [r["prompt_ms"] / 1000 for r in rs if r["rep"] == 1]
        out[key] = {
            "tok_s": median(r["predicted_per_second"] for r in rs),
            "prompt_tok_s": median(r["prompt_per_second"] for r in rs),
            "ttft_s": median(r["prompt_ms"] / 1000 for r in rs),
            "cold_ttft_s": cold[0] if cold else None,
            "predicted_n": median(r["predicted_n"] for r in rs),
            "accept_rate": accepted / drafted if drafted else None,
            "tool_ok": all(r.get("tool_ok", True) for r in rs),
            "n": len(rs),
        }
    return out


def gains(summary, label, base="off", mode="greedy"):
    """{config: {workload: tok_s ratio - 1}} against the base config of one label."""
    ref = {k[2]: v["tok_s"] for k, v in summary.items() if k[0] == label and k[1] == base and k[3] == mode}
    out = {}
    for (lab, config, workload, m), v in summary.items():
        if lab == label and m == mode and config != base and workload in ref and ref[workload] > 0:
            out.setdefault(config, {})[workload] = v["tok_s"] / ref[workload] - 1
    return out


def greedy_matches(rows, label, base="off"):
    """{config: (matched, total)}: greedy texts equal to the base config, per workload and rep."""
    texts = {}
    for r in rows:
        if r.get("kind") == "row" and r["label"] == label and r["mode"] == "greedy" \
                and not r.get("warmup") and not r.get("error"):
            texts[(r["config"], r["workload"], r["rep"])] = r["text_sha256"]
    out = {}
    for (config, workload, rep), sha in texts.items():
        if config == base or (base, workload, rep) not in texts:
            continue
        m, t = out.get(config, (0, 0))
        out[config] = (m + (sha == texts[(base, workload, rep)]), t + 1)
    return out


def gate(summary, matches, label, chat=CHAT, min_gain=0.10, max_loss=0.05, min_match=0.9,
         allowed=None):
    """Pick the config with the best mean chat gain that passes every rule."""
    reasons, best = [], (None, None)
    for config, g in sorted(gains(summary, label).items()):
        if allowed is not None and config not in allowed:
            continue
        if not all(w in g for w in chat):
            reasons.append(f"{config}: missing chat workloads")
            continue
        chat_gain = sum(g[w] for w in chat) / len(chat)
        worst = min(g.values())
        matched, total = matches.get(config, (0, 0))
        if chat_gain < min_gain:
            reasons.append(f"{config}: chat gain {chat_gain:+.1%} < {min_gain:+.0%}")
        elif worst < -max_loss:
            reasons.append(f"{config}: a workload is {worst:+.1%} slower")
        elif not total or matched / total < min_match:
            reasons.append(f"{config}: greedy match {matched}/{total} < {min_match:.0%}")
        elif best[1] is None or chat_gain > best[1]:
            best = (config, chat_gain)
    return best[0], reasons


def compare(summary, before="before", after="after"):
    """{(workload, mode): (before tok/s, after tok/s, ratio)} for two endpoint labels."""
    out = {}
    for (label, _, workload, mode), v in summary.items():
        if label == before and (after, after, workload, mode) in summary:
            a = summary[(after, after, workload, mode)]["tok_s"]
            out[(workload, mode)] = (v["tok_s"], a, a / v["tok_s"] if v["tok_s"] else None)
    return out


def fmt(x, pattern="{:.1f}"):
    return "–" if x is None else pattern.format(x)


def report_md(rows):
    summary = summarize(rows)
    lines = ["# Speculative decoding measurements", ""]
    spend = [r for r in rows if r.get("kind") == "spend"]
    if spend:
        lines += [f"RunPod spend: ${sum(r['usd'] for r in spend):.2f} over {len(spend)} pod(s).", ""]
    pairs = compare(summary)
    if pairs:
        lines += ["## Before and after through `lobo up`", "",
                  "| Workload | Mode | Before tok/s | After tok/s | Speed-up |", "|---|---|---:|---:|---:|"]
        for (w, m), (b, a, ratio) in sorted(pairs.items()):
            lines.append(f"| {w} | {m} | {b:.1f} | {a:.1f} | {fmt(ratio, '{:.2f}×')} |")
        lines.append("")
    for label in sorted({k[0] for k in summary} - {"before", "after"}):
        configs = sorted({k[1] for k in summary if k[0] == label}, key=lambda c: (c != "off", c))
        work = sorted({k[2] for k in summary if k[0] == label})
        lines += [f"## Sweep `{label}`: greedy output tok/s (gain against off)", "",
                  "| Config | " + " | ".join(work) + " | Accept | Greedy match |",
                  "|---|" + "---:|" * len(work) + "---:|---:|"]
        g = gains(summary, label)
        matches = greedy_matches(rows, label)
        for c in configs:
            cells, acc = [], []
            for w in work:
                v = summary.get((label, c, w, "greedy"))
                if v is None:
                    cells.append("–")
                    continue
                gain = g.get(c, {}).get(w)
                cells.append(f"{v['tok_s']:.1f}" + (f" ({gain:+.0%})" if gain is not None else ""))
                if v["accept_rate"] is not None:
                    acc.append(v["accept_rate"])
            m, t = matches.get(c, (0, 0))
            lines.append(f"| {c} | " + " | ".join(cells) + f" | {fmt(median(acc) if acc else None, '{:.0%}')} | "
                         + (f"{m}/{t}" if t else "–") + " |")
        winner, reasons = gate(summary, matches, label)
        lines += ["", f"Gate winner: `{winner}`" if winner else "Gate winner: none", ""]
        lines += [f"- {r}" for r in reasons] + [""]
        ttft = [(c, summary.get((label, c, "W4", "greedy")), summary.get((label, c, "W5", "greedy"))) for c in configs]
        lines += ["| Config | W4 cold TTFT s | W4 prompt tok/s | W5 TTFT s (cached prefix) |", "|---|---:|---:|---:|"]
        for c, w4, w5 in ttft:
            if w4:
                lines.append(f"| {c} | {fmt(w4['cold_ttft_s'], '{:.2f}')} | {w4['prompt_tok_s']:.0f} | "
                             f"{fmt(w5['ttft_s'] if w5 else None, '{:.2f}')} |")
        vram = [r for r in rows if r.get("kind") == "vram" and r["label"] == label]
        if vram:
            lines += ["", "| Config | VRAM used MiB |", "|---|---:|"]
            lines += [f"| {r['config']} | {r['used_mb']} |" for r in vram]
        errors = [r for r in rows if r.get("kind") == "error" and r["label"] == label]
        lines += [""] + [f"- ⚠ `{r['config']}`: {r['msg']}" for r in errors]
        lines.append("")
    return "\n".join(lines).rstrip() + "\n"


# ---------- I/O ----------

def http(method, url, body=None, headers=None, timeout=900):
    data = None if body is None else json.dumps(body).encode()
    req = urllib.request.Request(url, data=data, method=method,
                                 headers={"Content-Type": "application/json", "User-Agent": "lobo-spec-bench/1",
                                          **(headers or {})})
    with urllib.request.urlopen(req, timeout=timeout) as resp:
        raw = resp.read()
        return resp.status, (json.loads(raw) if raw else None)


def complete(base, w, sampling, key=None):
    body = {"model": MODEL["alias"], "messages": w["messages"], "max_tokens": w["max_tokens"],
            "stream": False, **sampling}
    if w.get("tools"):
        body["tools"] = w["tools"]
    headers = {"Authorization": f"Bearer {key}"} if key else {}
    t0 = time.monotonic()
    _, resp = http("POST", f"{base}/v1/chat/completions", body, headers)
    wall = time.monotonic() - t0
    t = resp.get("timings") or {}
    msg = resp["choices"][0]["message"]
    text = (msg.get("content") or "") + json.dumps(msg.get("tool_calls") or [], sort_keys=True)
    row = {"prompt_n": t.get("prompt_n"), "prompt_ms": t.get("prompt_ms"),
           "prompt_per_second": t.get("prompt_per_second"), "predicted_n": t.get("predicted_n"),
           "predicted_ms": t.get("predicted_ms"), "predicted_per_second": t.get("predicted_per_second"),
           "draft_n": t.get("draft_n"), "draft_n_accepted": t.get("draft_n_accepted"),
           "finish_reason": resp["choices"][0].get("finish_reason"), "wall_s": round(wall, 3),
           "text_sha256": hashlib.sha256(text.encode()).hexdigest()}
    if row["predicted_per_second"] is None:
        usage = resp.get("usage") or {}
        row["predicted_n"] = usage.get("completion_tokens")
        row["predicted_per_second"] = (row["predicted_n"] or 0) / wall if wall else None
        row["prompt_ms"], row["prompt_per_second"] = 0, None
    if w.get("tools"):
        try:
            call = msg["tool_calls"][0]["function"]
            row["tool_ok"] = call["name"] == "read_file" and "path" in json.loads(call["arguments"])
        except (KeyError, IndexError, TypeError, ValueError):
            row["tool_ok"] = False
    return row


class Out:
    def __init__(self, path):
        self.f = open(path, "a", buffering=1)

    def write(self, **row):
        row["ts"] = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())
        self.f.write(json.dumps(row) + "\n")
        print(json.dumps({k: row.get(k) for k in ("kind", "label", "config", "workload", "mode", "rep",
                                                   "predicted_per_second", "draft_n_accepted", "msg")
                          if row.get(k) is not None}), flush=True)


def run_workloads(out, base, label, config, reps, sampled, key=None):
    ws = workloads()
    plan = [("greedy", w) for w in ws] + ([("sampled", w) for w in ws if w["id"] in CHAT] if sampled else [])
    try:
        out.write(kind="row", label=label, config=config, workload="W1", mode="greedy", rep=0, warmup=True,
                  **complete(base, ws[0], GREEDY, key))
    except (urllib.error.URLError, OSError, KeyError, ValueError) as e:
        out.write(kind="error", label=label, config=config, msg=f"warm-up failed: {e}")
        return False
    for rep in range(1, reps + 1):
        for mode, w in plan:
            try:
                out.write(kind="row", label=label, config=config, workload=w["id"], mode=mode, rep=rep,
                          **complete(base, w, GREEDY if mode == "greedy" else SAMPLED, key))
            except (urllib.error.URLError, OSError, KeyError, ValueError) as e:
                out.write(kind="row", label=label, config=config, workload=w["id"], mode=mode, rep=rep,
                          error=str(e))
    return True


class RunPod:
    def __init__(self):
        self.key = os.environ["RUNPOD_API_KEY"]

    def call(self, method, path, body=None):
        try:
            return http(method, RUNPOD + path, body, {"Authorization": f"Bearer {self.key}"}, timeout=60)[1]
        except urllib.error.HTTPError as e:
            raise RuntimeError(f"RunPod {method} {path}: HTTP {e.code}: {e.read()[:500]!r}") from e

    def rent(self, cloud):
        script = ("mkdir -p /root/.ssh /run/sshd && echo \"$PUBLIC_KEY\" > /root/.ssh/authorized_keys && "
                  "chmod 700 /root/.ssh && chmod 600 /root/.ssh/authorized_keys && ssh-keygen -A >/dev/null && "
                  "/usr/sbin/sshd -o PasswordAuthentication=no && sleep infinity")
        return self.call("POST", "/pods", {
            "name": POD_NAME, "imageName": IMAGE, "gpuTypeIds": [GPU], "gpuCount": 1, "cloudType": cloud,
            "containerDiskInGb": 80, "volumeInGb": 0, "ports": ["22/tcp"], "supportPublicIp": True,
            "dockerEntrypoint": ["bash", "-c"], "dockerStartCmd": [script],
            "env": {"PUBLIC_KEY": SSH_KEY.with_suffix(".pub").read_text().strip()}})

    def ssh_addr(self, pod_id, timeout=900):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            p = self.call("GET", f"/pods/{pod_id}")
            ports = p.get("portMappings") or {}
            port = ports.get("22") if isinstance(ports, dict) else next(
                (m.get("publicPort") for m in ports if m.get("privatePort") == 22), None)
            if p.get("publicIp") and port:
                return p["publicIp"], int(port), p
            time.sleep(10)
        raise TimeoutError(f"pod {pod_id} has no SSH address after {timeout}s")

    def terminate(self, pod_id):
        try:
            self.call("DELETE", f"/pods/{pod_id}")
        except RuntimeError as e:
            if "HTTP 404" not in str(e):
                raise

    def bench_pods(self):
        return [p for p in self.call("GET", "/pods") or [] if p.get("name") == POD_NAME]


class Pod:
    def __init__(self, ip, port):
        self.target = ["-i", str(SSH_KEY), "-p", str(port), "-o", "StrictHostKeyChecking=no",
                       "-o", "UserKnownHostsFile=/dev/null", "-o", "LogLevel=ERROR",
                       "-o", "ServerAliveInterval=15", "-o", "ConnectTimeout=15", f"root@{ip}"]
        self.tunnel = None

    def sh(self, script, timeout=3600, check=True):
        r = subprocess.run(["ssh", *self.target, "bash", "-s"], input=script, text=True,
                           capture_output=True, timeout=timeout)
        if check and r.returncode:
            raise RuntimeError(f"remote script failed ({r.returncode}): {r.stderr[-2000:]}")
        return r.stdout

    def wait_ssh(self, timeout=600):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            if subprocess.run(["ssh", *self.target, "true"], capture_output=True, timeout=60).returncode == 0:
                return
            time.sleep(10)
        raise TimeoutError("SSH never answered")

    def forward(self):
        self.tunnel = subprocess.Popen(["ssh", "-N", "-L", f"{LOCAL_PORT}:127.0.0.1:8080", *self.target])

    def close(self):
        if self.tunnel:
            self.tunnel.terminate()


def hf_sha(repo, name):
    _, info = http("GET", f"https://huggingface.co/api/models/{repo}?blobs=true", timeout=60)
    return next(s["lfs"]["sha256"] for s in info["siblings"] if s["rfilename"] == name)


def stage(pod, need_draft):
    files = [(MODEL["file"], f"https://huggingface.co/{MODEL['repo']}/resolve/main/{MODEL['file']}", MODEL["sha256"])]
    if need_draft:
        files.append((DRAFT_FILE, f"https://huggingface.co/{DRAFT_REPO}/resolve/main/{DRAFT_FILE}",
                      hf_sha(DRAFT_REPO, DRAFT_FILE)))
    script = ["set -e", "command -v aria2c >/dev/null || (apt-get update -qq && "
              "DEBIAN_FRONTEND=noninteractive apt-get install -y -qq aria2 >/dev/null)", "mkdir -p /models"]
    for name, url, _ in files:
        script.append(f"[ -f /models/{name}.ok ] || (aria2c -q -x16 -s16 -k64M -d /models -o {shlex.quote(name)} "
                      f"{shlex.quote(url)} && sha256sum /models/{name} | cut -d' ' -f1 > /models/{name}.ok)")
        script.append(f"echo {name} $(cat /models/{name}.ok)")
    got = dict(line.split() for line in pod.sh("\n".join(script), timeout=5400).split("\n") if line.strip())
    for name, _, sha in files:
        if got.get(name) != sha:
            pod.sh(f"rm -f /models/{name}.ok")
            raise RuntimeError(f"{name}: sha256 {got.get(name)} != {sha}")


def start_server(pod, extra, timeout=600):
    argv = " ".join(shlex.quote(a) for a in ["/app/llama-server", *BASE_ARGS, *extra])
    pod.sh(f"pkill -x llama-server || true; sleep 3; LD_LIBRARY_PATH=/app:$LD_LIBRARY_PATH setsid nohup {argv} > /root/llama.log 2>&1 < /dev/null &")
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        time.sleep(5)
        try:
            if http("GET", f"http://127.0.0.1:{LOCAL_PORT}/health", timeout=10)[0] == 200:
                return None
        except (urllib.error.URLError, OSError):
            pass
        if not pod.sh("pgrep -x llama-server || true").strip():
            break
    return pod.sh("tail -n 25 /root/llama.log", check=False)


def cmd_sweep(a):
    out, rp = Out(a.out), RunPod()
    configs = dict(CONFIGS)
    for d in a.define or []:
        name, flags = d.split("=", 1)
        configs[name] = shlex.split(flags)
    names = a.configs.split(",")
    pod_id, started = a.pod, time.time()
    if not pod_id:
        p = rp.rent(a.cloud)
        pod_id = p["id"]
        out.write(kind="meta", label=a.label, msg=f"rented {pod_id} {p.get('costPerHr')}$/h", pod=pod_id,
                  cost_per_hr=p.get("costPerHr"), started=started)
        Path(a.out).with_suffix(".pod").write_text(json.dumps({"id": pod_id, "started": started,
                                                               "cost_per_hr": p.get("costPerHr")}))
    pod = None
    try:
        ip, port, info = rp.ssh_addr(pod_id)
        pod = Pod(ip, port)
        pod.wait_ssh()
        stage(pod, any("-md" in configs[n] for n in names))
        out.write(kind="meta", label=a.label, msg="weights staged and verified", pod=pod_id)
        pod.forward()
        for name in names:
            err = start_server(pod, configs[name])
            if err is not None:
                out.write(kind="error", label=a.label, config=name, msg="llama-server did not become healthy",
                          log_tail=err)
                continue
            used = pod.sh("nvidia-smi --query-gpu=memory.used --format=csv,noheader,nounits").strip()
            out.write(kind="vram", label=a.label, config=name, used_mb=int(used.split()[0]),
                      argv=configs[name])
            run_workloads(out, f"http://127.0.0.1:{LOCAL_PORT}", a.label, name, a.reps,
                          sampled=name in (a.sampled or "").split(","))
            out.write(kind="vram", label=a.label, config=name + "@end",
                      used_mb=int(pod.sh("nvidia-smi --query-gpu=memory.used --format=csv,noheader,nounits").split()[0]))
    finally:
        if pod:
            pod.close()
        if not a.keep:
            rp.terminate(pod_id)
            meta = json.loads(Path(a.out).with_suffix(".pod").read_text()) if Path(a.out).with_suffix(".pod").exists() else {}
            hours = (time.time() - meta.get("started", started)) / 3600
            out.write(kind="spend", label=a.label, pod=pod_id, hours=round(hours, 3),
                      usd=round(hours * (meta.get("cost_per_hr") or 0), 2), msg="terminated")


def cmd_endpoint(a):
    out = Out(a.out)
    key = os.environ.get(a.key_env) if a.key_env else None
    ok = run_workloads(out, a.url.rstrip("/").removesuffix("/v1"), a.label, a.label, a.reps, a.sampled, key)
    sys.exit(0 if ok else 1)


def cmd_report(a):
    rows = [json.loads(line) for f in a.files for line in Path(f).read_text().splitlines() if line.strip()]
    text = report_md(rows)
    if a.out:
        Path(a.out).write_text(text)
    print(text)


def cmd_pods(_):
    for p in RunPod().bench_pods():
        print(p["id"], p.get("desiredStatus"), p.get("costPerHr"))


def cmd_terminate(a):
    RunPod().terminate(a.pod)


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)
    s = sub.add_parser("sweep")
    s.add_argument("--configs", required=True)
    s.add_argument("--define", action="append", help="name='extra llama-server flags'")
    s.add_argument("--out", required=True)
    s.add_argument("--label", default="sweep")
    s.add_argument("--reps", type=int, default=2)
    s.add_argument("--sampled", default="", help="comma list of configs that also run sampled W1/W2")
    s.add_argument("--pod", help="attach to this pod instead of renting")
    s.add_argument("--keep", action="store_true", help="do not terminate the pod at the end")
    s.add_argument("--cloud", default="SECURE")
    s.set_defaults(func=cmd_sweep)
    e = sub.add_parser("endpoint")
    e.add_argument("--url", required=True)
    e.add_argument("--key-env")
    e.add_argument("--label", required=True)
    e.add_argument("--out", required=True)
    e.add_argument("--reps", type=int, default=2)
    e.add_argument("--sampled", action="store_true")
    e.set_defaults(func=cmd_endpoint)
    r = sub.add_parser("report")
    r.add_argument("files", nargs="+")
    r.add_argument("--out")
    r.set_defaults(func=cmd_report)
    sub.add_parser("pods").set_defaults(func=cmd_pods)
    t = sub.add_parser("terminate")
    t.add_argument("--pod", required=True)
    t.set_defaults(func=cmd_terminate)
    a = ap.parse_args(argv)
    a.func(a)


if __name__ == "__main__":
    main()
