"""Owned loopback runtime. No subprocess, process signals or generation."""
import json
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
import secrets
import threading
from urllib.parse import urlsplit


class RuntimeFixture:
    def __init__(self, root, info):
        self.root = Path(root).resolve(strict=True)
        metadata = json.loads((self.root / "fixture.json").read_text())
        if not self.root.name.startswith("lobo-native-e2e-") or info["mode"] != "runtime-ui-only" or metadata != info:
            raise ValueError("invalid runtime fixture root or mode")
        if info["root"] != str(self.root):
            raise ValueError("invalid runtime fixture identity")
        if Path(info["config"]).resolve(strict=True) != self.root / "config.env":
            raise ValueError("invalid runtime fixture config")
        self.lock = threading.RLock()
        self.release = threading.Event()
        self.release.set()
        self.token = secrets.token_urlsafe(32)
        self.expected_key = "sk-isolated-native-app-e2e"
        self.present, self.stage, self.boot = True, "load", "fixture-boot-1"
        self.hold, self.hold_entered = False, 0
        self.counts = {}
        owner = self

        class Handler(BaseHTTPRequestHandler):
            def log_message(self, *_args):
                pass  # HTTP headers and request content must not enter artifacts.

            def reply(self, code, body):
                data = json.dumps(body).encode()
                self.send_response(code)
                self.send_header("Content-Type", "application/json")
                self.send_header("Content-Length", str(len(data)))
                self.end_headers()
                try:
                    self.wfile.write(data)
                except (BrokenPipeError, ConnectionResetError):
                    pass

            def do_GET(self):
                owner.request(self, False)

            def do_POST(self):
                owner.request(self, True)

        self.server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        self.server.daemon_threads = True
        self.origin = f"http://127.0.0.1:{self.server.server_port}"
        info.update(runtime_origin=self.origin, control_token=self.token,
                    agent_url=self.origin, api_url=self.origin + "/v1")
        (self.root / "fixture.json").write_text(json.dumps(info, indent=2) + "\n")
        (self.root / "fixture.json").chmod(0o600)
        self.thread = threading.Thread(target=self.server.serve_forever, name="lobo-owned-fixture", daemon=True)
        self.thread.start()

    def evidence(self):
        return {"counts": dict(self.counts), "present": self.present,
                "stage": self.stage, "boot": self.boot, "hold_entered": self.hold_entered}

    def record(self, event):
        self.counts[event] = self.counts.get(event, 0) + 1
        (self.root / "runtime-evidence.json").write_text(json.dumps(self.evidence(), indent=2) + "\n")

    def instance(self):
        return {"provider": "local", "id": "fixture-local", "status": "running",
                "detail": "owned native fixture", "cost_per_hr": 0,
                "api_url": self.origin + "/v1", "agent_url": self.origin}

    def request(self, handler, post):
        route = urlsplit(handler.path).path
        # Reject oversized bodies before reading. Only the authenticated control
        # and exact owned-delete routes accept JSON.
        body = {}
        if post:
            try:
                size = int(handler.headers.get("Content-Length", "0"))
                if not 0 <= size <= 4096:
                    raise ValueError()
                body = json.loads(handler.rfile.read(size) or b"{}")
                if not isinstance(body, dict):
                    raise ValueError()
            except (ValueError, json.JSONDecodeError):
                return handler.reply(400, {"error": "invalid fixture request"})
        with self.lock:
            if route.startswith("/v1/") and route != "/v1/models":
                self.record("forbidden_generation")
                return handler.reply(403, {"error": "generation is disabled"})
            if route.startswith("/fixture/"):
                if handler.headers.get("X-Lobo-Fixture-Control") != self.token:
                    return handler.reply(403, {"error": "fixture authentication failed"})
                if route == "/fixture/evidence" and not post:
                    return handler.reply(200, self.evidence())
                if route == "/fixture/control" and post:
                    action = body.get("action")
                    if action == "stage" and body.get("stage") in ("load", "ready"):
                        self.stage = body["stage"]
                    elif action == "auth":
                        self.expected_key = "invalid-fixture-key" if body.get("reject") else "sk-isolated-native-app-e2e"
                    elif action == "hold":
                        self.hold = True
                        self.release.clear()
                    elif action == "release":
                        self.hold = False
                        self.release.set()
                    elif action == "reset" and not self.present:
                        boot = body.get("boot", "")
                        if not isinstance(boot, str) or not boot.startswith("fixture-boot-") or len(boot) > 64:
                            return handler.reply(400, {"error": "invalid fixture boot"})
                        self.present, self.stage, self.boot = True, "ready", boot
                    elif action == "replace_boot" and self.present:
                        # A fixed token avoids reflecting arbitrary request data.
                        self.boot = "fixture-boot-replaced"
                    elif action == "restore_boot":
                        self.boot = "fixture-boot-1"
                    else:
                        return handler.reply(409, {"error": "fixture control rejected"})
                    self.record("control." + action)
                    return handler.reply(200, self.evidence())
                prefix = "/fixture/provider/"
                if route.startswith(prefix):
                    parts = route[len(prefix):].split("/")
                    provider, method = parts[:2] if len(parts) >= 2 else ("", "")
                    if provider not in ("local", "runpod", "vast") or method not in ("list", "get", "identity", "delete-owned", "rent", "delete"):
                        return handler.reply(404, {"error": "unknown fixture method"})
                    self.record(provider + "." + method)
                    if provider != "local":
                        return handler.reply(503, {"error": "owned cloud fixture unavailable"})
                    if method in ("rent", "delete"):
                        self.record("forbidden_provider_method")
                        return handler.reply(403, {"error": "unscoped operation denied"})
                    if not post and method == "list":
                        return handler.reply(200, [self.instance()] if self.present else [])
                    if not post and len(parts) == 3 and parts[2] == "fixture-local":
                        if method == "get":
                            return handler.reply(200, self.instance()) if self.present else handler.reply(404, {})
                        if method == "identity":
                            return handler.reply(200, [self.boot, 4242, 100] if self.present else None)
                    if post and method == "delete-owned":
                        if body == {"id": "fixture-local", "boot_id": self.boot, "start_id": 100} and self.present:
                            self.present = False
                            self.record("local.deleted")
                            return handler.reply(200, {})
                        return handler.reply(409, {"error": "owned identity mismatch"})
                return handler.reply(404, {})
            if route == "/api/status" and not post:
                self.record("agent.status")
                return handler.reply(200, {"boot_id": self.boot, "stage": self.stage,
                                          "stage_detail": "owned native fixture", "model": "q6", "ctx": 8192})
            if route == "/api/version" and not post:
                return handler.reply(200, {"version": "fixture", "git_sha": "fixture"})
            if route == "/api/logs" and not post:
                if handler.headers.get("Authorization") != "Bearer " + self.expected_key:
                    return handler.reply(401, {})
                data = b"owned native fixture\n"
                handler.send_response(200)
                handler.send_header("Content-Length", str(len(data)))
                handler.end_headers()
                handler.wfile.write(data)
                return
            if route != "/v1/models" or post:
                return handler.reply(404, {})
            self.record("models.request")
            if handler.headers.get("Authorization") != "Bearer " + self.expected_key:
                self.record("models.auth_rejected")
                return handler.reply(401, {})
            self.record("models.authenticated")
            held = self.hold
            if held:
                self.hold_entered += 1
                self.record("models.hold_entered")
        # Never retain the state lock while a model-list response is held.
        if held and not self.release.wait(timeout=7):
            return handler.reply(503, {"error": "fixture hold timed out"})
        catalog = json.loads((Path(__file__).resolve().parents[2] / "crates/lobo-proto/catalog.json").read_text())
        alias = next(model["alias"] for model in catalog if model["id"] == "q6")
        handler.reply(200, {"data": [{"id": alias}]})

    def close(self):
        self.release.set()
        self.server.shutdown()
        self.server.server_close()
        self.thread.join(timeout=5)
        with self.lock:
            self.record("fixture.closed")


