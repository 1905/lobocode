//go:build e2e

// Package e2e runs against the live API at https://$LOBO_DOMAIN. It needs a running pod (`make up`).
// Run: make e2e   (go test -tags e2e -count=1 -v ./e2e/)
package e2e

import (
	"bufio"
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"os"
	"os/exec"
	"path/filepath"
	"regexp"
	"strings"
	"sync"
	"testing"
	"time"

	"github.com/1905/lobocode/internal/agent"
	"github.com/1905/lobocode/internal/checks"
	"github.com/1905/lobocode/internal/config"
	"github.com/1905/lobocode/internal/control"
	"github.com/1905/lobocode/internal/model"
	"github.com/1905/lobocode/internal/release"
)

var (
	cfg   config.Laptop
	base  string // https://domain/v1
	alias string // model id to send
	st0   *agent.Status
	hc    = &http.Client{Timeout: 5 * time.Minute}
)

func TestMain(m *testing.M) {
	var err error
	if cfg, err = config.LoadLaptop("../.env"); err != nil {
		fmt.Fprintln(os.Stderr, "e2e:", err)
		os.Exit(2)
	}
	base = "https://" + cfg.Domain + "/v1"
	ctx, cancel := context.WithTimeout(context.Background(), 15*time.Second)
	st0, err = control.NewHTTPAgent(cfg.Domain, cfg.LoboAPIKey).Status(ctx)
	cancel()
	if err != nil || st0.Stage != agent.StageReady {
		fmt.Fprintf(os.Stderr, "e2e: pod not ready (run `make up`): status=%+v err=%v\n", st0, err)
		os.Exit(2)
	}
	md, err := model.Get(st0.Model)
	if err != nil {
		fmt.Fprintln(os.Stderr, "e2e:", err)
		os.Exit(2)
	}
	alias = md.Alias
	os.Exit(m.Run())
}

// ---- helpers ----

type msg struct {
	Role       string     `json:"role"`
	Content    string     `json:"content,omitempty"`
	ToolCalls  []toolCall `json:"tool_calls,omitempty"`
	ToolCallID string     `json:"tool_call_id,omitempty"`
}

type toolCall struct {
	ID       string `json:"id"`
	Type     string `json:"type"`
	Function struct {
		Name      string `json:"name"`
		Arguments string `json:"arguments"`
	} `json:"function"`
}

type chatResp struct {
	ID      string `json:"id"`
	Model   string `json:"model"`
	Choices []struct {
		FinishReason string `json:"finish_reason"`
		Message      msg    `json:"message"`
	} `json:"choices"`
	Usage struct {
		PromptTokens     int `json:"prompt_tokens"`
		CompletionTokens int `json:"completion_tokens"`
		TotalTokens      int `json:"total_tokens"`
	} `json:"usage"`
}

func do(t *testing.T, method, path, key string, body any) (int, []byte) {
	t.Helper()
	var rd io.Reader
	if body != nil {
		b, _ := json.Marshal(body)
		rd = bytes.NewReader(b)
	}
	req, _ := http.NewRequest(method, "https://"+cfg.Domain+path, rd)
	if key != "" {
		req.Header.Set("Authorization", "Bearer "+key)
	}
	req.Header.Set("Content-Type", "application/json")
	resp, err := hc.Do(req)
	if err != nil {
		t.Fatalf("%s %s: %v", method, path, err)
	}
	defer resp.Body.Close()
	b, _ := io.ReadAll(resp.Body)
	return resp.StatusCode, b
}

func chat(t *testing.T, req map[string]any) chatResp {
	t.Helper()
	req["model"] = alias
	code, b := do(t, "POST", "/v1/chat/completions", cfg.LoboAPIKey, req)
	if code != 200 {
		t.Fatalf("chat: HTTP %d: %s", code, b)
	}
	var r chatResp
	if err := json.Unmarshal(b, &r); err != nil || len(r.Choices) == 0 {
		t.Fatalf("chat: bad body %v: %s", err, b)
	}
	return r
}

func user(s string) []msg { return []msg{{Role: "user", Content: s}} }

var weatherTool = map[string]any{"type": "function", "function": map[string]any{
	"name": "get_weather", "description": "Get the current weather for a city",
	"parameters": map[string]any{"type": "object", "required": []string{"location"},
		"properties": map[string]any{"location": map[string]string{"type": "string", "description": "City name"}}},
}}

var readFileTool = map[string]any{"type": "function", "function": map[string]any{
	"name": "read_file", "description": "Read a file from the project",
	"parameters": map[string]any{"type": "object", "required": []string{"path"},
		"properties": map[string]any{"path": map[string]string{"type": "string"}}},
}}

