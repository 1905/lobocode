#!/usr/bin/env python3
"""Build a self-contained Swift/Tauri render comparison from saved PNGs."""
import argparse
import base64
import html
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def picture(path, label, width):
    if not path.is_file():
        return '<p class="muted">No Swift baseline for this state.</p>'
    data = base64.b64encode(path.read_bytes()).decode()
    return (
        f'<figure><figcaption>{html.escape(label)}</figcaption>'
        f'<img alt="{html.escape(label)}" width="{width}" '
        f'src="data:image/png;base64,{data}"></figure>'
    )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=ROOT / "bin/app-renders/review/index.html")
    args = parser.parse_args()
    baseline = ROOT / "plans/2026-09-29-rust-rewrite/p5-baseline"
    renders = ROOT / "bin/app-renders/ui"
    names = sorted(json.loads(p.read_text())["name"] for p in (ROOT / "app/ui/src/fixtures").glob("panel_*.json"))
    sections = []
    for name in names + ["settings"]:
        file = "settings.png" if name == "settings" else f"panel_{name}.png"
        width = 520 if name == "settings" else 340
        if not (renders / file).is_file():
            raise SystemExit(f"Missing current render: {file}")
        sections.append(
            f'<section id="{name}"><h2>{name}</h2><div class="pair">'
            + picture(baseline / file, "Swift baseline", width)
            + picture(renders / file, "Tauri UI / Chromium", width)
            + "</div></section>"
        )
    for name in names:
        file = f"menubar_{name}.png"
        if not (renders / file).is_file():
            raise SystemExit(f"Missing current render: {file}")
        sections.append(
            f'<section><h2>Menu bar: {name}</h2><div class="pair">'
            + picture(baseline / file, "Swift baseline", 160)
            + picture(renders / file, "Tauri preview", 160)
            + "</div></section>"
        )
    sections.append(
        '<section><h2>App icon</h2><div class="pair">'
        + picture(baseline / "icon_1024.png", "Swift", 256)
        + picture(ROOT / "bin/app-renders/icon_1024.png", "Tauri", 256)
        + "</div></section>"
    )
    page = """<!doctype html><html lang="en"><meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>Lobocode P5 render review</title>
<style>
:root{color-scheme:dark;font:14px/1.5 system-ui;background:#0b0d10;color:#d6dee8}
body{max-width:1140px;margin:auto;padding:24px}h1{font-size:24px}h2{font:16px monospace}
section{border-top:1px solid #3a424d;padding:20px 0}.pair{display:flex;gap:24px;align-items:flex-start;flex-wrap:wrap}
figure{margin:0;max-width:100%}figcaption,.muted{color:#a5afbd;margin-bottom:10px}
img{display:block;max-width:100%;height:auto}a{color:#00e5ff}
</style><h1>Lobocode P5 render review</h1>
<p>Candidate implementation. Native app E2E and final acceptance remain pending.</p>
<ul><li>All 20 panels, settings and 20 menu-bar previews captured at 2× and inspected.</li>
<li>Chromium captures: the installed managed Playwright CLI could not install WebKit.</li>
<li>Minor browser font metrics and progress-bar texture differ from Swift.</li>
<li>The app icon uses a rounded rectangle; it does not reproduce Apple's continuous squircle.</li>
<li>The native tray title uses the system font. Preview images do not prove native menu-bar placement.</li>
<li>Values are fixed fixtures. No credentials or live provider data are included.</li></ul>
""" + "\n".join(sections) + "</html>\n"
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(page)
    print(f"Wrote {args.output} ({len(names)} panels, settings, {len(names)} menu strips, icon)")


if __name__ == "__main__":
    main()
