package control

import (
	"context"
	"crypto/rand"
	"encoding/base64"
	"encoding/hex"
	"errors"
	"fmt"
	"os"
	"strconv"
	"strings"
	"time"

	"github.com/1905/lobocode/internal/agent"
	"github.com/1905/lobocode/internal/model"
	"github.com/1905/lobocode/internal/provider"
	"github.com/1905/lobocode/internal/release"
)

// running lists lobo instances on every configured provider.
func running(ctx context.Context, d Deps) ([]provider.Instance, error) {
	out, err := listAll(ctx, d)
	if err != nil {
		return nil, err
	}
	return out, nil
}

// listAll lists every provider; a provider that fails does not hide the others' instances.
// err joins the per-provider failures.
func listAll(ctx context.Context, d Deps) ([]provider.Instance, error) {
	var out []provider.Instance
	var errs []error
	for _, name := range d.providerNames() {
		l, err := d.Providers[name].List(ctx)
		if err != nil {
			errs = append(errs, fmt.Errorf("%s: %w", name, err))
			continue
		}
		out = append(out, l...)
	}
	return out, errors.Join(errs...)
}

// Up rents the pod and streams progress until ready, failure or timeout. The channel closes at the end.
func Up(ctx context.Context, d Deps, o UpOpts) <-chan Event {
	ch := make(chan Event, 16)
	go func() {
		defer close(ch)
		if err := up(ctx, d, o, ch); err != nil {
			ch <- Event{Phase: "failed", Err: err, Done: true}
		}
	}()
	return ch
}

func up(ctx context.Context, d Deps, o UpOpts, ch chan<- Event) error {
	start := d.now()
	if o.Provider == "" {
		o.Provider = "runpod"
	}
	p := d.Providers[o.Provider]
	if p == nil {
		return fmt.Errorf("provider %q is not configured (key missing in the config? run `lobo config`)", o.Provider)
	}
	l, err := running(ctx, d)
	if err != nil {
		return err
	}
	if len(l) > 0 {
		return fmt.Errorf("lobo already running: %s %s (%s). Run `lobo down` first", l[0].Provider, l[0].ID, l[0].Status)
	}
	if o.Image == "" {
		o.Image = d.Cfg.PodImage
	}
	var rel release.Resolved
	if o.Image != "" { // baked agent: the image is the release, no bucket manifest needed
		rel.Manifest = release.Manifest{Version: o.Image, Model: release.ModelRef{ID: release.DefaultModel}, Defaults: release.DefaultDefaults}
	} else if rel, err = d.Releases.Resolve(ctx, o.Release); err != nil {
		return err
	}
	def := rel.Manifest.Defaults
	if o.Model == "" {
		o.Model = rel.Manifest.Model.ID
	}
	if o.Ctx == 0 {
		o.Ctx = def.Ctx
	}
	if o.IdleMin == 0 {
		o.IdleMin = def.IdleMin
	}
	if o.MaxLife == 0 {
		o.MaxLife = time.Duration(def.MaxHours) * time.Hour
	}
	if o.Timeout == 0 {
		o.Timeout = 20 * time.Minute
	}
	if o.Ctx < 512 || o.IdleMin < 1 || o.MaxLife < time.Minute {
		return fmt.Errorf("bad options: ctx %d (min 512), idle-min %d (min 1), max-life %s (min 1m)", o.Ctx, o.IdleMin, o.MaxLife)
	}
	m, err := model.Get(o.Model)
	if err != nil {
		return err
	}
	source := o.Source
	if source == "" {
		switch {
		case d.Cfg.ModelSource == "r2" || (d.Cfg.ModelSource == "" && d.Presign != nil):
			source = "r2" // measured 2026-09-25: 713 MB/s at 32 streams vs 3.5–31 MB/s from the model server
		case strings.HasPrefix(d.Cfg.ModelSource, "ssh://"):
			source = "ssh"
		default:
			source = "public"
		}
	}
	switch {
	case o.Conns == 0 && (source == "r2" || source == "feesh"):
		o.Conns = 32
	case o.Conns == 0:
		o.Conns = 8
	case source == "ssh" && o.Conns > 8:
		o.Conns = 8 // model server sshd MaxStartups 10: more parallel handshakes get dropped and count toward fail2ban
	}
	var sshKey, presigned string
	switch source {
	case "public":
	case "feesh":
		if d.Cfg.FeeshHTTPURL == "" {
			return fmt.Errorf("model source feesh needs LOBO_FEESH_HTTP_URL in the lobo config")
		}
	case "r2":
		if d.Presign == nil {
			return fmt.Errorf("model source r2 needs R2 keys in the lobo config")
		}
		if presigned, err = d.Presign.PresignGet(ctx, "models/"+m.File, 12*time.Hour); err != nil {
			return fmt.Errorf("presign model: %w", err)
		}
	case "ssh":
		if !strings.HasPrefix(d.Cfg.ModelSource, "ssh://") {
			return fmt.Errorf("model source ssh needs LOBO_MODEL_SOURCE=ssh://… in the lobo config")
		}
		b, err := os.ReadFile(d.Cfg.ModelSSHKeyFile)
		if err != nil {
			return fmt.Errorf("model ssh key: %w", err)
		}
		sshKey = base64.StdEncoding.EncodeToString(b)
	default:
		return fmt.Errorf("unknown model source %q (r2, feesh, ssh, public)", source)
	}
	co := provider.CreateOpts{
		Image: rel.Manifest.LlamaImage, ReleaseURL: rel.ZipURL(d.Cfg.BucketURL), ReleaseSHA256: rel.ZipSHA256,
		ModelURL: m.URL(d.Cfg.BucketURL), LoboAPIKey: d.Cfg.LoboAPIKey, CFTunnelToken: d.Cfg.CFTunnelToken,
		Model: m.ID, Ctx: o.Ctx, IdleMin: o.IdleMin, ExpiresAt: start.Add(o.MaxLife), SSHPubKey: o.SSHKey, Cloud: o.Cloud,
	}
	co.DLConns = o.Conns
	co.MinMBps = o.MinMBps
	if co.MinMBps == 0 {
		co.MinMBps = 100
		if n, err := strconv.Atoi(d.Cfg.MinMBps); err == nil && n > 0 {
			co.MinMBps = n
		}
	}
	feesh := ""
	if d.Cfg.FeeshHTTPURL != "" {
		feesh = strings.TrimRight(d.Cfg.FeeshHTTPURL, "/") + "/" + m.File
	}
	if presigned != "" {
		co.ModelURL = presigned
		co.ModelFallback = feesh // R2 itself can be slow (2026-09-25): the pod switches instead of re-renting
	}
	if source == "feesh" {
		co.ModelURL = feesh
	}
	if sshKey != "" { // model from the model server over SSH instead of the public bucket
		co.ModelURL = strings.TrimRight(d.Cfg.ModelSource, "/")
		co.ModelSSHKey, co.ModelHostKey = sshKey, d.Cfg.ModelSSHHostKey
	}
	if o.Image != "" {
		co.Image, co.ReleaseURL, co.ReleaseSHA256 = o.Image, "", ""
	}
	for attempt := 1; ; attempt++ {
		retry, err := boot(ctx, d, p, o, co, attempt, rel.Manifest.Version, m.ID, start, ch)
		if !retry {
			return err
		}
		if attempt >= maxGPURetries {
			return fmt.Errorf("gave up: %d pods in a row landed on bad hosts (all deleted)", attempt)
		}
		ch <- Event{Phase: "create", Detail: fmt.Sprintf("bad host, renting another pod (%d/%d)", attempt+1, maxGPURetries)}
	}
}

