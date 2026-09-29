// Package controltest has fakes for control.Deps, shared by control and tui tests.
package controltest

import (
	"context"
	"errors"
	"sync"
	"time"

	"github.com/1905/lobocode/internal/agent"
	"github.com/1905/lobocode/internal/config"
	"github.com/1905/lobocode/internal/control"
	"github.com/1905/lobocode/internal/provider"
	"github.com/1905/lobocode/internal/release"
	"github.com/1905/lobocode/internal/runpod"
)

// Created is one create call: the neutral options plus RunPod's cloud and network tier.
type Created struct {
	provider.CreateOpts
	CloudType       string
	MinDownloadMbps float64
}

// RunPod is a fake RunPod REST client (runpod.API); wrap it in runpod.Provider.
type RunPod struct {
	mu        sync.Mutex
	Pods      []runpod.Pod
	NoCap     map[string]bool
	MaxMbps   float64 // >0: hosts faster than this don't exist
	Created   []Created
	Deleted   []string
	CreateErr error
}

func (f *RunPod) Create(_ context.Context, o provider.CreateOpts, cloud string, mbps float64) (runpod.Pod, error) {
	f.mu.Lock()
	defer f.mu.Unlock()
	f.Created = append(f.Created, Created{o, cloud, mbps})
	if f.NoCap[cloud] || (f.MaxMbps > 0 && mbps > f.MaxMbps) {
		return runpod.Pod{}, runpod.ErrNoCapacity
	}
	if f.CreateErr != nil {
		return runpod.Pod{}, f.CreateErr
	}
	p := runpod.Pod{ID: "pod1", Name: "lobo", CostPerHr: 0.69, DesiredStatus: "RUNNING"}
	f.Pods = append(f.Pods, p)
	return p, nil
}
func (f *RunPod) List(context.Context) ([]runpod.Pod, error) {
	f.mu.Lock()
	defer f.mu.Unlock()
	return append([]runpod.Pod(nil), f.Pods...), nil
}
func (f *RunPod) Get(_ context.Context, id string) (runpod.Pod, error) {
	f.mu.Lock()
	defer f.mu.Unlock()
	for _, p := range f.Pods {
		if p.ID == id {
			return p, nil
		}
	}
	return runpod.Pod{}, runpod.ErrNotFound
}
func (f *RunPod) Delete(_ context.Context, id string) error {
	f.mu.Lock()
	defer f.mu.Unlock()
	f.Deleted = append(f.Deleted, id)
	var keep []runpod.Pod
	for _, p := range f.Pods {
		if p.ID != id {
			keep = append(keep, p)
		}
	}
	f.Pods = keep
	return nil
}

type Releases struct{ R release.Resolved }

func (f Releases) Resolve(_ context.Context, v string) (release.Resolved, error) {
	if v == "missing" {
		return release.Resolved{}, errors.New("release missing: HTTP 404")
	}
	return f.R, nil
}

// Agent plays a scripted list of statuses, one per poll; nil = not reachable yet.
type Agent struct {
	mu     sync.Mutex
	Script []*agent.Status
	i      int
	calls  int
}

// Calls counts Status, Version and Logs calls.
func (f *Agent) Calls() int {
	f.mu.Lock()
	defer f.mu.Unlock()
	return f.calls
}

func (f *Agent) Status(context.Context) (*agent.Status, error) {
	f.mu.Lock()
	defer f.mu.Unlock()
	f.calls++
	if len(f.Script) == 0 {
		return nil, errors.New("530")
	}
	s := f.Script[f.i]
	if f.i < len(f.Script)-1 {
		f.i++
	}
	if s == nil {
		return nil, errors.New("530")
	}
	return s, nil
}
func (f *Agent) Version(context.Context) (*release.Manifest, error) {
	f.mu.Lock()
	defer f.mu.Unlock()
	f.calls++
	return &release.Manifest{Version: "2026.09.23-1", GitSHA: "abc1234"}, nil
}
func (f *Agent) Logs(context.Context, int) (string, error) {
	f.mu.Lock()
	defer f.mu.Unlock()
	f.calls++
	return "last log line", nil
}

// Local URLs of the fake local provider (the supervisor on its default ports).
const (
	LocalAPIURL   = "http://127.0.0.1:8931/v1"
	LocalAgentURL = "http://127.0.0.1:8932"
)