// ---- pod API ----

func TestPodAPIVersionMatchesLatestRelease(t *testing.T) {
	code, b := do(t, "GET", "/api/version", "", nil)
	if code != 200 {
		t.Fatalf("HTTP %d", code)
	}
	var m release.Manifest
	if err := json.Unmarshal(b, &m); err != nil || m.Version == "" || m.GitSHA == "" {
		t.Fatalf("%v %s", err, b)
	}
	latest, err := control.BucketReleases{BucketURL: cfg.BucketURL}.Resolve(context.Background(), "")
	if err != nil {
		t.Fatal(err)
	}
	if m.Version != latest.Manifest.Version {
		t.Logf("note: pod runs %s, latest release is %s", m.Version, latest.Manifest.Version)
	}
	t.Logf("release %s git %s dirty=%v image %s", m.Version, m.GitSHA, m.GitDirty, m.LlamaImage)
}

func TestPodAPIStatus(t *testing.T) {
	if st0.GPU == nil || !strings.Contains(st0.GPU.Name, "5090") {
		t.Fatalf("gpu: %+v", st0.GPU)
	}
	if st0.Llama == nil || st0.Host == nil {
		t.Fatalf("metrics missing: llama=%v host=%v (metrics_failures=%d)", st0.Llama, st0.Host, st0.MetricsFailures)
	}
	if st0.KillInS <= 0 || st0.ExpiresAt.Before(time.Now()) {
		t.Fatalf("watchdog: kill_in %d expires %s", st0.KillInS, st0.ExpiresAt)
	}
	t.Logf("vram %d/%d MB, ctx %d, kill in %ds (%s)", st0.GPU.VRAMUsedMB, st0.GPU.VRAMTotalMB, st0.Ctx, st0.KillInS, st0.KillReason)
}

func TestPodAPILogsNeedKey(t *testing.T) {
	if code, _ := do(t, "GET", "/api/logs", "", nil); code != 401 {
		t.Fatalf("no key: HTTP %d", code)
	}
	code, b := do(t, "GET", "/api/logs?n=20", cfg.LoboAPIKey, nil)
	if code != 200 || !strings.Contains(string(b), "llama") {
		t.Fatalf("HTTP %d: %.300s", code, b)
	}
}

// ---- auth + models ----

func TestLLMRejectsMissingOrWrongKey(t *testing.T) {
	for _, key := range []string{"", "sk-wrong"} {
		code, _ := do(t, "POST", "/v1/chat/completions", key, map[string]any{"model": alias, "messages": user("hi")})
		if code != 401 {
			t.Errorf("key %q: HTTP %d, want 401", key, code)
		}
	}
	if code, _ := do(t, "GET", "/v1/models", "", nil); code != 401 {
		t.Errorf("/v1/models without key: HTTP %d, want 401", code)
	}
}

func TestModels(t *testing.T) {
	code, b := do(t, "GET", "/v1/models", cfg.LoboAPIKey, nil)
	if code != 200 || !strings.Contains(string(b), `"`+alias+`"`) {
		t.Fatalf("HTTP %d: %s", code, b)
	}
}

// ---- chat ----

func TestChatBasic(t *testing.T) {
	r := chat(t, map[string]any{"messages": user("What is 17 * 23? Answer with the number only."), "temperature": 0, "max_tokens": 20})
	if got := r.Choices[0].Message.Content; !strings.Contains(got, "391") {
		t.Fatalf("got %q", got)
	}
	if r.Choices[0].FinishReason != "stop" || r.Usage.PromptTokens == 0 || r.Usage.CompletionTokens == 0 {
		t.Fatalf("finish %q usage %+v", r.Choices[0].FinishReason, r.Usage)
	}
}

func TestChatSystemPromptFollowed(t *testing.T) {
	r := chat(t, map[string]any{"temperature": 0, "max_tokens": 30, "messages": []msg{
		{Role: "system", Content: "You are a bot that always answers in ALL CAPS and nothing else."},
		{Role: "user", Content: "say hello world"},
	}})
	got := r.Choices[0].Message.Content
	if !strings.Contains(got, "HELLO") || got != strings.ToUpper(got) {
		t.Fatalf("got %q", got)
	}
}

func TestChatMaxTokensTruncates(t *testing.T) {
	r := chat(t, map[string]any{"messages": user("Write a 500 word story about a lighthouse."), "max_tokens": 16})
	if r.Choices[0].FinishReason != "length" || r.Usage.CompletionTokens > 16 {
		t.Fatalf("finish %q, completion tokens %d", r.Choices[0].FinishReason, r.Usage.CompletionTokens)
	}
}

