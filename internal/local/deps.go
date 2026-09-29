package local

import (
	"context"
	"errors"
	"fmt"
	"io"
	"os"
	"os/exec"
	"path/filepath"
	"strconv"
	"strings"
	"sync/atomic"
	"time"

	"github.com/1905/lobocode/internal/agent"
	"github.com/1905/lobocode/internal/metrics"
	"github.com/1905/lobocode/internal/model"
)

// RunConfig is what `lobo local run` needs to drive llama-server on this Mac.
type RunConfig struct {
	Weights, LlamaServer, APIKey string
	Port, Ctx                    int
	Model                        model.Model
}

// NewDeps are the agent.Runner hooks for the Mac. stop is the Killer: the supervisor's own shutdown.
// waitLlama is true once llama-server has exited or never started, false after timeout.
func NewDeps(cfg RunConfig, logs io.Writer, stop func()) (deps agent.Deps, waitLlama func(timeout time.Duration) bool) {
	d := newDeps(cfg, logs)
	coll := metrics.Collector{LlamaURL: d.llamaURL, APIKey: cfg.APIKey}
	return agent.Deps{
		// No tunnel locally: a channel that never fires.
		StartTunnel: func(context.Context, context.Context) (<-chan error, error) { return make(chan error), nil },
		CheckGPU:    d.checkGPU,
		Download:    d.download,
		StartLlama:  d.startLlama,
		WaitHealthy: d.waitHealthy,
		Llama:       coll.Llama,
		GPU:         d.gpu,
		Host: func(context.Context) (metrics.Host, error) {
			return metrics.Host{}, errors.New("host metrics: not collected on macOS")
		},
		Killer: stopKiller(stop),
	}, d.waitLlama
}

// macDeps holds the hooks; the func fields are swapped in tests.
type macDeps struct {
	cfg      RunConfig
	logs     io.Writer
	hfBase   string
	llamaURL string
	poll     time.Duration // WaitHealthy period

	usableMiB func() (int, error)
	sysctl    func(name string) (string, error)
	memBytes  func() (uint64, error)
	ps        func(ctx context.Context, pid int) (string, error) // `ps -o rss= -p PID` output, KiB

	pid          atomic.Int64  // llama-server, 0 until started
	llamaStarted atomic.Bool   // set before the start: a shutdown racing it still waits
	llamaDone    chan struct{} // closed when llama-server exits or fails to start
}

func newDeps(cfg RunConfig, logs io.Writer) *macDeps {
	return &macDeps{
		cfg: cfg, logs: logs, hfBase: HFBase, poll: 2 * time.Second, llamaDone: make(chan struct{}),
		llamaURL:  "http://127.0.0.1:" + strconv.Itoa(cfg.Port),
		usableMiB: UsableMiB, sysctl: sysctlString, memBytes: memBytes,
		ps: func(ctx context.Context, pid int) (string, error) {
			b, err := exec.CommandContext(ctx, "ps", "-o", "rss=", "-p", strconv.Itoa(pid)).Output()
			return string(b), err
		},
	}
}

type stopKiller func()

func (s stopKiller) KillSelf(context.Context) error { s(); return nil }

// checkGPU: llama-server must list a Metal device, and the model must fit in what Metal can wire.
func (d *macDeps) checkGPU(ctx context.Context) error {
	cctx, cancel := context.WithTimeout(ctx, time.Minute)
	defer cancel()
	cmd := exec.CommandContext(cctx, d.cfg.LlamaServer, "--list-devices")
	cmd.Env = agent.CleanEnv(os.Environ())
	cmd.WaitDelay = 5 * time.Second
	out, _ := cmd.CombinedOutput()
	_, _ = fmt.Fprintf(d.logs, "[gpu-check] %s\n", strings.TrimSpace(string(out)))
	if !strings.Contains(string(out), "MTL0") {
		detail := "no output"
		if strings.TrimSpace(string(out)) != "" {
			detail = agent.LastLine(string(out))
		}
		return fmt.Errorf("llama-server sees no Metal device: %s", detail)
	}
	usable, err := d.usableMiB()
	if err != nil {
		return err
	}
	if need := model.MinFreeMiB(d.cfg.Model.Size); need > usable {
		return fmt.Errorf("%s needs %.1f GB, this Mac allows ~%.0f GB to the GPU", d.cfg.Model.ID, float64(need)/1024, float64(usable)/1024)
	}
	return nil
}

