package control_test

import (
	"context"
	"errors"
	"fmt"
	"net/http"
	"net/http/httptest"
	"strings"
	"sync"
	"testing"
	"time"

	"github.com/1905/lobocode/internal/agent"
	. "github.com/1905/lobocode/internal/control"
	ct "github.com/1905/lobocode/internal/control/controltest"
	"github.com/1905/lobocode/internal/provider"
	"github.com/1905/lobocode/internal/release"
	"github.com/1905/lobocode/internal/runpod"
)

func collect(ch <-chan Event) []Event {
	var evs []Event
	for e := range ch {
		evs = append(evs, e)
	}
	return evs
}

func TestUpHappy(t *testing.T) {
	rp := &ct.RunPod{NoCap: map[string]bool{"SECURE": true}}
	t0 := time.Date(2026, 9, 23, 10, 0, 0, 0, time.UTC)
	evs := collect(Up(context.Background(), ct.Deps(rp, &ct.Agent{Script: ct.BootScript()}, func() time.Time { return t0 }), UpOpts{}))
	var phases []string
	for _, e := range evs {
		phases = append(phases, e.Phase)
	}
	if got := strings.Join(phases, ","); got != "create,image,tunnel,download,load,ready" {
		t.Fatal(got)
	}
	if evs[3].Download == nil || evs[3].Download.Bytes != 12357400000 {
		t.Fatal(evs[3])
	}
	last := evs[len(evs)-1]
	if !last.Done || last.Ready == nil || last.Ready.URL != "https://lobo.example.com/v1" || last.Ready.GitSHA != "abc1234" {
		t.Fatalf("%+v", last)
	}
	if len(rp.Created) != 1 || rp.Created[0].CloudType != "COMMUNITY" { // cheapest only by default
		t.Fatal(rp.Created)
	}
	c := rp.Created[0]
	if c.Image != "img:b1" || c.Ctx != 8192 || c.IdleMin != 30 || !c.ExpiresAt.Equal(t0.Add(12*time.Hour)) ||
		c.ReleaseURL != "https://pub-x.r2.dev/releases/lobo-2026.09.23-1.zip" || c.ReleaseSHA256 != "zipsha" ||
		!strings.HasSuffix(c.ModelURL, "/models/Qwen3.5-27B-Uncensored-HauhauCS-Aggressive-Q8_0.gguf") {
		t.Fatalf("%+v", c)
	}
}

func TestUpOverrides(t *testing.T) {
	rp := &ct.RunPod{}
	collect(Up(context.Background(), ct.Deps(rp, &ct.Agent{Script: ct.BootScript()}, nil), UpOpts{Model: "q6", Ctx: 16384, IdleMin: 3, MaxLife: 12 * time.Minute}))
	c := rp.Created[0]
	if c.Model != "q6" || c.Ctx != 16384 || c.IdleMin != 3 || time.Until(c.ExpiresAt) > 13*time.Minute {
		t.Fatalf("%+v", c)
	}
}

func TestUpBakedImage(t *testing.T) {
	for _, tc := range []struct {
		name, cfg, flag, want string
		baked                 bool
	}{
		{"config", "ghcr.io/1905/lobocode@sha256:cfg", "", "ghcr.io/1905/lobocode@sha256:cfg", true},
		{"flag wins", "ghcr.io/1905/lobocode@sha256:cfg", "ghcr.io/1905/lobocode:v9", "ghcr.io/1905/lobocode:v9", true},
		{"none = release zip", "", "", "img:b1", false},
	} {
		t.Run(tc.name, func(t *testing.T) {
			rp := &ct.RunPod{}
			d := ct.Deps(rp, &ct.Agent{Script: ct.BootScript()}, nil)
			d.Cfg.PodImage = tc.cfg
			evs := collect(Up(context.Background(), d, UpOpts{Image: tc.flag}))
			c := rp.Created[0]
			if c.Image != tc.want || (c.ReleaseURL == "") != tc.baked || (c.ReleaseSHA256 == "") != tc.baked {
				t.Fatalf("%+v", c)
			}
			if tc.baked && !strings.Contains(evs[0].Detail, "release "+tc.want) {
				t.Fatal(evs[0].Detail)
			}
		})
	}
}

// noReleases fails every lookup: a bucket with only the GGUF, no releases/latest.json.
type noReleases struct{}

func (noReleases) Resolve(context.Context, string) (release.Resolved, error) {
	return release.Resolved{}, errors.New("releases/latest.json: HTTP 404")
}

