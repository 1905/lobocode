package config

import (
	"fmt"
	"os"
	"strconv"
	"strings"
	"time"
)

// Agent is the pod-side config, read from the container env that `lobo up` set.
type Agent struct {
	LoboAPIKey    string `env:"LOBO_API_KEY" validate:"required"`
	CFTunnelToken string `env:"CF_TUNNEL_TOKEN" validate:"required"`
	Model         string `env:"LOBO_MODEL" validate:"required"`
	ModelURL      string `env:"LOBO_MODEL_URL" validate:"required,url"` // https://… or ssh://user@host:port
	ModelFallback string `env:"LOBO_MODEL_URL_FALLBACK"`                // optional 2nd source if the first is too slow
	Provider      string `env:"LOBO_PROVIDER"`                          // "runpod" (default) | "vast"
	RunPodPodID   string `env:"RUNPOD_POD_ID"`                          // runpod: injected by RunPod
	RunPodAPIKey  string `env:"RUNPOD_API_KEY"`                         // runpod: pod-scoped key RunPod injects
	VastID        string `env:"CONTAINER_ID"`                           // vast: injected by Vast
	VastAPIKey    string `env:"CONTAINER_API_KEY"`                      // vast: instance-scoped key (can GET/DELETE only this instance)
	ModelSSHKey   string `env:"LOBO_MODEL_SSH_KEY"`                     // base64 private key, only for ssh:// model URLs
	ModelHostKey  string `env:"LOBO_MODEL_SSH_HOSTKEY"`
	BootID        string `env:"LOBO_BOOT_ID"` // set by `up`; echoed in /api/status
	Ctx           int
	IdleMin       int
	DLConns       int // parallel ranged streams for the model download
	MinMBps       int // download slower than this after 20 s = drop the source/host
	ExpiresAt     time.Time
	BootTimeout   time.Duration
}

// LoadAgent reads the pod env. Errors name the env var to fix.
func LoadAgent() (Agent, error) {
	m := map[string]string{}
	for _, k := range []string{"LOBO_API_KEY", "CF_TUNNEL_TOKEN", "LOBO_MODEL", "LOBO_MODEL_URL", "LOBO_PROVIDER", "RUNPOD_POD_ID", "RUNPOD_API_KEY", "CONTAINER_ID", "CONTAINER_API_KEY", "LOBO_MODEL_SSH_KEY", "LOBO_MODEL_SSH_HOSTKEY", "LOBO_MODEL_URL_FALLBACK", "LOBO_BOOT_ID"} {
		m[k] = os.Getenv(k)
	}
	var a Agent
	fill(&a, m)
	if err := validate(a); err != nil {
		return Agent{}, err
	}
	if a.Provider == "" {
		a.Provider = "runpod"
	}
	switch a.Provider {
	case "runpod":
		if a.RunPodPodID == "" || a.RunPodAPIKey == "" {
			return Agent{}, fmt.Errorf("config: LOBO_PROVIDER=runpod needs RUNPOD_POD_ID and RUNPOD_API_KEY (RunPod injects them)")
		}
	case "vast":
		if a.VastID == "" || a.VastAPIKey == "" {
			return Agent{}, fmt.Errorf("config: LOBO_PROVIDER=vast needs CONTAINER_ID and CONTAINER_API_KEY (Vast injects them)")
		}
	default:
		return Agent{}, fmt.Errorf("config: LOBO_PROVIDER: want runpod or vast, got %q", a.Provider)
	}
	if strings.HasPrefix(a.ModelURL, "ssh://") && (a.ModelSSHKey == "" || a.ModelHostKey == "") {
		return Agent{}, fmt.Errorf("config: LOBO_MODEL_URL is ssh://: LOBO_MODEL_SSH_KEY and LOBO_MODEL_SSH_HOSTKEY are required")
	}
	var err error
	if a.Ctx, err = envInt("LOBO_CTX", 8192); err != nil {
		return Agent{}, err
	}
	if a.IdleMin, err = envInt("LOBO_IDLE_MIN", 30); err != nil {
		return Agent{}, err
	}
	if a.MinMBps, err = envInt("LOBO_MIN_MBPS", 100); err != nil {
		return Agent{}, err
	}
	if a.DLConns, err = envInt("LOBO_DL_CONNS", 1); err != nil { // 1 = the path verified live; >1 not measured yet
		return Agent{}, err
	}
	exp := os.Getenv("LOBO_EXPIRES_AT")
	if a.ExpiresAt, err = time.Parse(time.RFC3339, exp); err != nil {
		return Agent{}, fmt.Errorf("config: LOBO_EXPIRES_AT: want RFC3339, got %q", exp)
	}
	a.BootTimeout = 40 * time.Minute
	if v := os.Getenv("LOBO_BOOT_TIMEOUT"); v != "" {
		if a.BootTimeout, err = time.ParseDuration(v); err != nil {
			return Agent{}, fmt.Errorf("config: LOBO_BOOT_TIMEOUT: %w", err)
		}
	}
	return a, nil
}

func envInt(name string, def int) (int, error) {
	v := os.Getenv(name)
	if v == "" {
		return def, nil
	}
	n, err := strconv.Atoi(v)
	if err != nil || n <= 0 {
		return 0, fmt.Errorf("config: %s: want positive int, got %q", name, v)
	}
	return n, nil
}
