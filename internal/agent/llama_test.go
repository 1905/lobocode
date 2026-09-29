package agent

import (
	"slices"
	"testing"

	"github.com/1905/lobocode/internal/model"
)

func TestLlamaArgs(t *testing.T) {
	m, err := model.Get("q8")
	if err != nil {
		t.Fatal(err)
	}
	// The pod's flags before LlamaArgs existed (cmd/lobo-agent/main.go).
	modelDir, host, port, ctx := "/models", "127.0.0.1", "8080", 65536
	want := []string{
		"-m", modelDir + "/" + m.File, "--alias", m.Alias,
		"--host", host, "--port", port,
		"-ngl", "99", "-c", "65536", "--parallel", "1",
		"-fa", "on", "--cache-type-k", "q8_0", "--cache-type-v", "q8_0",
		"--jinja", "--reasoning", "off",
		"--metrics", "--no-webui",
	}
	got := append([]string{"-m", modelDir + "/" + m.File}, LlamaArgs(m, host, port, ctx)...)
	if !slices.Equal(got, want) {
		t.Fatalf("got  %q\nwant %q", got, want)
	}
}
