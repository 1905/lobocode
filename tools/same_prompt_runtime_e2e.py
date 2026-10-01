#!/usr/bin/env python3
"""One direct Q6 diagnostic: 47,000 synthetic input tokens, at most 32 output.

Use only after explicit authorization and normal production memory admission.
The runtime must already be ready and task-owned. This tool never starts, stops,
downloads, rents, retries, or launches OpenCode. --self-test uses HTTP fakes only.
The original request size was approximate; this is a size comparison, not a replay.
"""

import json
from pathlib import Path
import socket
import sys
import threading
import time

import bounded_runtime_e2e as bounded


INPUT_TOKENS = 47000
CONTEXT_TOKENS = 65536
OUTPUT_TOKENS = 32
GENERATION_SECONDS = 1800.0
RUN_SECONDS = GENERATION_SECONDS + 120.0
REQUEST_BYTES = 1048576
MARKER = "LOBO_SYNTHETIC_FILLER_19C54A0E"
FILLER = "Neutral reference text for a local performance measurement. "
MESSAGE = ("The following reference text is padding for a performance measurement.\n"
           + MARKER + "\nEnd of reference text.\nWhat is 2 + 2? Reply only 4.")


def emit(event, **numbers):
    """Callers supply only fixed event names and numeric measurements."""
    print(json.dumps({"event": event, **numbers}, allow_nan=False), flush=True)


def tokenize(http, port, text, add_special):
    value = http.json(port, "/tokenize", {"content": text,
                      "add_special": add_special, "parse_special": True,
                      "with_pieces": False})
    bounded.require(type(value) is dict and type(value.get("tokens")) is list,
                    "tokenizer_invalid")
    tokens = value["tokens"]
    bounded.require(0 < len(tokens) <= 4096, "tokenizer_invalid")
    for token in tokens:
        bounded.integer(token, "tokenizer_invalid", 0, 2**31 - 1)
    return tokens


