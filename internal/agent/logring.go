package agent

import (
	"strings"
	"sync"
)

// LogRing keeps the last N log lines from the agent, llama-server and cloudflared.
type LogRing struct {
	mu      sync.Mutex
	lines   []string
	max     int
	partial string
}

func NewLogRing(n int) *LogRing { return &LogRing{max: n} }

func (r *LogRing) Write(p []byte) (int, error) {
	r.mu.Lock()
	defer r.mu.Unlock()
	parts := strings.Split(r.partial+string(p), "\n")
	r.partial = parts[len(parts)-1]
	r.lines = append(r.lines, parts[:len(parts)-1]...)
	if over := len(r.lines) - r.max; over > 0 {
		r.lines = append([]string(nil), r.lines[over:]...)
	}
	return len(p), nil
}

// Tail returns up to n most recent complete lines.
func (r *LogRing) Tail(n int) []string {
	r.mu.Lock()
	defer r.mu.Unlock()
	if n > len(r.lines) {
		n = len(r.lines)
	}
	return append([]string(nil), r.lines[len(r.lines)-n:]...)
}
