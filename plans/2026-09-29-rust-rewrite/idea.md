# Rust rewrite — idea

**Date:** 2026-09-29
**Status:** done

User ask (2026-09-29): "The dependency on the CLI is horrible. Let's plan to rewrite everything in Rust and share code as much as possible. CLI usage will be on Rust and app should be written in Tauri based on Rust … so we can have shared code."

Why now: the mac app shells out to the bundled `lobo` for every action (`macos/Sources/Lobocode/CLI.swift:13` "this is the only door"). Two languages (Go + Swift) share nothing but JSON.

Decisions from the brainstorm (all by the user, 2026-09-29):
- Scope: everything in Rust, including the pod agent (option A, not "keep Go agent").
- App: Tauri, same shape as today (tray/menu bar item + small popover panel).
- Migration: big-bang replace on a branch (Go deleted there; master keeps Go until the merge).
- v1 = full parity before merge: RunPod + Vast + local, pod agent, baked image, config wizard, live `status` dashboard, `test`, `logs`, `release`, brew, tray app.
