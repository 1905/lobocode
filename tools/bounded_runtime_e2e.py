#!/usr/bin/env python3
"""Deferred, direct two-request measurement of an already-owned local runtime.

This tool never starts, stops, downloads, rents, or changes a runtime. Invoke
only when the deferred E2E stage is authorized. --self-test uses HTTP fakes only.
No prompts, response content, token IDs, credentials, or raw errors are saved.
"""

import argparse
import ctypes
import http.client
import json
import math
import os
from pathlib import Path
import re
import socket
import stat
import subprocess
import sys
import tempfile
import threading
import time


PROMPTS = (("A", "The opposite of hot is"), ("B", "The capital of France is"))
RUN_SECONDS = 240.0
GENERATION_SECONDS = 90.0
PREFLIGHT_SECONDS = 10.0
BODY_BYTES = 262144
LINE_BYTES = 65536
CATALOG = Path(__file__).resolve().parents[1] / "crates/lobo-proto/catalog.json"
FINAL_FIELDS = [
    "stop", "model", "tokens_evaluated", "tokens_predicted", "truncated",
    "stop_type", "timings", "generation_settings/n_predict",
]


class Blocked(Exception):
    """Only constant categories may cross the reporting boundary."""

    def __init__(self, category):
        super().__init__(category)
        self.category = category


def require(condition, category):
    if not condition:
        raise Blocked(category)


def integer(value, category, minimum=0, maximum=2**63 - 1):
    require(type(value) is int and minimum <= value <= maximum, category)
    return value


def scalar(value, category):
    require(type(value) in (int, float) and math.isfinite(value) and value >= 0, category)
    return value


def object_json(data):
    try:
        # Reject nonstandard NaN and duplicate keys rather than guessing.
        def pairs(items):
            out = {}
            for key, value in items:
                require(key not in out, "invalid_json")
                out[key] = value
            return out

        return json.loads(data, object_pairs_hook=pairs,
                          parse_constant=lambda _: require(False, "invalid_json"))
    except (ValueError, UnicodeError, RecursionError):
        raise Blocked("invalid_json") from None


def private_read(path, limit, category, owned=False):
    try:
        # A supplied FIFO must fail the regular-file check instead of blocking
        # before fstat. Nonblocking has no effect on ordinary local files.
        flags = os.O_RDONLY | getattr(os, "O_NOFOLLOW", 0) | getattr(os, "O_NONBLOCK", 0)
        fd = os.open(path, flags)
        with os.fdopen(fd, "rb") as source:
            info = os.fstat(source.fileno())
            require(stat.S_ISREG(info.st_mode), category)
            if owned:
                require(info.st_uid == os.getuid(), category)
            data = source.read(limit + 1)
        require(len(data) <= limit, category)
        return data
    except OSError:
        raise Blocked(category) from None


def config_key(path):
    """Accept a deliberately small, literal subset of the dotenv syntax.

    Reject unsupported syntax throughout the file. This prevents a multiline
    value in another setting from being mistaken for the credential assignment.
    No shell, environment fallback, interpolation, or credential recovery.
    """
    try:
        text = private_read(path, 131072, "config_unavailable", owned=True).decode("utf-8")
    except UnicodeError:
        raise Blocked("config_unsupported") from None
    found = None
    assignment = re.compile(r"(?:export[ \t]+)?([A-Za-z_][A-Za-z0-9_.]*)[ \t]*=[ \t]*(.*)")
    for line in text.splitlines():
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        match = assignment.fullmatch(line)
        require(match is not None, "config_unsupported")
        name, value = match.groups()
        if value.startswith(("'", '"')):
            quote = value[0]
            end = value.find(quote, 1)
            require(end >= 1, "config_unsupported")
            suffix = value[end + 1:]
            require(not suffix or (suffix[0].isspace() and suffix.lstrip().startswith("#")),
                    "config_unsupported")
            value = value[1:end]
        else:
            value = re.split(r"[ \t]+#", value, maxsplit=1)[0].strip()
            require("'" not in value and '"' not in value, "config_unsupported")
        require("\\" not in value and "$" not in value and "\x00" not in value,
                "config_unsupported")
        if name == "LOBO_API_KEY":
            require(found is None, "config_unsupported")
            require(re.fullmatch(r"[!-~]{1,4096}", value) is not None, "config_unsupported")
            found = value
    require(found is not None, "config_key_missing")
    return found


def state_path():
    base = os.environ.get("XDG_STATE_HOME")
    return (Path(base) if base else Path.home() / ".local/state") / "lobo/local.json"


