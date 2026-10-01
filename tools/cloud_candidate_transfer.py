#!/usr/bin/env python3
"""Split a verified Q6 OCI export into bounded parts and restore its exact bytes."""

import argparse
from contextlib import contextmanager
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import stat
import sys


PART_BYTES = 512 * 1024 * 1024
MAX_BYTES = 64 * 1024 * 1024 * 1024
MAX_PARTS = 128
MAX_MANIFEST = 4 * 1024 * 1024
MAX_FILES = MAX_MANIFEST // 128  # Every permitted file record needs more bytes.
BUFFER_BYTES = 1024 * 1024
MANIFEST = "transfer-manifest.json"
HEX = re.compile(r"[0-9a-f]{64}\Z")
SOURCE_SHA = re.compile(r"[0-9a-f]{40}\Z")
IMAGE_DIGEST = re.compile(r"sha256:[0-9a-f]{64}\Z")
FILE_PATH = re.compile(
    r"(?:q6-metadata\.json|q6-oci/(?:index\.json|oci-layout|blobs/sha256/[0-9a-f]{64}))\Z"
)
REQUIRED_FILES = {"q6-metadata.json", "q6-oci/index.json", "q6-oci/oci-layout"}


class InvalidTransfer(ValueError):
    """A static, credential-free validation error."""


def require(condition, message):
    if not condition:
        raise InvalidTransfer(message)


def identity(source_sha, image_digest):
    require(isinstance(source_sha, str) and SOURCE_SHA.fullmatch(source_sha),
            "Invalid source SHA.")
    require(isinstance(image_digest, str) and IMAGE_DIGEST.fullmatch(image_digest),
            "Invalid image digest.")


def absolute(path):
    path = Path(path)
    require(".." not in path.parts, "Parent traversal is not allowed.")
    return path.absolute()


def directory(path):
    for parent in (*reversed(path.parents), path):
        require(stat.S_ISDIR(parent.lstat().st_mode),
                "Directories must exist and must not be symlinks.")


def regular(path):
    directory(path.parent)
    info = path.lstat()
    require(stat.S_ISREG(info.st_mode), "Expected a regular file without symlinks.")
    return info


def snapshot(info):
    return (info.st_dev, info.st_ino, info.st_size, info.st_mtime_ns, info.st_ctime_ns)


@contextmanager
def read_regular(path, expected=None):
    before = regular(path)
    require(expected is None or snapshot(before) == snapshot(expected),
            "Input changed before reading.")
    # A concurrent replacement with a FIFO must fail validation without blocking.
    descriptor = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
    with os.fdopen(descriptor, "rb") as stream:
        require(snapshot(os.fstat(stream.fileno())) == snapshot(before),
                "Input changed before opening.")
        yield stream
        require(snapshot(os.fstat(stream.fileno())) == snapshot(before)
                and snapshot(regular(path)) == snapshot(before), "Input changed while reading.")


def new_output(path, inputs):
    directory(path.parent)
    try:
        path.lstat()
    except FileNotFoundError:
        pass
    else:
        raise InvalidTransfer("Output already exists.")
    require(all(not path.is_relative_to(other) and not other.is_relative_to(path)
                for other in inputs), "Input and output paths overlap.")


def entries(path):
    result = []
    with os.scandir(path) as iterator:
        for entry in iterator:
            require(len(result) < MAX_FILES, "Too many input files.")
            result.append(entry.name)
    return sorted(result)


def source_files(layout, metadata):
    directory(layout)
    require(entries(layout) == ["blobs", "index.json", "oci-layout"],
            "Unexpected or missing OCI layout entries.")
    directory(layout / "blobs")
    require(entries(layout / "blobs") == ["sha256"], "Unexpected OCI blob directory.")
    directory(layout / "blobs" / "sha256")
    names = entries(layout / "blobs" / "sha256")
    require(names and all(HEX.fullmatch(name) for name in names), "Invalid OCI blob names.")
    files = {"q6-metadata.json": metadata, "q6-oci/index.json": layout / "index.json",
             "q6-oci/oci-layout": layout / "oci-layout"}
    files.update({"q6-oci/blobs/sha256/" + name: layout / "blobs" / "sha256" / name
                  for name in names})
    return [(name, path, regular(path)) for name, path in sorted(files.items())]


def encode_manifest(manifest):
    return (json.dumps(manifest, sort_keys=True, separators=(",", ":")) + "\n").encode("ascii")


def exact_keys(value, keys):
    return isinstance(value, dict) and set(value) == set(keys)


def integer(value, minimum, maximum):
    return type(value) is int and minimum <= value <= maximum


