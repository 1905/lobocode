package checks

import (
	"strings"
	"testing"
)

func TestValidateToolCall(t *testing.T) {
	ok := `{"choices":[{"finish_reason":"tool_calls","message":{"tool_calls":[{"type":"function","function":{"name":"get_weather","arguments":"{\"location\":\"Bali\"}"}}]}}]}`
	tests := []struct {
		name, body, wantErr string
	}{
		{"ok", ok, ""},
		{"object args", strings.Replace(ok, `"{\"location\":\"Bali\"}"`, `{"location":"Bali"}`, 1), "not a JSON string"},
		{"string not object", strings.Replace(ok, `"{\"location\":\"Bali\"}"`, `"Bali"`, 1), "not a JSON object"},
		{"no tool calls", `{"choices":[{"finish_reason":"stop","message":{"content":"hi"}}]}`, "no tool_calls"},
		{"wrong finish", strings.Replace(ok, `"tool_calls","message"`, `"stop","message"`, 1), "finish_reason"},
		{"no choices", `{"choices":[]}`, "no choices"},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			err := ValidateToolCall([]byte(tt.body))
			if tt.wantErr == "" && err != nil || tt.wantErr != "" && (err == nil || !strings.Contains(err.Error(), tt.wantErr)) {
				t.Fatalf("got %v, want %q", err, tt.wantErr)
			}
		})
	}
}

func TestReadStream(t *testing.T) {
	chunk := func(c string) string {
		return `data: {"choices":[{"delta":{"content":"` + c + `"},"finish_reason":null}]}` + "\n\n"
	}
	end := `data: {"choices":[{"delta":{},"finish_reason":"stop"}]}` + "\n\ndata: [DONE]\n\n"
	tests := []struct{ name, in, wantErr string }{
		{"ok", chunk("hel") + chunk("lo") + end, ""},
		{"no done", chunk("hi"), "without [DONE]"},
		{"error event", chunk("hi") + `data: {"error":{"message":"boom"}}` + "\n\n", "error event"},
		{"malformed", chunk("hi") + "data: {oops\n\n", "malformed"},
		{"no finish", chunk("hi") + "data: [DONE]\n\n", "finish_reason"},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got, err := ReadStream(strings.NewReader(tt.in))
			if tt.wantErr == "" && (err != nil || got != "hello") || tt.wantErr != "" && (err == nil || !strings.Contains(err.Error(), tt.wantErr)) {
				t.Fatalf("got %q %v", got, err)
			}
		})
	}
}
