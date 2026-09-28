package tui

import (
	"errors"
	"flag"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"github.com/charmbracelet/x/ansi"

	"github.com/1905/lobocode/internal/agent"
	"github.com/1905/lobocode/internal/control"
	ct "github.com/1905/lobocode/internal/control/controltest"
	"github.com/1905/lobocode/internal/metrics"
	"github.com/1905/lobocode/internal/provider"
	"github.com/1905/lobocode/internal/release"
)

var update = flag.Bool("update", false, "rewrite golden files")

func golden(t *testing.T, name, got string) {
	t.Helper()
	got = ansi.Strip(got)
	p := filepath.Join("testdata", name+".golden")
	if *update {
		if err := os.WriteFile(p, []byte(got), 0o644); err != nil {
			t.Fatal(err)
		}
		return
	}
	want, err := os.ReadFile(p)
	if err != nil {
		t.Fatalf("%v (run go test ./internal/tui -update)", err)
	}
	if string(want) != got {
		t.Fatalf("%s mismatch\n--- want\n%s\n--- got\n%s", name, want, got)
	}
}

// upUntil folds real control.Up events (on fakes) until phase is reached.
// Events arrive 7 s apart; the render clock is 5 s after the last one.
func upUntil(t *testing.T, script []*agent.Status, phase string) UpState {
	var s UpState
	t0 := time.Date(2026, 9, 25, 10, 0, 0, 0, time.UTC)
	for i, e := range ct.Events(script, map[string]bool{"SECURE": true}) {
		at := t0.Add(time.Duration(i) * 7 * time.Second)
		s.ApplyAt(e, at)
		s.At = at.Add(5 * time.Second)
		if e.Phase == phase {
			return s
		}
	}
	return s
}

func TestUpBootContainerHintAndReRentReset(t *testing.T) {
	var s UpState
	t0 := time.Date(2026, 9, 25, 10, 0, 0, 0, time.UTC)
	s.ApplyAt(control.Event{Phase: "create"}, t0)
	s.ApplyAt(control.Event{Phase: "image"}, t0.Add(2*time.Second))
	s.At = t0.Add(20 * time.Second)
	out := ansi.Strip(RenderUp(s, "*"))
	if !strings.Contains(out, "0:18") || !strings.Contains(out, "re-rent at 6:00") || !strings.Contains(out, "2s") {
		t.Fatal(out)
	}
	s.ApplyAt(control.Event{Phase: "create", Detail: "bad host, renting another pod (2/4)"}, t0.Add(6*time.Minute))
	if _, ok := s.took["image"]; ok {
		t.Fatal("re-rent must reset later timers")
	}
}

func TestUpGolden(t *testing.T) {
	golden(t, "up_download", RenderUp(upUntil(t, ct.BootScript(), "download"), "*"))
	golden(t, "up_ready", RenderUp(upUntil(t, ct.BootScript(), "ready"), "*"))
	failed := []*agent.Status{nil, {Stage: agent.StageTunnel}, {Stage: agent.StageFailed, StageDetail: "download: sha256 mismatch"}}
	golden(t, "up_failed", RenderUp(upUntil(t, failed, "failed"), "*"))
}

func snap() control.Snap {
	at := time.Date(2026, 9, 23, 12, 0, 0, 0, time.UTC)
	return control.Snap{
		At:      at,
		Pod:     &provider.Instance{Provider: "runpod", ID: "3ab00vd3rf1575", Status: "RUNNING", CostPerHr: 0.69, StartedAt: at.Add(-90 * time.Minute)},
		Version: &release.Manifest{Version: "2026.09.23-1", GitSHA: "feaaaa5", LlamaImage: "ghcr.io/ggml-org/llama.cpp:server-cuda-b11118"},
		Status: &agent.Status{Stage: agent.StageReady, Model: "q8", Ctx: 8192, IdleS: 312, KillInS: 1488, KillReason: "idle",
			ExpiresAt: at.Add(10 * time.Hour),
			GPU:       &metrics.GPU{Name: "NVIDIA GeForce RTX 5090", VRAMUsedMB: 30112, VRAMTotalMB: 32607, UtilPct: 87},
			Host:      &metrics.Host{Load1: 1.2, Load5: 0.9, Load15: 0.7, MemUsedMB: 9120, MemTotalMB: 450560},
			Llama:     &metrics.Llama{RequestsProcessing: 1, PromptTokensTotal: 182340, GenTokensTotal: 21044, PromptTPS: 2410.5, GenTPS: 48.3}},
	}
}

func TestStatusGolden(t *testing.T) {
	time.Local = time.UTC
	golden(t, "status_ready", RenderStatus(snap()))

	dl := snap()
	dl.Status = &agent.Status{Stage: agent.StageDownload, Model: "q8", Ctx: 8192, KillInS: 43000, KillReason: "expired", ExpiresAt: dl.At.Add(12 * time.Hour),
		Download: agent.DownloadProgress{Bytes: 12357400000, Total: 28595762272, MBps: 51.3}, MetricsFailures: 0}
	golden(t, "status_downloading", RenderStatus(dl))

	na := snap()
	na.Status.GPU, na.Status.Host, na.Status.Llama, na.Status.MetricsFailures = nil, nil, nil, 4
	golden(t, "status_metrics_unavailable", RenderStatus(na))

	golden(t, "status_down", RenderStatus(control.Snap{Down: true, At: dl.At}))
}

func TestUpStateErr(t *testing.T) {
	var s UpState
	s.Apply(control.Event{Phase: "create"})
	s.Apply(control.Event{Phase: "failed", Err: errors.New("boom"), Done: true})
	if s.Phase != "create" || s.Err == nil || !s.Done {
		t.Fatalf("%+v", s)
	}
}

func TestStatusModelKeepsLastSnapOnError(t *testing.T) {
	m := StatusModel{}
	next, _ := m.Update(snapMsg{err: errors.New("runpod 502")})
	m = next.(StatusModel)
	if m.snap != nil || !strings.Contains(ansi.Strip(m.View().Content), "runpod 502") {
		t.Fatal("error-only view expected")
	}
	good := snap()
	next, _ = m.Update(snapMsg{s: good})
	next, _ = next.(StatusModel).Update(snapMsg{err: errors.New("blip")})
	m = next.(StatusModel)
	if m.snap == nil || m.snap.Pod.ID != good.Pod.ID {
		t.Fatal("last good snapshot lost")
	}
	_ = RenderStatus(control.Snap{}) // zero snap must not panic
}
