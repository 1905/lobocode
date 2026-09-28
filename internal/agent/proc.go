package agent

import (
	"context"
	"io"
	"os/exec"
)

// StartProcess runs a child with output into logs. exited gets its Wait error once.
func StartProcess(ctx context.Context, name string, args, env []string, logs io.Writer) (<-chan error, error) {
	cmd := exec.CommandContext(ctx, name, args...)
	cmd.Env = env
	cmd.Stdout, cmd.Stderr = logs, logs
	if err := cmd.Start(); err != nil {
		return nil, err
	}
	exited := make(chan error, 1)
	go func() { exited <- cmd.Wait() }()
	return exited, nil
}
