// Package control is the laptop side of up/down/status. It only calls the GPU providers, the public bucket and the pod /api.
package control

import (
	"context"
	"time"

	"github.com/1905/lobocode/internal/agent"
	"github.com/1905/lobocode/internal/config"
	"github.com/1905/lobocode/internal/provider"
	"github.com/1905/lobocode/internal/release"
)

// Phases: create, image (container booting, agent not answering yet), then agent stages
// tunnel, download, load, ready, failed; terminated when `up` gave up and deleted the pod.
type ReadyInfo struct {
	PodID            string         `json:"pod_id"`
	Provider         string         `json:"provider"`
	Detail           string         `json:"detail"`              // provider's host description (cloud/tier or offer)
	Attempts         int            `json:"attempts"`            // pods rented, incl. bad hosts replaced
	RentS            float64        `json:"rent_to_container_s"` // this pod's create → bootstrap start
	HostDownloadMbps int            `json:"host_download_mbps"`  // what the provider says the host has (0 = unknown)
	Timings          *agent.Timings `json:"timings,omitempty"`
	URL              string         `json:"url"`
	Version          string         `json:"version"`
	GitSHA           string         `json:"git_sha"`
	CostPerHr        float64        `json:"usd_per_h"`
	Elapsed          time.Duration  `json:"elapsed_ns"`
}

type Event struct {
	Phase    string                  `json:"phase"`
	Detail   string                  `json:"detail,omitempty"`
	Download *agent.DownloadProgress `json:"download,omitempty"`
	Ready    *ReadyInfo              `json:"ready,omitempty"`
	Err      error                   `json:"-"`
	Done     bool                    `json:"done,omitempty"`
}

type ReleaseResolver interface {
	Resolve(ctx context.Context, version string) (release.Resolved, error)
}

type AgentAPI interface {
	Status(ctx context.Context) (*agent.Status, error)
	Version(ctx context.Context) (*release.Manifest, error)
	Logs(ctx context.Context, n int) (string, error)
}

// Presigner signs bucket keys for the r2 model source (laptop holds the R2 keys).
type Presigner interface {
	PresignGet(ctx context.Context, key string, ttl time.Duration) (string, error)
}

type Deps struct {
	Presign   Presigner                    // nil unless the r2 source is used
	Providers map[string]provider.Provider // configured providers by name ("runpod", "vast")
	Releases  ReleaseResolver
	Agent     AgentAPI
	Cfg       config.Laptop
	Clock     func() time.Time
	Poll      time.Duration // 3 s in real use
}

func (d Deps) now() time.Time {
	if d.Clock != nil {
		return d.Clock()
	}
	return time.Now()
}

type UpOpts struct {
	Model    string
	Ctx      int
	Release  string
	IdleMin  int
	MaxLife  time.Duration
	Timeout  time.Duration // give up after this long without progress
	Source   string        // model source override: ssh | r2 | public ("" = from the config)
	Conns    int           // parallel download streams (0 = agent default)
	Cloud    string        // runpod: "" or "secure" = datacenter first; "community" = cheaper community hosts first
	Provider string        // "runpod" (default) | "vast"
	MinMBps  int           // minimum model download speed; 0 = LOBO_MIN_MBPS from the config, else 100
	SSHKey   string        // debug: public key to allow SSH into the pod
}

// providerNames returns configured provider names in a stable order.
func (d Deps) providerNames() []string {
	var out []string
	for _, n := range []string{"runpod", "vast"} {
		if d.Providers[n] != nil {
			out = append(out, n)
		}
	}
	return out
}
