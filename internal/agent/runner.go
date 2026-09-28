package agent

import (
	"context"
	"errors"
	"fmt"
	"sync"
	"sync/atomic"
	"time"

	"github.com/rs/zerolog"

	"github.com/1905/lobocode/internal/metrics"
	"github.com/1905/lobocode/internal/watchdog"
)

// Deps are the side effects the runner drives. Real ones are wired in cmd/lobo-agent.
type Deps struct {
	StartTunnel func(bootCtx, lifeCtx context.Context) (exited <-chan error, err error) // bootCtx bounds setup (binary fetch), lifeCtx the process
	CheckGPU    func(ctx context.Context) error                                         // CUDA must init; one community host failed it live
	Download    func(ctx context.Context, onProgress func(DownloadProgress)) error
	StartLlama  func(ctx context.Context) (exited <-chan error, err error)
	WaitHealthy func(ctx context.Context) error
	Llama       func(ctx context.Context) (metrics.Llama, error)
	GPU         func(ctx context.Context) (metrics.GPU, error)
	Host        func(ctx context.Context) (metrics.Host, error)
	Killer      Killer
}

type RunnerConfig struct {
	BootID      string
	Timings     Timings // prefilled with bootstrap times and download source/conns
	Model       string
	Ctx         int
	Idle        time.Duration
	ExpiresAt   time.Time
	BootTimeout time.Duration
	Tick        time.Duration // watchdog + status refresh period (30 s)
	FailGrace   time.Duration // how long a failed pod keeps /api/status up before dying (2 min)
}

type Runner struct {
	d   Deps
	cfg RunnerConfig
	log zerolog.Logger

	mu      sync.Mutex
	st      Status
	wd      *watchdog.State
	start   time.Time
	killed  chan struct{}
	once    sync.Once
	gpuOK   atomic.Bool
	stageAt time.Time
	failed  atomic.Bool
}

func NewRunner(d Deps, cfg RunnerConfig, log zerolog.Logger) *Runner {
	now := time.Now()
	return &Runner{d: d, cfg: cfg, log: log, start: now, wd: watchdog.NewState(now), killed: make(chan struct{}),
		st: Status{BootID: cfg.BootID, Stage: StageBoot, Model: cfg.Model, Ctx: cfg.Ctx, ExpiresAt: cfg.ExpiresAt, Timings: cfg.Timings}}
}

func (r *Runner) Status() Status {
	r.mu.Lock()
	defer r.mu.Unlock()
	s := r.st
	s.UptimeS = int64(time.Since(r.start).Seconds())
	return s
}

func (r *Runner) setStage(s Stage, detail string) {
	r.mu.Lock()
	now := time.Now()
	if !r.stageAt.IsZero() {
		d := now.Sub(r.stageAt).Seconds()
		switch r.st.Stage {
		case StageTunnel:
			r.st.Timings.TunnelS = d
		case StageGPU:
			r.st.Timings.GPUCheckS = d
		case StageDownload:
			r.st.Timings.DownloadS = d
		case StageVerify:
			r.st.Timings.VerifyS = d
		case StageLoad:
			r.st.Timings.LoadS = d
		}
	}
	if s == StageReady {
		r.st.Timings.ReadyAt = now
	}
	r.stageAt = now
	r.st.Stage, r.st.StageDetail = s, detail
	r.mu.Unlock()
	r.log.Info().Str("stage", string(s)).Str("detail", detail).Msg("stage")
}

// Run boots, then watches until the pod is killed (returns nil) or ctx ends.
func (r *Runner) Run(ctx context.Context) error {
	go r.watch(ctx)
	go r.boot(ctx)
	select {
	case <-r.killed:
		return nil
	case <-ctx.Done():
		return ctx.Err()
	}
}

