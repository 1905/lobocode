package watchdog

import (
	"testing"
	"time"
)

var t0 = time.Date(2026, 9, 23, 10, 0, 0, 0, time.UTC)

func min(n int) time.Duration { return time.Duration(n) * time.Minute }

func cfg() Config { return Config{Idle: min(30), ExpiresAt: t0.Add(12 * time.Hour)} }

func TestDecide(t *testing.T) {
	tests := []struct {
		name       string
		ready      bool
		samples    []Sample
		now        time.Time
		wantKill   bool
		wantReason string
		wantIn     time.Duration
	}{
		{"not ready never idles", false, nil, t0.Add(min(300)), false, "expired", 12*time.Hour - min(300)},
		{"idle 29m", true, nil, t0.Add(min(29)), false, "idle", min(1)},
		{"idle 31m", true, nil, t0.Add(min(31)), true, "idle", 0},
		{"busy keeps alive", true, []Sample{{At: t0.Add(min(25)), OK: true, Processing: 1}}, t0.Add(min(31)), false, "idle", min(24)},
		{"deferred keeps alive", true, []Sample{{At: t0.Add(min(25)), OK: true, Deferred: 2}}, t0.Add(min(31)), false, "idle", min(24)},
		{"tokens moved reset", true, []Sample{
			{At: t0.Add(min(1)), OK: true, PromptTokens: 10},
			{At: t0.Add(min(20)), OK: true, PromptTokens: 50},
		}, t0.Add(min(40)), false, "idle", min(10)},
		{"failed samples add nothing", true, []Sample{
			{At: t0.Add(min(5)), OK: false}, {At: t0.Add(min(20)), OK: false}, {At: t0.Add(min(30)), OK: false},
		}, t0.Add(min(31)), true, "idle", 0},
		{"expired while busy", true, []Sample{{At: t0.Add(12*time.Hour - time.Second), OK: true, Processing: 1}}, t0.Add(12 * time.Hour), true, "expired", 0},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			s := NewState(t0)
			if tt.ready {
				s.SetReady(t0)
			}
			for _, sm := range tt.samples {
				s.Observe(sm)
			}
			d := s.Decide(tt.now, cfg())
			if d.Kill != tt.wantKill || d.Reason != tt.wantReason || d.KillIn != tt.wantIn {
				t.Fatalf("got %+v", d)
			}
		})
	}
}

func TestExpiredAtStart(t *testing.T) {
	s := NewState(t0)
	d := s.Decide(t0, Config{Idle: min(30), ExpiresAt: t0.Add(-time.Minute)})
	if !d.Kill || d.Reason != "expired" {
		t.Fatalf("%+v", d)
	}
}

func TestFailedSamples(t *testing.T) {
	s := NewState(t0)
	s.Observe(Sample{OK: false})
	s.Observe(Sample{OK: false})
	if s.FailedSamples() != 2 {
		t.Fatal(s.FailedSamples())
	}
	s.Observe(Sample{OK: true})
	if s.FailedSamples() != 0 {
		t.Fatal(s.FailedSamples())
	}
}
