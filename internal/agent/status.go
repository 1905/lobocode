package agent

import (
	"time"

	"github.com/1905/lobocode/internal/metrics"
)

type Stage string

const (
	StageBoot        Stage = "boot"
	StageTunnel      Stage = "tunnel"
	StageGPU         Stage = "gpu"
	StageVerify      Stage = "verify"
	StageDownload    Stage = "download"
	StageLoad        Stage = "load"
	StageReady       Stage = "ready"
	StageFailed      Stage = "failed"
	StageTerminating Stage = "terminating"
)

// Status is the /api/status body. nil sections = unavailable.
type Status struct {
	BootID          string           `json:"boot_id,omitempty"` // LOBO_BOOT_ID of this rent (empty on old releases)
	Stage           Stage            `json:"stage"`
	StageDetail     string           `json:"stage_detail"`
	Download        DownloadProgress `json:"download"`
	UptimeS         int64            `json:"uptime_s"`
	IdleS           int64            `json:"idle_s"`
	KillInS         int64            `json:"kill_in_s"`
	KillReason      string           `json:"kill_reason"`
	ExpiresAt       time.Time        `json:"expires_at"`
	GPU             *metrics.GPU     `json:"gpu"`
	Host            *metrics.Host    `json:"host"`
	Llama           *metrics.Llama   `json:"llama"`
	MetricsFailures int              `json:"metrics_failures"`
	Model           string           `json:"model"`
	Ctx             int              `json:"ctx"`
	Timings         Timings          `json:"timings"`
}

// Timings of one boot. Seconds; 0 = not reached yet.
type Timings struct {
	ContainerStartedAt time.Time `json:"container_started_at"` // bootstrap began (container is running)
	BootstrapAptS      float64   `json:"bootstrap_apt_s"`
	BootstrapZipS      float64   `json:"bootstrap_zip_s"`
	TunnelS            float64   `json:"tunnel_s"`
	GPUCheckS          float64   `json:"gpu_check_s"`
	DownloadS          float64   `json:"download_s"`
	DownloadMBps       float64   `json:"download_mbps"`
	DownloadSource     string    `json:"download_source"`
	DownloadConns      int       `json:"download_conns"`
	VerifyS            float64   `json:"verify_s"` // sha256 pass after a parallel download
	LoadS              float64   `json:"load_s"`
	ReadyAt            time.Time `json:"ready_at"`
}
