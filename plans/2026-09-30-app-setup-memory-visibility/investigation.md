# Local OpenCode wait and misleading app counters

**Date:** 2026-09-30
**Status:** observed; product fixes pending

## Observations

- The running server serves `qwen3.5-27b-uncensored-q6` at `http://127.0.0.1:8931/v1`, with a 65,536-token context.
- OpenCode previously pointed at an old HTTPS domain and selected Q8. Its global config now points at the active endpoint and Q6.
- Authentication succeeds for `/v1/models`. `/health` returns HTTP 200. These are metadata checks, not inference requests.
- The active OpenCode session used agent `build`. Its user message contains 18 characters.
- The server completed the title request with 558 prompt tokens and four generated tokens. Generation averaged 9.86 tokens/second.
- The next request entered prompt processing. The log reached 16,384 processed tokens at 35%, with an average of 58.26 tokens/second. The total is approximately 47,000 tokens; the log percentage is rounded.
- During that request, `/metrics` reported `requests_processing 1`, `requests_deferred 0`, and both speed gauges at zero.
- `/slots` reported `is_processing: true`, `n_prompt_tokens_processed: 16384`, and `n_decoded: 0`. `n_prompt_tokens: 18432` was the current slot token-buffer length, not the request total.
- The OpenCode assistant message had no completion timestamp or output text at the observation time. No current stream error was found.
- macOS reported zero swap usage at the observation time. This does not prove there can be no later memory pressure.

## Diagnosis

The connection fix worked. The visible wait was prompt processing before response generation. Most request input came from attached context, instructions and tool definitions, rather than the short user message. The exact contribution of each source was not captured.

The existing app presents the two speed gauges without a request activity state. Zero speed therefore looks like an idle or failed server while the server is busy. The short title request explains the earlier nonzero generation rate.

The global OpenCode default agent now selects the existing `lobo` agent. That agent disables MCP tools and skills while retaining core coding tools. Existing agents and global MCP settings were preserved. This change affects new sessions; the in-flight request was not cancelled or replaced. No smaller-prompt inference benchmark was run.

## Evidence locations

- Runtime log: `~/.local/state/lobo/local.log`.
- OpenCode log: `~/.local/share/opencode/log/opencode.log`.
- OpenCode session metadata: a read-only query of `~/.local/share/opencode/opencode.db`.
- Authenticated GET requests to the running server's `/v1/models`, `/metrics`, `/props` and `/slots`.
- Config backup files remain beside `~/.config/opencode/opencode.json`. They contain private configuration and must not be copied into the repository.

No model prompt, API key, full conversation or private config is reproduced here. The earlier image build and implementation queue remain paused.

## Final observation

The request reached 28,672 prompt tokens and 61% progress before cancellation. It had generated no reply tokens. OpenCode recorded `MessageAbortedError`. The local supervisor logged a clean stop at `2026-09-30T09:28:01Z`, with zero errors. The user confirmed that they quit OpenCode and stopped the runtime. The endpoint then refused connections. This stop was user initiated; it is not evidence of a crash or an idle-watchdog failure. The model was not restarted.

## Implementation research follow-up

OpenCode v1.18.33 accepts agent-scoped tool rules. A new agent can deny `*`, allow the core coding tools, and deny `read` patterns `mcp:*` while allowing normal file reads. There is no built-in `list` tool. Edit/write share the edit permission, including `apply_patch`. Some MCP resource tool definitions can remain present even when their execution is denied. Existing custom agent controls must stay unchanged.

Sources: [agent normalization](https://github.com/anomalyco/opencode/blob/v1.18.33/packages/core/src/v1/config/agent.ts), [tool registry](https://github.com/anomalyco/opencode/blob/v1.18.33/packages/opencode/src/tool/registry.ts), [permission filtering](https://github.com/anomalyco/opencode/blob/v1.18.33/packages/opencode/src/permission/index.ts), [request tool filtering](https://github.com/anomalyco/opencode/blob/v1.18.33/packages/opencode/src/session/llm/request.ts).

The pinned `jsonc-parser` defaults accept extra syntax beyond JSONC. Disable loose keys, missing commas, single quotes, hexadecimal numbers and unary plus. Reject duplicate decoded object keys before editing. CST append may expand compact objects; use strict parsed ranges for insertions when needed to preserve unrelated bytes. Reparse the final result. Sources: [ParseOptions](https://docs.rs/jsonc-parser/0.33.2/jsonc_parser/struct.ParseOptions.html), [CST source](https://docs.rs/jsonc-parser/0.33.2/src/jsonc_parser/cst/mod.rs.html).

These are source-verified implementation findings, not live runtime acceptance.

Pinned llama.cpp b11118 `/slots` reports active slots but no queue count. A slot query can run before pending work because it is high priority. `/health` and `/props` do not fill this gap. Show `No active request` for inactive slots with unknown queue state. Keep queued count null. Do not add another metrics scrape. Sources: [server routes](https://github.com/ggml-org/llama.cpp/blob/b11118/tools/server/server-context.cpp), [server documentation](https://github.com/ggml-org/llama.cpp/blob/b11118/tools/server/README.md). The current shared startup flags do not disable slots.
