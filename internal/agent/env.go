package agent

import "strings"

// CleanEnv is env for llama-server and cloudflared: no LLAMA_ARG_* (they override flags; the pod image sets
// LLAMA_ARG_HOST=0.0.0.0), no LD_LIBRARY_PATH (the caller sets its own), and no secrets: LOBO_*, *_API_KEY
// (RUNPOD_, CONTAINER_, LOBO_, LLAMA_ …) and *_TOKEN (CF_TUNNEL_TOKEN …).
func CleanEnv(env []string) []string {
	var out []string
	for _, kv := range env {
		k, _, _ := strings.Cut(kv, "=")
		if strings.HasPrefix(k, "LLAMA_ARG_") || strings.HasPrefix(k, "LOBO_") || k == "LD_LIBRARY_PATH" ||
			strings.HasSuffix(k, "_API_KEY") || strings.HasSuffix(k, "_TOKEN") {
			continue
		}
		out = append(out, kv)
	}
	return out
}
