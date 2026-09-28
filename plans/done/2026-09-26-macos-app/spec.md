# Lobo for macOS: menu bar app over the lobo CLI

**Date:** 2026-09-26
**Scope:** ~/dev/lobotomized-ai (`macos/`, small `cmd/lobo` additions)
**Status:** done

## TL;DR

**P1 — CLI hooks for the app**
- What: `lobo config show --json` (keys masked, plus which secrets are set) and `lobo config set KEY=value …` (writes through the same `config.Save`). `lobo down --json` prints `{"spent_usd": …}`.
- Why: the app must never have its own dotenv writer or provider code. One implementation, in Go.
- You do: nothing.
- Does NOT: change `up`/`status` behaviour.

**P2 — menu bar app `Lobo.app`**
- What: SwiftUI menu bar app (no Dock icon), dark demoscene/terminal look (mono type, phosphor green, one copper gradient, small raster/glitch motion; not cringe). Icon shows the state: off, booting with %, ready, failed. Click → a small panel: Start (provider + model pick) / live boot timeline with MB/s / ready card with endpoint + copy key, tok/s, VRAM, idle countdown, $ spent / Stop. Settings window edits the same `~/.config/lobo/config.env`. Notifications on ready, failure, and auto-stop.
- Why: one click instead of a terminal.
- You do: nothing. `make mac` builds, `make install-mac` puts it in `~/Applications`.
- Does NOT: reimplement renting, re-rent rules or checks in Swift. It runs the bundled `lobo` binary (`up --json`, `status --json`, `down`, `config …`).

**P3 — one Astra review, fixes, merge**
- Branch `feat/macos` stacks on `feat/cli` (not merged yet: it waits for the live Vast e2e). Merged after `feat/cli`.

## Problem(s)

1. Using lobo needs a terminal and remembering commands (`cmd/lobo/main.go`); the state of a paid pod is invisible unless `lobo status` is open.
2. A pod that auto-stops (idle 30 min) or fails at boot tells nobody; the user finds out on the next request.

## Goals

1. Start/stop lobo and see its live state from the menu bar. (1)
2. Same config file and same behaviour as the CLI: the app runs the CLI. (1)
3. Notify on ready, failure, and auto-stop. (2)

## Non-goals

- Swift ports of providers, config writing, or boot logic.
- Windows/Linux, App Store, notarization, auto-update, login item by default.

## Architecture

```
Lobo.app/Contents/
  MacOS/Lobo              SwiftUI MenuBarExtra (.window style), LSUIElement
  Resources/lobo          the CLI, built from the same commit (fallback: PATH)
        │ Process + JSON lines
        ▼
  lobo up --json          event stream  → boot timeline
  lobo status --json      snapshot poll → state, metrics (5 s open/booting, 30 s idle)
  lobo down --json        stop
  lobo config show --json / set K=V   → settings window
```

State machine in the app: `noConfig → off → booting(stage, %, MB/s) → ready → (stopping) → off`; `failed(msg)` from an up error or a vanished pod. The poll is the source of truth; the `up` stream only adds detail while it runs.

## UX — "crack intro, but grown up"

