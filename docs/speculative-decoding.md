# Speculative decoding for qwen38-q6

Since image release 0.2.5, the pod agent starts `llama-server` with speculative decoding. The model's built-in MTP head proposes up to 5 tokens. n-gram lookup proposes text that is already in the context. The 27B model checks every proposed token in one pass and keeps a token only when it samples the same token itself. Weights, quantization and the sampled distribution do not change.

The flags come from `Spec::args()` in `crates/lobo-agent/src/process.rs`:

```
--spec-type draft-mtp,ngram-mod --spec-draft-n-max 5
--spec-ngram-mod-n-match 24 --spec-ngram-mod-n-min 48 --spec-ngram-mod-n-max 64
```

The MTP head is part of the Q6_K GGUF, so no extra file downloads. VRAM use rises by about 1.5 GB (23.4 GB to 24.9 GB on the RTX 5090 at ctx 65,536).

## Switch

The agent reads `LOBO_SPEC` from its environment. Values: `mtp-ngram` (default), `mtp`, `ngram`, `off`. Any other value stops the agent with a config error. Users cannot set this key; it is an operator switch for the pod image.

## Results, 2026-10-31

Measured through the real `lobo up` path on RTX 5090 SECURE pods at ctx 65,536. Before: image 0.2.4 (`2409895`, no speculation). After: image 0.2.5 (`548ac7a`). Output tok/s, median of 2 reps after one warm-up.

| Workload | Before | After | Speed-up | Draft acceptance |
|---|---:|---:|---:|---:|
| W1 free prose, greedy | 55.6 | 102.2 | 1.84× | 36% |
| W1 free prose, sampled | 53.8 | 89.7 | 1.67× | 29% |
| W2 Go code, greedy | 54.5 | 147.9 | 2.71× | 60% |
| W2 Go code, sampled | 54.0 | 147.5 | 2.73× | 52% |
| W3 code edit (2.5k-token file) | 54.4 | 504.4 | 9.28× | 88% |
| W4 quote from a 24,733-token document | 50.4 | 212.4 | 4.22× | 63% |
| W5 second question, same document | 50.2 | 242.3 | 4.83× | 75% |
| W6 tool call | 52.6 | 137.3 | 2.61× | 86% |

The before and after runs used different hosts of the same GPU type. Time to first token did not change materially: W4 went from 4.76 s to 5.17 s, and the other workloads stayed within ±0.1 s.

## How the mode was chosen

`tools/spec_bench.py sweep` ran every mode on one bench pod, so the hosts were the same. Output tok/s, greedy, one rep:

| Mode | W1 | W2 | W3 | W4 | W5 | W6 |
|---|---:|---:|---:|---:|---:|---:|
| off | 58.9 | 59.2 | 59.2 | 54.9 | 54.3 | 58.3 |
| mtp, 3 drafts | 104.7 | 148.2 | 158.8 | 148.2 | 143.8 | 124.0 |
| mtp, 5 drafts | 95.2 | 160.7 | 198.7 | 177.4 | 165.8 | 142.6 |
| ngram | 58.5 | 58.5 | 463.3 | 88.7 | 85.1 | 60.3 |
| mtp-ngram, 3 drafts | 100.6 | 151.5 | 544.5 | 164.2 | 162.1 | 109.4 |
| mtp-ngram, 4 drafts | 96.3 | 154.1 | 546.7 | 168.2 | 172.5 | 116.7 |
| **mtp-ngram, 5 drafts** | 88.7 | 155.9 | 536.5 | 182.7 | 189.2 | 141.5 |
| DFlash2 drafter, 7 | 87.3 | 164.3 | 183.9 | 124.8 | 134.3 | 207.6 |

mtp-ngram with 5 drafts is the best overall choice for coding-agent work: code, edits, long documents and tool calls. Three drafts are about 15% faster on free prose only. The DFlash2 drafter needs a separate 2.06 GB file and is slower on long documents. A DFlash2 run with 15 drafts accepted exactly the same tokens as the run with 7 drafts and was stopped early.

## Output agreement

In the sweep, W2–W5 outputs were byte-identical to `off` for every MTP mode. W1 differed, and so did `ngram` W1 between two reps of the same prompt. This points to floating-point differences when tokens are checked as a batch, not to a quality change; the texts were not compared token by token. The earlier W6 mismatches came from llama-server's random tool-call ids in the hash; the bench now ignores those ids, and both W6 reps in the after run are identical.

## Rollback

- One pod: start the agent with `LOBO_SPEC=off`.
- All new starts: tag `ghcr.io/1905/lobocode@sha256:3d64617400c59420f0aea21a0bb6c0ac83376a8ee377dd2710ef63d5fc41fc0a` again as `latest-qwen38-q6`.

## Reproduce

```sh
# Bench pod (rents an RTX 5090 SECURE pod, terminates it at the end)
RUNPOD_API_KEY=... python3 tools/spec_bench.py sweep --configs off,mtp-ngram --out sweep.jsonl
# Any running endpoint, for example after `lobo up`
LOBO_API_KEY=... python3 tools/spec_bench.py endpoint --url http://127.0.0.1:8933 \
  --key-env LOBO_API_KEY --label after --out after.jsonl --reps 2 --sampled
python3 tools/spec_bench.py report sweep.jsonl after.jsonl
# While a bench pod is up: alert on idle GPU, stalled log or orphaned pod, once a minute
python3 tools/pod_watch.py HOST PORT sweep.jsonl SWEEP_PID
```

Total cost of the 2026-10-31 measurements: $1.60 (sweep pod $1.41, before pod $0.12, after pod $0.07).
