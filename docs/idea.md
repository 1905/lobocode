# Qwen3.5-27B Uncensored Q8 on a rented RTX 5090

**Goal:** Run `Qwen3.5-27B-Uncensored-HauhauCS-Aggressive-Q8_0.gguf` on a rented RTX 5090, expose an OpenAI-compatible API, and connect that API to an OpenCode coding-agent harness.

**Target hardware:** 1× RTX 5090, 32 GB VRAM  
**Target model:** Qwen3.5-27B Uncensored HauhauCS Aggressive, Q8_0  
**Model file size:** ~28.6 GB  
**Recommended OS:** Ubuntu 24.04 / current RunPod CUDA image  
**Recommended system RAM:** 64 GB minimum; 96+ GB is nicer if any CPU offload is needed.

---

## 1. Recommended architecture

```text
Your laptop
┌──────────────────────────────┐
│ OpenCode                     │
│                              │
│ OpenAI-compatible provider   │
│ baseURL: localhost:8080/v1   │
└──────────────┬───────────────┘
               │
               │ SSH tunnel
               │
               ▼
Rented RTX 5090 machine
┌──────────────────────────────┐
│ llama-server                 │
│ OpenAI-compatible API        │
│ /v1/chat/completions         │
│ /v1/models                   │
│                              │
│ Qwen3.5-27B Uncensored Q8_0  │
│ CUDA → RTX 5090              │
└──────────────────────────────┘
```

### Why `llama-server` instead of putting Python in the inference hot path?

For this exact setup, use the latest upstream **llama.cpp `llama-server`** as the primary inference server.

Reasons:

1. GGUF is llama.cpp's native format.
2. CUDA support for the RTX 5090 is mature.
3. `llama-server` exposes an OpenAI-compatible `/v1` API.
4. OpenCode explicitly documents llama.cpp as a supported local/custom provider.
5. OpenCode depends heavily on tool/function calling. Qwen3.5 tool parsing has had compatibility bugs in older llama.cpp versions, so using the latest upstream build is preferable to a Python wheel that may bundle an older llama.cpp core.

Python can still be used for:
- API smoke tests.
- benchmarks.
- a small proxy if compatibility workarounds are ever needed.
- custom app clients.

A `llama-cpp-python` server alternative is included later if running the inference process itself from Python is important.

---

# 2. Rent the GPU

Use a **single RTX 5090 32 GB** instance.

Recommended pod/container resources:

- GPU: 1× RTX 5090 32 GB
- RAM: >=64 GB
- Disk: >=80 GB persistent workspace
- CUDA: recent CUDA 12.x/13.x image
- SSH access enabled

The Q8 model is **28.6 GB**, so 32 GB VRAM is tight. Do not initially request a huge context window.

Start at:

```text
8,192 tokens
```

Then measure VRAM and move upward only if there is room.

If Q8 becomes annoying for OpenCode because of context pressure, the fallback is the same model at **Q6_K (~22.1 GB)**. The quality difference should be very small, while the extra ~6.5 GB VRAM is extremely useful for KV cache.

---

# 3. Verify the rented machine

```bash
nvidia-smi
nvcc --version || true
free -h
df -h
```

Expected:

```text
GPU: NVIDIA GeForce RTX 5090
VRAM: ~32 GB
```

Create a persistent working directory:

```bash
mkdir -p /workspace/{models,src,logs}
cd /workspace
```

---

# 4. Install basic dependencies

Ubuntu:

```bash
apt-get update

apt-get install -y \
  git \
  git-lfs \
  build-essential \
  cmake \
  ninja-build \
  curl \
  python3 \
  python3-pip \
  python3-venv
```

Create a Python environment for downloading/testing:

```bash
python3 -m venv /workspace/.venv
source /workspace/.venv/bin/activate

pip install -U pip
pip install -U huggingface_hub hf_xet openai
```

---

# 5. Download the exact model

Repository:

```text
HauhauCS/Qwen3.5-27B-Uncensored-HauhauCS-Aggressive
```

File:

```text
Qwen3.5-27B-Uncensored-HauhauCS-Aggressive-Q8_0.gguf
```

