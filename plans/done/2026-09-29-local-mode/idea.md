# Local mode — idea

**Date:** 2026-09-29
**Status:** done

User ask (2026-09-29): "redesign the app UX to allow to run it locally with possible to set where to save the weights. and then run it locally. wire support fully too."

Why now: RunPod community 5090 hosts were all broken today (5/5), and the weights are already downloading to `/Volumes/Extreme/_lobocode/` on this Mac (M1 Max, 64 GB). Q6 needs ~22.6 GiB, Q8 ~28.6 GiB: both fit, one at a time.

First-guess scope: `lobo up --local` runs llama-server (pinned llama.cpp b11118, macOS arm64 Metal build) on this Mac against a GGUF in a user-chosen weights folder. `status/down/test` work the same. The mac app gets a Local vs Cloud choice and a weights-folder setting. Then run it for real on this Mac.
