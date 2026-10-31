**Date:** 2026-10-31
**Status:** done (spec approved 2026-10-31)

# Faster qwen38-q6 output with speculative decoding

- Ask: apply the HyperQwen speed-up (syv-ai/HyperQwen, ex `qwen38-27b-rtx3090`) to lobocode and keep quality.
- Assumption: the qwen38-q6-only PR (`review/pr-2`) is merged. One model, one GPU class (RTX 5090), llama.cpp b11118.
- HyperQwen = vLLM + 4-bit W4A16 + MTP/DFlash2 speculation + prompt lookup. The 4-bit part costs quality; the speculation part does not.
- llama.cpp b11118 already ships `--spec-type draft-mtp,draft-dflash,ngram-mod`. Our Q6_K GGUF already holds the MTP head.
- First-guess scope: measure first, then turn on MTP + n-gram prompt lookup by flags. DFlash2 drafter only if the numbers justify a second weight file.