def supervisor_start(pid, boot_id):
    """Match the core supervisor check and capture the kernel start identity."""
    try:
        # Captured command stays private. No subprocess inherits the API key.
        result = subprocess.run(["/bin/ps", "-ww", "-o", "command=", "-p", str(pid)],
                                capture_output=True, timeout=2, check=False,
                                env={"PATH": "/usr/bin:/bin", "LC_ALL": "C"})
        require(result.returncode == 0 and len(result.stdout) <= LINE_BYTES,
                "supervisor_unavailable")
        fields = result.stdout.decode("utf-8").split()
        adjacent = list(zip(fields, fields[1:]))
        require(("local", "run") in adjacent and ("--boot-id", boot_id) in adjacent,
                "supervisor_mismatch")
        if sys.platform == "darwin":
            class BsdInfo(ctypes.Structure):
                _fields_ = [(name, ctypes.c_uint32) for name in (
                    "flags", "status", "xstatus", "pid", "ppid", "uid", "gid",
                    "ruid", "rgid", "svuid", "svgid", "reserved")]
                _fields_ += [("comm", ctypes.c_char * 16), ("name", ctypes.c_char * 32)]
                _fields_ += [(name, ctypes.c_uint32) for name in (
                    "nfiles", "pgid", "pjobc", "tdev", "tpgid")]
                _fields_ += [("nice", ctypes.c_int32), ("start_sec", ctypes.c_uint64),
                             ("start_usec", ctypes.c_uint64)]

            lib = ctypes.CDLL("/usr/lib/libproc.dylib", use_errno=True)
            lib.proc_pidinfo.argtypes = [ctypes.c_int, ctypes.c_int, ctypes.c_uint64,
                                        ctypes.c_void_p, ctypes.c_int]
            lib.proc_pidinfo.restype = ctypes.c_int
            info = BsdInfo()
            size = ctypes.sizeof(info)
            require(lib.proc_pidinfo(pid, 3, 0, ctypes.byref(info), size) == size,
                    "supervisor_unavailable")  # PROC_PIDTBSDINFO
            require(info.uid == os.getuid() and info.pid == pid, "supervisor_unowned")
            start = info.start_sec * 1000000 + info.start_usec
        elif sys.platform.startswith("linux"):
            path = Path("/proc") / str(pid) / "stat"
            raw = private_read(path, LINE_BYTES, "supervisor_unavailable", owned=True)
            start = int(raw.decode("utf-8").rsplit(")", 1)[1].split()[19])
        else:
            raise Blocked("host_unsupported")
        return integer(start, "supervisor_unavailable", 1)
    except (OSError, ValueError, UnicodeError, IndexError, subprocess.TimeoutExpired):
        raise Blocked("supervisor_unavailable") from None


class LocalRuntime:
    def __init__(self):
        self.path = state_path()
        catalog = object_json(private_read(CATALOG, 1048576, "catalog_unavailable"))
        require(type(catalog) is list, "catalog_invalid")
        self.models = {entry["id"]: entry for entry in catalog
                       if type(entry) is dict and entry.get("id") in ("q6", "q8")}
        self.identity = self.snapshot()
        self.pid, self.boot, self.model, self.port, self.agent_port = self.identity[:5]
        self.start = self.identity[5]
        self.alias = self.models[self.model]["alias"]
        require(re.fullmatch(r"[a-z0-9.-]{1,80}", self.alias) is not None, "catalog_invalid")

    def snapshot(self):
        state = object_json(private_read(self.path, LINE_BYTES, "runtime_absent", owned=True))
        require(type(state) is dict, "state_invalid")
        pid = integer(state.get("pid"), "state_invalid", 1, 2**31 - 1)
        boot = state.get("boot_id")
        require(type(boot) is str and re.fullmatch(r"[A-Za-z0-9_-]{1,64}", boot), "state_invalid")
        model = state.get("model")
        require(type(model) is str and model in self.models, "model_unknown")
        port = integer(state.get("port"), "state_invalid", 1, 65535)
        agent = integer(state.get("api_port"), "state_invalid", 1, 65535)
        require(port != agent, "state_invalid")
        weights = state.get("weights")
        require(type(weights) is str and Path(weights).is_absolute(), "state_invalid")
        entry = self.models[model]
        filename = entry.get("file")
        require(type(filename) is str and Path(filename).name == filename, "catalog_invalid")
        try:
            info = (Path(weights) / filename).stat()
        except OSError:
            raise Blocked("model_absent") from None
        # Catalog entries describe one complete GGUF, not separate shard files.
        expected_size = integer(entry.get("size"), "catalog_invalid", 1)
        require(stat.S_ISREG(info.st_mode) and info.st_size == expected_size, "model_absent")
        start = supervisor_start(pid, boot)
        # Paths and inode metadata are compared privately, never emitted.
        return (pid, boot, model, port, agent, start, weights,
                info.st_dev, info.st_ino, info.st_size, info.st_mtime_ns)

    def unchanged(self):
        require(self.snapshot() == self.identity, "runtime_changed")