Download:

```bash
hf download \
  HauhauCS/Qwen3.5-27B-Uncensored-HauhauCS-Aggressive \
  Qwen3.5-27B-Uncensored-HauhauCS-Aggressive-Q8_0.gguf \
  --local-dir /workspace/models
```

Verify:

```bash
ls -lh /workspace/models
```

Expected file size is approximately **28.6 GB**.

Optional SHA256 verification:

```bash
sha256sum \
  /workspace/models/Qwen3.5-27B-Uncensored-HauhauCS-Aggressive-Q8_0.gguf
```

Expected SHA256 at the time of writing:

```text
78ae0800c6062b0a2fbfb3649233fd152aaf0890d3cd871965795de0119e8dd5
```

---

# 6. Build the latest llama.cpp with CUDA

Do not start with an old distro package. Build current upstream so Qwen3.5 tool parsing is as recent as possible.

```bash
cd /workspace/src

git clone https://github.com/ggml-org/llama.cpp.git
cd llama.cpp

cmake -B build \
  -DGGML_CUDA=ON \
  -DCMAKE_BUILD_TYPE=Release

cmake --build build \
  --config Release \
  -j"$(nproc)" \
  --target llama-server llama-cli
```

Check:

```bash
./build/bin/llama-server --version
./build/bin/llama-server --help | head
```

---

# 7. First direct model test

Before introducing HTTP/OpenCode, verify that CUDA inference works.

```bash
cd /workspace/src/llama.cpp

./build/bin/llama-cli \
  -m /workspace/models/Qwen3.5-27B-Uncensored-HauhauCS-Aggressive-Q8_0.gguf \
  -ngl 99 \
  -c 8192 \
  -p "Write a short Go function that reverses a UTF-8 string correctly."
```

In another shell:

```bash
watch -n 1 nvidia-smi
```

Things to verify:

- Most/all model layers are on the 5090.
- No CUDA OOM.
- Generation speed is reasonable.
- VRAM does not sit at an unsafe 31.9/32 GB.

---

# 8. Launch the OpenAI-compatible server

Generate an API key:

```bash
export QWEN_API_KEY="sk-$(openssl rand -hex 24)"
echo "$QWEN_API_KEY"
```

Initial conservative server:

```bash
cd /workspace/src/llama.cpp

./build/bin/llama-server \
  -m /workspace/models/Qwen3.5-27B-Uncensored-HauhauCS-Aggressive-Q8_0.gguf \
  --alias qwen3.5-27b-uncensored-q8 \
  --host 127.0.0.1 \
  --port 8080 \
  -ngl 99 \
  -c 8192 \
  --parallel 1 \
  -fa on \
  --cache-type-k q8_0 \
  --cache-type-v q8_0 \
  --jinja \
  --chat-template-kwargs '{"enable_thinking":false}' \
  --reasoning-budget 0 \
  --api-key "$QWEN_API_KEY"
```

Notes:

- `127.0.0.1` is intentional. Do not expose the inference endpoint directly to the internet for the first setup.
- `-ngl 99` requests full GPU offload.
- `-c 8192` is deliberately conservative for Q8 + 32 GB.
- Q8 KV cache reduces KV-cache memory compared with FP16.
- `--jinja` is important for model-native chat/tool templates.
- Thinking is initially disabled because Qwen3.5 + agent frameworks has had compatibility issues involving assistant prefill/reasoning. Establish stable tool calling first.
- Once everything works, test thinking mode separately.

Health check:

```bash
curl http://127.0.0.1:8080/health
```

Models endpoint:

```bash
curl \
  -H "Authorization: Bearer $QWEN_API_KEY" \
  http://127.0.0.1:8080/v1/models
```

Expected model ID:

```text
qwen3.5-27b-uncensored-q8
```

---

# 9. Test it exactly like an OpenAI API

Create `/workspace/test_openai.py`:

```python
from openai import OpenAI

client = OpenAI(
    base_url="http://127.0.0.1:8080/v1",
    api_key="YOUR_KEY_HERE",
)

response = client.chat.completions.create(
    model="qwen3.5-27b-uncensored-q8",
    messages=[
        {
            "role": "user",
            "content": "Explain Go channels in 5 concise bullet points."
        }
    ],
    temperature=0.2,
    max_tokens=500,
)

print(response.choices[0].message.content)
```

Run:

```bash
source /workspace/.venv/bin/activate
python /workspace/test_openai.py
```

If this succeeds, the basic OpenAI-compatible API is working.

---

# 10. Critical test: tool/function calling

OpenCode is not just chat. It needs tools.

Run this before connecting OpenCode:

```bash
curl \
  http://127.0.0.1:8080/v1/chat/completions \
  -H "Authorization: Bearer $QWEN_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "qwen3.5-27b-uncensored-q8",
    "stream": false,
    "messages": [
      {
        "role": "user",
        "content": "Use the get_weather tool to check Bali."
      }
    ],
    "tools": [
      {
        "type": "function",
        "function": {
          "name": "get_weather",
          "description": "Get weather for a location",
          "parameters": {
            "type": "object",
            "properties": {
              "location": {
                "type": "string"
              }
            },
            "required": ["location"]
          }
        }
      }
    ],
    "tool_choice": "auto"
  }'
```

A healthy OpenAI-compatible result should include something structurally like:

```json
{
  "choices": [
    {
      "finish_reason": "tool_calls",
      "message": {
        "tool_calls": [
          {
            "type": "function",
            "function": {
              "name": "get_weather",
              "arguments": "{\"location\":\"Bali\"}"
            }
          }
        ]
      }
    }
  ]
}
```

### Important

Check that:

```text
tool_calls[].function.arguments
```

is a **JSON string**, not an already-parsed JSON object.

Older llama.cpp Qwen3.5 builds had tool parser incompatibilities that could break agent frameworks.

If this test fails:

1. `git pull` latest llama.cpp.
2. rebuild.
3. keep `enable_thinking=false`.
4. retry the tool-call test.
5. only then consider a proxy/workaround.

Do not debug OpenCode until this raw API test works.

---

# 11. Connect your laptop securely

The simplest setup is an SSH tunnel.

On your laptop:

```bash
ssh \
  -L 8080:127.0.0.1:8080 \
  root@YOUR_RUNPOD_HOST \
  -p YOUR_SSH_PORT
```

Now on your laptop:

```text
http://127.0.0.1:8080/v1
```

routes securely to the 5090 server.

Test locally:

```bash
curl \
  -H "Authorization: Bearer $QWEN_API_KEY" \
  http://127.0.0.1:8080/v1/models
```

For the first version, prefer SSH tunnelling over opening port 8080 publicly.

---

# 12. Configure OpenCode

Use a custom OpenAI-compatible provider.

Create/update `opencode.json` in your project:

```json
{
  "$schema": "https://opencode.ai/config.json",
  "provider": {
    "qwen5090": {
      "npm": "@ai-sdk/openai-compatible",
      "name": "Qwen 3.5 27B Uncensored Q8",
      "options": {
        "baseURL": "http://127.0.0.1:8080/v1",
        "apiKey": "{env:QWEN_API_KEY}"
      },
      "models": {
        "qwen3.5-27b-uncensored-q8": {
          "name": "Qwen 3.5 27B Uncensored Q8",
          "limit": {
            "context": 8192,
            "output": 4096
          }
        }
      }
    }
  }
}
```

On your laptop:

```bash
export QWEN_API_KEY="same-key-used-on-server"
```

Start OpenCode and select:

```text
qwen5090/qwen3.5-27b-uncensored-q8
```

If you use OpenCode's `/models` picker, the provider/model should appear there.

---

# 13. OpenCode acceptance tests

Do not begin with a giant repository.

Use this sequence.

## Test 1 — plain reasoning

```text
Explain this project's architecture. Do not modify anything.
```

## Test 2 — read tool

```text
Read go.mod and tell me the Go version and major dependencies.
```

## Test 3 — multiple reads

```text
Find where database connections are initialized and summarize the flow.
```