// maxGPURetries: pods on a bad host (broken CUDA, VRAM taken, slow network) get replaced this many times in total.
const maxGPURetries = 4

// containerTimeout: no agent answer this long after rent = the host never started the container.
// Normal container start measured 26–97 s (image pull on a fresh host is the slow case).
const containerTimeout = ContainerTimeout

// staleSlack: how much longer than this pod's age an agent's uptime may be before its status is taken
// to come from the previous (deleted) pod.
const staleSlack = 15 * time.Second

// podCheckEvery: how often RunPod is asked whether the pod still exists while its agent is unreachable.
const podCheckEvery = 30 * time.Second

// ContainerTimeout is exported for the TUI hint ("re-rent at 6:00").
const ContainerTimeout = 6 * time.Minute

// retriable failures are about the host, not our code: another pod may work.
func retriable(detail string) bool {
	return strings.HasPrefix(detail, "gpu: ") || strings.Contains(detail, "host: ") // runner prefixes the stage: "download: host: …"
}

// boot rents one instance and follows it. retry=true means the host was bad and the instance was deleted.
func boot(ctx context.Context, d Deps, p provider.Provider, o UpOpts, co provider.CreateOpts, attempt int, relVersion, modelID string, start time.Time, ch chan<- Event) (retry bool, _ error) {
	co.BootID = newBootID()
	pod, err := p.Rent(ctx, co, func(s string) { ch <- Event{Phase: "create", Detail: s} })
	if err != nil {
		return false, fmt.Errorf("rent on %s: %w", p.Name(), err)
	}
	ch <- Event{Phase: "create", Detail: fmt.Sprintf("%s %s, %s, $%.2f/h, release %s, %s ctx %d", p.Name(), pod.ID, pod.Detail, pod.CostPerHr, relVersion, modelID, o.Ctx)}

	poll := d.Poll
	if poll == 0 {
		poll = 3 * time.Second
	}
	lastPhase, lastBytes, lastProgress, seen := "", int64(-1), d.now(), false
	created := d.now()
	lastPodCheck := created
	for {
		st, _ := d.Agent.Status(ctx)
		// All pods share one tunnel hostname: a status with another boot id is some other pod (the one
		// just deleted, or one that `down` is still tearing down). Never act on it.
		if st != nil && st.BootID != "" && st.BootID != co.BootID {
			st = nil
		}
		// Releases before boot ids: fall back to the uptime heuristic. Right after a re-rent the deleted pod's agent can still answer
		// for a few seconds; its "failed: gpu: …" would get this new, healthy pod deleted too. An agent
		// that has been up longer than this pod has existed is the old one. Wall clock (Round(0)): the
		// monotonic clock stops while the laptop sleeps.
		if st != nil && st.BootID == "" && attempt > 1 && !seen && time.Duration(st.UptimeS)*time.Second > d.now().Round(0).Sub(created.Round(0))+staleSlack {
			st = nil
		}
		// Agent gone quiet after it answered (tunnel died, pod deleted itself): ask RunPod whether the pod
		// still exists instead of waiting out the whole no-progress timeout.
		if st == nil && seen && d.now().Sub(lastPodCheck) >= podCheckEvery {
			lastPodCheck = d.now()
			if _, err := p.Get(ctx, pod.ID); errors.Is(err, provider.ErrNotFound) {
				logs, _ := d.Agent.Logs(ctx, 20)
				ch <- Event{Phase: "failed", Detail: "pod is gone", Err: fmt.Errorf("pod %s is gone (deleted) while the agent was unreachable in phase %s\n%s", pod.ID, lastPhase, strings.TrimSpace(logs)), Done: true}
				return false, nil
			}
		}
		phase := "image"
		if st != nil {
			phase = string(st.Stage)
			seen = true
		} else if seen {
			phase = lastPhase // one failed poll after the agent answered is not a phase change
		}
		var dl *agent.DownloadProgress
		if st != nil && st.Stage == agent.StageDownload {
			p := st.Download
			dl = &p
		}
		bytes := int64(0)
		switch {
		case dl != nil:
			bytes = dl.Bytes
		case st == nil && seen:
			bytes = lastBytes // a failed poll is not progress: keep the stall timer running
		}
		if phase != lastPhase || bytes != lastBytes {
			lastProgress = d.now()
			ev := Event{Phase: phase, Download: dl}
			if st != nil {
				ev.Detail = st.StageDetail
			}
			switch phase {
			case string(agent.StageReady):
				ver, _ := d.Agent.Version(ctx)
				tim := st.Timings
				ri := &ReadyInfo{URL: "https://" + d.Cfg.Domain + "/v1", CostPerHr: pod.CostPerHr, Elapsed: d.now().Sub(start), Version: relVersion,
					PodID: pod.ID, Provider: p.Name(), Detail: pod.Detail, Attempts: attempt, Timings: &tim, HostDownloadMbps: pod.HostDownloadMbps}
				if !pod.StartedAt.IsZero() && !tim.ContainerStartedAt.IsZero() {
					ri.RentS = tim.ContainerStartedAt.Sub(pod.StartedAt).Seconds()
				}
				if ver != nil {
					ri.Version, ri.GitSHA = ver.Version, ver.GitSHA
				}
				ev.Ready, ev.Done = ri, true
				ch <- ev
				return false, nil
			case string(agent.StageFailed), string(agent.StageTerminating):
				if retriable(st.StageDetail) {
					ch <- Event{Phase: "image", Detail: st.StageDetail}
					if err := p.Delete(context.Background(), pod.ID); err != nil {
						return false, fmt.Errorf("delete pod %s with broken GPU: %w", pod.ID, err)
					}
					return true, nil
				}
				logs, _ := d.Agent.Logs(ctx, 20)
				why := st.StageDetail
				if st.Stage == agent.StageTerminating && st.KillReason != "failed" {
					why = "watchdog: " + st.KillReason // e.g. expired before it became ready
				}
				ch <- Event{Phase: "failed", Detail: why, Err: fmt.Errorf("pod stopped: %s (it deletes itself)\n%s", why, logs), Done: true}
				return false, nil
			}
			ch <- ev
			lastPhase, lastBytes = phase, bytes
		}
		// Container never came up (seen live: SECURE pod with no IP/ports for 13+ min): treat as a bad host.
		if !seen && d.now().Sub(created) > containerTimeout {
			ch <- Event{Phase: "image", Detail: fmt.Sprintf("host: container not started after %s", containerTimeout)}
			if err := p.Delete(context.Background(), pod.ID); err != nil {
				return false, fmt.Errorf("delete pod %s: %w", pod.ID, err)
			}
			return true, nil
		}
		if d.now().Sub(lastProgress) > o.Timeout {
			logs, _ := d.Agent.Logs(ctx, 20)
			derr := p.Delete(context.Background(), pod.ID)
			ch <- Event{Phase: "terminated", Err: fmt.Errorf("no progress for %s in phase %s; pod %s deleted (err=%v)\n%s", o.Timeout, phase, pod.ID, derr, strings.TrimSpace(logs)), Done: true}
			return false, nil
		}
		select {
		case <-ctx.Done():
			return false, ctx.Err()
		case <-time.After(poll):
		}
	}
}

// newBootID tags one rent; the agent echoes it so `up` only trusts its own pod's status.
func newBootID() string {
	b := make([]byte, 8)
	_, _ = rand.Read(b)
	return hex.EncodeToString(b)
}
