package agent

import (
	"context"
	"io"
	"os/exec"
	"strings"
)

// StartProcess runs a child with output into logs. exited gets its Wait error once.
func StartProcess(ctx context.Context, name string, args, env []string, logs io.Writer) (<-chan error, error) {
	_, exited, err := StartProcessPID(ctx, name, args, env, logs)
	return exited, err
}

// StartProcessPID is StartProcess that also returns the child pid (local mode reads its RSS).
func StartProcessPID(ctx context.Context, name string, args, env []string, logs io.Writer) (int, <-chan error, error) {
	cmd := exec.CommandContext(ctx, name, args...)
	cmd.Env = env
	cmd.Stdout, cmd.Stderr = logs, logs
	if err := cmd.Start(); err != nil {
		return 0, nil, err
	}
	exited := make(chan error, 1)
	go func() { exited <- cmd.Wait() }()
	return cmd.Process.Pid, exited, nil
}

// LastLine is the last line of a child's output that mentions "error" or "fail", else its last line.
func LastLine(s string) string {
	lines := strings.Split(strings.TrimSpace(s), "\n")
	for i := len(lines) - 1; i >= 0; i-- {
		if strings.Contains(lines[i], "error") || strings.Contains(lines[i], "fail") {
			return strings.TrimSpace(lines[i])
		}
	}
	return strings.TrimSpace(lines[len(lines)-1])
}