## Test 4 — shell tool

```text
Run the unit tests and summarize any failures. Do not edit files.
```

## Test 5 — edit

```text
Add a small unit test for X. Show me the diff before doing anything else.
```

## Test 6 — multi-step agent loop

Give it a small real bug that requires:

```text
search → read → reason → edit → test
```

This matters more than a generic chat benchmark because it verifies the exact OpenCode workflow.

---

# 14. VRAM tuning for Q8 + 5090

Q8 is a tight fit.

Approximate starting point:

```text
model file         ~28.6 GB
5090 VRAM           32 GB
remaining           ~3.4 GB before runtime/KV overhead
```

The actual resident VRAM depends on llama.cpp buffers, offload choices, KV cache, context length, and driver overhead.

Therefore:

### Start

```text
context = 8K
parallel = 1
Q8 KV cache
```

Then inspect:

```bash
nvidia-smi
```

### If there is plenty of room

Try:

```text
12K
16K
24K
32K
```

one step at a time.

### If Q8 OOMs even at 8K

Options in preferred order:

1. reduce context.
2. offload a few layers to CPU instead of all layers.
3. switch to Q6_K.

For OpenCode specifically, **Q6_K may ultimately be the better operational choice** even if Q8 is marginally higher precision, because coding agents benefit greatly from more context.

Q6_K size:

```text
~22.1 GB
```

That gives roughly another **6.5 GB** of VRAM headroom versus Q8.

---

# 15. Benchmark Q8 before deciding

Record:

- model load VRAM
- 8K-context VRAM
- prompt processing tokens/sec
- generation tokens/sec
- first-token latency
- tool-call correctness
- OpenCode agent success

Useful test prompts:

### Coding generation

```text
Implement an LRU cache in Go with O(1) Get and Put and include tests.
```

### Debugging

Give it a real failing test from your repo.

### Agent test

```text
Find the cause of this test failure, patch it, rerun tests, and explain the change.
```

The last one is the important benchmark.

A model that produces 50 tok/s but fails tool loops is worse for OpenCode than a model producing 30 tok/s reliably.

---

# 16. Python inference-server alternative

If you specifically want the inference server itself to be a Python process, use `llama-cpp-python`.

## Install

Use a recent CUDA build or compile against the pod's CUDA toolkit:

```bash
source /workspace/.venv/bin/activate

CMAKE_ARGS="-DGGML_CUDA=on" \
FORCE_CMAKE=1 \
pip install --upgrade --force-reinstall \
  "llama-cpp-python[server]"
```

Launch:

```bash
python3 -m llama_cpp.server \
  --model /workspace/models/Qwen3.5-27B-Uncensored-HauhauCS-Aggressive-Q8_0.gguf \
  --model_alias qwen3.5-27b-uncensored-q8 \
  --host 127.0.0.1 \
  --port 8080 \
  --n_gpu_layers -1 \
  --n_ctx 8192 \
  --flash_attn true \
  --api_key "$QWEN_API_KEY"
```

Its API is also OpenAI-compatible.

### Recommendation

For ordinary Python apps this is fine.

For **OpenCode + Qwen3.5 tool calling**, prefer the current native `llama-server` unless the Python package has been verified against the same tool-call tests. The native project tends to receive parser/template fixes first.

---

# 17. Optional: persistent launch script

Create `/workspace/start-qwen.sh`:

```bash
#!/usr/bin/env bash
set -euo pipefail

: "${QWEN_API_KEY:?QWEN_API_KEY is required}"

cd /workspace/src/llama.cpp

exec ./build/bin/llama-server \
  -m /workspace/models/Qwen3.5-27B-Uncensored-HauhauCS-Aggressive-Q8_0.gguf \
  --alias qwen3.5-27b-uncensored-q8 \
  --host 127.0.0.1 \
  --port 8080 \
  -ngl 99 \
  -c 8192 \
  --parallel 1 \
  -fa on \
  --cache-type-k q8_0 \
  --cache-type-v q8_0 \
  --jinja \
  --chat-template-kwargs '{"enable_thinking":false}' \
  --reasoning-budget 0 \
  --api-key "$QWEN_API_KEY"
```