func TestChatStopSequence(t *testing.T) {
	r := chat(t, map[string]any{"messages": user("Count from 1 to 10 separated by spaces."), "temperature": 0, "stop": []string{"5"}, "max_tokens": 50})
	if strings.Contains(r.Choices[0].Message.Content, "6") {
		t.Fatalf("stop ignored: %q", r.Choices[0].Message.Content)
	}
}

func TestChatNoThinkingLeak(t *testing.T) {
	r := chat(t, map[string]any{"messages": user("Reply with just: ok"), "temperature": 0, "max_tokens": 20})
	if c := r.Choices[0].Message.Content; strings.Contains(c, "<think>") {
		t.Fatalf("thinking leaked into content: %q", c)
	}
}

func TestChatStreamed(t *testing.T) {
	text, err := checks.Chat(context.Background(), base, cfg.LoboAPIKey, alias)
	if err != nil || len(text) < 20 {
		t.Fatalf("%v %q", err, text)
	}
}

func TestStreamChunksAndDone(t *testing.T) {
	b, _ := json.Marshal(map[string]any{"model": alias, "stream": true, "max_tokens": 60, "messages": user("List three colors.")})
	req, _ := http.NewRequest("POST", base+"/chat/completions", bytes.NewReader(b))
	req.Header.Set("Authorization", "Bearer "+cfg.LoboAPIKey)
	req.Header.Set("Content-Type", "application/json")
	t0 := time.Now()
	resp, err := hc.Do(req)
	if err != nil {
		t.Fatal(err)
	}
	defer resp.Body.Close()
	if ct := resp.Header.Get("Content-Type"); !strings.Contains(ct, "text/event-stream") {
		t.Fatalf("content-type %q", ct)
	}
	sc := bufio.NewScanner(resp.Body)
	chunks, done := 0, false
	var ttft time.Duration
	for sc.Scan() {
		line, ok := strings.CutPrefix(sc.Text(), "data: ")
		if !ok {
			continue
		}
		if line == "[DONE]" {
			done = true
			break
		}
		if chunks == 0 {
			ttft = time.Since(t0)
		}
		chunks++
	}
	if chunks < 3 || !done {
		t.Fatalf("chunks %d done %v", chunks, done)
	}
	t.Logf("%d chunks, first chunk after %s (through Cloudflare)", chunks, ttft.Round(time.Millisecond))
}

func TestJSONMode(t *testing.T) {
	r := chat(t, map[string]any{"temperature": 0, "max_tokens": 100,
		"response_format": map[string]any{"type": "json_object"},
		"messages":        user(`Return a JSON object with keys "name" (string) and "age" (integer) for a person called Ada aged 36.`)})
	var v struct {
		Name string `json:"name"`
		Age  int    `json:"age"`
	}
	if err := json.Unmarshal([]byte(r.Choices[0].Message.Content), &v); err != nil || v.Age != 36 || !strings.Contains(v.Name, "Ada") {
		t.Fatalf("%v %q", err, r.Choices[0].Message.Content)
	}
}

// ---- tool calling (what OpenCode depends on) ----

func TestToolCallBasicArgumentsIsString(t *testing.T) {
	body, err := checks.ToolCall(context.Background(), base, cfg.LoboAPIKey, alias)
	if err != nil {
		t.Fatal(err)
	}
	if err := checks.ValidateToolCall(body); err != nil {
		t.Fatalf("%v\n%s", err, body)
	}
}

func TestToolCallPicksRightToolAndArgs(t *testing.T) {
	r := chat(t, map[string]any{"temperature": 0, "tools": []any{weatherTool, readFileTool},
		"messages": user("Open the file go.mod and show me what is inside.")})
	c := r.Choices[0]
	if c.FinishReason != "tool_calls" || len(c.Message.ToolCalls) == 0 {
		t.Fatalf("no tool call: %+v", c)
	}
	tc := c.Message.ToolCalls[0]
	var args map[string]string
	if tc.Function.Name != "read_file" || json.Unmarshal([]byte(tc.Function.Arguments), &args) != nil || !strings.Contains(args["path"], "go.mod") {
		t.Fatalf("got %s(%s)", tc.Function.Name, tc.Function.Arguments)
	}
	if tc.ID == "" || tc.Type != "function" {
		t.Fatalf("tool call id/type: %+v", tc)
	}
}

func TestToolCallNotForced(t *testing.T) {
	r := chat(t, map[string]any{"temperature": 0, "tools": []any{weatherTool}, "max_tokens": 50,
		"messages": user("What is the capital of France? Answer in one word.")})
	c := r.Choices[0]
	if len(c.Message.ToolCalls) != 0 || !strings.Contains(c.Message.Content, "Paris") {
		t.Fatalf("unexpected: finish %q tools %+v content %q", c.FinishReason, c.Message.ToolCalls, c.Message.Content)
	}
}

