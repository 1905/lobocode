// Package checks runs the OpenAI-compatibility smoke tests against the public endpoint.
package checks

import (
	"bufio"
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"strings"
)

// ValidateToolCall checks an OpenAI chat completion: finish_reason tool_calls, and
// tool_calls[0].function.arguments is a JSON *string* that parses to an object.
func ValidateToolCall(body []byte) error {
	var r struct {
		Choices []struct {
			FinishReason string `json:"finish_reason"`
			Message      struct {
				ToolCalls []struct {
					Function struct {
						Name      string          `json:"name"`
						Arguments json.RawMessage `json:"arguments"`
					} `json:"function"`
				} `json:"tool_calls"`
			} `json:"message"`
		} `json:"choices"`
	}
	if err := json.Unmarshal(body, &r); err != nil {
		return fmt.Errorf("tool call: bad json: %w", err)
	}
	if len(r.Choices) == 0 {
		return fmt.Errorf("tool call: no choices")
	}
	c := r.Choices[0]
	if len(c.Message.ToolCalls) == 0 {
		return fmt.Errorf("tool call: no tool_calls (finish_reason %q)", c.FinishReason)
	}
	if c.FinishReason != "tool_calls" {
		return fmt.Errorf("tool call: finish_reason %q, want tool_calls", c.FinishReason)
	}
	raw := c.Message.ToolCalls[0].Function.Arguments
	var s string
	if err := json.Unmarshal(raw, &s); err != nil {
		return fmt.Errorf("tool call: arguments is not a JSON string: %s", raw)
	}
	var obj map[string]any
	if err := json.Unmarshal([]byte(s), &obj); err != nil {
		return fmt.Errorf("tool call: arguments string is not a JSON object: %q", s)
	}
	return nil
}

func post(ctx context.Context, url, key string, body any) (*http.Response, error) {
	b, _ := json.Marshal(body)
	req, err := http.NewRequestWithContext(ctx, http.MethodPost, url, bytes.NewReader(b))
	if err != nil {
		return nil, err
	}
	req.Header.Set("Authorization", "Bearer "+key)
	req.Header.Set("Content-Type", "application/json")
	resp, err := http.DefaultClient.Do(req)
	if err != nil {
		return nil, err
	}
	if resp.StatusCode != http.StatusOK {
		defer resp.Body.Close()
		msg, _ := io.ReadAll(resp.Body)
		return nil, fmt.Errorf("HTTP %d: %s", resp.StatusCode, strings.TrimSpace(string(msg)))
	}
	return resp, nil
}

// Chat sends a streamed chat request and returns the assembled text.
func Chat(ctx context.Context, baseURL, key, model string) (string, error) {
	resp, err := post(ctx, baseURL+"/chat/completions", key, map[string]any{
		"model": model, "stream": true, "temperature": 0.2, "max_tokens": 200,
		"messages": []map[string]string{{"role": "user", "content": "Explain Go channels in 3 concise bullet points."}},
	})
	if err != nil {
		return "", err
	}
	defer resp.Body.Close()
	return ReadStream(resp.Body)
}

// ReadStream assembles an SSE chat stream. It fails on error events, malformed chunks,
// a missing finish_reason or a missing [DONE].
func ReadStream(r io.Reader) (string, error) {
	var sb strings.Builder
	finish, done := "", false
	sc := bufio.NewScanner(r)
	sc.Buffer(make([]byte, 1<<20), 1<<20)
	for sc.Scan() {
		line, ok := strings.CutPrefix(sc.Text(), "data: ")
		if !ok {
			continue
		}
		if line == "[DONE]" {
			done = true
			break
		}
		var ch struct {
			Error   json.RawMessage `json:"error"`
			Choices []struct {
				FinishReason *string `json:"finish_reason"`
				Delta        struct {
					Content string `json:"content"`
				} `json:"delta"`
			} `json:"choices"`
		}
		if err := json.Unmarshal([]byte(line), &ch); err != nil {
			return sb.String(), fmt.Errorf("chat: malformed chunk %q", line)
		}
		if len(ch.Error) > 0 && string(ch.Error) != "null" {
			return sb.String(), fmt.Errorf("chat: error event %s", ch.Error)
		}
		if len(ch.Choices) > 0 {
			sb.WriteString(ch.Choices[0].Delta.Content)
			if f := ch.Choices[0].FinishReason; f != nil {
				finish = *f
			}
		}
	}
	if err := sc.Err(); err != nil {
		return sb.String(), err
	}
	switch {
	case !done:
		return sb.String(), fmt.Errorf("chat: stream ended without [DONE]")
	case finish != "stop" && finish != "length":
		return sb.String(), fmt.Errorf("chat: finish_reason %q", finish)
	case sb.Len() == 0:
		return "", fmt.Errorf("chat: empty streamed reply")
	}
	return sb.String(), nil
}

// ToolCall sends the docs/idea.md §10 get_weather request (non-streamed) and returns the raw body.
func ToolCall(ctx context.Context, baseURL, key, model string) ([]byte, error) {
	resp, err := post(ctx, baseURL+"/chat/completions", key, map[string]any{
		"model": model, "stream": false, "tool_choice": "auto",
		"messages": []map[string]string{{"role": "user", "content": "Use the get_weather tool to check Bali."}},
		"tools": []map[string]any{{"type": "function", "function": map[string]any{
			"name": "get_weather", "description": "Get weather for a location",
			"parameters": map[string]any{"type": "object", "properties": map[string]any{"location": map[string]string{"type": "string"}}, "required": []string{"location"}},
		}}},
	})
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()
	return io.ReadAll(resp.Body)
}
