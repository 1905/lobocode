// Package metrics reads llama-server Prometheus text, nvidia-smi CSV and /proc on the pod.
package metrics

import (
	"bufio"
	"fmt"
	"io"
	"strconv"
	"strings"
)

// Llama is the subset of llama-server /metrics that lobo shows. Totals count since llama-server start.
type Llama struct {
	RequestsProcessing int     `json:"requests_processing"`
	RequestsDeferred   int     `json:"requests_deferred"`
	PromptTokensTotal  int64   `json:"prompt_tokens_total"`
	GenTokensTotal     int64   `json:"gen_tokens_total"`
	PromptTPS          float64 `json:"prompt_tps"`
	GenTPS             float64 `json:"gen_tps"`
}

// ParseLlama parses Prometheus text. requests_processing and prompt_tokens_total are required.
func ParseLlama(r io.Reader) (Llama, error) {
	vals := map[string]float64{}
	sc := bufio.NewScanner(r)
	for sc.Scan() {
		line := strings.TrimSpace(sc.Text())
		if line == "" || strings.HasPrefix(line, "#") {
			continue
		}
		f := strings.Fields(line)
		if len(f) < 2 {
			return Llama{}, fmt.Errorf("metrics: bad line %q", line)
		}
		v, err := strconv.ParseFloat(f[1], 64)
		if err != nil {
			return Llama{}, fmt.Errorf("metrics: bad value in %q", line)
		}
		vals[strings.TrimPrefix(f[0], "llamacpp:")] = v
	}
	if err := sc.Err(); err != nil {
		return Llama{}, err
	}
	for _, k := range []string{"requests_processing", "prompt_tokens_total"} {
		if _, ok := vals[k]; !ok {
			return Llama{}, fmt.Errorf("metrics: missing llamacpp:%s", k)
		}
	}
	return Llama{
		RequestsProcessing: int(vals["requests_processing"]),
		RequestsDeferred:   int(vals["requests_deferred"]),
		PromptTokensTotal:  int64(vals["prompt_tokens_total"]),
		GenTokensTotal:     int64(vals["tokens_predicted_total"]),
		PromptTPS:          vals["prompt_tokens_seconds"],
		GenTPS:             vals["predicted_tokens_seconds"],
	}, nil
}
