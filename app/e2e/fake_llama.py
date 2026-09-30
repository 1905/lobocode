#!/usr/bin/env python3
"""Local-only HTTP fixture launched by the actual Rust supervisor."""
import json
import os
from pathlib import Path
import sys
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

ROOT = Path(__file__).resolve().parents[2]
MODE = ROOT / ".e2e-mode.json"
mode = json.loads(MODE.read_text()) if MODE.exists() else {}
if "--list-devices" in sys.argv:
    print("MTL0: isolated E2E fixture")
    sys.exit(0)
if mode.get("exit"):
    print("error: isolated E2E runtime failure", flush=True)
    sys.exit(2)
port = int(sys.argv[sys.argv.index("--port") + 1])
alias = sys.argv[sys.argv.index("--alias") + 1]
started = time.monotonic()
(ROOT / ".e2e-llama.pid").write_text(str(os.getpid()))


class Handler(BaseHTTPRequestHandler):
    def log_message(self, *_):
        pass

    def respond(self, code, body, content_type="application/json"):
        data = body.encode() if isinstance(body, str) else json.dumps(body).encode()
        self.send_response(code)
        self.send_header("Content-Type", content_type)
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def do_GET(self):
        if self.path == "/health":
            ready = time.monotonic() - started >= mode.get("health_delay", 0)
            self.respond(200 if ready else 503, {"status": "ok" if ready else "loading"})
        elif self.headers.get("Authorization") != "Bearer " + os.environ["LLAMA_API_KEY"]:
            self.respond(401, {"error": "unauthorized"})
        elif self.path == "/v1/models":
            self.respond(200, {"object": "list", "data": [{"id": alias, "object": "model"}]})
        elif self.path == "/metrics":
            self.respond(200, "llamacpp:requests_processing 0\nllamacpp:prompt_tokens_total 42\nllamacpp:tokens_predicted_total 7\nllamacpp:prompt_tokens_seconds 123\nllamacpp:predicted_tokens_seconds 12\n", "text/plain")
        else:
            self.respond(404, {"error": "not found"})


ThreadingHTTPServer(("127.0.0.1", port), Handler).serve_forever()
