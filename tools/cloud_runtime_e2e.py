#!/usr/bin/env python3
"""One direct RunPod Q6 diagnostic through its existing private SSH tunnel.

Exactly 47,000 synthetic input tokens, at most 32 output, context 65,536.
This tool never rents, starts, stops, retries, reads provider credentials for use,
or calls a provider. The caller must verify the expected image against the
provider's creation receipt before and after this test. --self-test is fake-only.
"""

from datetime import datetime, timezone
import json
import os
from pathlib import Path
import re
import stat
import sys
import tempfile
import time

import bounded_runtime_e2e as bounded
import same_prompt_runtime_e2e as same


IMAGE_PATTERN = r"ghcr\.io/1905/lobocode@sha256:[0-9a-f]{64}"


def parse_expiry(value):
    # Python 3.10 cannot parse Rust's nanoseconds. Truncate only parser input;
    # snapshot still binds the exact original desired.json bytes.
    matched = re.fullmatch(r"(\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2})"
                           r"(?:\.(\d{1,9}))?(Z|[+-]\d{2}:\d{2})", value)
    if matched is None:
        raise ValueError("invalid expiry")
    fraction = "." + matched[2][:6].ljust(6, "0") if matched[2] else ""
    zone = "+00:00" if matched[3] == "Z" else matched[3]
    return datetime.fromisoformat(matched[1] + fraction + zone)


def receipt(instance, boot, image):
    bounded.require(type(instance) is str and re.fullmatch(r"[A-Za-z0-9_-]{1,64}", instance),
                    "expected_instance_invalid")
    bounded.require(type(boot) is str and re.fullmatch(r"[0-9a-f]{16}", boot),
                    "expected_boot_invalid")
    bounded.require(type(image) is str and re.fullmatch(IMAGE_PATTERN, image),
                    "expected_image_invalid")


def private_file(path, optional=False):
    """Bounded, owned, regular, private files only; no symlink or FIFO reads."""
    try:
        fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
        with os.fdopen(fd, "rb") as source:
            info = os.fstat(source.fileno())
            bounded.require(stat.S_ISREG(info.st_mode) and info.st_uid == os.getuid()
                            and stat.S_IMODE(info.st_mode) & 0o077 == 0,
                            "cloud_state_not_private")
            value = source.read(bounded.LINE_BYTES + 1)
        bounded.require(len(value) <= bounded.LINE_BYTES, "cloud_state_oversized")
        return value
    except FileNotFoundError:
        bounded.require(optional, "cloud_state_absent")
        return None
    except OSError:
        raise bounded.Blocked("cloud_state_unavailable") from None


class CloudRuntime:
    model = "q6"

    def __init__(self, config, instance, boot, image):
        receipt(instance, boot, image)
        self.config = Path(os.path.abspath(config))
        self.instance, self.boot, self.image = instance, boot, image
        self.path = self.config.with_suffix(".cloud") / "desired.json"
        self.owner_path = self.config.with_suffix(".app-runtime.json")
        catalog = bounded.object_json(bounded.private_read(
            bounded.CATALOG, 1048576, "catalog_unavailable"))
        bounded.require(type(catalog) is list, "catalog_invalid")
        matches = [item for item in catalog if type(item) is dict and item.get("id") == self.model]
        bounded.require(len(matches) == 1, "catalog_invalid")
        self.alias = matches[0].get("alias")
        bounded.require(type(self.alias) is str and re.fullmatch(r"[a-z0-9.-]{1,80}", self.alias),
                        "catalog_invalid")
        self.identity = self.snapshot()
        desired = bounded.object_json(self.identity[0])
        self.port = desired["port"]
        self.agent_port = self.port + 1

    def snapshot(self):
        raw = private_file(self.path)
        desired = bounded.object_json(raw)
        bounded.require(type(desired) is dict, "cloud_state_invalid")
        bounded.require(desired.get("provider") == "runpod"
                        and desired.get("id") == self.instance
                        and desired.get("boot_id") == self.boot,
                        "cloud_identity_mismatch")
        bounded.require(desired.get("config_path") == str(self.config), "cloud_config_mismatch")
        port = bounded.integer(desired.get("port"), "cloud_port_invalid", 1024, 65534)
        expires = desired.get("expires_at")
        bounded.require(type(expires) is str, "cloud_expiry_invalid")
        try:
            parsed = parse_expiry(expires)
            bounded.require(parsed.tzinfo is not None, "cloud_expiry_invalid")
            bounded.require(parsed > datetime.now(timezone.utc), "cloud_expired")
        except ValueError:
            raise bounded.Blocked("cloud_expiry_invalid") from None
        owner_raw = private_file(self.owner_path, optional=True)
        if owner_raw is not None:
            owner = bounded.object_json(owner_raw)
            bounded.require(type(owner) is dict and owner.get("provider") == "runpod"
                            and owner.get("instance_id") == self.instance
                            and owner.get("boot_id") == self.boot
                            and owner.get("agent_url") == f"http://127.0.0.1:{port + 1}"
                            and owner.get("api_url") == f"http://127.0.0.1:{port}/v1"
                            and owner.get("local_pid") is None
                            and owner.get("local_start_id") is None,
                            "cloud_owner_mismatch")
        # Keep raw bytes private. A credential change or owner-file appearance is
        # a changed runtime, even if selected parsed identity fields still match.
        return raw, owner_raw, private_file(self.config)

    def unchanged(self):
        bounded.require(self.snapshot() == self.identity, "runtime_changed")