func TestUpBakedImageNeedsNoReleaseManifest(t *testing.T) {
	rp := &ct.RunPod{}
	d := ct.Deps(rp, &ct.Agent{Script: ct.BootScript()}, nil)
	d.Releases = noReleases{}
	evs := collect(Up(context.Background(), d, UpOpts{Image: "ghcr.io/1905/lobocode:v9"}))
	if last := evs[len(evs)-1]; last.Ready == nil {
		t.Fatalf("%+v", last)
	}
	c := rp.Created[0]
	if c.Image != "ghcr.io/1905/lobocode:v9" || c.Model != release.DefaultModel || c.Ctx != release.DefaultDefaults.Ctx || c.ReleaseURL != "" {
		t.Fatalf("%+v", c)
	}
}

func TestUpImageAndReleaseConflict(t *testing.T) {
	rp := &ct.RunPod{}
	evs := collect(Up(context.Background(), ct.Deps(rp, &ct.Agent{Script: ct.BootScript()}, nil), UpOpts{Image: "img", Release: "2026.09.23-1"}))
	if last := evs[len(evs)-1]; last.Err == nil || len(rp.Created) != 0 {
		t.Fatalf("%+v", last)
	}
}

func TestUpAlreadyRunning(t *testing.T) {
	rp := &ct.RunPod{Pods: []runpod.Pod{{ID: "old", Name: "lobo"}, {ID: "x", Name: "other-project"}}}
	evs := collect(Up(context.Background(), ct.Deps(rp, &ct.Agent{}, nil), UpOpts{}))
	if len(evs) != 1 || evs[0].Err == nil || !strings.Contains(evs[0].Err.Error(), "already running") || len(rp.Created) != 0 {
		t.Fatal(evs)
	}
}

func TestUpNoCapacityAnywhere(t *testing.T) {
	rp := &ct.RunPod{NoCap: map[string]bool{"COMMUNITY": true, "SECURE": true}}
	evs := collect(Up(context.Background(), ct.Deps(rp, &ct.Agent{}, nil), UpOpts{}))
	if last := evs[len(evs)-1]; !errors.Is(last.Err, runpod.ErrNoCapacity) {
		t.Fatal(evs)
	}
}

func TestUpAgentFailed(t *testing.T) {
	ag := &ct.Agent{Script: []*agent.Status{nil, {Stage: agent.StageFailed, StageDetail: "download: sha256 mismatch"}}}
	evs := collect(Up(context.Background(), ct.Deps(&ct.RunPod{}, ag, nil), UpOpts{}))
	last := evs[len(evs)-1]
	if last.Phase != "failed" || !strings.Contains(last.Err.Error(), "sha256 mismatch") || !strings.Contains(last.Err.Error(), "last log line") {
		t.Fatalf("%+v", last)
	}
}

func TestUpTimeout(t *testing.T) {
	rp := &ct.RunPod{}
	t0 := time.Now()
	n := 0
	clock := func() time.Time { n++; return t0.Add(time.Duration(n) * time.Minute) }
	evs := collect(Up(context.Background(), ct.Deps(rp, &ct.Agent{Script: []*agent.Status{nil}}, clock), UpOpts{Timeout: 5 * time.Minute}))
	last := evs[len(evs)-1]
	if last.Phase != "terminated" || last.Err == nil || len(rp.Deleted) != 1 {
		t.Fatalf("%+v %v", last, rp.Deleted)
	}
}

func TestDownAndSnapshot(t *testing.T) {
	t0 := time.Date(2026, 9, 23, 10, 0, 0, 0, time.UTC)
	started := runpod.Time{Time: t0.Add(-2 * time.Hour)}
	rp := &ct.RunPod{Pods: []runpod.Pod{{ID: "a", Name: "lobo", CostPerHr: 0.5, LastStartedAt: started}, {ID: "keep", Name: "snofs"}}}
	d := ct.Deps(rp, &ct.Agent{}, func() time.Time { return t0 })
	s, err := Snapshot(context.Background(), d)
	if err != nil || s.Down || s.Pod.ID != "a" || s.Status != nil {
		t.Fatal(s, err)
	}
	spent, err := Down(context.Background(), d)
	if err != nil || spent != 1.0 || len(rp.Deleted) != 1 || rp.Deleted[0] != "a" {
		t.Fatal(spent, err, rp.Deleted)
	}
	if s, _ = Snapshot(context.Background(), d); !s.Down {
		t.Fatal("want down")
	}
}