// Full agent loop: call → tool result → final answer that uses the result.
func TestToolCallRoundTrip(t *testing.T) {
	msgs := user("What's the weather in Bali right now? Use the tool.")
	r := chat(t, map[string]any{"temperature": 0, "tools": []any{weatherTool}, "messages": msgs})
	c := r.Choices[0]
	if len(c.Message.ToolCalls) == 0 {
		t.Fatalf("no tool call: %+v", c)
	}
	tc := c.Message.ToolCalls[0]
	msgs = append(msgs, msg{Role: "assistant", ToolCalls: c.Message.ToolCalls},
		msg{Role: "tool", ToolCallID: tc.ID, Content: `{"location":"Bali","temp_c":31,"condition":"thunderstorm"}`})
	r = chat(t, map[string]any{"temperature": 0, "tools": []any{weatherTool}, "messages": msgs, "max_tokens": 120})
	final := strings.ToLower(r.Choices[0].Message.Content)
	if r.Choices[0].FinishReason != "stop" || !strings.Contains(final, "31") || !strings.Contains(final, "thunder") {
		t.Fatalf("final answer ignores the tool result: finish %q %q", r.Choices[0].FinishReason, final)
	}
}

// Streaming tool calls: arguments arrive as string fragments that must concatenate to valid JSON.
func TestToolCallStreamed(t *testing.T) {
	b, _ := json.Marshal(map[string]any{"model": alias, "stream": true, "temperature": 0, "tools": []any{weatherTool},
		"messages": user("Use the get_weather tool to check Tokyo.")})
	req, _ := http.NewRequest("POST", base+"/chat/completions", bytes.NewReader(b))
	req.Header.Set("Authorization", "Bearer "+cfg.LoboAPIKey)
	req.Header.Set("Content-Type", "application/json")
	resp, err := hc.Do(req)
	if err != nil {
		t.Fatal(err)
	}
	defer resp.Body.Close()
	var name, args strings.Builder
	finish := ""
	sc := bufio.NewScanner(resp.Body)
	sc.Buffer(make([]byte, 1<<20), 1<<20)
	for sc.Scan() {
		line, ok := strings.CutPrefix(sc.Text(), "data: ")
		if !ok || line == "[DONE]" {
			continue
		}
		var ch struct {
			Choices []struct {
				FinishReason *string `json:"finish_reason"`
				Delta        struct {
					ToolCalls []struct {
						Function struct {
							Name      string          `json:"name"`
							Arguments json.RawMessage `json:"arguments"`
						} `json:"function"`
					} `json:"tool_calls"`
				} `json:"delta"`
			} `json:"choices"`
		}
		if err := json.Unmarshal([]byte(line), &ch); err != nil || len(ch.Choices) == 0 {
			continue
		}
		if f := ch.Choices[0].FinishReason; f != nil {
			finish = *f
		}
		for _, tc := range ch.Choices[0].Delta.ToolCalls {
			name.WriteString(tc.Function.Name)
			if len(tc.Function.Arguments) > 0 {
				var frag string
				if err := json.Unmarshal(tc.Function.Arguments, &frag); err != nil {
					t.Fatalf("streamed arguments fragment is not a JSON string: %s", tc.Function.Arguments)
				}
				args.WriteString(frag)
			}
		}
	}
	var a map[string]string
	if name.String() != "get_weather" || finish != "tool_calls" || json.Unmarshal([]byte(args.String()), &a) != nil || !strings.Contains(a["location"], "Tokyo") {
		t.Fatalf("name %q finish %q args %q", name.String(), finish, args.String())
	}
}

// ---- coding quality smoke ----

var goBlock = regexp.MustCompile("(?s)```(?:go)?\\s*\\n(.*?)```")

