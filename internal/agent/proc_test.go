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
