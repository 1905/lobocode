package agent

import (
	"strconv"

	"github.com/1905/lobocode/internal/model"
)

// LlamaArgs are the llama-server flags shared by the pod and local mode.
// The caller prepends "-m <gguf path>": the path differs per host.
func LlamaArgs(m model.Model, host, port string, ctx int) []string {
	return []string{
		"--alias", m.Alias,
		"--host", host, "--port", port,
		"-ngl", "99", "-c", strconv.Itoa(ctx), "--parallel", "1",
		"-fa", "on", "--cache-type-k", "q8_0", "--cache-type-v", "q8_0",
		"--jinja", "--reasoning", "off",
		"--metrics", "--no-webui",
	}
}
