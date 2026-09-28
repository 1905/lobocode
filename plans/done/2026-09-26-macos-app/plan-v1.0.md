# Lobo macOS app — Implementation Plan v1.0

**Date:** 2026-09-26
**Status:** done
**Spec:** ./spec.md

**Goal:** a menu bar `Lobo.app` that drives the bundled `lobo` CLI, demoscene/terminal look.
**Architecture:** SwiftUI `MenuBarExtra(.window)` + `Settings` scene; a `@MainActor ObservableObject Store` owns state and talks to the CLI through `CLI` (Foundation `Process`, JSON lines). Go gets three small JSON hooks.
**Tech:** Swift 6 toolchain in Swift 5 mode, SwiftPM, macOS 13+, SwiftUI, UserNotifications. Go side: cobra.

Branch `feat/macos` (stacked on `feat/cli`). Orchestrator implements directly; one Astra review at the end.

## Locked interfaces

```
lobo config show --json  → {"path": "...", "exists": bool, "values": {K: masked-or-plain}, "set": {K: bool}}
lobo config set K=V ...  → saves via config.Save; K must match ^[A-Z][A-Z0-9_]*$; "K=" removes K; prints nothing, exit 0
lobo down --json         → {"spent_usd": 1.23}
```

```swift
enum Phase { case noConfig, off, booting, ready, stopping, failed(String) }
struct Snapshot: Decodable  // pod?, version?, status?, down, at  (mirrors control.Snap)
struct UpEvent: Decodable   // phase, detail?, download?, ready?, done?, err?
final class CLI { func run(_ args: [String]) async throws -> Data; func stream(_ args: [String], onLine: @escaping (Data) -> Void) -> Process }
@MainActor final class Store: ObservableObject { @Published phase, snap, steps, … ; start(provider:model:), stop(), refresh() }
```

## Tasks

- [ ] **M1 Go hooks** — `cmd/lobo/config.go` (`show --json`, `set`), `main.go` (`down --json`), tests in `cmd/lobo/defaults_test.go`.
- [ ] **M2 Package + models + CLI runner** — `macos/Package.swift`, `Models.swift`, `CLI.swift`; fixtures `Tests/LoboTests/Fixtures/*.json` from the live CLI; decode tests.
- [ ] **M3 Store** — phase derivation from snapshot + up events, poll cadence, auto-stop detection, notifications; transition tests.
- [ ] **M4 Theme + views** — `Theme.swift` (colours, mono font, block bar, glitch text, raster bar), `PanelView`, `BootLog`, `ReadyCard`, `FailCard`, `StartCard`, `SettingsView`; `--render` mode writes PNGs for every state.
- [ ] **M5 Bundle** — `macos/Info.plist`, Makefile `mac` / `install-mac` (release build, `.app` layout, copy `bin/lobo`, `codesign -s -`), launch check.
- [ ] **M6 Docs + exit** — README section; `swift test`, `go test ./...`, lint; one `/rival-astra` review → fixes → merge after `feat/cli` → push → notify.

## Self-review
- Every spec row maps to M1–M6. No Swift provider/config writer: settings go through `config set`.
- Risk: MenuBarExtra window style sizing → fixed 340 pt width, content-driven height.
- Risk: ImageRenderer can't render some AppKit-backed controls → render views use plain SwiftUI shapes/text only.
