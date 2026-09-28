package metrics

import (
	"context"
	"fmt"
	"net/http"
	"os"
	"time"
)

// Collector reads live numbers on the pod. Each read has its own 3 s timeout.
type Collector struct {
	LlamaURL string // e.g. http://127.0.0.1:8080
	APIKey   string // llama-server --api-key; upstream requires it on /metrics
	HTTP     *http.Client
	SMI      func(ctx context.Context) (string, error)
	ProcDir  string // "/proc" when empty
}

const readTimeout = 3 * time.Second

func (c Collector) Llama(ctx context.Context) (Llama, error) {
	ctx, cancel := context.WithTimeout(ctx, readTimeout)
	defer cancel()
	req, err := http.NewRequestWithContext(ctx, http.MethodGet, c.LlamaURL+"/metrics", nil)
	if err != nil {
		return Llama{}, err
	}
	req.Header.Set("Authorization", "Bearer "+c.APIKey)
	hc := c.HTTP
	if hc == nil {
		hc = http.DefaultClient
	}
	resp, err := hc.Do(req)
	if err != nil {
		return Llama{}, err
	}
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusOK {
		return Llama{}, fmt.Errorf("metrics: HTTP %d", resp.StatusCode)
	}
	return ParseLlama(resp.Body)
}

func (c Collector) GPU(ctx context.Context) (GPU, error) {
	ctx, cancel := context.WithTimeout(ctx, readTimeout)
	defer cancel()
	out, err := c.SMI(ctx)
	if err != nil {
		return GPU{}, err
	}
	return ParseNvidiaSMI(out)
}

func (c Collector) Host(context.Context) (Host, error) {
	dir := c.ProcDir
	if dir == "" {
		dir = "/proc"
	}
	la, err := os.ReadFile(dir + "/loadavg")
	if err != nil {
		return Host{}, err
	}
	mi, err := os.ReadFile(dir + "/meminfo")
	if err != nil {
		return Host{}, err
	}
	return ParseHost(string(la), string(mi))
}
