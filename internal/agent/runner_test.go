package agent

import (
	"context"
	"errors"
	"strings"
	"sync/atomic"
	"testing"
	"time"

	"github.com/rs/zerolog"

	"github.com/1905/lobocode/internal/metrics"
)

type fakeKiller struct{ n atomic.Int32 }

func (k *fakeKiller) KillSelf(context.Context) error { k.n.Add(1); return nil }

func okDeps(k *fakeKiller) Deps {
	never := func(context.Context) (<-chan error, error) { return make(chan error), nil }
	neverTunnel := func(context.Context, context.Context) (<-chan error, error) { return make(chan error), nil }
	return Deps{
		StartTunnel: neverTunnel,
		CheckGPU:    func(context.Context) error { return nil },
		Download: func(_ context.Context, p func(DownloadProgress)) error {
			p(DownloadProgress{Bytes: 5, Total: 10})
			return nil
		},
		StartLlama:  never,
		WaitHealthy: func(context.Context) error { return nil },
		Llama:       func(context.Context) (metrics.Llama, error) { return metrics.Llama{}, nil },
		GPU:         func(context.Context) (metrics.GPU, error) { return metrics.GPU{Name: "RTX 5090"}, nil },
		Host:        func(context.Context) (metrics.Host, error) { return metrics.Host{}, errors.New("no proc") },
		Killer:      k,
	}
}

func cfg() RunnerConfig {
	return RunnerConfig{Model: "q8", Ctx: 8192, Idle: time.Hour, ExpiresAt: time.Now().Add(time.Hour),
		BootTimeout: time.Minute, Tick: 5 * time.Millisecond, FailGrace: 20 * time.Millisecond}
}

func run(t *testing.T, d Deps, c RunnerConfig) (*Runner, chan error) {
	r := NewRunner(d, c, zerolog.Nop())
	done := make(chan error, 1)
	go func() { done <- r.Run(context.Background()) }()
	return r, done
}

func waitStage(t *testing.T, r *Runner, s Stage) Status {
	t.Helper()
	deadline := time.Now().Add(2 * time.Second)
	for time.Now().Before(deadline) {
		if st := r.Status(); st.Stage == s {
			return st
		}
		time.Sleep(2 * time.Millisecond)
	}
	t.Fatalf("stage %s not reached, at %+v", s, r.Status())
	return Status{}
}

func waitDone(t *testing.T, done chan error) {
	t.Helper()
	select {
	case <-done:
	case <-time.After(3 * time.Second):
		t.Fatal("runner did not finish")
	}
}

func TestHappyPath(t *testing.T) {
	k := &fakeKiller{}
	r, _ := run(t, okDeps(k), cfg())
	waitStage(t, r, StageReady)
	time.Sleep(20 * time.Millisecond)
	st := r.Status()
	if st.Download.Bytes != 5 || st.Model != "q8" || st.Llama == nil || st.GPU == nil || st.Host != nil {
		t.Fatalf("%+v", st)
	}
	if k.n.Load() != 0 {
		t.Fatal("killed a healthy pod")
	}
}

func TestDownloadFails(t *testing.T) {
	k := &fakeKiller{}
	d := okDeps(k)
	d.Download = func(context.Context, func(DownloadProgress)) error { return errors.New("sha mismatch") }
	r, done := run(t, d, cfg())
	st := waitStage(t, r, StageFailed)
	if !strings.Contains(st.StageDetail, "sha mismatch") {
		t.Fatal(st.StageDetail)
	}
	waitDone(t, done)
	if k.n.Load() != 1 || r.Status().KillReason != "failed" {
		t.Fatal(k.n.Load(), r.Status().KillReason)
	}
}

func TestDownloadHangsHitsBootTimeout(t *testing.T) {
	k := &fakeKiller{}
	d := okDeps(k)
	d.Download = func(ctx context.Context, _ func(DownloadProgress)) error { <-ctx.Done(); return ctx.Err() }
	c := cfg()
	c.BootTimeout = 30 * time.Millisecond
	r, done := run(t, d, c)
	st := waitStage(t, r, StageFailed)
	if !strings.Contains(st.StageDetail, "boot timeout") {
		t.Fatal(st.StageDetail)
	}
	waitDone(t, done)
}

func TestNeverHealthy(t *testing.T) {
	k := &fakeKiller{}
	d := okDeps(k)
	d.WaitHealthy = func(ctx context.Context) error { <-ctx.Done(); return ctx.Err() }
	c := cfg()
	c.BootTimeout = 30 * time.Millisecond
	_, done := run(t, d, c)
	waitDone(t, done)
	if k.n.Load() != 1 {
		t.Fatal(k.n.Load())
	}
}

