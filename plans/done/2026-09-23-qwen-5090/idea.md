# Idea — Qwen3.5-27B uncensored on a rented 5090

**Date:** 2026-09-23
**Status:** done

- User ask: run `Qwen3.5-27B-Uncensored-HauhauCS-Aggressive-Q8_0.gguf` on a RunPod RTX 5090, llama-server OpenAI API, hook OpenCode to it.
- Full source write-up: `docs/idea.md`.
- Provider: RunPod. Key: `RUNPOD_API_KEY` from `~/dev/ai-image-studio/.env`.
- Public endpoint on a `*.example.com` subdomain via Cloudflare (instead of the SSH tunnel in docs/idea.md §11).
- Code in private repo `1905/lobotomized-ai`.

Verified 2026-09-23:
- RTX 5090 32 GB on RunPod: $0.69/h community, $0.99/h secure, stock "Low".
- HF repo files: Q8_0 28.6 GB, Q6_K 22.08 GB, Q5_K_M 19.4 GB, mmproj 0.93 GB.