class CloudHarness(same.SamePromptHarness):
    def summary(self, category=None):
        # Deliberately do not call the local summary: cloud has no supervisor PID.
        return {"schema": 1, "status": "passed" if category is None else "blocked",
                "error_category": category, "diagnostic": "same-size-synthetic-cloud",
                "provider": "runpod", "instance_id": self.runtime.instance,
                "boot_id": self.runtime.boot, "model": self.runtime.model,
                "expected_image": self.runtime.image,
                "image_verification": "external-provider-receipt-required",
                "agent_revision": self.revision,
                "transport": "existing-private-ssh-loopback",
                "generation_attempts": self.attempts, "request_limit": 1,
                "max_input_tokens": same.INPUT_TOKENS,
                "max_output_tokens": same.OUTPUT_TOKENS,
                "context_tokens": same.CONTEXT_TOKENS, "thinking_enabled": False,
                "original_input_count_approximate": True, "endpoint": "/completion",
                "generation_deadline_seconds": same.GENERATION_SECONDS,
                "preflight": self.preflight, "measurements": self.results}


def arguments(argv):
    parser = bounded.SafeParser(description=__doc__, allow_abbrev=False)
    parser.add_argument("--config", type=Path)
    parser.add_argument("--expected-instance")
    parser.add_argument("--expected-boot")
    parser.add_argument("--expected-image")
    parser.add_argument("--evidence-dir", type=Path)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args(argv)
    fields = (args.config, args.expected_instance, args.expected_boot,
              args.expected_image, args.evidence_dir)
    if args.self_test:
        if any(value is not None for value in fields):
            parser.error("self-test isolation")
    elif any(value is None for value in fields):
        parser.error("paths and receipt required")
    return args


def main(argv=None):
    args = arguments(argv)
    if args.self_test:
        return self_test()
    harness, reserved = None, False
    try:
        receipt(args.expected_instance, args.expected_boot, args.expected_image)
        bounded.evidence_directory(args.evidence_dir)
        reserved = True
        runtime = CloudRuntime(args.config, args.expected_instance, args.expected_boot,
                               args.expected_image)
        key = bounded.config_key(args.config)
        runtime.unchanged()
        http = bounded.DirectHTTP(key, time.monotonic() + same.RUN_SECONDS,
                                  generation_seconds=same.GENERATION_SECONDS,
                                  request_bytes=same.REQUEST_BYTES)
        harness = CloudHarness(runtime, http)
        harness.run()
        bounded.write_evidence(args.evidence_dir, harness.summary())
        print("passed: one 47000-input-token direct RunPod Q6 request", flush=True)
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
            "diagnostic": "same-size-synthetic-cloud", "provider": "runpod",
            "generation_attempts": 0, "request_limit": 1,
            "max_input_tokens": same.INPUT_TOKENS, "max_output_tokens": same.OUTPUT_TOKENS,
            "measurements": []}
        try:
            bounded.write_evidence(args.evidence_dir, value)
        except bounded.Blocked:
            category = "evidence_write_failed"
    print("blocked: " + category, flush=True)
    return 2


