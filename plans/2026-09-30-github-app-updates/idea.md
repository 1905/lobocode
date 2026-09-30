# GitHub-only desktop updates

**Date:** 2026-09-30
**Status:** done

- The user wants Lobocode to update itself using GitHub, without their own host.
- The user requested a detailed design through the `spec` skill.
- GitHub Releases should serve the standalone app and its update metadata.
- GitHub Actions should build and sign update archives.
- Keep Homebrew CLI updates and latest-public-GPU-image resolution separate.
- Preserve native macOS windows, no scrolling, and the existing release hold.
- Proposed behavior: automatic checks/downloads; install and restart on user action while idle.
- Success: an installed app gets a verified newer app from GitHub without damaging user data or interrupting a runtime.
