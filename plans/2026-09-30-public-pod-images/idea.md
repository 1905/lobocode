# Complete public GPU images

**Date:** 2026-09-30
**Status:** done

Lobocode must work as a public application without the developer's storage or private hosts.
The GPU image must contain the agent, inference runtime, SSH daemon and selected model weights.
The app must resolve the latest public image for the selected model before every new cloud start.
A failed registry lookup must not select an older cached image or rent a GPU.
The DMG installs only the standalone Mac app. The optional CLI installs separately through Homebrew.
Preserve domain-free SSH, native macOS behavior and windows without scrolling.
Run backend and container checks on Dell. Keep publication and GPU rental on hold.