User ask: hacker style, a bit like demoscene crack intros, no music, cool not cringe. Rules that keep it on the right side:
- One dark surface (#0B0D10), one monospace face (SF Mono → Menlo fallback), two accents only: phosphor green #39FF88 (ok/ready) and a cyan→magenta copper gradient (#00E5FF → #FF2BD6) used in exactly two places: the logo bar and progress fills. Amber #FFB020 warnings, red #FF4D5E errors.
- Motion is small and purposeful: a 2 px raster bar sweeping under the logo while booting (static when idle), a blinking block cursor on the active step, a one-shot glitch/scramble on the state word when it changes (≈250 ms). No looping marquee, no CRT curvature, no scanline overlay over text (a faint 3 % scanline texture on the header only).
- Everything respects Reduce Motion (animations off).

Menu bar: monospace text item, no emoji. `▢ lobo` off · `▣ 42%` booting · `● lobo` green ready · `✕ lobo` red failed.

Panel (340 pt, dark, rounded 12):
- Header: block-letter `LOBO` logo (5-line ASCII, gradient fill) + a thin copper bar + one status line `sys: READY  runpod · SECURE ≥5000 Mbps · $0.69/h`.
- Off: `> provider [runpod] vast` and `> model [q8] q6` as bracketed toggles (click or ←/→), then a wide `[ START ]` button, `⏎` starts.
- Booting: terminal log style
  ```
  [ OK ] rent       runpod p1a2b3 · 0:08
  [ OK ] container  0:41
  [ OK ] tunnel     0:43
  [ >> ] download   ▓▓▓▓▓▓▓▓░░░░░░  12.4/28.6 GB  713 MB/s  0:24
  [ .. ] load
  [ .. ] ready
  ```
  plus elapsed `T+01:12` and `[ ABORT ]` (runs `down`).
- Ready: `endpoint  https://lobo…/v1   [copy]`, `api key  sk-9…7e4d   [copy]`, two big mono numbers `45.1 tok/s gen` / `504 tok/s prompt`, VRAM as block bar `▓▓▓▓▓▓▓▓▓░ 29.3/32.6 GB`, `idle-kill in 27:14 · spent $1.23`, `[ STOP ]`.
- Failed: red `[FAIL]` line with the error, last log lines in dim mono, `[ RETRY ]` `[ DISMISS ]`.
- Footer (dim): `settings ⌘,  ·  config file  ·  quit ⌘q`.

Settings window: same dark mono theme; sections `// providers`, `// access`, `// defaults`; secrets as masked `rpa_…a1b2` with keep/clear; file path + "reveal" + "plain KEY=value, edit by hand any time". Save = `lobo config set` with changed keys only.

Notifications (UserNotifications): "lobo ready — https://…/v1", "boot failed: …", "lobo stopped (idle)" when a ready pod disappears without the user pressing Stop.

## File-level changes

| File | Change |
|---|---|
| `cmd/lobo/config.go` | `config show --json` (masked values + `set` booleans + path); `config set KEY=value…` (validates key names, `config.Save`). |
| `cmd/lobo/main.go` | `down --json`. |
| `macos/Package.swift` | SwiftPM, macOS 13+, executable `Lobo`, test target. Swift 5 language mode. |
| `macos/Sources/Lobo/*.swift` | `LoboApp` (MenuBarExtra + Settings), `CLI` (Process runner, JSON lines), `Models` (Snapshot/Event/ConfigShow Codable), `Store` (state machine, polling, notifications), views `PanelView`, `BootTimeline`, `ReadyCard`, `SettingsView`, `Theme`. |
| `macos/Tests/LoboTests/*.swift` | decode real `status --json` / `up --json` fixtures, state machine transitions, time/size formatting. |
| `macos/Info.plist` | `LSUIElement`, bundle id `cc.example.lobo`, version from git. |
| `Makefile` | `mac` (swift build release + assemble `.app` + copy CLI + ad-hoc codesign), `install-mac` (→ `~/Applications/Lobo.app`). |
| `README.md` | macOS app section. |

## Tests

- Go: `config show --json` masks secrets; `config set` round-trips and rejects unknown/lowercase keys; `down --json` shape.
- Swift unit: fixtures captured from the live CLI (status ready/off, up event stream incl. failure); store transitions (off→booting→ready, ready→gone = auto-stop notification, up error = failed).
- Visual: `Lobo --render <dir>` renders each panel state to PNG with `ImageRenderer` (fake store), checked by eye.
- Manual: app launches, icon appears, panel opens, settings saves to a scratch config (`LOBO_APP_CONFIG` → `--config`).

## Failure modes & decisions

| Failure | Behaviour |
|---|---|
| No config file | Panel shows "Set up lobo" with Settings button; Start hidden. |
| Bundled CLI missing | Fall back to `lobo` on PATH (`~/.local/bin`, `/opt/homebrew/bin`); else error card. |
| `status --json` fails (network, provider API) | Keep last state, show a small warning line; retry on the next tick. |
| App quits during boot | `up` keeps running as a detached child; the next launch picks the pod up from `status`. |
| Start while a pod exists | Button disabled; the CLI refuses anyway. |

## Out of scope

- Login item, Sparkle updates, signing with a Developer ID, brew cask.

## Rollout

- P1 CLI hooks (commit). P2 app (commits). P3 one `/rival-astra` review → fixes → merge after `feat/cli`.