def synthetic_tokens(http, port):
    value = http.json(port, "/apply-template", {
        "messages": [{"role": "user", "content": MESSAGE}],
        "add_generation_prompt": True,
        "chat_template_kwargs": {"enable_thinking": False},
    })
    bounded.require(type(value) is dict and type(value.get("prompt")) is str,
                    "template_invalid")
    prompt = value["prompt"]
    bounded.require(prompt.count(MARKER) == 1, "template_invalid")
    prefix, suffix = prompt.split(MARKER)
    bounded.require(prefix and suffix, "template_invalid")
    # Only the prefix inserts BOS. The native integer-array endpoint does not
    # tokenize again. Truncating repeated filler IDs cannot remove the question
    # or assistant template suffix, and keeps the input count exact.
    before = tokenize(http, port, prefix, True)
    filler = tokenize(http, port, FILLER, False)
    after = tokenize(http, port, suffix, False)
    remaining = INPUT_TOKENS - len(before) - len(after)
    bounded.require(remaining > 0, "template_oversized")
    tokens = before + (filler * ((remaining + len(filler) - 1) // len(filler)))[:remaining] + after
    bounded.require(len(tokens) == INPUT_TOKENS, "input_count_mismatch")
    return tokens, {"prefix_tokens": len(before), "filler_tokens": remaining,
                    "suffix_tokens": len(after), "input_tokens": len(tokens)}


class SamePromptHarness(bounded.Harness):
    def __init__(self, runtime, http, progress=emit):
        super().__init__(runtime, http, INPUT_TOKENS, OUTPUT_TOKENS)
        self.progress = progress

    def ready(self):
        bounded.require(self.runtime.model == "q6", "diagnostic_model_mismatch")
        status = super().ready()
        bounded.require(type(status.get("ctx")) is int and status["ctx"] == CONTEXT_TOKENS,
                        "diagnostic_context_mismatch")
        return status

    def run(self):
        self.ready()
        self.quiet()
        tokens, counts = synthetic_tokens(self.http, self.runtime.port)
        self.preflight.append(counts)
        self.ready()
        self.quiet()
        self.progress("prepared", input_tokens=INPUT_TOKENS,
                      max_output_tokens=OUTPUT_TOKENS, context_tokens=CONTEXT_TOKENS)
        started = time.monotonic()
        finished = threading.Event()

        def heartbeat():
            while not finished.wait(30):
                self.progress("request_waiting", elapsed_seconds=round(time.monotonic() - started, 1))

        thread = threading.Thread(target=heartbeat, daemon=True)
        thread.start()
        # An uncertain failed POST still counts. There is no retry path.
        self.attempts += 1
        try:
            result = self.http.completion(self.runtime.port, tokens, self.runtime.alias,
                                          OUTPUT_TOKENS, input_limit=INPUT_TOKENS,
                                          expected_content="4")
        finally:
            finished.set()
            thread.join(timeout=1)
        self.runtime.unchanged()
        self.ready()
        self.results.append({"prompt_id": "same-size-synthetic", **result})
        bounded.require(result["content_seen"] and result["actual_output_tokens"] > 0,
                        "no_output_content")
        bounded.require(result["cached_input_tokens"] == 0, "prompt_cache_used")
        bounded.require(self.attempts == 1 and len(self.results) == 1, "request_count_invalid")
        self.progress("completed", input_tokens=result["actual_input_tokens"],
                      output_tokens=result["actual_output_tokens"],
                      request_wall_ms=round(result["request_wall_ms"], 1))

    def summary(self, category=None):
        value = super().summary(category)
        value.update({"diagnostic": "same-size-synthetic", "request_limit": 1,
                      "context_tokens": CONTEXT_TOKENS, "thinking_enabled": False,
                      "original_input_count_approximate": True,
                      "endpoint": "/completion", "generation_deadline_seconds": GENERATION_SECONDS})
        return value


def arguments(argv):
    parser = bounded.SafeParser(description=__doc__, allow_abbrev=False)
    parser.add_argument("--config", type=Path)
    parser.add_argument("--evidence-dir", type=Path)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args(argv)
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
    harness, reserved = None, False
    try:
        bounded.evidence_directory(args.evidence_dir)
        reserved = True
        key = bounded.config_key(args.config)
        runtime = bounded.LocalRuntime()
        http = bounded.DirectHTTP(key, time.monotonic() + RUN_SECONDS,
                                  generation_seconds=GENERATION_SECONDS,
                                  request_bytes=REQUEST_BYTES)
        harness = SamePromptHarness(runtime, http)
        harness.run()
        bounded.write_evidence(args.evidence_dir, harness.summary())
        print("passed: one 47000-input-token direct Q6 request", flush=True)
        return 0
    except bounded.Blocked as error:
        category = error.category
    except KeyboardInterrupt:
        category = "interrupted"
    except Exception:
        category = "internal_error"
    if reserved:
        value = harness.summary(category) if harness else {
            "schema": 1, "status": "blocked", "error_category": category,
            "diagnostic": "same-size-synthetic", "generation_attempts": 0,
            "request_limit": 1, "max_input_tokens": INPUT_TOKENS,
            "max_output_tokens": OUTPUT_TOKENS, "measurements": []}
        try:
            bounded.write_evidence(args.evidence_dir, value)
        except bounded.Blocked:
            category = "evidence_write_failed"
    print("blocked: " + category, flush=True)
    return 2


def self_test():
    """No state/config/model reads: all runtime and HTTP data belong to fakes."""
    from http.server import BaseHTTPRequestHandler, HTTPServer
    from contextlib import redirect_stderr
    from io import StringIO

    bounded.self_test()
    alias, secret = "qwen3.5-27b-uncensored-q6", "fake-secret-private"
    cases = {"ok": (None, 1), "wrong_answer": (None, 1),
             "context": ("diagnostic_context_mismatch", 0),
             "model": ("diagnostic_model_mismatch", 0),
             "template": ("template_invalid", 0), "bool_token": ("tokenizer_invalid", 0),
             "missing_content": ("no_output_content", 1),
             "count": ("input_count_mismatch", 1), "cached": ("prompt_cache_used", 1),
             "changed": ("runtime_changed", 1), "truncated": ("response_truncated", 1),
             "auth": ("auth_failed", 1)}
    for scenario, (expected, attempts) in cases.items():
        seen = []

        class FakeRuntime:
            pid, boot, start, model = 123, "fake-boot", 456, "q6"
            completed = False

            def unchanged(self):
                bounded.require(not (scenario == "changed" and self.completed), "runtime_changed")

        class Handler(BaseHTTPRequestHandler):
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
                bounded.require(size <= REQUEST_BYTES, "self_test_body")
                self.route(json.loads(self.rfile.read(size)))

            def route(self, payload):
                seen.append((self.path, payload))
                if self.path == "/api/status":
                    self.answer({"stage": "ready", "boot_id": "fake-boot", "model": "q6",
                                 "ctx": 8192 if scenario == "context" else CONTEXT_TOKENS})
                elif self.path == "/api/version":
                    self.answer({"git_sha": "abcdef123456"})
                elif self.headers.get("Authorization") != "Bearer " + secret:
                    self.answer({"error": secret}, 401)
                elif self.path == "/v1/models":
                    self.answer({"data": [{"id": alias}]})
                elif self.path == "/slots":
                    self.answer([{"is_processing": False}])
                elif self.path == "/apply-template":
                    bounded.require(payload["messages"] == [{"role": "user", "content": MESSAGE}]
                                    and payload["chat_template_kwargs"] == {"enable_thinking": False},
                                    "self_test_template")
                    self.answer({"prompt": "bad" if scenario == "template" else "prefix" + MARKER + "suffix"})
                elif self.path == "/tokenize":
                    bounded.require(payload["parse_special"] is True and payload["with_pieces"] is False,
                                    "self_test_tokenize")
                    before = payload["content"] == "prefix"
                    bounded.require(payload["add_special"] is before, "self_test_bos")
                    tokens = [1, 900001] if before else [900002, 900003] if payload["content"] == FILLER else [900004]
                    self.answer({"tokens": [True] if scenario == "bool_token" else tokens})
                elif self.path == "/completion":
                    tokens = payload["prompt"]
                    bounded.require(len(tokens) == INPUT_TOKENS and tokens[:2] == [1, 900001]
                                    and tokens[-1] == 900004 and tokens.count(1) == 1,
                                    "self_test_exact_input")
                    bounded.require(len(json.dumps(payload).encode()) > bounded.BODY_BYTES,
                                    "self_test_large_request")
                    bounded.require(payload["n_predict"] == OUTPUT_TOKENS and payload["n_cmpl"] == 1
                                    and payload["cache_prompt"] is False, "self_test_limits")
                    if scenario == "auth":
                        self.answer({"error": secret}, 401)
                        return
                    self.send_response(200)
                    self.send_header("Content-Type", "text/event-stream")
                    self.end_headers()
                    content = "" if scenario == "missing_content" else "5" if scenario == "wrong_answer" else "4"
                    partial = {"stop": False, "content": content}
                    final = {"stop": True, "model": alias, "tokens_evaluated": INPUT_TOKENS - (scenario == "count"),
                             "tokens_predicted": 2, "truncated": scenario == "truncated",
                             "generation_settings/n_predict": OUTPUT_TOKENS,
                             "timings": {"prompt_n": INPUT_TOKENS - int(scenario == "cached"),
                                         "cache_n": int(scenario == "cached"), "predicted_n": 2,
                                         "prompt_ms": 1000, "predicted_ms": 100,
                                         "prompt_per_second": 47, "predicted_per_second": 10}}
                    runtime.completed = True
                    for value in (partial, final):
                        self.wfile.write(("data: " + json.dumps(value) + "\n\n").encode())
                    self.wfile.flush()
                else:
                    self.answer({}, 404)

        with HTTPServer(("127.0.0.1", 0), Handler) as server:
            thread = threading.Thread(target=server.serve_forever, kwargs={"poll_interval": 0.01}, daemon=True)
            thread.start()
            runtime = FakeRuntime()
            runtime.alias = alias
            if scenario == "model":
                runtime.model = "q8"
            runtime.port = runtime.agent_port = server.server_port
            http = bounded.DirectHTTP(secret, time.monotonic() + 10, generation_seconds=2,
                                      request_bytes=REQUEST_BYTES)
            harness = SamePromptHarness(runtime, http, progress=lambda *_args, **_kwargs: None)
            connect = socket.create_connection

            def only_fake(address, *args, **kwargs):
                bounded.require(address == ("127.0.0.1", server.server_port), "self_test_external_access")
                return connect(address, *args, **kwargs)

            category = None
            socket.create_connection = only_fake
            try:
                harness.run()
            except bounded.Blocked as error:
                category = error.category
            finally:
                socket.create_connection = connect
                server.shutdown()
                thread.join(timeout=2)
            bounded.require(not thread.is_alive(), "self_test_cleanup")
            bounded.require(category == expected and harness.attempts == attempts,
                            "self_test_diagnostic_failed")
            bounded.require(sum(path == "/completion" for path, _ in seen) == attempts,
                            "self_test_retry")
            summary = json.dumps(harness.summary(category))
            bounded.require(secret not in summary and MARKER not in summary and MESSAGE not in summary
                            and '"content"' not in summary and '"prompt"' not in summary,
                            "self_test_privacy")
            if scenario in ("ok", "wrong_answer"):
                bounded.require(harness.results[0]["expected_content_matched"] == (scenario == "ok"),
                                "self_test_expected_answer")
    for flags in (("--requests", "2"), ("--max-input-tokens", "1"),
                  ("--max-output-tokens", "33"), ("--concurrency", "2"),
                  ("--self-test", "--config", "forbidden")):
        with redirect_stderr(StringIO()) as errors:
            try:
                arguments(list(flags))
            except SystemExit as error:
                bounded.require(error.code == 2 and errors.getvalue() == "blocked: invalid_arguments\n",
                                "self_test_arguments")
            else:
                raise bounded.Blocked("self_test_arguments")
    bounded.require(bounded.DirectHTTP(secret, 0).request_bytes == bounded.BODY_BYTES,
                    "self_test_default_changed")
    print("passed: 12 same-size diagnostic HTTP fake scenarios; short harness unchanged")
    return 0


if __name__ == "__main__":
    sys.exit(main())