def validate_manifest(manifest, source_sha, image_digest):
    identity(source_sha, image_digest)
    require(exact_keys(manifest, ("schema", "source_sha", "image_digest", "part_size",
                                  "total_bytes", "files", "parts")), "Invalid manifest fields.")
    require(type(manifest["schema"]) is int and manifest["schema"] == 1,
            "Unsupported manifest schema.")
    require(manifest["source_sha"] == source_sha and manifest["image_digest"] == image_digest,
            "Manifest identity differs from the approved candidate.")
    part_size, total = manifest["part_size"], manifest["total_bytes"]
    require(integer(part_size, 1, PART_BYTES) and integer(total, 1, MAX_BYTES),
            "Invalid transfer size limits.")
    files, parts = manifest["files"], manifest["parts"]
    require(isinstance(files, list) and 4 <= len(files) <= MAX_FILES,
            "Invalid manifest file list.")
    require(isinstance(parts, list) and 1 <= len(parts) <= MAX_PARTS,
            "Invalid manifest part list.")
    paths, offset = [], 0
    for file in files:
        require(exact_keys(file, ("path", "size", "sha256", "offset")),
                "Invalid file record.")
        require(isinstance(file["path"], str) and FILE_PATH.fullmatch(file["path"]),
                "Invalid file path.")
        require(integer(file["size"], 0, MAX_BYTES) and type(file["offset"]) is int
                and file["offset"] == offset, "Invalid file size or offset.")
        require(isinstance(file["sha256"], str) and HEX.fullmatch(file["sha256"]),
                "Invalid file digest.")
        offset += file["size"]
        require(offset <= total, "Files exceed declared total size.")
        paths.append(file["path"])
    require(paths == sorted(set(paths)) and REQUIRED_FILES.issubset(paths),
            "File paths must be unique, complete and ordered.")
    require(offset == total and len(parts) == (total + part_size - 1) // part_size,
            "File and part coverage differs from declared total size.")
    for index, part in enumerate(parts):
        require(exact_keys(part, ("name", "size", "sha256")), "Invalid part record.")
        require(part["name"] == f"part-{index:06d}.bin", "Part names must be sequential.")
        require(type(part["size"]) is int
                and part["size"] == min(part_size, total - index * part_size),
                "Invalid part size.")
        require(isinstance(part["sha256"], str) and HEX.fullmatch(part["sha256"]),
                "Invalid part digest.")


def pack(layout, metadata, source_sha, image_digest, output, part_bytes=PART_BYTES):
    identity(source_sha, image_digest)
    require(integer(part_bytes, 1, PART_BYTES), "Invalid part size limit.")
    layout, metadata, output = map(absolute, (layout, metadata, output))
    new_output(output, (layout, metadata))
    sources = source_files(layout, metadata)
    files, total = [], 0
    for name, _, info in sources:
        files.append({"path": name, "size": info.st_size, "sha256": "0" * 64, "offset": total})
        total += info.st_size
        require(total <= MAX_BYTES, "Input exceeds total size limit.")
    count = (total + part_bytes - 1) // part_bytes
    require(1 <= count <= MAX_PARTS, "Invalid number of parts.")
    parts = [{"name": f"part-{index:06d}.bin", "size": min(part_bytes, total - index * part_bytes),
              "sha256": "0" * 64} for index in range(count)]
    manifest = {"schema": 1, "source_sha": source_sha, "image_digest": image_digest,
                "part_size": part_bytes, "total_bytes": total, "files": files, "parts": parts}
    validate_manifest(manifest, source_sha, image_digest)
    require(len(encode_manifest(manifest)) <= MAX_MANIFEST, "Manifest exceeds size limit.")
    output.mkdir(mode=0o700)
    stream, part_hash, index, used = None, None, 0, 0
    try:
        for file, (_, path, before) in zip(files, sources):
            file_hash = hashlib.sha256()
            with read_regular(path, before) as source:
                remaining = before.st_size
                while remaining:
                    block = source.read(min(BUFFER_BYTES, remaining, part_bytes - used))
                    require(block, "Source file is truncated.")
                    if stream is None:
                        stream = (output / parts[index]["name"]).open("xb")
                        part_hash = hashlib.sha256()
                    require(stream.write(block) == len(block), "Incomplete part write.")
                    file_hash.update(block)
                    part_hash.update(block)
                    remaining -= len(block)
                    used += len(block)
                    if used == parts[index]["size"]:
                        stream.close()
                        stream = None
                        parts[index]["sha256"] = part_hash.hexdigest()
                        (output / parts[index]["name"]).chmod(0o444)
                        index, used = index + 1, 0
                require(not source.read(1), "Source file grew while reading.")
            file["sha256"] = file_hash.hexdigest()
        require(index == count and stream is None, "Incomplete part coverage.")
        raw = encode_manifest(manifest)
        (output / MANIFEST).write_bytes(raw)
        (output / MANIFEST).chmod(0o444)
    except BaseException:
        try:
            if stream is not None:
                stream.close()
        finally:
            shutil.rmtree(output)
        raise
    return {"manifest_sha256": hashlib.sha256(raw).hexdigest(), "part_count": count,
            "total_bytes": total}


def unique_object(pairs):
    value = {}
    for key, item in pairs:
        require(key not in value, "Duplicate manifest key.")
        value[key] = item
    return value


