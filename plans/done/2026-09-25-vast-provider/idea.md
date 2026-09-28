# Idea — Vast.ai as a second GPU provider

**Date:** 2026-09-25
**Status:** done

- User ask: "add vastai support, do spec pipe, review in the very end with astra 1 time." Test on the real account.
- Why now: RunPod gave container-never-starts hosts, slow hosts, no 5090 in SECURE, and its API can only filter network speed, not sort.
- Key: `VASTAI_API_KEY` from `~/dev/ai-image-studio/.env` (read-only probe 2026-09-25: auth 200, credit $16.18).
- Probe: 20+ 1× RTX 5090 offers, sorted by `inet_down` desc: 18,877 / 17,805 / 16,618 Mbps, $0.726–0.92/h, verified, US/EU.
- User decisions: RunPod stays default; Vast via `--provider vast`. Host pick = fastest network among verified, reliable hosts.
- Prior art: `~/dev/ai-image-studio/src-tauri/src/vast.rs` (create = `PUT /api/v0/asks/{id}`, runtype must be `ssh`, `onstart` runs as `/root/onstart.sh`, destroy = `DELETE /api/v0/instances/{id}`).
