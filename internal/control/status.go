package control

import (
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"strings"
	"time"

	"github.com/1905/lobocode/internal/agent"
	"github.com/1905/lobocode/internal/provider"
	"github.com/1905/lobocode/internal/release"
)

type Snap struct {
	Pod     *provider.Instance `json:"pod"`
	Version *release.Manifest  `json:"version"`
	Status  *agent.Status      `json:"status"`
	Down    bool               `json:"down"`
	At      time.Time          `json:"at"`
}

// Snapshot collects pod + release + agent status. An unreachable agent, or a cloud pod with no LOBO_DOMAIN, is not an error.
func Snapshot(ctx context.Context, d Deps) (Snap, error) {
	l, err := running(ctx, d)
	if err != nil {
		return Snap{}, err
	}
	if len(l) == 0 {
		return Snap{Down: true, At: d.now()}, nil
	}
	s := Snap{Pod: &l[0], At: d.now()}
	if l[0].Provider != "local" && d.Cfg.Domain == "" {
		return s, nil // no LOBO_DOMAIN: the pod's agent has no address, show the pod alone
	}
	ag := d.agentAt(l[0].AgentURL)
	s.Status, _ = ag.Status(ctx)
	s.Version, _ = ag.Version(ctx)
	return s, nil
}

// HTTPAgent calls the agent /api: a pod through the public domain, or the local supervisor.
type HTTPAgent struct {
	Base string // https://lobo.example.com or http://127.0.0.1:8932
	Key  string
	HTTP *http.Client
}

func NewHTTPAgent(domain, key string) *HTTPAgent { return NewHTTPAgentURL("https://"+domain, key) }

// NewHTTPAgentURL is an agent at base, e.g. the local supervisor http://127.0.0.1:8932.
func NewHTTPAgentURL(base, key string) *HTTPAgent {
	return &HTTPAgent{Base: strings.TrimRight(base, "/"), Key: key, HTTP: &http.Client{Timeout: 5 * time.Second}}
}

func (a *HTTPAgent) get(ctx context.Context, path string, auth bool) ([]byte, error) {
	req, err := http.NewRequestWithContext(ctx, http.MethodGet, a.Base+path, nil)
	if err != nil {
		return nil, err
	}
	if auth {
		req.Header.Set("Authorization", "Bearer "+a.Key)
	}
	resp, err := a.HTTP.Do(req)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()
	b, err := io.ReadAll(resp.Body)
	if err != nil {
		return nil, err
	}
	if resp.StatusCode != http.StatusOK {
		return nil, fmt.Errorf("%s: HTTP %d", path, resp.StatusCode)
	}
	return b, nil
}

func (a *HTTPAgent) Status(ctx context.Context) (*agent.Status, error) {
	b, err := a.get(ctx, "/api/status", false)
	if err != nil {
		return nil, err
	}
	var s agent.Status
	if err := json.Unmarshal(b, &s); err != nil {
		return nil, err
	}
	return &s, nil
}

func (a *HTTPAgent) Version(ctx context.Context) (*release.Manifest, error) {
	b, err := a.get(ctx, "/api/version", false)
	if err != nil {
		return nil, err
	}
	var m release.Manifest
	if err := json.Unmarshal(b, &m); err != nil {
		return nil, err
	}
	return &m, nil
}

func (a *HTTPAgent) Logs(ctx context.Context, n int) (string, error) {
	b, err := a.get(ctx, fmt.Sprintf("/api/logs?n=%d", n), true)
	return string(b), err
}

// BucketReleases resolves releases from the public bucket URL.
type BucketReleases struct {
	BucketURL string
	HTTP      *http.Client
}

func (b BucketReleases) Resolve(ctx context.Context, version string) (release.Resolved, error) {
	hc := b.HTTP
	if hc == nil {
		hc = &http.Client{Timeout: 15 * time.Second}
	}
	return release.Resolve(ctx, hc, b.BucketURL, version)
}
