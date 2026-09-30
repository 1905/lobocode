#!/usr/bin/env python3
"""Verify a completed, read-only Q6 OCI export before its optional CI upload."""

import argparse
import hashlib
import json
import re
import stat
import sys
import tempfile
from pathlib import Path


DIGEST = re.compile(r"sha256:[0-9a-f]{64}\Z")
SHA = re.compile(r"[0-9a-f]{40}\Z")
INDEX_TYPES = {
    "application/vnd.oci.image.index.v1+json",
    "application/vnd.docker.distribution.manifest.list.v2+json",
}
MANIFEST_TYPES = {
    "application/vnd.oci.image.manifest.v1+json",
    "application/vnd.docker.distribution.manifest.v2+json",
}
CONFIG_TYPES = {
    "application/vnd.oci.image.config.v1+json",
    "application/vnd.docker.container.image.v1+json",
}
SOURCE = "https://github.com/1905/lobocode"


class InvalidCandidate(ValueError):
    pass


def require(condition, message):
    if not condition:
        raise InvalidCandidate(message)


def regular_file(path):
    info = path.lstat()
    require(stat.S_ISREG(info.st_mode), f"Not a regular file: {path.name}")
    return info


def json_file(path):
    require(regular_file(path).st_size <= 4 * 1024 * 1024, "JSON exceeds size limit")
    value = json.loads(path.read_bytes())
    require(isinstance(value, dict), "Expected a JSON object")
    return value


def verify(layout, metadata_path, source_sha, image_digest):
    require(SHA.fullmatch(source_sha), "source_sha must be a full lowercase Git SHA")
    require(DIGEST.fullmatch(image_digest), "image_digest must be a SHA256 digest")
    for directory in (layout, layout / "blobs", layout / "blobs" / "sha256"):
        require(stat.S_ISDIR(directory.lstat().st_mode), "OCI directories must not be symlinks")
    require(json_file(layout / "oci-layout").get("imageLayoutVersion") == "1.0.0", "Unsupported OCI layout")
    metadata = json_file(metadata_path)
    require(metadata.get("containerimage.digest") == image_digest, "BuildKit image digest differs from requested digest")
    index = json_file(layout / "index.json")
    require(index.get("schemaVersion") == 2, "Invalid OCI index schema")
    roots = index.get("manifests")
    require(isinstance(roots, list) and len(roots) == 1, "Expected one exported OCI root")
    require(isinstance(roots[0], dict), "Invalid exported OCI root")
    require(roots[0].get("digest") == image_digest, "OCI root differs from requested digest")

    # Hash all exported blobs, including any attestations or unreferenced blobs.
    sizes = {}
    total_bytes = 0
    for path in sorted((layout / "blobs" / "sha256").iterdir()):
        require(re.fullmatch(r"[0-9a-f]{64}", path.name), "Invalid OCI blob name")
        before = regular_file(path)
        digest = hashlib.sha256()
        with path.open("rb") as stream:
            for block in iter(lambda: stream.read(1024 * 1024), b""):
                digest.update(block)
        after = regular_file(path)
        require((before.st_ino, before.st_size, before.st_mtime_ns) ==
                (after.st_ino, after.st_size, after.st_mtime_ns), "OCI blob changed while hashing")
        require(digest.hexdigest() == path.name, f"OCI blob checksum mismatch: {path.name}")
        sizes["sha256:" + path.name] = before.st_size
        total_bytes += before.st_size
    require(sizes, "OCI layout has no blobs")

    visited = set()
    images = []

    def descriptor(value):
        require(isinstance(value, dict), "Invalid OCI descriptor")
        digest = value.get("digest", "")
        size = value.get("size")
        require(isinstance(digest, str) and DIGEST.fullmatch(digest), "Unsupported descriptor digest")
        require(type(size) is int and size >= 0, "Invalid descriptor size")
        require(digest in sizes and sizes[digest] == size, "Descriptor blob is missing or has the wrong size")
        require(not value.get("urls"), "External OCI blobs are not allowed")
        visited.add(digest)
        return layout / "blobs" / "sha256" / digest.removeprefix("sha256:")

    def walk(value, depth=0, platforms=()):
        require(depth <= 8, "OCI graph is too deep")
        path = descriptor(value)
        if "platform" in value:
            require(isinstance(value["platform"], dict), "Invalid OCI descriptor platform")
            platforms = (*platforms, value["platform"])
        media_type = value.get("mediaType")
        require(media_type in INDEX_TYPES | MANIFEST_TYPES, "Unsupported OCI graph media type")
        document = json_file(path)
        require(document.get("schemaVersion") == 2, "Invalid OCI graph schema")
        require(document.get("mediaType", media_type) == media_type, "OCI media type mismatch")
        if media_type in INDEX_TYPES:
            children = document.get("manifests")
            require(isinstance(children, list) and 0 < len(children) <= 16, "Invalid OCI image index")
            for child in children:
                walk(child, depth + 1, platforms)
            return
        config_descriptor = document.get("config")
        config_path = descriptor(config_descriptor)
        require(config_descriptor.get("mediaType") in CONFIG_TYPES, "Unsupported image config type")
        config = json_file(config_path)
        layers = document.get("layers")
        require(isinstance(layers, list) and len(layers) <= 1024, "Invalid image layers")
        for layer in layers:
            descriptor(layer)
        annotations = value.get("annotations", {})
        require(isinstance(annotations, dict), "Invalid OCI annotations")
        if annotations.get("vnd.docker.reference.type") == "attestation-manifest":
            require(config.get("os") == "unknown" and config.get("architecture") == "unknown", "Invalid attestation config")
            return
        images.append((config_descriptor, config, layers, platforms))

    walk(roots[0])
    require(len(images) == 1, "Expected exactly one runnable image")
    config_descriptor, config, layers, platforms = images[0]
    require(config.get("os") == "linux" and config.get("architecture") == "amd64", "Image must be Linux AMD64")
    for platform in platforms:
        require(platform.get("os") == "linux" and platform.get("architecture") == "amd64",
                "Runnable OCI descriptor platform differs from Linux AMD64 image config")
    require(layers, "Complete image has no layers")
    image_config = config.get("config", {})
    require(isinstance(image_config, dict), "Invalid runtime config")
    require(image_config.get("Entrypoint") == ["/lobo/start"], "Image entrypoint is not /lobo/start")
    environment = image_config.get("Env", [])
    require(isinstance(environment, list) and all(isinstance(item, str) for item in environment), "Invalid image environment")
    require([item for item in environment if item.startswith("LOBO_IMAGE_MODEL=")] == ["LOBO_IMAGE_MODEL=1"], "Complete-model flag is missing or ambiguous")
    require("2222/tcp" in image_config.get("ExposedPorts", {}), "Image does not expose its SSH port")
    labels = image_config.get("Labels", {})
    require(isinstance(labels, dict), "Invalid image labels")
    require(labels.get("org.opencontainers.image.revision") == source_sha, "Baked source revision differs from requested SHA")
    require(labels.get("org.opencontainers.image.source") == SOURCE, "Baked source repository differs")
    require(labels.get("io.lobocode.model") == "q6", "Baked model identity is not Q6")
    if "containerimage.config.digest" in metadata:
        require(metadata["containerimage.config.digest"] == config_descriptor["digest"], "BuildKit config digest differs from image config")
    return {"source_sha": source_sha, "image_digest": image_digest, "model": "q6",
            "platform": "linux/amd64", "blob_count": len(sizes), "graph_blob_count": len(visited),
            "verified_bytes": total_bytes, "immutable_tag": "sha-" + source_sha[:12] + "-q6"}