// Local is a fake local provider (Name "local"): Rent records the options and runs one instance.
// Like the real one, every instance it returns carries the local URLs.
type Local struct {
	mu      sync.Mutex
	Running []provider.Instance
	Created []provider.CreateOpts
	Deleted []string
}

func (f *Local) Name() string      { return "local" }
func (f *Local) Replaceable() bool { return false }
func (f *Local) Rent(_ context.Context, o provider.CreateOpts, _ func(string)) (provider.Instance, error) {
	f.mu.Lock()
	defer f.mu.Unlock()
	f.Created = append(f.Created, o)
	in := provider.Instance{Provider: "local", ID: "4242", Status: "running", Detail: "this Mac, " + o.Model}
	f.Running = append(f.Running, in)
	return localURLs(in), nil
}
func (f *Local) List(context.Context) ([]provider.Instance, error) {
	f.mu.Lock()
	defer f.mu.Unlock()
	var out []provider.Instance
	for _, in := range f.Running {
		out = append(out, localURLs(in))
	}
	return out, nil
}
func (f *Local) Get(_ context.Context, id string) (provider.Instance, error) {
	f.mu.Lock()
	defer f.mu.Unlock()
	for _, in := range f.Running {
		if in.ID == id {
			return localURLs(in), nil
		}
	}
	return provider.Instance{}, provider.ErrNotFound
}
func (f *Local) Delete(_ context.Context, id string) error {
	f.mu.Lock()
	defer f.mu.Unlock()
	f.Deleted = append(f.Deleted, id)
	var keep []provider.Instance
	for _, in := range f.Running {
		if in.ID != id {
			keep = append(keep, in)
		}
	}
	f.Running = keep
	return nil
}

func localURLs(in provider.Instance) provider.Instance {
	in.APIURL, in.AgentURL = LocalAPIURL, LocalAgentURL
	return in
}

// LocalBootScript is a local boot: supervisor answering at once, Metal check, download, load, ready.
func LocalBootScript() []*agent.Status {
	return []*agent.Status{
		{Stage: agent.StageGPU},
		{Stage: agent.StageDownload, Download: agent.DownloadProgress{Bytes: 1 << 30, Total: 22082528352, MBps: 12}},
		{Stage: agent.StageLoad},
		{Stage: agent.StageReady},
	}
}

func Release() release.Resolved {
	return release.Resolved{
		Manifest: release.Manifest{Version: "2026.09.23-1", LlamaImage: "img:b1", Model: release.ModelRef{ID: "q8"},
			Defaults: release.Defaults{Ctx: 8192, IdleMin: 30, MaxHours: 12}},
		ZipKey: "releases/lobo-2026.09.23-1.zip", ZipSHA256: "zipsha",
	}
}

// Deps: RunPod on lobo.example.com; every agent client is ag.
func Deps(rp *RunPod, ag *Agent, clock func() time.Time) control.Deps {
	return control.Deps{Providers: map[string]provider.Provider{"runpod": runpod.Provider{C: rp, Domain: "lobo.example.com"}}, Releases: Releases{Release()},
		NewAgent: func(string) control.AgentAPI { return ag }, Poll: time.Millisecond, Clock: clock,
		Cfg: config.Laptop{Domain: "lobo.example.com", BucketURL: "https://pub-x.r2.dev", LoboAPIKey: "sk", CFTunnelToken: "tok"}}
}

// BootScript is a full boot: agent unreachable twice, then every stage up to ready.
func BootScript() []*agent.Status {
	return []*agent.Status{
		nil, nil,
		{Stage: agent.StageTunnel},
		{Stage: agent.StageDownload, Download: agent.DownloadProgress{Bytes: 12357400000, Total: 28595762272, MBps: 51.3}},
		{Stage: agent.StageLoad},
		{Stage: agent.StageReady},
	}
}

// Events runs control.Up against fakes with a fixed clock (1 s per call) and returns every event.
func Events(script []*agent.Status, noCap map[string]bool) []control.Event {
	rp := &RunPod{NoCap: noCap}
	t0 := time.Date(2026, 9, 23, 10, 0, 0, 0, time.UTC)
	var mu sync.Mutex
	n := 0
	clock := func() time.Time { mu.Lock(); defer mu.Unlock(); n++; return t0.Add(time.Duration(n) * time.Second) }
	var evs []control.Event
	for e := range control.Up(context.Background(), Deps(rp, &Agent{Script: script}, clock), control.UpOpts{}) {
		evs = append(evs, e)
	}
	return evs
}