func (r *Runner) boot(ctx context.Context) {
	bctx, cancel := context.WithTimeout(ctx, r.cfg.BootTimeout)
	defer cancel()
	fail := func(stage Stage, err error) {
		if errors.Is(bctx.Err(), context.DeadlineExceeded) {
			err = fmt.Errorf("boot timeout %s during %s: %w", r.cfg.BootTimeout, stage, err)
		}
		r.fail(fmt.Errorf("%s: %w", stage, err))
	}

	r.setStage(StageTunnel, "")
	tunnelExit, err := r.d.StartTunnel(bctx, ctx)
	if err != nil {
		fail(StageTunnel, err)
		return
	}
	// A dead tunnel makes the pod unreachable: fail at once, in any stage, and stop the boot.
	go func() {
		select {
		case err := <-tunnelExit:
			// Record the cause before cancelling: the cancelled stage would otherwise race to fail first
			// with "context canceled" (and "gpu: context canceled" reads as a bad host to the laptop).
			r.fail(fmt.Errorf("cloudflared exited: %v", err))
			cancel()
		case <-r.killed:
		case <-ctx.Done():
		}
	}()
	r.setStage(StageGPU, "")
	if err := r.d.CheckGPU(bctx); err != nil {
		r.fail(fmt.Errorf("gpu: %w", err))
		return
	}
	r.gpuOK.Store(true)
	r.setStage(StageDownload, "")
	if err := r.d.Download(bctx, func(p DownloadProgress) {
		r.mu.Lock()
		if p.Verifying && r.st.Stage == StageDownload {
			r.mu.Unlock()
			r.setStage(StageVerify, "")
			r.mu.Lock()
		}
		r.st.Download = p
		if p.Source != "" {
			r.st.Timings.DownloadSource = p.Source
		}
		if p.Total > 0 && p.Bytes == p.Total && p.MBps > 0 {
			r.st.Timings.DownloadMBps = p.MBps
		}
		r.mu.Unlock()
	}); err != nil {
		fail(StageDownload, err)
		return
	}
	r.setStage(StageLoad, "")
	llamaExit, err := r.d.StartLlama(ctx)
	if err != nil {
		fail(StageLoad, err)
		return
	}
	// Watch the process while waiting: an OOM on load exits llama-server and /health never comes.
	hctx, hcancel := context.WithCancel(bctx)
	healthy := make(chan error, 1)
	go func() { healthy <- r.d.WaitHealthy(hctx) }()
	select {
	case err := <-healthy:
		hcancel()
		if err != nil {
			fail(StageLoad, err)
			return
		}
	case err := <-llamaExit:
		hcancel()
		fail(StageLoad, fmt.Errorf("llama-server exited while loading: %v", err))
		return
	}
	r.mu.Lock()
	r.wd.SetReady(time.Now())
	r.mu.Unlock()
	r.setStage(StageReady, "")

	select {
	case err := <-llamaExit:
		r.fail(fmt.Errorf("llama-server exited: %v", err))
	case <-ctx.Done():
	case <-r.killed:
	}
}

// fail shows the error on /api/status for FailGrace, then deletes the pod.
func (r *Runner) fail(err error) {
	if !r.failed.CompareAndSwap(false, true) {
		return // first failure wins; a cancelled boot must not overwrite the real cause
	}
	r.setStage(StageFailed, err.Error())
	r.log.Error().Err(err).Dur("grace", r.cfg.FailGrace).Msg("boot failed, terminating after grace")
	go func() {
		time.Sleep(r.cfg.FailGrace)
		r.kill("failed")
	}()
}

func (r *Runner) kill(reason string) {
	r.once.Do(func() {
		r.mu.Lock()
		detail := r.st.StageDetail
		r.st.KillReason = reason
		r.mu.Unlock()
		r.setStage(StageTerminating, detail)
		// Fresh context: the failed stage's ctx must not cancel the delete.
		if err := r.d.Killer.KillSelf(context.Background()); err != nil {
			r.log.Error().Err(err).Msg("kill self")
		}
		close(r.killed)
	})
}

func (r *Runner) watch(ctx context.Context) {
	t := time.NewTicker(r.cfg.Tick)
	defer t.Stop()
	for {
		r.tick(ctx)
		select {
		case <-t.C:
		case <-ctx.Done():
			return
		case <-r.killed:
			return
		}
	}
}

func (r *Runner) tick(ctx context.Context) {
	now := time.Now()
	r.mu.Lock()
	ready := r.st.Stage == StageReady
	r.mu.Unlock()

	var lp *metrics.Llama
	sample := watchdog.Sample{At: now}
	if ready {
		if l, err := r.d.Llama(ctx); err == nil {
			lp = &l
			sample = watchdog.Sample{At: now, OK: true, Processing: l.RequestsProcessing, Deferred: l.RequestsDeferred,
				PromptTokens: l.PromptTokensTotal, GenTokens: l.GenTokensTotal}
		}
	}
	// nvidia-smi only after the CUDA check passed: early NVML calls are a suspect in the CUDA-init failures.
	var gp *metrics.GPU
	if r.gpuOK.Load() {
		if g, err := r.d.GPU(ctx); err == nil {
			gp = &g
		}
	}
	var hp *metrics.Host
	if h, err := r.d.Host(ctx); err == nil {
		hp = &h
	}

	r.mu.Lock()
	if ready {
		r.wd.Observe(sample)
	}
	d := r.wd.Decide(now, watchdog.Config{Idle: r.cfg.Idle, ExpiresAt: r.cfg.ExpiresAt})
	r.st.Llama, r.st.GPU, r.st.Host = lp, gp, hp
	r.st.MetricsFailures = r.wd.FailedSamples()
	r.st.IdleS = int64(r.wd.IdleFor(now).Seconds())
	r.st.KillInS = int64(d.KillIn.Seconds())
	if r.st.Stage != StageTerminating {
		r.st.KillReason = d.Reason // what will kill the pod next
	}
	r.mu.Unlock()

	if d.Kill {
		r.log.Info().Str("reason", d.Reason).Msg("watchdog kill")
		go r.kill(d.Reason)
	}
}