func TestEventsHelper(t *testing.T) {
	evs := ct.Events(ct.BootScript(), nil)
	if last := evs[len(evs)-1]; last.Ready == nil || last.Ready.Elapsed <= 0 {
		t.Fatalf("%+v", last)
	}
}

func TestUpReplacesPodWithBrokenGPU(t *testing.T) {
	rp := &ct.RunPod{}
	script := append([]*agent.Status{nil, {Stage: agent.StageFailed, StageDetail: "gpu: llama-server sees no CUDA device on this host"}}, ct.BootScript()...)
	evs := collect(Up(context.Background(), ct.Deps(rp, &ct.Agent{Script: script}, nil), UpOpts{}))
	last := evs[len(evs)-1]
	if last.Ready == nil || len(rp.Created) != 2 || len(rp.Deleted) != 1 {
		t.Fatalf("last %+v created %d deleted %v", last, len(rp.Created), rp.Deleted)
	}
}

func TestUpGivesUpAfterFourBadHosts(t *testing.T) {
	rp := &ct.RunPod{}
	bad := &agent.Status{Stage: agent.StageFailed, StageDetail: "gpu: no CUDA"}
	evs := collect(Up(context.Background(), ct.Deps(rp, &ct.Agent{Script: []*agent.Status{bad}}, nil), UpOpts{}))
	if last := evs[len(evs)-1]; last.Err == nil || !strings.Contains(last.Err.Error(), "gave up") {
		t.Fatalf("want gave-up error, got %+v", last)
	}
	clouds := []string{}
	for _, c := range rp.Created {
		clouds = append(clouds, c.CloudType)
	}
	if strings.Join(clouds, ",") != "COMMUNITY,COMMUNITY,COMMUNITY,COMMUNITY" || len(rp.Deleted) != 4 {
		t.Fatalf("clouds %v deleted %d", clouds, len(rp.Deleted))
	}
	if len(rp.Created) != 4 {
		t.Fatalf("created %d deleted %d", len(rp.Created), len(rp.Deleted))
	}
}

func TestUpReportsExpiryReason(t *testing.T) {
	ag := &ct.Agent{Script: []*agent.Status{nil, {Stage: agent.StageTerminating, KillReason: "expired"}}}
	evs := collect(Up(context.Background(), ct.Deps(&ct.RunPod{}, ag, nil), UpOpts{}))
	if last := evs[len(evs)-1]; last.Err == nil || !strings.Contains(last.Err.Error(), "watchdog: expired") {
		t.Fatalf("%+v", last)
	}
}

func TestUpRejectsBadOptionsBeforeRenting(t *testing.T) {
	for _, o := range []UpOpts{{Ctx: -1}, {IdleMin: -1}, {MaxLife: -time.Hour}} {
		rp := &ct.RunPod{}
		evs := collect(Up(context.Background(), ct.Deps(rp, &ct.Agent{}, nil), o))
		if last := evs[len(evs)-1]; last.Err == nil || !strings.Contains(last.Err.Error(), "bad options") || len(rp.Created) != 0 {
			t.Fatalf("%+v: %+v created %d", o, last, len(rp.Created))
		}
	}
}

func TestUpRetriesSlowHostAndIgnoresPollBlip(t *testing.T) {
	rp := &ct.RunPod{}
	script := []*agent.Status{nil, {Stage: agent.StageDownload}, nil, {Stage: agent.StageFailed, StageDetail: "download: host: download too slow"}}
	script = append(script, ct.BootScript()...)
	evs := collect(Up(context.Background(), ct.Deps(rp, &ct.Agent{Script: script}, nil), UpOpts{}))
	var phases []string
	for _, e := range evs {
		phases = append(phases, e.Phase)
	}
	if got := strings.Join(phases, ","); !strings.HasPrefix(got, "create,image,download,image,create,create,image") || evs[len(evs)-1].Ready == nil || len(rp.Deleted) != 1 {
		t.Fatalf("%s deleted %v", got, rp.Deleted)
	}
}

func TestUpReplacesPodWhoseContainerNeverStarts(t *testing.T) {
	rp := &ct.RunPod{}
	t0 := time.Now()
	n := 0
	clock := func() time.Time { n++; return t0.Add(time.Duration(n) * 30 * time.Second) }
	script := make([]*agent.Status, 20) // nil = agent never answers for the first pod
	script = append(script, ct.BootScript()...)
	evs := collect(Up(context.Background(), ct.Deps(rp, &ct.Agent{Script: script}, clock), UpOpts{Timeout: time.Hour}))
	if last := evs[len(evs)-1]; last.Ready == nil || len(rp.Deleted) < 1 || len(rp.Created) != len(rp.Deleted)+1 {
		t.Fatalf("last %+v created %d deleted %v", last, len(rp.Created), rp.Deleted)
	}
}