def start(root, fixture_info):
    return RuntimeFixture(root, fixture_info)


def self_test():
    """Deferred contract checks. This starts only owned HTTP services when called."""
    import tempfile
    import unittest
    from urllib.error import HTTPError
    from urllib.request import Request, urlopen

    class Contract(unittest.TestCase):
        def setUp(self):
            self.directory = tempfile.TemporaryDirectory(prefix="lobo-native-e2e-")
            self.root = Path(self.directory.name).resolve()
            (self.root / "config.env").write_text("LOBO_API_KEY=sk-isolated-native-app-e2e\n")
            self.info = {"root": str(self.root), "config": str(self.root / "config.env"), "mode": "runtime-ui-only"}
            (self.root / "fixture.json").write_text(json.dumps(self.info))
            self.service = None

        def tearDown(self):
            if self.service is not None:
                self.service.close()
                self.assertFalse(self.service.thread.is_alive())
            self.directory.cleanup()

        def launch(self):
            self.service = start(self.root, self.info)

        def call(self, route, body=None, api_key=None):
            headers = {"X-Lobo-Fixture-Control": self.service.token}
            if api_key is not None:
                headers["Authorization"] = "Bearer " + api_key
            request = Request(self.service.origin + route, data=json.dumps(body).encode() if body is not None else None,
                              headers=headers)
            try:
                with urlopen(request, timeout=3) as response:
                    return response.status, json.load(response)
            except HTTPError as error:
                return error.code, json.load(error)

        def test_bad_metadata_and_config_fail_before_server(self):
            self.info["mode"] = "setup"
            with self.assertRaises(ValueError):
                start(self.root, self.info)
            self.info["mode"] = "runtime-ui-only"
            self.info["config"] = str(self.root / "other.env")
            (self.root / "fixture.json").write_text(json.dumps(self.info))
            with self.assertRaises(FileNotFoundError):
                start(self.root, self.info)

        def test_identity_cloud_and_generation_boundaries(self):
            self.launch()
            wrong = {"id": "fixture-local", "boot_id": "wrong", "start_id": 100}
            self.assertEqual(self.call("/fixture/provider/local/delete-owned", wrong)[0], 409)
            self.assertTrue(self.service.present)
            for provider in ("runpod", "vast"):
                for method in ("list", "get", "identity", "rent", "delete", "delete-owned"):
                    self.assertEqual(self.call(f"/fixture/provider/{provider}/{method}")[0], 503)
            self.assertEqual(self.call("/v1/chat/completions", {})[0], 403)
            self.assertEqual(self.service.counts["forbidden_generation"], 1)
            correct = {"id": "fixture-local", "boot_id": "fixture-boot-1", "start_id": 100}
            self.assertEqual(self.call("/fixture/provider/local/delete-owned", correct)[0], 200)
            self.assertFalse(self.service.present)
            self.assertFalse((self.root / "state/lobo/local.json").exists())

        def test_auth_hold_release_and_cleanup_after_runner_failure(self):
            self.launch()
            self.assertEqual(self.call("/v1/models", api_key="wrong")[0], 401)
            self.call("/fixture/control", {"action": "hold"})
            result = []
            held = threading.Thread(target=lambda: result.append(self.call("/v1/models", api_key="sk-isolated-native-app-e2e")))
            held.start()
            # Explicit hold-entry evidence separates admission from response.
            import time
            deadline = time.monotonic() + 2
            while time.monotonic() < deadline:
                if self.call("/fixture/evidence")[1]["hold_entered"]:
                    break
                time.sleep(0.01)
            self.assertEqual(self.call("/fixture/evidence")[1]["hold_entered"], 1)
            self.assertEqual(result, [])
            self.call("/fixture/control", {"action": "release"})
            held.join(timeout=3)
            self.assertFalse(held.is_alive())
            self.assertEqual(result[0][0], 200)
            self.service.close()
            self.assertFalse(self.service.thread.is_alive())
            evidence = json.loads((self.root / "runtime-evidence.json").read_text())
            self.assertNotIn(self.service.token, json.dumps(evidence))
            self.service = None

    return unittest.TextTestRunner(verbosity=2).run(unittest.defaultTestLoader.loadTestsFromTestCase(Contract)).wasSuccessful()


if __name__ == "__main__":
    import argparse
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true")
    arguments = parser.parse_args()
    if not arguments.self_test:
        parser.error("this fixture is owned by tools/native_app_e2e.py; only explicit --self-test is supported")
    raise SystemExit(0 if self_test() else 1)