// download makes <weights>/<file> complete and verified. Full size + valid marker (markerValid): done. Full
// size, no valid marker: hash once, then mark. Short or missing: HF download (agent.Download hashes inline), then mark.
// A sha mismatch moves the file to <weights>/.bad/ and fails: never serve an unverified file.
func (d *macDeps) download(ctx context.Context, onProgress func(agent.DownloadProgress)) error {
	m, w := d.cfg.Model, d.cfg.Weights
	dst, marker := filepath.Join(w, m.File), MarkerPath(w, m.File)
	fi, err := os.Stat(dst)
	if err == nil && fi.Size() == m.Size {
		if markerValid(w, m) {
			return nil
		}
		got, err := agent.HashFile(ctx, dst, m.Size, onProgress)
		if err != nil {
			return err
		}
		if got != m.SHA256 {
			return d.quarantine(dst, fmt.Errorf("%s: sha256 %s, want %s", m.File, got, m.SHA256))
		}
		return writeMarker(w, m)
	}
	if err := os.Remove(marker); err != nil && !errors.Is(err, os.ErrNotExist) {
		return err
	}
	if err := agent.Download(ctx, agent.HTTPSource{URL: d.hfBase + m.File}, dst, m.SHA256, onProgress); err != nil {
		if _, serr := os.Stat(dst + ".bad"); serr == nil { // agent.Download renames a sha mismatch to .bad
			return d.quarantine(dst+".bad", err)
		}
		return err
	}
	return writeMarker(w, m)
}

// quarantine moves a bad file to <weights>/.bad/<file>.<unix> and returns cause.
func (d *macDeps) quarantine(src string, cause error) error {
	dir := filepath.Join(d.cfg.Weights, ".bad")
	if err := os.MkdirAll(dir, 0o755); err != nil {
		return fmt.Errorf("%w (quarantine: %v)", cause, err)
	}
	to := filepath.Join(dir, fmt.Sprintf("%s.%d", d.cfg.Model.File, time.Now().Unix()))
	if err := os.Rename(src, to); err != nil {
		return fmt.Errorf("%w (quarantine: %v)", cause, err)
	}
	return fmt.Errorf("%w, moved to %s", cause, to)
}

func (d *macDeps) startLlama(ctx context.Context) (<-chan error, error) {
	d.llamaStarted.Store(true)
	args := append([]string{"-m", filepath.Join(d.cfg.Weights, d.cfg.Model.File)},
		agent.LlamaArgs(d.cfg.Model, "127.0.0.1", strconv.Itoa(d.cfg.Port), d.cfg.Ctx)...)
	env := append(agent.CleanEnv(os.Environ()), "LLAMA_API_KEY="+d.cfg.APIKey)
	pid, exited, err := agent.StartProcessPID(ctx, d.cfg.LlamaServer, args, env, d.logs)
	if err != nil {
		close(d.llamaDone)
		return nil, err
	}
	d.pid.Store(int64(pid))
	out := make(chan error, 1)
	go func() {
		e := <-exited
		close(d.llamaDone)
		out <- e
	}()
	return out, nil
}

func (d *macDeps) waitLlama(timeout time.Duration) bool {
	if !d.llamaStarted.Load() {
		return true
	}
	select {
	case <-d.llamaDone:
		return true
	case <-time.After(timeout):
		return false
	}
}

func (d *macDeps) waitHealthy(ctx context.Context) error {
	return agent.WaitHealthy(ctx, d.llamaURL, d.poll)
}

// gpu: unified memory, so "VRAM" used = llama-server RSS and total = hw.memsize. No util counter.
func (d *macDeps) gpu(ctx context.Context) (metrics.GPU, error) {
	name, err := d.sysctl("machdep.cpu.brand_string")
	if err != nil {
		return metrics.GPU{}, err
	}
	mem, err := d.memBytes()
	if err != nil {
		return metrics.GPU{}, err
	}
	g := metrics.GPU{Name: strings.TrimSpace(name), VRAMTotalMB: int(mem >> 20)}
	pid := int(d.pid.Load())
	if pid == 0 {
		return g, nil
	}
	ctx, cancel := context.WithTimeout(ctx, 3*time.Second)
	defer cancel()
	out, err := d.ps(ctx, pid)
	if err != nil {
		return metrics.GPU{}, fmt.Errorf("ps %d: %w", pid, err)
	}
	kib, err := strconv.Atoi(strings.TrimSpace(out))
	if err != nil {
		return metrics.GPU{}, fmt.Errorf("ps %d: bad rss %q", pid, strings.TrimSpace(out))
	}
	g.VRAMUsedMB = kib / 1024
	return g, nil
}