func TestUpStepsDownNetworkTiers(t *testing.T) {
	rp := &ct.RunPod{MaxMbps: 2500}
	evs := collect(Up(context.Background(), ct.Deps(rp, &ct.Agent{Script: ct.BootScript()}, nil), UpOpts{}))
	var got []string
	for _, c := range rp.Created {
		got = append(got, fmt.Sprintf("%s@%.0f", c.CloudType, c.MinDownloadMbps))
	}
	// 10000 and 5000 don't exist, 2500 does.
	if strings.Join(got, ",") != "COMMUNITY@10000,COMMUNITY@5000,COMMUNITY@2500" || evs[len(evs)-1].Ready == nil || !strings.Contains(evs[len(evs)-1].Ready.Detail, "≥2500") {
		t.Fatalf("%v %+v", got, evs[len(evs)-1])
	}
}

// Default = cheapest only: no community capacity must not fall back to SECURE.
func TestUpCommunityOnlyByDefault(t *testing.T) {
	rp := &ct.RunPod{NoCap: map[string]bool{"COMMUNITY": true}}
	evs := collect(Up(context.Background(), ct.Deps(rp, &ct.Agent{Script: ct.BootScript()}, nil), UpOpts{}))
	if last := evs[len(evs)-1]; last.Err == nil {
		t.Fatalf("want no-capacity error, got %+v", last)
	}
	for _, c := range rp.Created {
		if c.CloudType != "COMMUNITY" {
			t.Fatalf("tried %s: default must never fall back to SECURE", c.CloudType)
		}
	}
}

func TestUpSecureAllTiersBeforeCommunity(t *testing.T) {
	rp := &ct.RunPod{NoCap: map[string]bool{"SECURE": true}, MaxMbps: 5000}
	collect(Up(context.Background(), ct.Deps(rp, &ct.Agent{Script: ct.BootScript()}, nil), UpOpts{Cloud: "secure"}))
	var got []string
	for _, c := range rp.Created {
		got = append(got, fmt.Sprintf("%s@%.0f", c.CloudType, c.MinDownloadMbps))
	}
	if strings.Join(got, ",") != "SECURE@10000,SECURE@5000,SECURE@2500,SECURE@1000,SECURE@0,COMMUNITY@10000,COMMUNITY@5000" {
		t.Fatal(got)
	}
}

func TestUpR2GetsFeeshFallback(t *testing.T) {
	rp := &ct.RunPod{}
	d := ct.Deps(rp, &ct.Agent{Script: ct.BootScript()}, nil)
	d.Cfg.FeeshHTTPURL = "http://feesh:8088/tok"
	d.Presign = fakePresign{}
	collect(Up(context.Background(), d, UpOpts{Source: "r2"}))
	c := rp.Created[0]
	if !strings.HasPrefix(c.ModelURL, "https://signed/") || c.ModelFallback != "http://feesh:8088/tok/Qwen3.5-27B-Uncensored-HauhauCS-Aggressive-Q8_0.gguf" || c.DLConns != 32 {
		t.Fatalf("%+v", c)
	}
	rp2 := &ct.RunPod{}
	d.Providers = map[string]provider.Provider{"runpod": runpod.Provider{C: rp2}}
	collect(Up(context.Background(), d, UpOpts{Source: "feesh"}))
	if c := rp2.Created[0]; c.ModelURL != "http://feesh:8088/tok/Qwen3.5-27B-Uncensored-HauhauCS-Aggressive-Q8_0.gguf" || c.ModelFallback != "" {
		t.Fatalf("%+v", c)
	}
}

type fakePresign struct{}

func (fakePresign) PresignGet(_ context.Context, key string, _ time.Duration) (string, error) {
	return "https://signed/" + key + "?X-Amz-Signature=x", nil
}

func TestUpMinMBps(t *testing.T) {
	got := func(cfgMin string, o UpOpts) int {
		rp := &ct.RunPod{}
		d := ct.Deps(rp, &ct.Agent{Script: ct.BootScript()}, nil)
		d.Cfg.MinMBps = cfgMin
		collect(Up(context.Background(), d, o))
		return rp.Created[0].MinMBps
	}
	if a, b, c := got("", UpOpts{}), got("250", UpOpts{}), got("250", UpOpts{MinMBps: 50}); a != 100 || b != 250 || c != 50 {
		t.Fatalf("default %d, .env %d, flag %d", a, b, c)
	}
}

