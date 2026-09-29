package local

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
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
func NewDeps(cfg RunConfig, logs io.Writer, stop func()) agent.Deps {
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
	}
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

	pid atomic.Int64 // llama-server, 0 until started
}

func newDeps(cfg RunConfig, logs io.Writer) *macDeps {
	return &macDeps{
		cfg: cfg, logs: logs, hfBase: HFBase, poll: 2 * time.Second,
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

// childEnv drops LLAMA_ARG_* (they override flags) and lobo/cloud secrets from the llama-server env.
func childEnv(env []string) []string {
	var out []string
	for _, kv := range env {
		k, _, _ := strings.Cut(kv, "=")
		if strings.HasPrefix(k, "LLAMA_ARG_") || strings.HasPrefix(k, "LOBO_") || k == "LLAMA_API_KEY" ||
			strings.HasSuffix(k, "_API_KEY") || strings.HasSuffix(k, "_TOKEN") {
			continue
		}
		out = append(out, kv)
	}
	return out
}

// checkGPU: llama-server must list a Metal device, and the model must fit in what Metal can wire.
func (d *macDeps) checkGPU(ctx context.Context) error {
	cctx, cancel := context.WithTimeout(ctx, time.Minute)
	defer cancel()
	cmd := exec.CommandContext(cctx, d.cfg.LlamaServer, "--list-devices")
	cmd.Env = childEnv(os.Environ())
	cmd.WaitDelay = 5 * time.Second
	out, _ := cmd.CombinedOutput()
	_, _ = fmt.Fprintf(d.logs, "[gpu-check] %s\n", strings.TrimSpace(string(out)))
	if !strings.Contains(string(out), "MTL0") {
		return fmt.Errorf("llama-server sees no Metal device: %s", lastLine(string(out)))
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

func lastLine(s string) string {
	s = strings.TrimSpace(s)
	if s == "" {
		return "no output"
	}
	return s[strings.LastIndexByte(s, '\n')+1:]
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
		got, err := hashFile(ctx, dst, m.Size, onProgress)
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

// hashFile sha256s path, reporting Verifying progress at most every 500 ms and once at the end.
func hashFile(ctx context.Context, path string, total int64, onProgress func(agent.DownloadProgress)) (string, error) {
	f, err := os.Open(path)
	if err != nil {
		return "", err
	}
	defer f.Close()
	h := sha256.New()
	buf := make([]byte, 4<<20)
	var n int64
	start, last := time.Now(), time.Time{}
	report := func(force bool) {
		if onProgress == nil || (!force && time.Since(last) < 500*time.Millisecond) {
			return
		}
		last = time.Now()
		mbps := 0.0
		if s := time.Since(start).Seconds(); s > 0 {
			mbps = float64(n) / s / 1e6
		}
		onProgress(agent.DownloadProgress{Bytes: n, Total: total, MBps: mbps, Verifying: true})
	}
	report(true)
	for {
		if err := ctx.Err(); err != nil {
			return "", err
		}
		k, err := f.Read(buf)
		h.Write(buf[:k])
		n += int64(k)
		report(false)
		if errors.Is(err, io.EOF) {
			break
		}
		if err != nil {
			return "", err
		}
	}
	report(true)
	return hex.EncodeToString(h.Sum(nil)), nil
}

func (d *macDeps) startLlama(ctx context.Context) (<-chan error, error) {
	args := append([]string{"-m", filepath.Join(d.cfg.Weights, d.cfg.Model.File)},
		agent.LlamaArgs(d.cfg.Model, "127.0.0.1", strconv.Itoa(d.cfg.Port), d.cfg.Ctx)...)
	env := append(childEnv(os.Environ()), "LLAMA_API_KEY="+d.cfg.APIKey)
	pid, exited, err := agent.StartProcessPID(ctx, d.cfg.LlamaServer, args, env, d.logs)
	if err != nil {
		return nil, err
	}
	d.pid.Store(int64(pid))
	return exited, nil
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
