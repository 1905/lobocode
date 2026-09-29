package agent

import (
	"context"
	"strings"
	"testing"
)

func TestStartProcess(t *testing.T) {
	logs := NewLogRing(10)
	exited, err := StartProcess(context.Background(), "/bin/sh", []string{"-c", "echo hi; exit 3"}, nil, logs)
	if err != nil {
		t.Fatal(err)
	}
	if err := <-exited; err == nil || !strings.Contains(err.Error(), "exit status 3") {
		t.Fatal(err)
	}
	if got := logs.Tail(1); len(got) != 1 || got[0] != "hi" {
		t.Fatal(got)
	}
}

func TestLastLine(t *testing.T) {
	for in, want := range map[string]string{
		"Available devices:\n  ggml_metal_init: error: failed\n  done\n": "ggml_metal_init: error: failed",
		"a\n b \n": "b",
		"one line": "one line",
	} {
		if got := LastLine(in); got != want {
			t.Fatalf("LastLine(%q) = %q, want %q", in, got, want)
		}
	}
}