// After a re-rent the deleted pod's agent may still answer through the shared tunnel for a moment.
// Its old "failed: gpu" status must not get the new pod deleted too.
func TestUpIgnoresPreviousPodStatusAfterReRent(t *testing.T) {
	rp := &ct.RunPod{}
	old := &agent.Status{Stage: agent.StageFailed, StageDetail: "gpu: no CUDA", UptimeS: 90}
	script := append([]*agent.Status{nil, old, old, old}, ct.BootScript()...)
	evs := collect(Up(context.Background(), ct.Deps(rp, &ct.Agent{Script: script}, nil), UpOpts{}))
	if last := evs[len(evs)-1]; last.Ready == nil || len(rp.Created) != 2 || len(rp.Deleted) != 1 {
		t.Fatalf("last %+v created %d deleted %v", last, len(rp.Created), rp.Deleted)
	}
}

// goneRunPod: the pod deleted itself (e.g. cloudflared died → agent failed → self-terminate).
type goneRunPod struct{ *ct.RunPod }

func (goneRunPod) Get(context.Context, string) (runpod.Pod, error) {
	return runpod.Pod{}, runpod.ErrNotFound
}

// Agent answered once, then went silent and the pod vanished: fail fast, don't wait out the timeout.
func TestUpDetectsPodGoneWhileAgentSilent(t *testing.T) {
	t0 := time.Now()
	n := 0
	clock := func() time.Time { n++; return t0.Add(time.Duration(n) * 10 * time.Second) }
	script := []*agent.Status{nil, {Stage: agent.StageDownload, Download: agent.DownloadProgress{Bytes: 1, Total: 10}}, nil}
	d := ct.Deps(&ct.RunPod{}, &ct.Agent{Script: script}, clock)
	d.Providers = map[string]provider.Provider{"runpod": runpod.Provider{C: goneRunPod{&ct.RunPod{}}}}
	evs := collect(Up(context.Background(), d, UpOpts{Timeout: time.Hour}))
	last := evs[len(evs)-1]
	if last.Phase != "failed" || last.Err == nil || !strings.Contains(last.Err.Error(), "is gone") {
		t.Fatalf("%+v", last)
	}
}

// lagRunPod still lists deleted pods for the first few List calls after a delete.
type lagRunPod struct {
	*ct.RunPod
	lag   int
	calls int
}

func (l *lagRunPod) List(ctx context.Context) ([]runpod.Pod, error) {
	l.calls++
	if len(l.Deleted) > 0 && l.lag > 0 {
		l.lag--
		return []runpod.Pod{{ID: "a", Name: "lobo"}}, nil
	}
	return l.RunPod.List(ctx)
}

func TestDownWaitsForListToCatchUp(t *testing.T) {
	rp := &lagRunPod{RunPod: &ct.RunPod{Pods: []runpod.Pod{{ID: "a", Name: "lobo"}}}, lag: 3}
	d := ct.Deps(rp.RunPod, &ct.Agent{}, nil)
	d.Providers = map[string]provider.Provider{"runpod": runpod.Provider{C: rp}}
	start := time.Now()
	if _, err := Down(context.Background(), d); err != nil {
		t.Fatal(err, rp.calls)
	}
	if time.Since(start) < 3*d.Poll {
		t.Fatal("no wait between list checks")
	}
}

// fakeProv is a minimal second provider (stands in for Vast in control tests).
type fakeProv struct {
	mu      sync.Mutex
	name    string
	running []provider.Instance
	rented  int
	deleted []string
	listErr error
}

func (f *fakeProv) Name() string      { return f.name }
func (f *fakeProv) Replaceable() bool { return true }
func (f *fakeProv) Rent(_ context.Context, _ provider.CreateOpts, _ func(string)) (provider.Instance, error) {
	f.mu.Lock()
	defer f.mu.Unlock()
	f.rented++
	in := provider.Instance{Provider: f.name, ID: fmt.Sprintf("v%d", f.rented), Status: "created", CostPerHr: 0.73, Detail: "offer 1, 20313 Mbps down"}
	f.running = append(f.running, in)
	return in, nil
}
func (f *fakeProv) List(context.Context) ([]provider.Instance, error) {
	f.mu.Lock()
	defer f.mu.Unlock()
	if f.listErr != nil {
		return nil, f.listErr
	}
	return append([]provider.Instance(nil), f.running...), nil
}
func (f *fakeProv) Get(_ context.Context, id string) (provider.Instance, error) {
	f.mu.Lock()
	defer f.mu.Unlock()
	for _, in := range f.running {
		if in.ID == id {
			return in, nil
		}
	}
	return provider.Instance{}, provider.ErrNotFound
}
func (f *fakeProv) Delete(_ context.Context, id string) error {
	f.mu.Lock()
	defer f.mu.Unlock()
	f.deleted = append(f.deleted, id)
	var keep []provider.Instance
	for _, in := range f.running {
		if in.ID != id {
			keep = append(keep, in)
		}
	}
	f.running = keep
	return nil
}

