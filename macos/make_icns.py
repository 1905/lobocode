"""Build AppIcon.icns from one 1024 px PNG: sips resizes into an .iconset, iconutil packs it."""
import pathlib
import subprocess
import sys
import tempfile

src, out = sys.argv[1], sys.argv[2]
with tempfile.TemporaryDirectory() as d:
    iconset = pathlib.Path(d) / "AppIcon.iconset"
    iconset.mkdir()
    for size in (16, 32, 128, 256, 512):
        for scale in (1, 2):
            px = size * scale
            name = f"icon_{size}x{size}{'@2x' if scale == 2 else ''}.png"
            subprocess.run(["sips", "-z", str(px), str(px), src, "--out", str(iconset / name)], check=True, capture_output=True)
    subprocess.run(["iconutil", "-c", "icns", str(iconset), "-o", out], check=True)
