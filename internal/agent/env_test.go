package agent

import (
	"strings"
	"testing"
)

func TestCleanEnv(t *testing.T) {
	in := []string{"LLAMA_ARG_HOST=0.0.0.0", "LOBO_MODEL_SSH_KEY=k", "LOBO_API_KEY=sk", "LLAMA_API_KEY=sk", "RUNPOD_API_KEY=r",
		"CONTAINER_API_KEY=c", "CF_TUNNEL_TOKEN=t", "LD_LIBRARY_PATH=/x", "PATH=/bin", "HOME=/root"}
	if got := strings.Join(CleanEnv(in), " "); got != "PATH=/bin HOME=/root" {
		t.Fatal(got)
	}
}