func TestExpiresDuringDownload(t *testing.T) {
	k := &fakeKiller{}
	d := okDeps(k)
	d.Download = func(ctx context.Context, _ func(DownloadProgress)) error { <-ctx.Done(); return ctx.Err() }
	c := cfg()
	c.ExpiresAt = time.Now().Add(30 * time.Millisecond)
	r, done := run(t, d, c)
	waitDone(t, done)
	if r.Status().KillReason != "expired" || k.n.Load() != 1 {
		t.Fatal(r.Status().KillReason, k.n.Load())
	}
}

func TestExpiredAtStart(t *testing.T) {
	k := &fakeKiller{}
	c := cfg()
	c.ExpiresAt = time.Now().Add(-time.Minute)
	_, done := run(t, okDeps(k), c)
	waitDone(t, done)
	if k.n.Load() != 1 {
		t.Fatal(k.n.Load())
	}
}

func TestIdleKillOnce(t *testing.T) {
	k := &fakeKiller{}
	c := cfg()
	c.Idle = 40 * time.Millisecond
	r, done := run(t, okDeps(k), c)
	waitDone(t, done)
	time.Sleep(20 * time.Millisecond)
	if k.n.Load() != 1 || r.Status().KillReason != "idle" {
		t.Fatal(k.n.Load(), r.Status().KillReason)
	}
}

func TestBusyNotKilled(t *testing.T) {
	k := &fakeKiller{}
	d := okDeps(k)
	var tokens atomic.Int64
	d.Llama = func(context.Context) (metrics.Llama, error) {
		return metrics.Llama{PromptTokensTotal: tokens.Add(1)}, nil
	}
	c := cfg()
	c.Idle = 40 * time.Millisecond
	run(t, d, c)
	time.Sleep(150 * time.Millisecond)
	if k.n.Load() != 0 {
		t.Fatal("busy pod killed")
	}
}

func TestLlamaExitsAfterReady(t *testing.T) {
	k := &fakeKiller{}
	d := okDeps(k)
	exit := make(chan error, 1)
	d.StartLlama = func(context.Context) (<-chan error, error) { return exit, nil }
	r, done := run(t, d, cfg())
	waitStage(t, r, StageReady)
	exit <- errors.New("exit status 3")
	st := waitStage(t, r, StageFailed)
	if !strings.Contains(st.StageDetail, "llama-server exited") {
		t.Fatal(st.StageDetail)
	}
	waitDone(t, done)
}

func TestNoGPUFailsBeforeDownload(t *testing.T) {
	k := &fakeKiller{}
	d := okDeps(k)
	d.CheckGPU = func(context.Context) error { return errors.New("no CUDA device") }
	downloaded := false
	d.Download = func(context.Context, func(DownloadProgress)) error { downloaded = true; return nil }
	r, done := run(t, d, cfg())
	st := waitStage(t, r, StageFailed)
	if !strings.HasPrefix(st.StageDetail, "gpu: ") || downloaded {
		t.Fatal(st.StageDetail, downloaded)
	}
	waitDone(t, done)
}

func TestLlamaExitsWhileLoading(t *testing.T) {
	k := &fakeKiller{}
	d := okDeps(k)
	exit := make(chan error, 1)
	exit <- errors.New("exit status 1")
	d.StartLlama = func(context.Context) (<-chan error, error) { return exit, nil }
	d.WaitHealthy = func(ctx context.Context) error { <-ctx.Done(); return ctx.Err() }
	r, done := run(t, d, cfg())
	st := waitStage(t, r, StageFailed)
	if !strings.Contains(st.StageDetail, "exited while loading") {
		t.Fatal(st.StageDetail)
	}
	waitDone(t, done)
}

func TestTunnelExitDuringDownload(t *testing.T) {
	k := &fakeKiller{}
	d := okDeps(k)
	exit := make(chan error, 1)
	d.StartTunnel = func(context.Context, context.Context) (<-chan error, error) { return exit, nil }
	d.Download = func(ctx context.Context, _ func(DownloadProgress)) error {
		exit <- errors.New("exit status 1")
		<-ctx.Done()
		return ctx.Err()
	}
	r, done := run(t, d, cfg())
	st := waitStage(t, r, StageFailed)
	if !strings.Contains(st.StageDetail, "cloudflared exited") {
		t.Fatal(st.StageDetail)
	}
	waitDone(t, done)
}

// cloudflared dying mid-stage must be the reported cause, not the stage's "context canceled".
func TestTunnelExitIsTheReportedCause(t *testing.T) {
	for i := 0; i < 30; i++ {
		k := &fakeKiller{}
		d := okDeps(k)
		exit := make(chan error, 1)
		d.StartTunnel = func(context.Context, context.Context) (<-chan error, error) { return exit, nil }
		d.CheckGPU = func(ctx context.Context) error {
			exit <- errors.New("signal: killed")
			<-ctx.Done()
			return ctx.Err()
		}
		r, done := run(t, d, cfg())
		st := waitStage(t, r, StageFailed)
		if !strings.Contains(st.StageDetail, "cloudflared exited") {
			t.Fatalf("run %d: detail %q", i, st.StageDetail)
		}
		waitDone(t, done)
	}
}