class FileSink:
    """Write one ordered file at a time, including files spanning many parts."""

    def __init__(self, output, files):
        self.output, self.files, self.index = output, files, 0
        self.stream = None
        self.advance()

    def advance(self):
        while self.index < len(self.files):
            file = self.files[self.index]
            self.path = self.output / file["path"]
            self.stream = self.path.open("xb")
            self.hash = hashlib.sha256()
            self.remaining = file["size"]
            if self.remaining:
                return
            self.finish_file()

    def finish_file(self):
        self.stream.flush()
        require(self.hash.hexdigest() == self.files[self.index]["sha256"],
                "Restored file checksum mismatch.")
        self.stream.close()
        self.stream = None
        self.path.chmod(0o444)
        self.index += 1

    def write(self, block):
        view = memoryview(block)
        while view:
            require(self.stream is not None, "Part data exceeds file coverage.")
            piece = view[:self.remaining]
            require(self.stream.write(piece) == len(piece), "Incomplete restored file write.")
            self.hash.update(piece)
            self.remaining -= len(piece)
            view = view[len(piece):]
            if not self.remaining:
                self.finish_file()
                self.advance()

    def flush(self):
        if self.stream is not None:
            self.stream.flush()

    def close(self):
        if self.stream is not None:
            self.stream.close()
            self.stream = None


def restore(parts_dir, manifest_sha256, source_sha, image_digest, output, consume_parts=False):
    identity(source_sha, image_digest)
    require(isinstance(manifest_sha256, str) and HEX.fullmatch(manifest_sha256),
            "Invalid manifest digest.")
    parts_dir, output = map(absolute, (parts_dir, output))
    directory(parts_dir)
    new_output(output, (parts_dir,))
    manifest_path = parts_dir / MANIFEST
    info = regular(manifest_path)
    require(info.st_size <= MAX_MANIFEST, "Manifest exceeds size limit.")
    with read_regular(manifest_path, info) as stream:
        raw = stream.read(MAX_MANIFEST + 1)
    require(len(raw) <= MAX_MANIFEST and hashlib.sha256(raw).hexdigest() == manifest_sha256,
            "Manifest checksum mismatch.")
    manifest = json.loads(raw, object_pairs_hook=unique_object)
    validate_manifest(manifest, source_sha, image_digest)
    expected_names = sorted([MANIFEST] + [part["name"] for part in manifest["parts"]])
    require(entries(parts_dir) == expected_names, "Missing or unexpected transfer entries.")
    infos = []
    for part in manifest["parts"]:
        info = regular(parts_dir / part["name"])
        require(info.st_size == part["size"], "Part size differs from manifest.")
        infos.append(info)
    output.mkdir(mode=0o700)
    sink = None
    try:
        (output / "q6-oci" / "blobs" / "sha256").mkdir(parents=True, mode=0o700)
        sink = FileSink(output, manifest["files"])
        for part, before in zip(manifest["parts"], infos):
            path, part_hash = parts_dir / part["name"], hashlib.sha256()
            with read_regular(path, before) as stream:
                remaining = part["size"]
                while remaining:
                    block = stream.read(min(BUFFER_BYTES, remaining))
                    require(block, "Transfer part is truncated.")
                    part_hash.update(block)
                    sink.write(block)
                    remaining -= len(block)
                require(not stream.read(1), "Transfer part has trailing bytes.")
            require(part_hash.hexdigest() == part["sha256"], "Transfer part checksum mismatch.")
            # Flush before consuming this part, even when its file spans later parts.
            sink.flush()
            if consume_parts:
                require(snapshot(regular(path)) == snapshot(before), "Part changed before removal.")
                path.unlink()
        require(sink.index == len(manifest["files"]) and sink.stream is None,
                "Incomplete restored files.")
    except BaseException:
        try:
            if sink is not None:
                sink.close()
        finally:
            shutil.rmtree(output)
        raise
    return {"source_sha": source_sha, "image_digest": image_digest,
            "restored_bytes": manifest["total_bytes"], "part_count": len(manifest["parts"])}


class Parser(argparse.ArgumentParser):
    def error(self, message):
        raise InvalidTransfer("Invalid transfer arguments.")


def main():
    parser = Parser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True, parser_class=Parser)
    for command in ("pack", "restore"):
        sub = commands.add_parser(command)
        sub.add_argument("--source-sha", required=True)
        sub.add_argument("--image-digest", required=True)
        sub.add_argument("--output", required=True, type=Path)
        if command == "pack":
            sub.add_argument("--layout", required=True, type=Path)
            sub.add_argument("--metadata", required=True, type=Path)
            sub.add_argument("--part-bytes", type=int, default=PART_BYTES)
        else:
            sub.add_argument("--parts-dir", required=True, type=Path)
            sub.add_argument("--manifest-sha256", required=True)
            sub.add_argument("--consume-parts", action="store_true", required=True)
    args = vars(parser.parse_args())
    command = args.pop("command")
    print(json.dumps((pack if command == "pack" else restore)(**args), sort_keys=True))


if __name__ == "__main__":
    try:
        main()
    except InvalidTransfer as error:
        print("Transfer failed: " + str(error), file=sys.stderr)
        sys.exit(1)
    except (OSError, ValueError, TypeError, RecursionError):
        print("Transfer failed: invalid input or filesystem operation.", file=sys.stderr)
        sys.exit(1)
