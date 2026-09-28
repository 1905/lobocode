package control

import (
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
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

// Snapshot collects pod + release + agent status. An unreachable agent is not an error.
func Snapshot(ctx context.Context, d Deps) (Snap, error) {
	l, err := running(ctx, d)
	if err != nil {
		return Snap{}, err
	}
	if len(l) == 0 {
		return Snap{Down: true, At: d.now()}, nil
	}
	s := Snap{Pod: &l[0], At: d.now()}
	s.Status, _ = d.Agent.Status(ctx)
	s.Version, _ = d.Agent.Version(ctx)
	return s, nil
}

// HTTPAgent calls the pod /api through the public domain.
type HTTPAgent struct {
	Base string // https://lobo.example.com
	Key  string
	HTTP *http.Client
}

func NewHTTPAgent(domain, key string) *HTTPAgent {
	return &HTTPAgent{Base: "https://" + domain, Key: key, HTTP: &http.Client{Timeout: 5 * time.Second}}
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