def self_test():
    """Existing fake HTTP tests prove the exact generation; these test ownership."""
    same.self_test()
    instance, boot = "fake-instance", "1234567890abcdef"
    image = "ghcr.io/1905/lobocode@sha256:" + "a" * 64

    def expect(category, action):
        try:
            action()
        except bounded.Blocked as error:
            bounded.require(error.category == category, "self_test_category")
        else:
            raise bounded.Blocked("self_test_expected_failure")

    with tempfile.TemporaryDirectory(prefix="lobo-cloud-harness-test-") as temp:
        root = Path(temp)
        config = root / "config.env"
        desired_path = root / "config.cloud/desired.json"
        desired_path.parent.mkdir(mode=0o700)
        owner_path = root / "config.app-runtime.json"
        desired = {"provider": "runpod", "id": instance, "boot_id": boot,
                   "config_path": str(config), "port": 18933,
                   "expires_at": "2099-01-01T00:00:00.894477209Z"}
        owner = {"provider": "runpod", "instance_id": instance, "boot_id": boot,
                 "agent_url": "http://127.0.0.1:18934", "api_url": "http://127.0.0.1:18933/v1",
                 "local_pid": None, "local_start_id": None}

        def save(path, value):
            path.write_text(value if type(value) is str else json.dumps(value))
            path.chmod(0o600)

        def runtime():
            return CloudRuntime(config, instance, boot, image)

        save(config, "LOBO_API_KEY=fake-secret-private\n")
        save(desired_path, desired)
        baseline = runtime()
        baseline.unchanged()
        bounded.require((baseline.port, baseline.agent_port) == (18933, 18934), "self_test_ports")
        for key, value, category in (
            ("provider", "vast", "cloud_identity_mismatch"),
            ("id", "other-instance", "cloud_identity_mismatch"),
            ("boot_id", "ffffffffffffffff", "cloud_identity_mismatch"),
            ("config_path", str(root / "other.env"), "cloud_config_mismatch"),
            ("port", True, "cloud_port_invalid"),
            ("port", 65535, "cloud_port_invalid"),
            ("expires_at", "2000-01-01T00:00:00Z", "cloud_expired"),
            ("expires_at", "2099-01-01T00:00:00", "cloud_expiry_invalid"),
            ("expires_at", "invalid", "cloud_expiry_invalid"),
        ):
            save(desired_path, {**desired, key: value})
            expect(category, runtime)
        save(desired_path, desired)
        save(owner_path, owner)
        expect("runtime_changed", baseline.unchanged)
        runtime().unchanged()
        for key, value in (("boot_id", "ffffffffffffffff"), ("instance_id", "other"),
                           ("api_url", "http://example.com/v1"), ("local_pid", 42)):
            save(owner_path, {**owner, key: value})
            expect("cloud_owner_mismatch", runtime)
        save(owner_path, owner)
        baseline = runtime()
        save(config, "LOBO_API_KEY=other-fake-secret\n")
        expect("runtime_changed", baseline.unchanged)
        save(config, "LOBO_API_KEY=fake-secret-private\n")
        baseline = runtime()
        save(desired_path, json.dumps(desired) + "\n")
        expect("runtime_changed", baseline.unchanged)
        save(desired_path, desired)
        desired_path.chmod(0o644)
        expect("cloud_state_not_private", runtime)
        desired_path.unlink()
        desired_path.symlink_to(config)
        expect("cloud_state_unavailable", runtime)
        desired_path.unlink()
        expect("cloud_state_absent", runtime)
        save(desired_path, desired)
        class FakeHTTP:
            completed = 0
            remote_boot = boot

            def json(self, port, path, payload=None, auth=True):
                bounded.require(port == (18934 if path.startswith("/api/") else 18933),
                                "self_test_port")
                if path == "/api/status":
                    return {"stage": "ready", "boot_id": self.remote_boot,
                            "model": "q6", "ctx": same.CONTEXT_TOKENS}
                if path == "/api/version":
                    return {"git_sha": "abcdef123456"}
                if path == "/v1/models":
                    bounded.require(auth, "self_test_auth")
                    return {"data": [{"id": baseline.alias}]}
                if path == "/slots":
                    return [{"is_processing": False}]
                if path == "/apply-template":
                    return {"prompt": "prefix" + same.MARKER + "suffix"}
                if path == "/tokenize":
                    return {"tokens": [101, 102] if payload["add_special"] else [103]}
                raise bounded.Blocked("self_test_endpoint")

            def completion(self, port, tokens, alias, cap, **kwargs):
                self.completed += 1
                bounded.require(self.completed == 1 and port == 18933 and alias == baseline.alias
                                and len(tokens) == kwargs["input_limit"] == same.INPUT_TOKENS
                                and cap == same.OUTPUT_TOKENS and kwargs["expected_content"] == "4",
                                "self_test_request")
                return {"content_seen": True, "actual_input_tokens": same.INPUT_TOKENS,
                        "actual_output_tokens": 2, "cached_input_tokens": 0,
                        "request_wall_ms": 100, "expected_content_matched": True}

        http = FakeHTTP()
        harness = CloudHarness(runtime(), http, progress=lambda *_args, **_kwargs: None)
        harness.run()
        bounded.require(http.completed == harness.attempts == 1, "self_test_request_count")
        http.remote_boot = "ffffffffffffffff"
        rejected = CloudHarness(runtime(), http, progress=lambda *_args, **_kwargs: None)
        expect("runtime_identity_mismatch", rejected.run)
        bounded.require(rejected.attempts == 0 and http.completed == 1, "self_test_wrong_boot_sent")
        summary = harness.summary()
        text = json.dumps(summary)
        bounded.require("supervisor" not in text and "fake-secret" not in text
                        and str(config) not in text and summary["expected_image"] == image,
                        "self_test_privacy")
    expect("expected_boot_invalid", lambda: receipt(instance, "bad", image))
    expect("expected_instance_invalid", lambda: receipt("../bad", boot, image))
    expect("expected_image_invalid", lambda: receipt(instance, boot, "ghcr.io/1905/lobocode:latest-q6"))
    print("passed: cloud saved-connection identity, privacy, expiry and file-change checks")
    return 0


if __name__ == "__main__":
    sys.exit(main())
