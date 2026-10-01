#!/usr/bin/env python3
"""Small, offline transport fixtures. Run on Dell, never with model data."""

import copy
import hashlib
import json
import os
from pathlib import Path
import stat
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

import cloud_candidate_transfer as transfer


SOURCE = "a" * 40
DIGEST = "sha256:" + "b" * 64


class TransferTests(unittest.TestCase):
    def setUp(self):
        self.folder = tempfile.TemporaryDirectory(prefix="candidate-transfer-fixture-")
        self.addCleanup(self.folder.cleanup)
        self.root = Path(self.folder.name)
        self.layout = self.root / "source" / "q6-oci"
        blobs = self.layout / "blobs" / "sha256"
        blobs.mkdir(parents=True)
        self.metadata = self.root / "source" / "q6-metadata.json"
        self.metadata.write_bytes(b'{"fixture":"metadata"}')
        (self.layout / "index.json").write_bytes(b'{"fixture":"index"}')
        (self.layout / "oci-layout").write_bytes(b'{"imageLayoutVersion":"1.0.0"}')
        for content in (b"", bytes(range(251)), b"a short second blob"):
            (blobs / hashlib.sha256(content).hexdigest()).write_bytes(content)
        self.original = self.inventory(self.root / "source")
        for name in self.original:
            (self.root / "source" / name).chmod(0o444)
        self.parts = self.root / "parts"
        self.output = self.root / "restored"

    @staticmethod
    def inventory(root):
        return {str(path.relative_to(root)): path.read_bytes()
                for path in root.rglob("*") if path.is_file()}

    def pack(self, part_bytes=17):
        self.receipt = transfer.pack(self.layout, self.metadata, SOURCE, DIGEST,
                                     self.parts, part_bytes)
        self.manifest = json.loads((self.parts / transfer.MANIFEST).read_bytes())
        return self.receipt

    def restore(self, consume=True, **kwargs):
        values = {"parts_dir": self.parts, "manifest_sha256": self.receipt["manifest_sha256"],
                  "source_sha": SOURCE, "image_digest": DIGEST, "output": self.output,
                  "consume_parts": consume}
        values.update(kwargs)
        return transfer.restore(**values)

    def rewrite(self, manifest):
        path = self.parts / transfer.MANIFEST
        path.chmod(0o600)
        raw = transfer.encode_manifest(manifest)
        path.write_bytes(raw)
        self.receipt["manifest_sha256"] = hashlib.sha256(raw).hexdigest()

    def assert_rejected_before_output(self, **kwargs):
        with self.assertRaises((transfer.InvalidTransfer, OSError, ValueError)):
            self.restore(**kwargs)
        self.assertFalse(self.output.exists())

    def test_roundtrip_is_deterministic_and_preserves_readonly_source(self):
        self.pack()
        second = self.root / "parts-again"
        receipt = transfer.pack(self.layout, self.metadata, SOURCE, DIGEST, second, 17)
        self.assertEqual(receipt, self.receipt)
        self.assertEqual(self.inventory(second), self.inventory(self.parts))
        self.assertLess(self.manifest["parts"][-1]["size"], 17)
        self.assertTrue(any(file["size"] > 3 * 17 for file in self.manifest["files"]))
        result = self.restore()
        self.assertEqual(result["restored_bytes"], sum(map(len, self.original.values())))
        self.assertEqual(self.inventory(self.output), self.original)
        self.assertEqual(sorted(path.name for path in self.parts.iterdir()), [transfer.MANIFEST])
        self.assertEqual(self.inventory(self.root / "source"), self.original)
        self.assertTrue(all(not stat.S_IMODE((self.root / "source" / name).stat().st_mode) & 0o222
                            for name in self.original))

    def test_one_part_contains_many_files_and_empty_file(self):
        self.pack(1024)
        self.assertEqual(len(self.manifest["parts"]), 1)
        self.restore(consume=False)
        self.assertEqual(self.inventory(self.output), self.original)
        self.assertTrue((self.parts / "part-000000.bin").exists())

    def test_exact_part_boundary_has_no_extra_empty_part(self):
        self.metadata.chmod(0o600)
        total = sum(len(value) for value in self.original.values())
        self.metadata.write_bytes(self.metadata.read_bytes() + b"x" * (-total % 32))
        self.pack(32)
        self.assertEqual(self.manifest["parts"][-1]["size"], 32)
        self.restore()
        self.assertEqual(self.inventory(self.output), self.inventory(self.root / "source"))

    def test_consume_bounds_live_bytes_and_waits_for_flush(self):
        self.pack()
        original_unlink = Path.unlink
        observations = []

        def observe_unlink(path, *args, **kwargs):
            if path.parent == self.parts and path.name.endswith(".bin"):
                live = sum(p.stat().st_size for p in self.parts.glob("*.bin"))
                live += sum(p.stat().st_size for p in self.output.rglob("*") if p.is_file())
                observations.append(live)
                self.assertLessEqual(live, self.manifest["total_bytes"] + 17)
                index = int(path.stem.split("-")[1])
                reconstructed = sum(p.stat().st_size for p in self.output.rglob("*") if p.is_file())
                expected = sum(part["size"] for part in self.manifest["parts"][:index + 1])
                self.assertEqual(reconstructed, expected)
            return original_unlink(path, *args, **kwargs)

        with patch.object(Path, "unlink", observe_unlink):
            self.restore()
        self.assertEqual(len(observations), len(self.manifest["parts"]))

    def test_corrupt_part_is_never_consumed(self):
        self.pack()
        path = self.parts / "part-000000.bin"
        path.chmod(0o600)
        data = path.read_bytes()
        path.write_bytes(bytes([data[0] ^ 1]) + data[1:])
        self.assert_rejected_before_output()
        self.assertTrue(path.exists())

    def test_part_hash_is_checked_independently_of_file_hash(self):
        self.pack()
        self.manifest["parts"][0]["sha256"] = "f" * 64
        self.rewrite(self.manifest)
        self.assert_rejected_before_output()
        self.assertTrue((self.parts / "part-000000.bin").exists())

    def test_file_hash_is_checked_independently_of_part_hash(self):
        self.pack()
        self.manifest["files"][0]["sha256"] = "f" * 64
        self.rewrite(self.manifest)
        self.assert_rejected_before_output()

    def test_flush_failure_does_not_consume_current_part(self):
        self.pack()
        with patch.object(transfer.FileSink, "flush", side_effect=OSError("private detail")):
            with self.assertRaises(OSError):
                self.restore()
        self.assertTrue((self.parts / "part-000000.bin").exists())
        self.assertFalse(self.output.exists())

    def test_truncated_or_extended_part_is_rejected_before_consumption(self):
        self.pack()
        path = self.parts / self.manifest["parts"][-1]["name"]
        original = path.read_bytes()
        path.chmod(0o600)
        for content in (original[:-1], original + b"x"):
            with self.subTest(size=len(content)):
                path.write_bytes(content)
                self.assert_rejected_before_output()
                self.assertTrue((self.parts / "part-000000.bin").exists())

    def test_missing_extra_reordered_and_swapped_parts(self):
        self.pack()
        path = self.parts / "part-000000.bin"
        data = path.read_bytes()
        path.unlink()
        self.assert_rejected_before_output()
        path.write_bytes(data)
        extra = self.parts / "part-999999.bin"
        extra.write_bytes(b"extra")
        self.assert_rejected_before_output()
        extra.unlink()
        original = copy.deepcopy(self.manifest)
        self.manifest["parts"][0:2] = reversed(self.manifest["parts"][0:2])
        self.rewrite(self.manifest)
        self.assert_rejected_before_output()
        self.rewrite(original)
        other = self.parts / "part-000001.bin"
        other.chmod(0o600)
        path.write_bytes(other.read_bytes())
        other.write_bytes(data)
        self.assert_rejected_before_output()

    def test_manifest_hash_and_identity_are_required(self):
        self.pack()
        for values in ({"manifest_sha256": "0" * 64}, {"source_sha": "c" * 40},
                       {"image_digest": "sha256:" + "d" * 64}):
            with self.subTest(values=values):
                self.assert_rejected_before_output(**values)

    def test_invalid_schema_sizes_offsets_and_record_shapes(self):
        self.pack()
        baseline = copy.deepcopy(self.manifest)
        changes = [lambda m: m.update(schema=True), lambda m: m.update(total_bytes=True),
                   lambda m: m.update(total_bytes=transfer.MAX_BYTES + 1),
                   lambda m: m.update(part_size=transfer.PART_BYTES + 1),
                   lambda m: m.update(part_size=0), lambda m: m.update(part_size=1.5),
                   lambda m: m["files"][0].update(size=True),
                   lambda m: m["files"][0].update(size=-1),
                   lambda m: m["files"][0].update(offset=False),
                   lambda m: m["files"][1].update(offset=0),
                   lambda m: m["parts"][0].update(size=True),
                   lambda m: m["parts"][-1].update(size=0),
                   lambda m: m["files"].reverse(),
                   lambda m: m["files"].append(m["files"][0]),
                   lambda m: m["parts"].extend([m["parts"][0]] * 129),
                   lambda m: m.update(extra="not allowed"),
                   lambda m: m["files"][0].update(extra="not allowed")]
        for index, change in enumerate(changes):
            with self.subTest(index=index):
                manifest = copy.deepcopy(baseline)
                change(manifest)
                self.rewrite(manifest)
                self.assert_rejected_before_output()

    def test_rejects_all_nonallowlisted_manifest_paths(self):
        self.pack()
        baseline = copy.deepcopy(self.manifest)
        paths = ["../escape", "/tmp/escape", "q6-oci/../escape", "q6-oci//index.json",
                 "q6-oci/./index.json", "q6-oci\\index.json", "", "q6-oci/blobs/sha256/" + "A" * 64,
                 "q6-oci/blobs/sha256/" + "a" * 63, "q6-oci/extra", "q6-metadata.json\x00x"]
        for path in paths:
            with self.subTest(path=path):
                manifest = copy.deepcopy(baseline)
                manifest["files"][0]["path"] = path
                self.rewrite(manifest)
                self.assert_rejected_before_output()

    def test_existing_outputs_and_overlapping_trees_are_rejected(self):
        self.pack()
        self.output.mkdir()
        with self.assertRaises(transfer.InvalidTransfer):
            self.restore()
        self.output.rmdir()
        for output in (self.parts / "inside", self.parts, self.root):
            with self.subTest(output=output):
                with self.assertRaises(transfer.InvalidTransfer):
                    self.restore(output=output)
        with self.assertRaises(transfer.InvalidTransfer):
            transfer.pack(self.layout, self.metadata, SOURCE, DIGEST, self.layout / "inside")

    def test_symlink_roots_parents_parts_and_dangling_output(self):
        self.pack()
        alias = self.root / "alias"
        alias.symlink_to(self.parts, target_is_directory=True)
        self.assert_rejected_before_output(parts_dir=alias)
        self.output.symlink_to(self.root / "missing")
        with self.assertRaises(transfer.InvalidTransfer):
            self.restore()
        self.output.unlink()
        parent = self.root / "output-parent"
        parent.symlink_to(self.root, target_is_directory=True)
        with self.assertRaises(transfer.InvalidTransfer):
            self.restore(output=parent / "new")
        part = self.parts / "part-000000.bin"
        saved = self.root / "saved-part"
        part.rename(saved)
        part.symlink_to(saved)
        self.assert_rejected_before_output()

    def test_extra_source_symlink_and_nonregular_source_are_rejected(self):
        extra = self.layout / "unexpected"
        extra.write_bytes(b"extra")
        with self.assertRaises(transfer.InvalidTransfer):
            self.pack()
        self.assertFalse(self.parts.exists())
        extra.unlink()
        index = self.layout / "index.json"
        saved = self.root / "saved-index"
        index.rename(saved)
        index.symlink_to(saved)
        with self.assertRaises(transfer.InvalidTransfer):
            self.pack()
        index.unlink()
        os.mkfifo(index)
        with self.assertRaises(transfer.InvalidTransfer):
            self.pack()
        self.assertFalse(self.parts.exists())

    def test_regular_file_replaced_with_fifo_does_not_block(self):
        self.pack()
        target = self.parts / "part-000000.bin"
        original_open = os.open

        def replace_before_open(path, flags, *args, **kwargs):
            if Path(path) == target:
                target.unlink()
                os.mkfifo(target)
                self.assertTrue(flags & os.O_NONBLOCK)
            return original_open(path, flags, *args, **kwargs)

        with patch.object(os, "open", replace_before_open):
            self.assert_rejected_before_output()

    def test_duplicate_manifest_keys_and_manifest_size_cap(self):
        self.pack()
        path = self.parts / transfer.MANIFEST
        path.chmod(0o600)
        for raw in (b'{"schema":1,"schema":1}', b" " * (transfer.MAX_MANIFEST + 1)):
            with self.subTest(size=len(raw)):
                path.write_bytes(raw)
                self.receipt["manifest_sha256"] = hashlib.sha256(raw).hexdigest()
                self.assert_rejected_before_output()

    def test_cli_roundtrip_and_sanitized_failure(self):
        script = str(Path(transfer.__file__).resolve())
        result = subprocess.run([sys.executable, script, "pack", "--layout", str(self.layout),
                                 "--metadata", str(self.metadata), "--source-sha", SOURCE,
                                 "--image-digest", DIGEST, "--output", str(self.parts),
                                 "--part-bytes", "17"], capture_output=True, text=True, check=True)
        receipt = json.loads(result.stdout)
        result = subprocess.run([sys.executable, script, "restore", "--parts-dir", str(self.parts),
                                 "--manifest-sha256", receipt["manifest_sha256"],
                                 "--source-sha", SOURCE, "--image-digest", DIGEST,
                                 "--output", str(self.output), "--consume-parts"],
                                capture_output=True, text=True, check=True)
        self.assertEqual(json.loads(result.stdout)["restored_bytes"], receipt["total_bytes"])
        self.assertEqual(self.inventory(self.output), self.original)
        secret = "private-token-must-not-appear"
        result = subprocess.run([sys.executable, script, "pack", "--secret", secret],
                                capture_output=True, text=True)
        self.assertEqual(result.returncode, 1)
        self.assertEqual(result.stderr, "Transfer failed: Invalid transfer arguments.\n")
        self.assertNotIn(secret, result.stdout + result.stderr)


if __name__ == "__main__":
    unittest.main()