func TestUpOnVastAndDownAcrossProviders(t *testing.T) {
	rp := &ct.RunPod{}
	fv := &fakeProv{name: "vast"}
	d := ct.Deps(rp, &ct.Agent{Script: ct.BootScript()}, nil)
	d.Providers["vast"] = fv
	evs := collect(Up(context.Background(), d, UpOpts{Provider: "vast"}))
	last := evs[len(evs)-1]
	if last.Ready == nil || last.Ready.Provider != "vast" || fv.rented != 1 || len(rp.Created) != 0 {
		t.Fatalf("%+v rented %d runpod creates %d", last, fv.rented, len(rp.Created))
	}
	// a second up on RunPod must see the Vast instance and refuse
	evs = collect(Up(context.Background(), d, UpOpts{Provider: "runpod"}))
	if e := evs[len(evs)-1]; e.Err == nil || !strings.Contains(e.Err.Error(), "already running: vast") {
		t.Fatalf("%+v", e)
	}
	rp.Pods = append(rp.Pods, runpod.Pod{ID: "p1", Name: "lobo"})
	if _, err := Down(context.Background(), d); err != nil {
		t.Fatal(err)
	}
	if len(fv.deleted) != 1 || len(rp.Deleted) != 1 {
		t.Fatalf("vast deleted %v runpod deleted %v", fv.deleted, rp.Deleted)
	}
}

func TestUpUnconfiguredProvider(t *testing.T) {
	d := ct.Deps(&ct.RunPod{}, &ct.Agent{}, nil)
	evs := collect(Up(context.Background(), d, UpOpts{Provider: "vast"}))
	if e := evs[len(evs)-1]; e.Err == nil || !strings.Contains(e.Err.Error(), `provider "vast" is not configured`) {
		t.Fatalf("%+v", e)
	}
}

func TestDownKeepsGoingWhenOneProviderFails(t *testing.T) {
	rp := &ct.RunPod{Pods: []runpod.Pod{{ID: "p1", Name: "lobo"}}}
	d := ct.Deps(rp, &ct.Agent{}, nil)
	d.Poll = time.Millisecond
	d.Providers["vast"] = &fakeProv{name: "vast", listErr: errors.New("vast api down")}
	_, err := Down(context.Background(), d)
	if err == nil || !strings.Contains(err.Error(), "vast api down") {
		t.Fatalf("want the vast error reported, got %v", err)
	}
	if len(rp.Deleted) != 1 {
		t.Fatalf("runpod pod not deleted: %v", rp.Deleted)
	}
}

// A failed status from another boot (old pod still on the shared tunnel) must not delete this pod.
func TestUpIgnoresOtherBootStatus(t *testing.T) {
	rp := &ct.RunPod{}
	script := append([]*agent.Status{{Stage: agent.StageFailed, StageDetail: "gpu: no CUDA", BootID: "old-pod"}}, ct.BootScript()...)
	evs := collect(Up(context.Background(), ct.Deps(rp, &ct.Agent{Script: script}, nil), UpOpts{}))
	if last := evs[len(evs)-1]; last.Ready == nil || len(rp.Deleted) != 0 || len(rp.Created) != 1 {
		t.Fatalf("last %+v deleted %v created %d", last, rp.Deleted, len(rp.Created))
	}
}