def self_test():
    checks = 0
    with tempfile.TemporaryDirectory(prefix="lobo-candidate-fixture-") as folder:
        root = Path(folder)
        layout = root / "oci"
        blobs = layout / "blobs" / "sha256"
        blobs.mkdir(parents=True)
        (layout / "oci-layout").write_text('{"imageLayoutVersion":"1.0.0"}')
        source_sha = "a" * 40
        config = {"os": "linux", "architecture": "amd64", "config": {
            "Entrypoint": ["/lobo/start"], "Env": ["LOBO_IMAGE_MODEL=1"],
            "ExposedPorts": {"2222/tcp": {}}, "Labels": {
                "org.opencontainers.image.revision": source_sha,
                "org.opencontainers.image.source": SOURCE, "io.lobocode.model": "q6"}}}

        def blob(data, media_type):
            digest = hashlib.sha256(data).hexdigest()
            (blobs / digest).write_bytes(data)
            return {"mediaType": media_type, "digest": "sha256:" + digest, "size": len(data)}

        layer = blob(b"synthetic fixture layer", "application/vnd.oci.image.layer.v1.tar")

        def prepare(config_value, nested=False, attestation=False, platform=None, index_platform=None):
            config_desc = blob(json.dumps(config_value).encode(), "application/vnd.oci.image.config.v1+json")
            manifest = {"schemaVersion": 2, "mediaType": "application/vnd.oci.image.manifest.v1+json",
                        "config": config_desc, "layers": [layer]}
            descriptor = blob(json.dumps(manifest).encode(), manifest["mediaType"])
            if platform is not None:
                descriptor["platform"] = platform
            if nested:
                children = [descriptor]
                if attestation:
                    att_config = blob(b'{"os":"unknown","architecture":"unknown"}', "application/vnd.oci.image.config.v1+json")
                    att_manifest = {"schemaVersion": 2, "mediaType": manifest["mediaType"], "config": att_config, "layers": []}
                    att_desc = blob(json.dumps(att_manifest).encode(), manifest["mediaType"])
                    att_desc["annotations"] = {"vnd.docker.reference.type": "attestation-manifest"}
                    att_desc["platform"] = {"os": "unknown", "architecture": "unknown"}
                    children.append(att_desc)
                index = {"schemaVersion": 2, "mediaType": "application/vnd.oci.image.index.v1+json", "manifests": children}
                descriptor = blob(json.dumps(index).encode(), index["mediaType"])
                if index_platform is not None:
                    descriptor["platform"] = index_platform
            (layout / "index.json").write_text(json.dumps({"schemaVersion": 2, "manifests": [descriptor]}))
            metadata = root / "metadata.json"
            metadata.write_text(json.dumps({"containerimage.digest": descriptor["digest"], "containerimage.config.digest": config_desc["digest"]}))
            return metadata, descriptor["digest"]

        def rejected(call):
            nonlocal checks
            try:
                call()
            except (InvalidCandidate, FileNotFoundError):
                checks += 1
            else:
                raise AssertionError("invalid fixture was accepted")

        for nested in (False, True):
            metadata, digest = prepare(config, nested)
            assert verify(layout, metadata, source_sha, digest)["model"] == "q6"
            checks += 1
        metadata, digest = prepare(config, nested=True, attestation=True)
        assert verify(layout, metadata, source_sha, digest)["model"] == "q6"
        checks += 1
        for nested in (False, True):
            metadata, digest = prepare(config, nested=nested, platform={"os": "linux", "architecture": "amd64"})
            assert verify(layout, metadata, source_sha, digest)["platform"] == "linux/amd64"
            checks += 1
        for platform in ({"os": "linux", "architecture": "arm64"}, {"os": "darwin", "architecture": "amd64"}):
            metadata, digest = prepare(config, nested=True, platform=platform)
            rejected(lambda: verify(layout, metadata, source_sha, digest))
        metadata, digest = prepare(config, nested=True, index_platform={"os": "linux", "architecture": "arm64"})
        rejected(lambda: verify(layout, metadata, source_sha, digest))
        metadata, digest = prepare(config)
        rejected(lambda: verify(layout, metadata, "a" * 12, digest))
        rejected(lambda: verify(layout, metadata, source_sha, "sha256:" + "b" * 64))
        for key, value in (("architecture", "arm64"), ("os", "darwin")):
            bad = json.loads(json.dumps(config))
            bad[key] = value
            metadata, digest = prepare(bad)
            rejected(lambda: verify(layout, metadata, source_sha, digest))
        for key, value in (("Entrypoint", ["/bin/sh"]), ("Env", ["LOBO_IMAGE_MODEL=0"]), ("ExposedPorts", {})):
            bad = json.loads(json.dumps(config))
            bad["config"][key] = value
            metadata, digest = prepare(bad)
            rejected(lambda: verify(layout, metadata, source_sha, digest))
        for label in ("org.opencontainers.image.revision", "org.opencontainers.image.source", "io.lobocode.model"):
            bad = json.loads(json.dumps(config))
            bad["config"]["Labels"][label] = "wrong"
            metadata, digest = prepare(bad)
            rejected(lambda: verify(layout, metadata, source_sha, digest))
        metadata, digest = prepare(config)
        index = json_file(layout / "index.json")
        index["manifests"][0]["size"] += 1
        (layout / "index.json").write_text(json.dumps(index))
        rejected(lambda: verify(layout, metadata, source_sha, digest))
        metadata, digest = prepare(config)
        index = json_file(layout / "index.json")
        index["manifests"][0]["urls"] = ["https://example.invalid/blob"]
        (layout / "index.json").write_text(json.dumps(index))
        rejected(lambda: verify(layout, metadata, source_sha, digest))
        metadata, digest = prepare(config)
        layer_path = blobs / layer["digest"].removeprefix("sha256:")
        layer_path.write_bytes(b"corrupt")
        rejected(lambda: verify(layout, metadata, source_sha, digest))
        layer_path.unlink()
        layer_path.symlink_to(metadata)
        rejected(lambda: verify(layout, metadata, source_sha, digest))
        layer_path.unlink()
        rejected(lambda: verify(layout, metadata, source_sha, digest))
    print(json.dumps({"self_test": "passed", "checks": checks}))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--layout", type=Path, default=Path("/candidate/q6-oci"))
    parser.add_argument("--metadata", type=Path, default=Path("/candidate/q6-metadata.json"))
    parser.add_argument("--source-sha")
    parser.add_argument("--image-digest")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        self_test()
    else:
        require(args.source_sha and args.image_digest, "source SHA and image digest are required")
        print(json.dumps(verify(args.layout, args.metadata, args.source_sha, args.image_digest), sort_keys=True))


if __name__ == "__main__":
    try:
        main()
    except (InvalidCandidate, OSError, ValueError, TypeError, KeyError) as error:
        print(f"Candidate verification failed: {error}", file=sys.stderr)
        sys.exit(1)