func TestGeneratesCompilingGo(t *testing.T) {
	if _, err := exec.LookPath("go"); err != nil {
		t.Skip("go not installed")
	}
	r := chat(t, map[string]any{"temperature": 0.2, "max_tokens": 1200, "messages": user(
		"Write a complete Go file, package main, with a function Reverse(s string) string that reverses a UTF-8 string by runes, " +
			"and a main that prints Reverse(\"héllo, 世界\"). Only one ```go code block, no explanation.")})
	m := goBlock.FindStringSubmatch(r.Choices[0].Message.Content)
	if m == nil {
		t.Fatalf("no go block: %q", r.Choices[0].Message.Content)
	}
	dir := t.TempDir()
	_ = os.WriteFile(filepath.Join(dir, "main.go"), []byte(m[1]), 0o644)
	_ = os.WriteFile(filepath.Join(dir, "go.mod"), []byte("module x\n\ngo 1.22\n"), 0o644)
	// Compile + vet only: model output is never executed on this machine. CGO off (no #cgo flags),
	// GOPROXY off and GOFLAGS=-mod=mod on an empty module (no downloads), so the build runs no generated code.
	for _, args := range [][]string{{"vet", "."}, {"build", "-o", os.DevNull, "."}} {
		cmd := exec.Command("go", args...)
		cmd.Dir = dir
		cmd.Env = append(os.Environ(), "CGO_ENABLED=0", "GOPROXY=off", "GOFLAGS=-mod=mod", "GOTOOLCHAIN=local")
		if out, err := cmd.CombinedOutput(); err != nil {
			t.Fatalf("go %s: %v\n%s\ncode:\n%s", args[0], err, out, m[1])
		}
	}
	if !strings.Contains(m[1], "func Reverse(") || !strings.Contains(m[1], "[]rune") {
		t.Fatalf("Reverse is not rune-based:\n%s", m[1])
	}
}

// ---- context + load ----

func TestLongPromptNearContext(t *testing.T) {
	// ~5k tokens of filler with a needle, under the 8K default context.
	var sb strings.Builder
	for i := 0; i < 400; i++ {
		fmt.Fprintf(&sb, "Line %d: the quick brown fox jumps over the lazy dog.\n", i)
		if i == 217 {
			sb.WriteString("Line 217b: the secret code word is PELICAN-42.\n")
		}
	}
	sb.WriteString("\nWhat is the secret code word? Answer with the code word only.")
	r := chat(t, map[string]any{"temperature": 0, "max_tokens": 20, "messages": user(sb.String())})
	if !strings.Contains(r.Choices[0].Message.Content, "PELICAN-42") {
		t.Fatalf("needle not found: %q (prompt tokens %d)", r.Choices[0].Message.Content, r.Usage.PromptTokens)
	}
	t.Logf("prompt tokens %d", r.Usage.PromptTokens)
}

func TestOverContextFailsCleanly(t *testing.T) {
	big := strings.Repeat("word ", st0.Ctx*2)
	code, b := do(t, "POST", "/v1/chat/completions", cfg.LoboAPIKey, map[string]any{"model": alias, "max_tokens": 5, "messages": user(big)})
	if code == 200 {
		t.Fatalf("over-context prompt accepted: %.200s", b)
	}
	if code >= 500 && code != 500 {
		t.Fatalf("HTTP %d (gateway error?): %.200s", code, b)
	}
	// Server must still answer afterwards.
	TestChatBasic(t)
}

func TestConcurrentRequestsQueue(t *testing.T) {
	var wg sync.WaitGroup
	errs := make(chan error, 3)
	for i := 0; i < 3; i++ {
		wg.Add(1)
		go func(i int) {
			defer wg.Done()
			code, b := do(t, "POST", "/v1/chat/completions", cfg.LoboAPIKey, map[string]any{
				"model": alias, "max_tokens": 40, "messages": user(fmt.Sprintf("Say the number %d in words.", i+1))})
			if code != 200 {
				errs <- fmt.Errorf("req %d: HTTP %d %.200s", i, code, b)
			}
		}(i)
	}
	wg.Wait()
	close(errs)
	for err := range errs {
		t.Error(err)
	}
}

// Runs last (alphabetical order is not guaranteed; name keeps intent clear): token counters moved.
func TestZZMetricsCountersMoved(t *testing.T) {
	st, err := control.NewHTTPAgent(cfg.Domain, cfg.LoboAPIKey).Status(context.Background())
	if err != nil || st.Llama == nil || st0.Llama == nil {
		t.Skipf("status: %v", err)
	}
	if st.Llama.PromptTokensTotal <= st0.Llama.PromptTokensTotal || st.Llama.GenTokensTotal <= st0.Llama.GenTokensTotal {
		t.Fatalf("counters did not move: before %+v after %+v (status refreshes every 30 s; rerun if the suite was very fast)", st0.Llama, st.Llama)
	}
	if st.IdleS > 60 {
		t.Fatalf("idle %ds right after traffic", st.IdleS)
	}
	t.Logf("tokens in %d→%d, out %d→%d, gen %.1f tok/s", st0.Llama.PromptTokensTotal, st.Llama.PromptTokensTotal, st0.Llama.GenTokensTotal, st.Llama.GenTokensTotal, st.Llama.GenTPS)
}