// Failed polls between identical download snapshots are not progress: the stall timeout must still fire.
func TestUpFailedPollsDoNotResetStall(t *testing.T) {
	rp := &ct.RunPod{}
	dl := &agent.Status{Stage: agent.StageDownload, Download: agent.DownloadProgress{Bytes: 5, Total: 100}}
	var script []*agent.Status
	for i := 0; i < 500; i++ {
		script = append(script, dl, nil)
	}
	script = append(script, &agent.Status{Stage: agent.StageReady}) // reached only if the stall timer kept resetting
	t0 := time.Date(2026, 9, 23, 10, 0, 0, 0, time.UTC)
	var mu sync.Mutex
	n := 0
	clock := func() time.Time { mu.Lock(); defer mu.Unlock(); n++; return t0.Add(time.Duration(n) * time.Second) }
	evs := collect(Up(context.Background(), ct.Deps(rp, &ct.Agent{Script: script}, clock), UpOpts{Timeout: 30 * time.Second}))
	if last := evs[len(evs)-1]; last.Phase != "terminated" || len(rp.Deleted) != 1 {
		t.Fatalf("want stall termination, got %+v deleted %v", last, rp.Deleted)
	}
}

// localDeps: RunPod configured too (its agent must stay untouched), no release manifest anywhere, a local provider.
func localDeps(script []*agent.Status) (Deps, *ct.RunPod, *ct.Agent, *ct.Local, *ct.Agent) {
	rp, cloud, lp, la := &ct.RunPod{}, &ct.Agent{Script: ct.BootScript()}, &ct.Local{}, &ct.Agent{Script: script}
	d := ct.Deps(rp, cloud, nil)
	d.Releases = noReleases{}
	d.Providers["local"] = lp
	d.NewAgent = func(base string) AgentAPI {
		if base == ct.LocalAgentURL {
			return la
		}
		return cloud
	}
	return d, rp, cloud, lp, la
}

func TestUpLocal(t *testing.T) {
	d, rp, cloud, lp, la := localDeps(ct.LocalBootScript())
	d.Cfg.PodImage = "ghcr.io/1905/lobocode:v9" // cloud-only setting: ignored locally
	evs := collect(Up(context.Background(), d, UpOpts{Provider: "local", Model: "q6"}))
	var phases []string
	for _, e := range evs {
		phases = append(phases, e.Phase)
	}
	if got := strings.Join(phases, ","); got != "create,gpu,download,load,ready" {
		t.Fatal(got)
	}
	last := evs[len(evs)-1]
	if last.Ready == nil || last.Ready.URL != "http://127.0.0.1:8931/v1" || last.Ready.Provider != "local" || last.Ready.CostPerHr != 0 {
		t.Fatalf("%+v", last)
	}
	if len(lp.Created) != 1 || len(rp.Created) != 0 {
		t.Fatalf("local %d runpod %d", len(lp.Created), len(rp.Created))
	}
	c := lp.Created[0]
	if c.Model != "q6" || c.Ctx != release.DefaultDefaults.Ctx || c.IdleMin != release.DefaultDefaults.IdleMin || c.LoboAPIKey != "sk" ||
		c.BootID == "" || c.ModelURL != "" || c.Image != "" || c.ReleaseURL != "" || c.CFTunnelToken != "" {
		t.Fatalf("%+v", c)
	}
	if cloud.Calls() != 0 || la.Calls() == 0 {
		t.Fatalf("cloud agent calls %d, local %d", cloud.Calls(), la.Calls())
	}
}

// Locally a "gpu: " failure is not a bad host to replace: no re-rent, the run is stopped and reported.
func TestUpLocalFailureStopsRun(t *testing.T) {
	for _, detail := range []string{"gpu: q8 needs 29.1 GB, this Mac allows ~24 GB to the GPU", "download: sha256 mismatch"} {
		t.Run(detail, func(t *testing.T) {
			d, _, _, lp, _ := localDeps([]*agent.Status{{Stage: agent.StageFailed, StageDetail: detail}})
			evs := collect(Up(context.Background(), d, UpOpts{Provider: "local"}))
			last := evs[len(evs)-1]
			if last.Phase != "failed" || last.Err == nil || !strings.Contains(last.Err.Error(), detail) || !strings.Contains(last.Err.Error(), "last log line") {
				t.Fatalf("%+v", last)
			}
			if len(lp.Created) != 1 || len(lp.Deleted) != 1 {
				t.Fatalf("rented %d deleted %v", len(lp.Created), lp.Deleted)
			}
		})
	}
}

func TestSnapshotLocal(t *testing.T) {
	d, _, cloud, lp, la := localDeps([]*agent.Status{{Stage: agent.StageReady, Model: "q6"}})
	lp.Running = []provider.Instance{{Provider: "local", ID: "4242", Status: "running"}}
	s, err := Snapshot(context.Background(), d)
	if err != nil || s.Down || s.Pod.Provider != "local" || s.Status == nil || s.Status.Model != "q6" || s.Version == nil {
		t.Fatalf("%+v %v", s, err)
	}
	if cloud.Calls() != 0 || la.Calls() != 2 {
		t.Fatalf("cloud agent calls %d, local %d", cloud.Calls(), la.Calls())
	}
}

