//go:build capture

package main

import (
	"bytes"
	"context"
	"encoding/json"
	"flag"
	"os"
	"path/filepath"
	"testing"
	"time"

	"github.com/rs/zerolog"

	"github.com/1905/lobocode/internal/agent"
	"github.com/1905/lobocode/internal/control"
	ct "github.com/1905/lobocode/internal/control/controltest"
	"github.com/1905/lobocode/internal/runpod"
)

var captureOut = flag.String("out", "", "directory for Go CLI reference outputs")

// This opt-in test touches only fake providers and its temporary working directory.
func TestCapture(t *testing.T) {
	if *captureOut == "" {
		t.Fatal("-out is required")
	}
	out, err := filepath.Abs(*captureOut)
	if err != nil {
		t.Fatal(err)
	}
	if err := os.MkdirAll(out, 0o755); err != nil {
		t.Fatal(err)
	}
	write := func(name string, data []byte) {
		t.Helper()
		if err := os.WriteFile(filepath.Join(out, name), data, 0o644); err != nil {
			t.Fatal(err)
		}
	}
	at := time.Date(2026, 9, 25, 10, 0, 0, 0, time.UTC)
	oldLog, oldTime := log, zerolog.TimestampFunc
	zerolog.TimestampFunc = func() time.Time { return at }
	defer func() { log, zerolog.TimestampFunc = oldLog, oldTime }()
	stream := func(evs []control.Event) <-chan control.Event {
		ch := make(chan control.Event, len(evs))
		for _, e := range evs {
			ch <- e
		}
		close(ch)
		return ch
	}
	captureFile := func(which **os.File, f func()) []byte {
		t.Helper()
		file, err := os.CreateTemp(t.TempDir(), "capture")
		if err != nil {
			t.Fatal(err)
		}
		old := *which
		*which = file
		defer func() { *which = old; _ = file.Close() }()
		f()
		data, err := os.ReadFile(file.Name())
		if err != nil {
			t.Fatal(err)
		}
		return data
	}
	failed := []*agent.Status{nil, {Stage: agent.StageTunnel}, {Stage: agent.StageFailed, StageDetail: "download: sha256 mismatch"}}
	for _, item := range []struct {
		name   string
		script []*agent.Status
	}{{"boot", ct.BootScript()}, {"failed", failed}} {
		var buf bytes.Buffer
		log = zerolog.New(zerolog.ConsoleWriter{Out: &buf, NoColor: true, TimeFormat: "15:04:05"}).With().Timestamp().Logger()
		_ = plainUp(stream(ct.Events(item.script, map[string]bool{"SECURE": true})))
		write("plain_up_"+item.name+".txt", buf.Bytes())
		n := 0
		clock := func() time.Time {
			n++
			return time.Date(2026, 9, 23, 10, 0, 0, 0, time.UTC).Add(time.Duration(n) * time.Second)
		}
		events := control.Up(context.Background(), ct.Deps(&ct.RunPod{}, &ct.Agent{Script: item.script}, clock), control.UpOpts{Provider: "runpod", Cloud: "community"})
		write("json_up_"+item.name+".jsonl", captureFile(&os.Stdout, func() { _ = jsonUp(events) }))
	}
	for _, running := range []bool{false, true} {
		rp := &ct.RunPod{}
		name := "status_down.json"
		if running {
			name = "status_running.json"
			rp.Pods = []runpod.Pod{{ID: "pod1", Name: "lobo", DesiredStatus: "RUNNING", CostPerHr: 0.69, CreatedAt: runpod.Time{Time: at.Add(-time.Hour)}}}
		}
		d := ct.Deps(rp, &ct.Agent{Script: []*agent.Status{{Stage: agent.StageReady}}}, func() time.Time { return at })
		s, err := control.Snapshot(context.Background(), d)
		if err != nil {
			t.Fatal(err)
		}
		data, err := json.Marshal(s)
		if err != nil {
			t.Fatal(err)
		}
		write(name, append(data, '\n'))
		if running {
			spent, err := control.Down(context.Background(), d)
			if err != nil {
				t.Fatal(err)
			}
			data, _ := json.Marshal(map[string]float64{"spent_usd": spent})
			write("down_running.json", append(data, '\n'))
		}
	}
	var ready *control.ReadyInfo
	for _, e := range ct.Events(ct.BootScript(), nil) {
		if e.Ready != nil {
			ready = e.Ready
		}
	}
	if ready == nil {
		t.Fatal("missing ready event")
	}
	ready.Timings = &agent.Timings{BootstrapAptS: 2, BootstrapZipS: 1, TunnelS: 3, GPUCheckS: 4, DownloadS: 30, VerifyS: 5, LoadS: 6, DownloadMBps: 200, DownloadConns: 4, DownloadSource: "r2"}
	cwd, err := os.Getwd()
	if err != nil {
		t.Fatal(err)
	}
	if err := os.Chdir(t.TempDir()); err != nil {
		t.Fatal(err)
	}
	defer func() { _ = os.Chdir(cwd) }()
	write("report_boot.txt", captureFile(&os.Stderr, func() { reportBoot(ready, "r2", 4) }))
	data, err := os.ReadFile(bootsLog)
	if err != nil {
		t.Fatal(err)
	}
	var line map[string]any
	if err := json.Unmarshal(data, &line); err != nil {
		t.Fatal(err)
	}
	line["at"] = at
	data, err = json.Marshal(line)
	if err != nil {
		t.Fatal(err)
	}
	write("boots_line.json", append(data, '\n'))
}