class DirectHTTP:
    """Literal loopback only: no URL redirects, proxy lookup, or retries."""

    def __init__(self, key, run_end, metadata_seconds=PREFLIGHT_SECONDS,
                 generation_seconds=GENERATION_SECONDS, request_bytes=BODY_BYTES):
        self.key = key
        self.run_end = run_end
        self.metadata_seconds = metadata_seconds
        self.generation_seconds = generation_seconds
        self.request_bytes = integer(request_bytes, "request_limit_invalid", 1, 1048576)

    def exchange(self, port, method, path, payload, consume, seconds, auth=True):
        integer(port, "state_invalid", 1, 65535)
        require(path in ("/api/status", "/api/version", "/v1/models", "/slots",
                         "/tokenize", "/apply-template", "/completion"), "endpoint_rejected")
        end = min(self.run_end, time.monotonic() + seconds)
        require(end > time.monotonic(), "timeout")
        conn = http.client.HTTPConnection("127.0.0.1", port,
                                          timeout=end - time.monotonic())
        expired = threading.Event()

        def expire():
            expired.set()
            # Shutdown interrupts header/body reads even when pings keep arriving.
            sock = conn.sock
            if sock is not None:
                try:
                    sock.shutdown(socket.SHUT_RDWR)
                except OSError:
                    pass

        timer = threading.Timer(max(0, end - time.monotonic()), expire)
        timer.daemon = True
        timer.start()
        response = None
        try:
            conn.connect()
            require(not expired.is_set(), "timeout")
            body = None if payload is None else json.dumps(payload, allow_nan=False).encode("utf-8")
            headers = {"Accept": "text/event-stream" if path == "/completion" else "application/json",
                       "Connection": "close"}
            if body is not None:
                require(len(body) <= self.request_bytes, "request_oversized")
                headers["Content-Type"] = "application/json"
            if auth:
                headers["Authorization"] = "Bearer " + self.key
            start = time.monotonic_ns()
            conn.request(method, path, body=body, headers=headers)
            response = conn.getresponse()
            require(not expired.is_set(), "timeout")
            require(response.status == 200,
                    "auth_failed" if response.status in (401, 403) else "http_rejected")
            length = response.getheader("Content-Length")
            if length is not None:
                try:
                    require(0 <= int(length) <= BODY_BYTES, "response_oversized")
                except ValueError:
                    raise Blocked("http_invalid") from None
            require(response.getheader("Content-Encoding", "identity") == "identity", "http_invalid")
            # Keep a socket reference after getresponse: Connection: close can
            # detach it from conn, but the watchdog must still interrupt reads.
            body_socket = response.fp.raw._sock

            def expire_body():
                expired.set()
                try:
                    body_socket.shutdown(socket.SHUT_RDWR)
                except OSError:
                    pass

            timer.cancel()
            timer = threading.Timer(max(0, end - time.monotonic()), expire_body)
            timer.daemon = True
            timer.start()
            value = consume(response, start, end)
            require(not expired.is_set() and time.monotonic() <= end, "timeout")
            return value
        except Blocked:
            if expired.is_set() or time.monotonic() >= end:
                raise Blocked("timeout") from None
            raise
        except (socket.timeout, TimeoutError):
            raise Blocked("timeout") from None
        except (OSError, http.client.HTTPException):
            raise Blocked("timeout" if expired.is_set() else "transport_failed") from None
        finally:
            timer.cancel()
            if response is not None:
                response.close()
            conn.close()

    def json(self, port, path, payload=None, auth=True):
        def consume(response, _start, end):
            require(response.getheader("Content-Type", "").split(";", 1)[0].strip()
                    == "application/json", "http_invalid")
            chunks, size = [], 0
            while True:
                require(time.monotonic() < end, "timeout")
                chunk = response.read1(min(4096, BODY_BYTES + 1 - size))
                if not chunk:
                    break
                size += len(chunk)
                require(size <= BODY_BYTES, "response_oversized")
                chunks.append(chunk)
            return object_json(b"".join(chunks))

        return self.exchange(port, "GET" if payload is None else "POST", path, payload,
                             consume, self.metadata_seconds, auth)

    def completion(self, port, tokens, alias, cap, input_limit=64, expected_content=None):
        integer(input_limit, "input_limit_invalid", 1, 47000)
        require(0 < len(tokens) <= input_limit, "input_limit_exceeded")
        payload = {"prompt": tokens, "stream": True, "n_predict": cap, "n_cmpl": 1,
                   "temperature": 0, "seed": 1, "cache_prompt": False,
                   "return_progress": False, "timings_per_token": False,
                   "response_fields": FINAL_FIELDS}

        def consume(response, start, end):
            require(response.getheader("Content-Type", "").split(";", 1)[0].strip()
                    == "text/event-stream", "sse_invalid")
            first, final, measured = None, None, None
            observed_content = []
            size, event_size, event = 0, 0, []
            while True:
                require(time.monotonic() < end, "timeout")
                line = response.readline(LINE_BYTES + 1)
                size += len(line)
                require(size <= BODY_BYTES and len(line) <= LINE_BYTES, "response_oversized")
                if not line:
                    require(final is not None and not event, "sse_incomplete")
                    measured["stream_end_wall_ms"] = (time.monotonic_ns() - start) / 1000000
                    return measured
                require(line.endswith(b"\n"), "sse_incomplete")
                line = line.rstrip(b"\r\n")
                if line:
                    if line.startswith(b"data:"):
                        data = line[5:]
                        if data.startswith(b" "):
                            data = data[1:]
                        event_size += len(data) + 1
                        require(event_size <= LINE_BYTES, "response_oversized")
                        event.append(data)
                    continue  # comments, id, retry and event fields cause no action
                if not event:
                    continue
                value = object_json(b"\n".join(event))
                event, event_size = [], 0
                require(type(value) is dict and "error" not in value, "sse_error")
                require(final is None, "sse_after_final")
                now = time.monotonic_ns()
                content = value.get("content", "")
                require(type(content) is str, "sse_invalid")
                if content and first is None:
                    first = now
                if expected_content is not None:
                    observed_content.append(content)
                stop = value.get("stop")
                require(type(stop) is bool, "sse_invalid")
                if stop:
                    measured = validate_final(value, len(tokens), alias, cap, input_limit)
                    if expected_content is not None:
                        measured["expected_content_matched"] = "".join(observed_content).strip() == expected_content
                        observed_content.clear()
                    final = now
                    measured.update({"content_seen": first is not None,
                                     "time_to_first_content_ms": None if first is None else (first - start) / 1000000,
                                     "stream_generation_wall_ms": None if first is None else (final - first) / 1000000,
                                     "request_wall_ms": (final - start) / 1000000})
                # Raw events/token IDs never persist. The optional answer check
                # retains only this size-bounded response until the final event.
                del value, content

        return self.exchange(port, "POST", "/completion", payload, consume,
                             self.generation_seconds)


