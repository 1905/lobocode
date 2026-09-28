// Package runpod talks to RunPod: the REST API from the laptop, GraphQL self-terminate from the pod.
package runpod

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/http"
	"strings"
	"time"

	"github.com/1905/lobocode/internal/provider"
)

var (
	ErrNoCapacity = provider.ErrNoCapacity
	ErrNotFound   = provider.ErrNotFound
)

// Time parses RunPod's "2026-09-23 08:48:28.204 +0000 UTC" timestamps.
type Time struct{ time.Time }

func (t Time) MarshalJSON() ([]byte, error) { return t.Time.MarshalJSON() }

func (t *Time) UnmarshalJSON(b []byte) error {
	var s string
	if err := json.Unmarshal(b, &s); err != nil || s == "" {
		return nil
	}
	p, err := time.Parse("2006-01-02 15:04:05.999999999 -0700 MST", s)
	if err != nil {
		p, err = time.Parse(time.RFC3339Nano, s) // our own --json output
	}
	if err != nil {
		return fmt.Errorf("runpod: time %q: %w", s, err)
	}
	t.Time = p
	return nil
}

type Pod struct {
	ID            string         `json:"id"`
	Name          string         `json:"name"`
	DesiredStatus string         `json:"desiredStatus"`
	ImageName     string         `json:"imageName"`
	CostPerHr     float64        `json:"costPerHr"`
	LastStartedAt Time           `json:"lastStartedAt"`
	CreatedAt     Time           `json:"createdAt"`
	GPUCount      int            `json:"gpuCount"`
	PortMappings  map[string]int `json:"portMappings"`
	Machine       *struct {
		MaxDownloadSpeedMbps int `json:"maxDownloadSpeedMbps"`
		MaxUploadSpeedMbps   int `json:"maxUploadSpeedMbps"`
		DiskThroughputMBps   int `json:"diskThroughputMBps"`
	} `json:"machine,omitempty"`
}

// Client is the REST client used on the laptop with the account key.
type Client struct {
	BaseURL string
	key     string
	HTTP    *http.Client
}

func New(key string) *Client {
	return &Client{BaseURL: "https://rest.runpod.io/v1", key: key, HTTP: &http.Client{Timeout: 30 * time.Second}}
}

func (c *Client) do(ctx context.Context, method, path string, body any, out any) error {
	var rd io.Reader
	if body != nil {
		b, err := json.Marshal(body)
		if err != nil {
			return err
		}
		rd = bytes.NewReader(b)
	}
	req, err := http.NewRequestWithContext(ctx, method, c.BaseURL+path, rd)
	if err != nil {
		return err
	}
	req.Header.Set("Authorization", "Bearer "+c.key)
	req.Header.Set("Content-Type", "application/json")
	resp, err := c.HTTP.Do(req)
	if err != nil {
		return err
	}
	defer resp.Body.Close()
	b, _ := io.ReadAll(resp.Body)
	if resp.StatusCode == http.StatusNotFound {
		return ErrNotFound
	}
	if resp.StatusCode >= 300 {
		msg := strings.TrimSpace(string(b))
		if isNoCapacity(msg) {
			return fmt.Errorf("%w: %s", ErrNoCapacity, msg)
		}
		return fmt.Errorf("runpod %s %s: HTTP %d: %s", method, path, resp.StatusCode, msg)
	}
	if out != nil {
		return json.Unmarshal(b, out)
	}
	return nil
}

// isNoCapacity matches RunPod's no-stock wording (undocumented; one form verified live).
func isNoCapacity(msg string) bool {
	m := strings.ToLower(msg)
	// Seen live 2026-09-23: {"error":"create pod: There are no instances currently available","status":500}
	return strings.Contains(m, "no instances") || strings.Contains(m, "no longer any instances") ||
		strings.Contains(m, "instances available") || strings.Contains(m, "not enough") || strings.Contains(m, "no gpu")
}

func (c *Client) Create(ctx context.Context, o provider.CreateOpts, cloud string, minDownloadMbps float64) (Pod, error) {
	var p Pod
	err := c.do(ctx, http.MethodPost, "/pods", BuildCreatePayload(o, cloud, minDownloadMbps), &p)
	return p, err
}

func (c *Client) List(ctx context.Context) ([]Pod, error) {
	var ps []Pod
	err := c.do(ctx, http.MethodGet, "/pods", nil, &ps)
	return ps, err
}

func (c *Client) Get(ctx context.Context, id string) (Pod, error) {
	var p Pod
	err := c.do(ctx, http.MethodGet, "/pods/"+id, nil, &p)
	return p, err
}

// Delete terminates a pod. An already-gone pod is not an error.
func (c *Client) Delete(ctx context.Context, id string) error {
	err := c.do(ctx, http.MethodDelete, "/pods/"+id, nil, nil)
	if errors.Is(err, ErrNotFound) {
		return nil
	}
	return err
}