func TestTarget(t *testing.T) {
	tests := []struct {
		name     string
		local    bool
		pod      bool
		vastErr  bool // a failing cloud listing must not hide a local run
		domain   string
		wantURL  string
		wantErr  string
		wantLAgt bool
	}{
		{name: "local runs", local: true, domain: "lobo.example.com", wantURL: "http://127.0.0.1:8931/v1", wantLAgt: true},
		{name: "local runs, no domain", local: true, wantURL: "http://127.0.0.1:8931/v1", wantLAgt: true},
		{name: "local runs, vast list fails", local: true, vastErr: true, wantURL: "http://127.0.0.1:8931/v1", wantLAgt: true},
		{name: "nothing runs, vast list fails", vastErr: true, domain: "lobo.example.com", wantErr: "vast: boom"},
		{name: "pod runs", pod: true, domain: "lobo.example.com", wantURL: "https://lobo.example.com/v1"},
		{name: "nothing runs", domain: "lobo.example.com", wantURL: "https://lobo.example.com/v1"},
		{name: "nothing runs, no domain", wantErr: "LOBO_DOMAIN"},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			d, rp, cloud, lp, la := localDeps(nil)
			d.Cfg.Domain = tt.domain
			if tt.local {
				lp.Running = []provider.Instance{{Provider: "local", ID: "4242"}}
			}
			if tt.pod {
				rp.Pods = []runpod.Pod{{ID: "p1", Name: "lobo"}}
			}
			if tt.vastErr {
				d.Providers["vast"] = &fakeProv{name: "vast", listErr: errors.New("boom")}
			}
			ag, url, err := Target(context.Background(), d)
			if tt.wantErr != "" {
				if err == nil || !strings.Contains(err.Error(), tt.wantErr) {
					t.Fatalf("err %v, want %q", err, tt.wantErr)
				}
				return
			}
			if err != nil || url != tt.wantURL {
				t.Fatalf("url %q err %v", url, err)
			}
			want := AgentAPI(cloud)
			if tt.wantLAgt {
				want = la
			}
			if ag != want {
				t.Fatalf("wrong agent (want local %v)", tt.wantLAgt)
			}
		})
	}
}

// NewHTTPAgentURL talks to a plain base URL (the local supervisor); logs carry the key.
func TestHTTPAgentURL(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		switch r.URL.Path {
		case "/api/version":
			fmt.Fprint(w, `{"version":"dev","git_sha":"abc"}`)
		case "/api/logs":
			if r.Header.Get("Authorization") != "Bearer sk" {
				w.WriteHeader(http.StatusUnauthorized)
				return
			}
			fmt.Fprint(w, "line")
		default:
			w.WriteHeader(http.StatusNotFound)
		}
	}))
	defer srv.Close()
	a := NewHTTPAgentURL(srv.URL+"/", "sk")
	if v, err := a.Version(context.Background()); err != nil || v.Version != "dev" || v.GitSHA != "abc" {
		t.Fatalf("%+v %v", v, err)
	}
	if s, err := a.Logs(context.Background(), 5); err != nil || s != "line" {
		t.Fatalf("%q %v", s, err)
	}
	if NewHTTPAgent("lobo.example.com", "sk").Base != "https://lobo.example.com" {
		t.Fatal(NewHTTPAgent("lobo.example.com", "sk").Base)
	}
}

// A cloud pod with no LOBO_DOMAIN: show the pod, skip the agent (it has no address), no error.
func TestSnapshotNoDomain(t *testing.T) {
	tests := []struct {
		name      string
		domain    string
		wantCalls int
	}{
		{name: "domain set", domain: "lobo.example.com", wantCalls: 2},
		{name: "no domain", wantCalls: 0},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			rp := &ct.RunPod{Pods: []runpod.Pod{{ID: "a", Name: "lobo"}}}
			ag := &ct.Agent{Script: []*agent.Status{{Stage: agent.StageReady}}}
			d := ct.Deps(rp, ag, nil)
			d.Cfg.Domain = tt.domain
			s, err := Snapshot(context.Background(), d)
			if err != nil || s.Down || s.Pod == nil || s.Pod.ID != "a" {
				t.Fatalf("%+v %v", s, err)
			}
			if ag.Calls() != tt.wantCalls {
				t.Fatalf("agent calls %d want %d", ag.Calls(), tt.wantCalls)
			}
		})
	}
}