def validate_final(value, count, alias, cap, input_limit=64):
    require(value.get("model") == alias, "response_model_mismatch")
    require(value.get("truncated") is False, "response_truncated")
    require(integer(value.get("tokens_evaluated"), "input_count_invalid", 1, input_limit) == count,
            "input_count_mismatch")
    output = integer(value.get("tokens_predicted"), "output_count_invalid", 0, cap)
    # b11118 response_fields keeps slash-separated paths as literal flat keys.
    # task_params::to_json does not echo n_cmpl; the request still fixes it to 1.
    require(integer(value.get("generation_settings/n_predict"),
                    "response_settings_invalid", 1, 32) == cap,
            "response_settings_invalid")
    timings = value.get("timings")
    require(type(timings) is dict, "timings_invalid")
    prompt_n = integer(timings.get("prompt_n"), "timings_invalid", 0, count)
    cache_n = integer(timings.get("cache_n"), "timings_invalid", 0, count)
    require(prompt_n + cache_n == count, "timings_count_mismatch")
    require(integer(timings.get("predicted_n"), "timings_invalid", 0, cap) == output,
            "timings_count_mismatch")
    # Preserve the server's predicted_per_second. b11118 uses n-1, not n.
    result = {"actual_input_tokens": count, "actual_output_tokens": output,
              "processed_input_tokens": prompt_n, "cached_input_tokens": cache_n}
    for source, target in (("prompt_ms", "server_prompt_ms"),
                           ("predicted_ms", "server_generation_ms"),
                           ("prompt_per_second", "server_prompt_tokens_per_second"),
                           ("predicted_per_second", "server_output_tokens_per_second")):
        result[target] = scalar(timings.get(source), "timings_invalid")
    return result


