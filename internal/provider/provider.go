// Package provider is the neutral interface lobo uses to rent GPU hosts (RunPod, Vast).
package provider

import (
	"context"
	"errors"
	"time"
)

var (
	ErrNoCapacity = errors.New("provider: no gpu capacity")
	ErrNotFound   = errors.New("provider: instance not found")
)

// Name is the pod name / instance label that marks lobo's own instances.
const Name = "lobo"

// Instance is one rented host, as far as lobo cares.
type Instance struct {
	Provider         string    `json:"provider"` // "runpod" | "vast"
	ID               string    `json:"id"`
	Status           string    `json:"status"`
	Detail           string    `json:"detail"` // e.g. "SECURE, host ≥5000 Mbps" or "offer 51738273, 20313 Mbps, California"
	CostPerHr        float64   `json:"cost_per_hr"`
	StartedAt        time.Time `json:"started_at"`
	HostDownloadMbps int       `json:"host_download_mbps"`
}

// CreateOpts is everything the pod needs; the provider decides where it runs.
type CreateOpts struct {
	Image         string
	ReleaseURL    string
	ReleaseSHA256 string
	ModelURL      string
	ModelFallback string // optional 2nd model URL, used by the agent if ModelURL is too slow or fails
	LoboAPIKey    string
	CFTunnelToken string
	Model         string
	Ctx           int
	IdleMin       int
	DLConns       int // parallel download streams on the pod
	MinMBps       int // agent drops a source/host downloading slower than this
	ExpiresAt     time.Time
	Cloud         string // runpod: "secure" (default) | "community" preference; vast: ignored
	SSHPubKey     string // debug only (runpod): open 22/tcp and run sshd before the bootstrap
	ModelSSHKey   string // base64 private key for an ssh:// ModelURL (restricted model-server user)
	ModelHostKey  string
	BootID        string // random per rent; the agent echoes it in /api/status so `up` ignores other pods on the shared tunnel
}

// Provider rents and removes lobo instances on one GPU cloud.
type Provider interface {
	Name() string
	// Rent picks a host (RunPod: cloud + network tiers; Vast: fastest offer) and creates the instance.
	// note gets progress lines for the UI. ErrNoCapacity when nothing matches.
	Rent(ctx context.Context, o CreateOpts, note func(string)) (Instance, error)
	List(ctx context.Context) ([]Instance, error)         // lobo instances only
	Get(ctx context.Context, id string) (Instance, error) // ErrNotFound when gone
	Delete(ctx context.Context, id string) error          // already gone = nil
}