Then:

```bash
chmod +x /workspace/start-qwen.sh
/workspace/start-qwen.sh
```

Run it inside `tmux` if the pod shell disconnects:

```bash
tmux new -s qwen
/workspace/start-qwen.sh
```

Detach:

```text
Ctrl+B, then D
```

Reconnect:

```bash
tmux attach -t qwen
```

---

# 18. Security

For the first deployment:

- bind llama-server to `127.0.0.1`
- use an API key
- access it through SSH port forwarding
- do not expose port 8080 publicly
- do not expose llama.cpp built-in file/shell tools; OpenCode should own the tools
- keep OpenCode's own permission controls enabled for shell/edit operations

If this later becomes a permanent shared API, put it behind:

```text
Caddy/nginx → TLS → auth → llama-server
```

or use a private overlay network such as Tailscale.

---

# 19. What not to do initially

Avoid these until the basic agent loop is stable:

- 262K context
- multiple parallel slots
- thinking mode
- vision/mmproj
- speculative decoding
- public internet exposure
- custom reverse proxies
- enormous repositories for the first test

Establish:

```text
Q8 loads
→ chat works
→ OpenAI API works
→ tool calls work
→ OpenCode works
→ then optimize
```

---

# 20. Expected end state

Your laptop sees:

```text
http://127.0.0.1:8080/v1
```

as a normal OpenAI-compatible API.

OpenCode uses:

```text
provider: qwen5090
model: qwen3.5-27b-uncensored-q8
```

OpenCode remains responsible for:

- reading files
- editing files
- shell commands
- git operations
- other coding-agent tools

The rented server is responsible only for:

```text
prompt → Qwen inference → text/tool-call response
```

That separation is simple and is the setup I would use first.

---

# 21. Success criteria

The setup is complete when all of these pass:

- [ ] RTX 5090 is detected.
- [ ] Q8 model loads without OOM.
- [ ] `/health` returns OK.
- [ ] `/v1/models` returns `qwen3.5-27b-uncensored-q8`.
- [ ] Python `OpenAI(...)` client receives a normal chat response.
- [ ] Raw tool-call test returns valid `tool_calls`.
- [ ] `function.arguments` is valid OpenAI-compatible JSON-string content.
- [ ] SSH tunnel works from your laptop.
- [ ] OpenCode lists the custom model.
- [ ] OpenCode can read a repository file.
- [ ] OpenCode can invoke shell/tests.
- [ ] OpenCode can edit a file and continue the agent loop.
- [ ] VRAM remains stable during a real task.

---

# 22. Sources / references

Model:

- https://huggingface.co/HauhauCS/Qwen3.5-27B-Uncensored-HauhauCS-Aggressive
- https://huggingface.co/HauhauCS/Qwen3.5-27B-Uncensored-HauhauCS-Aggressive/blob/main/Qwen3.5-27B-Uncensored-HauhauCS-Aggressive-Q8_0.gguf

llama.cpp:

- https://github.com/ggml-org/llama.cpp
- https://github.com/ggml-org/llama.cpp/blob/master/tools/server/README.md

llama-cpp-python:

- https://github.com/abetlen/llama-cpp-python
- https://github.com/abetlen/llama-cpp-python/blob/main/docs/server.md

OpenCode provider documentation:

- https://opencode.ai/docs/providers

Known Qwen3.5/OpenCode compatibility history worth knowing about:

- https://github.com/ggml-org/llama.cpp/issues/21158
- https://github.com/anomalyco/opencode/issues/27920

---

## Bottom line

Yes, this architecture is viable:

```text
RTX 5090
  ↓
Qwen3.5-27B Uncensored Q8 GGUF
  ↓
latest llama-server
  ↓
OpenAI-compatible /v1/chat/completions
  ↓
SSH tunnel
  ↓
OpenCode
```

The single thing to validate early is **tool-call compatibility**. Plain chat is easy; a reliable `read → shell → edit → test` OpenCode loop is the real acceptance test.