class Harness:
    def __init__(self, runtime, http, input_cap=64, output_cap=32):
        self.runtime, self.http = runtime, http
        self.input_cap, self.output_cap = input_cap, output_cap
        self.attempts, self.results, self.preflight = 0, [], []
        self.revision = None

    def ready(self):
        runtime = self.runtime
        runtime.unchanged()
        status = self.http.json(runtime.agent_port, "/api/status", auth=False)
        require(type(status) is dict and status.get("stage") == "ready", "runtime_not_ready")
        require(status.get("boot_id") == runtime.boot and status.get("model") == runtime.model,
                "runtime_identity_mismatch")
        version = self.http.json(runtime.agent_port, "/api/version", auth=False)
        require(type(version) is dict, "revision_unknown")
        revision = version.get("git_sha")
        require(type(revision) is str and re.fullmatch(r"[0-9a-f]{7,40}", revision), "revision_unknown")
        if self.revision is not None:
            require(revision == self.revision, "runtime_changed")
        self.revision = revision
        models = self.http.json(runtime.port, "/v1/models")
        require(type(models) is dict and type(models.get("data")) is list, "model_metadata_invalid")
        ids = [item.get("id") for item in models["data"] if type(item) is dict]
        require(ids == [runtime.alias], "runtime_model_mismatch")
        runtime.unchanged()
        return status

    def quiet(self):
        slots = self.http.json(self.runtime.port, "/slots")
        require(type(slots) is list and len(slots) > 0, "slots_unavailable")
        for slot in slots:
            require(type(slot) is dict and type(slot.get("is_processing")) is bool,
                    "slots_unavailable")
            require(slot["is_processing"] is False, "runtime_busy")
        self.runtime.unchanged()

    def run(self):
        self.ready()
        prepared = []
        # Both preflights must pass before either generation is transmitted.
        for prompt_id, text in PROMPTS:
            response = self.http.json(self.runtime.port, "/tokenize",
                                      {"content": text, "add_special": True,
                                       "parse_special": True, "with_pieces": False})
            require(type(response) is dict and type(response.get("tokens")) is list,
                    "tokenizer_invalid")
            tokens = response["tokens"]
            require(0 < len(tokens) <= self.input_cap, "input_limit_exceeded")
            for token in tokens:
                integer(token, "tokenizer_invalid", 0, 2**31 - 1)
            prepared.append((prompt_id, tokens))
            self.preflight.append({"prompt_id": prompt_id, "input_tokens": len(tokens)})
        for prompt_id, tokens in prepared:
            self.ready()
            self.quiet()
            # An uncertain failed POST still counts. Never retry or replace it.
            self.attempts += 1
            result = self.http.completion(self.runtime.port, tokens, self.runtime.alias,
                                          self.output_cap)
            self.runtime.unchanged()
            self.ready()
            self.results.append({"prompt_id": prompt_id, **result})
        require(self.attempts == 2 and len(self.results) == 2, "request_count_invalid")

    def summary(self, category=None):
        runtime = self.runtime
        return {"schema": 1, "status": "passed" if category is None else "blocked",
                "error_category": category, "generation_attempts": self.attempts,
                "request_limit": 2, "max_input_tokens": self.input_cap,
                "max_output_tokens": self.output_cap, "preflight": self.preflight,
                "measurements": self.results, "model": runtime.model,
                "boot_id": runtime.boot, "supervisor_pid": runtime.pid,
                "supervisor_start_id": runtime.start, "app_revision": self.revision}


def evidence_directory(path):
    try:
        # Require a new leaf. Do not chmod or overwrite an existing user directory.
        path.mkdir(mode=0o700)
        require(stat.S_IMODE(path.stat().st_mode) & 0o077 == 0, "evidence_unavailable")
    except OSError:
        raise Blocked("evidence_unavailable") from None


def write_evidence(path, summary):
    # summary is constructed from explicit fields. Never dump state or HTTP JSON.
    try:
        with tempfile.NamedTemporaryFile(mode="w", dir=path, prefix=".summary-",
                                         delete=False, encoding="utf-8") as target:
            temp = Path(target.name)
            os.fchmod(target.fileno(), 0o600)
            json.dump(summary, target, allow_nan=False, sort_keys=True, indent=2)
            target.write("\n")
            target.flush()
            os.fsync(target.fileno())
        os.replace(temp, path / "summary.json")
    except (OSError, ValueError):
        raise Blocked("evidence_write_failed") from None


class SafeParser(argparse.ArgumentParser):
    def error(self, _message):
        self.exit(2, "blocked: invalid_arguments\n")


def arguments(argv):
    parser = SafeParser(description=__doc__, allow_abbrev=False)
    parser.add_argument("--config", type=Path)
    parser.add_argument("--requests", type=int, default=2)
    parser.add_argument("--max-input-tokens", type=int, default=64)
    parser.add_argument("--max-output-tokens", type=int, default=32)
    parser.add_argument("--evidence-dir", type=Path)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args(argv)
    if args.requests != 2 or not 0 < args.max_input_tokens <= 64 or not 0 < args.max_output_tokens <= 32:
        parser.error("limits")
    if args.self_test:
        if args.config is not None or args.evidence_dir is not None:
            parser.error("self-test isolation")
    elif args.config is None or args.evidence_dir is None:
        parser.error("paths required")
    return args


def main(argv=None):
    args = arguments(argv)
    if args.self_test:
        return self_test()
    harness = None
    reserved = False
    try:
        evidence_directory(args.evidence_dir)
        reserved = True
        run_end = time.monotonic() + RUN_SECONDS
        key = config_key(args.config)
        runtime = LocalRuntime()
        harness = Harness(runtime, DirectHTTP(key, run_end), args.max_input_tokens,
                          args.max_output_tokens)
        harness.run()
        write_evidence(args.evidence_dir, harness.summary())
        print("passed: 2 bounded sequential requests")
        return 0
    except Blocked as error:
        category = error.category
    except KeyboardInterrupt:
        category = "interrupted"
    except Exception:
        # Do not print exception strings, traceback, HTTP content or file paths.
        category = "internal_error"
    if reserved:
        summary = harness.summary(category) if harness else {
            "schema": 1, "status": "blocked", "error_category": category,
            "generation_attempts": 0, "request_limit": 2,
            "max_input_tokens": args.max_input_tokens,
            "max_output_tokens": args.max_output_tokens, "measurements": []}
        try:
            write_evidence(args.evidence_dir, summary)
        except Blocked:
            category = "evidence_write_failed"
    print("blocked: " + category)
    return 2


# Deferred HTTP fake tests. This branch never calls LocalRuntime or resolves
# real state. Config parsing uses private dummy files. HTTP is loopback only.
def self_test():
    from http.server import BaseHTTPRequestHandler, HTTPServer

    secret = "fake-secret-must-not-appear"
    alias = "qwen3.5-27b-uncensored-q6"

    class FakeRuntime:
        pid, boot, model, start = 123, "fake-boot", "q6", 123456
        port = agent_port = 0

        def __init__(self, scenario):
            self.scenario, self.checks = scenario, 0
            self.alias = alias
            self.completed = False

        def unchanged(self):
            self.checks += 1
            if self.scenario in ("stale_pid", "stale_start", "missing_model"):
                raise Blocked("model_absent" if self.scenario == "missing_model" else "runtime_changed")
            if self.scenario == "changed_after_first" and self.completed:
                raise Blocked("runtime_changed")

    def final(tokens, cap):
        return {"stop": True, "model": alias, "tokens_evaluated": len(tokens),
                "tokens_predicted": 2, "truncated": False,
                "generation_settings/n_predict": cap,
                "timings": {"prompt_n": len(tokens), "cache_n": 0, "predicted_n": 2,
                            "prompt_ms": 10, "predicted_ms": 5,
                            "prompt_per_second": 100, "predicted_per_second": 200}}

    scenarios = {
        "ok": (None, 2), "eos": (None, 2), "oversized_tokens": ("input_limit_exceeded", 0),
        "bool_token": ("tokenizer_invalid", 0), "stale_pid": ("runtime_changed", 0),
        "stale_start": ("runtime_changed", 0), "missing_model": ("model_absent", 0),
        "stale_boot": ("runtime_identity_mismatch", 0), "wrong_model": ("runtime_model_mismatch", 0),
        "not_ready": ("runtime_not_ready", 0), "busy": ("runtime_busy", 0),
        "auth": ("auth_failed", 0), "redirect": ("http_rejected", 0),
        "completion_auth": ("auth_failed", 1), "input_mismatch": ("input_count_mismatch", 1),
        "output_excess": ("output_count_invalid", 1), "truncated": ("response_truncated", 1),
        "model_mismatch": ("response_model_mismatch", 1), "missing_final": ("sse_incomplete", 1),
        "sse_error": ("sse_error", 1), "http_error": ("http_rejected", 1),
        "nan_timing": ("invalid_json", 1), "bool_timing": ("timings_invalid", 1),
        "wrong_settings": ("response_settings_invalid", 1),
        "nested_settings": ("response_settings_invalid", 1), "pings_timeout": ("timeout", 1),
        "large_body": ("response_oversized", 1),
        "changed_after_first": ("runtime_changed", 1),
    }

    for scenario, (expected, attempts) in scenarios.items():
        seen = []
        token_sets = [[99, 101, 102], [99, 201, 202, 203]]

        class Handler(BaseHTTPRequestHandler):
            protocol_version = "HTTP/1.0"

            def log_message(self, *_args):
                pass

            def answer(self, value, status=200):
                body = json.dumps(value).encode()
                self.send_response(status)
                self.send_header("Content-Type", "application/json")
                self.send_header("Content-Length", str(len(body)))
                self.end_headers()
                self.wfile.write(body)

            def do_GET(self):
                self.route(None)

            def do_POST(self):
                size = int(self.headers.get("Content-Length", "0"))
                require(size <= BODY_BYTES, "fake_request_oversized")
                self.route(json.loads(self.rfile.read(size)))

            def route(self, payload):
                seen.append((self.path, payload))
                if self.path.startswith("/api/"):
                    if self.path == "/api/version":
                        self.answer({"git_sha": "abcdef123456", "version": "dev"})
                    else:
                        self.answer({"stage": "load" if scenario == "not_ready" else "ready",
                                     "boot_id": "other" if scenario == "stale_boot" else "fake-boot",
                                     "model": "q6"})
                    return
                if self.headers.get("Authorization") != "Bearer " + secret or scenario == "auth":
                    self.answer({"error": secret}, 401)
                elif self.path == "/v1/models":
                    if scenario == "redirect":
                        self.send_response(302)
                        self.send_header("Location", "http://example.invalid/private")
                        self.end_headers()
                    else:
                        self.answer({"data": [{"id": "wrong" if scenario == "wrong_model" else alias}]})
                elif self.path == "/slots":
                    self.answer([{"is_processing": scenario == "busy"}])
                elif self.path == "/tokenize":
                    require(payload.get("add_special") is True and payload.get("parse_special") is True
                            and payload.get("with_pieces") is False, "fake_tokenizer_flags")
                    index = sum(path == "/tokenize" for path, _ in seen) - 1
                    tokens = token_sets[index]
                    if scenario == "oversized_tokens" and index == 1:
                        tokens = [99] * 65
                    if scenario == "bool_token":
                        tokens = [True]
                    self.answer({"tokens": tokens})
                elif self.path == "/completion":
                    index = sum(path == "/completion" for path, _ in seen) - 1
                    require(payload["prompt"] == token_sets[index], "fake_duplicate_bos")
                    require(payload["n_predict"] == 32 and payload["n_cmpl"] == 1
                            and payload["response_fields"] == FINAL_FIELDS, "fake_prediction_budget")
                    if scenario in ("completion_auth", "http_error"):
                        self.answer({"error": secret}, 401 if scenario == "completion_auth" else 500)
                        return
                    self.send_response(200)
                    self.send_header("Content-Type", "text/event-stream")
                    self.end_headers()
                    try:
                        if scenario == "pings_timeout":
                            for _ in range(30):
                                self.wfile.write(b": ping\r\n\r\n")
                                self.wfile.flush()
                                time.sleep(0.01)
                            return
                        if scenario == "large_body":
                            self.wfile.write(b"data: " + b"x" * (LINE_BYTES + 1) + b"\n\n")
                            return
                        partial = {"stop": False, "content": "" if scenario == "eos" else secret,
                                   "tokens": [789]}
                        # CRLF, comments, multiline data, and arbitrary write boundaries.
                        event = (": ping\r\n\r\ndata: " + json.dumps(partial) + "\r\n\r\n").encode()
                        self.wfile.write(event[:7])
                        self.wfile.flush()
                        self.wfile.write(event[7:])
                        if scenario == "missing_final":
                            return
                        if scenario == "sse_error":
                            value = {"error": secret}
                        else:
                            value = final(payload["prompt"], payload["n_predict"])
                            if scenario == "input_mismatch":
                                value["tokens_evaluated"] += 1
                            if scenario == "output_excess":
                                value["tokens_predicted"] = 33
                            if scenario == "truncated":
                                value["truncated"] = True
                            if scenario == "model_mismatch":
                                value["model"] = "wrong"
                            if scenario == "nan_timing":
                                value["timings"]["prompt_ms"] = float("nan")
                            if scenario == "bool_timing":
                                value["timings"]["prompt_ms"] = True
                            if scenario == "wrong_settings":
                                value["generation_settings/n_predict"] = 33
                            if scenario == "nested_settings":
                                value["generation_settings"] = {
                                    "n_predict": value.pop("generation_settings/n_predict")}
                            if scenario == "eos":
                                value["tokens_predicted"] = value["timings"]["predicted_n"] = 1
                                value["timings"]["predicted_per_second"] = 0
                        encoded = json.dumps(value)
                        # A newline after a JSON comma is valid SSE multiline JSON.
                        encoded = encoded.replace(', "model"', ',\ndata: "model"', 1)
                        self.wfile.write(("data: " + encoded + "\r\n\r\n").encode())
                        self.wfile.flush()
                        runtime.completed = True
                    except (BrokenPipeError, ConnectionResetError):
                        pass
                else:
                    self.answer({"error": secret}, 404)

        with HTTPServer(("127.0.0.1", 0), Handler) as server:
            server.timeout = 0.05
            finished = threading.Event()

            def serve():
                while not finished.is_set():
                    server.handle_request()

            thread = threading.Thread(target=serve, daemon=True)
            thread.start()
            runtime = FakeRuntime(scenario)
            runtime.port = runtime.agent_port = server.server_port
            http = DirectHTTP(secret, time.monotonic() + 5, metadata_seconds=1,
                              generation_seconds=0.1 if scenario == "pings_timeout" else 1)
            harness = Harness(runtime, http)
            category = None
            connect = socket.create_connection

            def fake_only_connect(address, *args, **kwargs):
                require(address == ("127.0.0.1", server.server_port), "self_test_external_access")
                return connect(address, *args, **kwargs)

            proxy_names = ("HTTP_PROXY", "HTTPS_PROXY", "ALL_PROXY", "http_proxy",
                           "https_proxy", "all_proxy", "NO_PROXY", "no_proxy")
            prior_proxy = {name: os.environ.get(name) for name in proxy_names}
            for name in proxy_names:
                os.environ[name] = "" if name.lower() == "no_proxy" else "http://example.invalid:1"
            socket.create_connection = fake_only_connect
            try:
                harness.run()
            except Blocked as error:
                category = error.category
            finally:
                socket.create_connection = connect
                for name, previous in prior_proxy.items():
                    if previous is None:
                        os.environ.pop(name, None)
                    else:
                        os.environ[name] = previous
                finished.set()
                thread.join(timeout=2)
            require(not thread.is_alive(), "self_test_cleanup")
            require(category == expected and harness.attempts == attempts, "self_test_failed")
            require(sum(path == "/completion" for path, _ in seen) == attempts, "self_test_retry")
            if attempts:
                positions = [i for i, (path, _) in enumerate(seen) if path == "/tokenize"]
                first_generation = next(i for i, (path, _) in enumerate(seen) if path == "/completion")
                require(len(positions) == 2 and max(positions) < first_generation, "self_test_preflight")
            summary = harness.summary(category)
            with tempfile.TemporaryDirectory(prefix="lobo-bounded-fake-") as root:
                directory = Path(root) / "evidence"
                evidence_directory(directory)
                write_evidence(directory, summary)
                saved = (directory / "summary.json").read_text()
                require(secret not in saved and '"tokens"' not in saved and '"content"' not in saved
                        and '"prompt"' not in saved and '"Authorization"' not in saved
                        and all(text not in saved for _, text in PROMPTS), "self_test_privacy")
                require(stat.S_IMODE(directory.stat().st_mode) == 0o700
                        and stat.S_IMODE((directory / "summary.json").stat().st_mode) == 0o600,
                        "self_test_permissions")
            if scenario == "eos":
                require(all(not result["content_seen"] and result["server_output_tokens_per_second"] == 0
                            for result in harness.results), "self_test_eos")
            if scenario == "ok":
                require(all(result["server_output_tokens_per_second"] == 200
                            for result in harness.results), "self_test_server_rate")

    # Literal config fixtures are private dummy files. No personal file access.
    with tempfile.TemporaryDirectory(prefix="lobo-bounded-config-fake-") as root:
        config = Path(root) / "config.env"
        for value in (secret, '"' + secret + '"', "'" + secret + "'"):
            config.write_text("# fixture\nOTHER=unchanged\nexport LOBO_API_KEY=" + value + " # fixture\n")
            require(config_key(config) == secret, "self_test_config")
        for value in ("LOBO_API_KEY=$EARLIER\n", 'LOBO_API_KEY="a\\nb"\n',
                      'OTHER="multiline\nLOBO_API_KEY=' + secret + '\n"\n',
                      "LOBO_API_KEY=" + secret + "\nLOBO_API_KEY=second\n"):
            config.write_text(value)
            try:
                config_key(config)
            except Blocked as error:
                require(error.category == "config_unsupported", "self_test_config")
            else:
                raise Blocked("self_test_config")

    # Invalid CLI counts/limits and unsupported concurrency must fail before I/O.
    from contextlib import redirect_stderr
    from io import StringIO
    for flags in (("--requests", "3"), ("--requests", "1"),
                  ("--max-input-tokens", "65"), ("--max-input-tokens", "0"),
                  ("--max-output-tokens", "33"), ("--max-output-tokens", "0"),
                  ("--concurrency", "2"), ("--self-test", "--config", "forbidden")):
        with redirect_stderr(StringIO()) as errors:
            try:
                arguments(list(flags))
            except SystemExit as error:
                require(error.code == 2 and errors.getvalue() == "blocked: invalid_arguments\n",
                        "self_test_arguments")
            else:
                raise Blocked("self_test_arguments")
    print("passed: HTTP fake self-test")
    return 0


if __name__ == "__main__":
    sys.exit(main())
