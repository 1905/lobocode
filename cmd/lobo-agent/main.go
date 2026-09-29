// lobo-agent runs as PID 1 on the RunPod pod: tunnel, model download, llama-server, watchdog, /api.
package main

import (
	"context"
	"encoding/base64"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"os"
	"os/exec"
	"regexp"
	"strconv"
	"strings"
	"time"

	"github.com/rs/zerolog"
	"github.com/spf13/cobra"
	"golang.org/x/crypto/ssh"

	"github.com/1905/lobocode/internal/agent"
	"github.com/1905/lobocode/internal/config"
	"github.com/1905/lobocode/internal/metrics"
	"github.com/1905/lobocode/internal/model"
	"github.com/1905/lobocode/internal/runpod"
	"github.com/1905/lobocode/internal/vast"
)

var version = "dev"

const (
	cloudflaredURL = "https://github.com/cloudflare/cloudflared/releases/download/2026.9.1/cloudflared-linux-amd64"
	releaseJSON    = "/lobo/release.json"
	binDir         = "/lobo/bin"
	modelDir       = "/models"
	llamaBin       = "/app/llama-server"
	// Decide early whether this host is worth paying for (floor = LOBO_MIN_MBPS, default 100).
	// Parallel R2 measured 345–878 MB/s on normal hosts and 12–96 MB/s on slow ones (2026-09-25).
	slowCheckAfter = 20 * time.Second
)

func main() {
	root := &cobra.Command{
		Use:          "lobo-agent",
		Short:        "Pod agent: tunnel, model download, llama-server, watchdog, /api",
		SilenceUsage: true,
		RunE: func(*cobra.Command, []string) error {
			err := run()
			if err != nil {
				// Anything that stops the agent before its watchdog runs (bad config, missing model)
				// must still delete the pod: PID 1 exiting only makes RunPod restart the container.
				fmt.Fprintln(os.Stderr, "lobo-agent fatal:", err)
				killSelfFromEnv()
			}
			return err
		},
	}
	root.AddCommand(&cobra.Command{Use: "version", Run: func(*cobra.Command, []string) { fmt.Println(version) }})
	root.AddCommand(benchCmd())
	if err := root.Execute(); err != nil {
		os.Exit(1)
	}
}

func run() error {
	logs := agent.NewLogRing(5000)
	out := io.MultiWriter(os.Stdout, logs)
	log := zerolog.New(out).With().Timestamp().Str("service", "lobo-agent").Logger()

	cfg, err := config.LoadAgent()
	if err != nil {
		return err
	}
	m, err := model.Get(cfg.Model)
	if err != nil {
		return err
	}
	ver, err := os.ReadFile(releaseJSON)
	if err != nil {
		ver = []byte(`{"version":"unknown"}`)
	}
	log.Info().RawJSON("release", ver).Str("model", m.File).Int("ctx", cfg.Ctx).Time("expires_at", cfg.ExpiresAt).Msg("start")

	childEnv := agent.CleanEnv(os.Environ())
	coll := metrics.Collector{
		LlamaURL: "http://" + agent.LlamaAddr, APIKey: cfg.LoboAPIKey,
		SMI: func(ctx context.Context) (string, error) {
			b, err := exec.CommandContext(ctx, "nvidia-smi", metrics.SMIArgs...).Output()
			return string(b), err
		},
	}
	deps := agent.Deps{
		StartTunnel: func(bootCtx, ctx context.Context) (<-chan error, error) {
			bin := binDir + "/cloudflared"
			if err := fetch(bootCtx, cloudflaredURL, bin); err != nil {
				return nil, err
			}
			env := append(childEnv, "TUNNEL_TOKEN="+cfg.CFTunnelToken)
			return agent.StartProcess(ctx, bin, []string{"tunnel", "--no-autoupdate", "run"}, env, prefix(logs, "cloudflared"))
		},
		// CUDA init failed with "unknown error" in the first seconds of several fresh pods (2026-09-23)
		// and worked on the same host a minute later. So: retry for up to 3 min before giving up.
		CheckGPU: func(ctx context.Context) error {
			start := time.Now()
			var out []byte
			for attempt := 1; ; attempt++ {
				// Per-try bound: a CUDA init that hangs instead of failing would otherwise eat the whole boot timeout.
				actx, acancel := context.WithTimeout(ctx, time.Minute)
				cmd := exec.CommandContext(actx, llamaBin, "--list-devices")
				cmd.Env = append(childEnv, "LD_LIBRARY_PATH=/app:/usr/local/cuda/lib64")
				cmd.WaitDelay = 5 * time.Second
				out, _ = cmd.CombinedOutput()
				acancel()
				if strings.Contains(string(out), "CUDA0:") {
					_, _ = prefix(logs, "gpu-check").Write(out)
					free, ok := freeMiB(string(out))
					if !ok || free < model.MinFreeMiB(m.Size) {
						// Seen live: a host with ~6 GB VRAM held outside our container → cudaMalloc OOM on load.
						return fmt.Errorf("only %d MiB VRAM free, %s needs %d MiB", free, m.ID, model.MinFreeMiB(m.Size))
					}
					log.Info().Int("attempt", attempt).Dur("after", time.Since(start)).Int("free_mib", free).Msg("gpu check ok")
					return nil
				}
				log.Warn().Int("attempt", attempt).Str("out", lastLine(string(out))).Msg("gpu check failed, retrying")
				if time.Since(start) > 3*time.Minute {
					return fmt.Errorf("llama-server sees no CUDA device after %d tries / %s: %s", attempt, time.Since(start).Round(time.Second), lastLine(string(out)))
				}
				select {
				case <-ctx.Done():
					return ctx.Err()
				case <-time.After(10 * time.Second):
				}
			}
		},
		Download: func(ctx context.Context, p func(agent.DownloadProgress)) error {
			if err := os.MkdirAll(modelDir, 0o755); err != nil {
				return err
			}
			urls := []string{cfg.ModelURL}
			if cfg.ModelFallback != "" {
				urls = append(urls, cfg.ModelFallback)
			}
			return downloadAny(ctx, urls, func(i int, u string) error {
				return downloadFrom(ctx, cfg, m, u, i > 0, p)
			}, func(err error, next string) {
				log.Warn().Err(err).Str("next", agent.RedactURL(next)).Msg("source failed, switching")
			})
		},
		StartLlama: func(ctx context.Context) (<-chan error, error) {
			host, port, _ := strings.Cut(agent.LlamaAddr, ":")
			args := append([]string{"-m", modelDir + "/" + m.File}, agent.LlamaArgs(m, host, port, cfg.Ctx)...)
			env := append(childEnv, "LD_LIBRARY_PATH=/app:/usr/local/cuda/lib64", "LLAMA_API_KEY="+cfg.LoboAPIKey)
			return agent.StartProcess(ctx, llamaBin, args, env, prefix(logs, "llama"))
		},
		WaitHealthy: func(ctx context.Context) error {
			return agent.WaitHealthy(ctx, "http://"+agent.LlamaAddr, 2*time.Second)
		},
		Llama:  coll.Llama,
		GPU:    coll.GPU,
		Host:   coll.Host,
		Killer: agent.RunPodKiller{API: selfAPI(cfg.Provider, cfg.RunPodPodID, cfg.RunPodAPIKey, cfg.VastID, cfg.VastAPIKey), Log: log},
	}
	tim := bootTimings()
	tim.DownloadConns = cfg.DLConns
	if u, err := url.Parse(cfg.ModelURL); err == nil {
		tim.DownloadSource = u.Scheme + "://" + u.Host
	}
	log.Info().Interface("bootstrap", tim).Msg("bootstrap timings")
	r := agent.NewRunner(deps, agent.RunnerConfig{
		Timings: tim, BootID: cfg.BootID,
		Model: cfg.Model, Ctx: cfg.Ctx, Idle: time.Duration(cfg.IdleMin) * time.Minute,
		ExpiresAt: cfg.ExpiresAt, BootTimeout: cfg.BootTimeout, Tick: 30 * time.Second, FailGrace: 2 * time.Minute,
	}, log)

	srv := &http.Server{Addr: agent.AgentAddr, Handler: agent.NewAPI(cfg.LoboAPIKey, ver, r.Status, logs), ReadHeaderTimeout: 10 * time.Second}
	go func() {
		if err := srv.ListenAndServe(); err != nil && err != http.ErrServerClosed {
			log.Error().Err(err).Msg("api server")
		}
	}()
	return r.Run(context.Background())
}

func fetch(ctx context.Context, url, dst string) error {
	if err := os.MkdirAll(binDir, 0o755); err != nil {
		return err
	}
	return agent.FetchFile(ctx, url, func() (*os.File, error) {
		return os.OpenFile(dst, os.O_CREATE|os.O_TRUNC|os.O_WRONLY, 0o755)
	}, -1, "")
}

// prefix tags each child output line with its source for /api/logs.
func prefix(w io.Writer, name string) io.Writer {
	pr, pw := io.Pipe()
	go func() {
		buf := make([]byte, 64*1024)
		var rest string
		for {
			n, err := pr.Read(buf)
			rest += string(buf[:n])
			for {
				i := strings.IndexByte(rest, '\n')
				if i < 0 {
					break
				}
				fmt.Fprintf(w, "[%s] %s\n", name, rest[:i])
				fmt.Fprintf(os.Stdout, "[%s] %s\n", name, rest[:i])
				rest = rest[i+1:]
			}
			if err != nil {
				return
			}
		}
	}()
	return pw
}

func lastLine(s string) string {
	lines := strings.Split(strings.TrimSpace(s), "\n")
	for i := len(lines) - 1; i >= 0; i-- {
		if strings.Contains(lines[i], "error") || strings.Contains(lines[i], "fail") {
			return strings.TrimSpace(lines[i])
		}
	}
	return strings.TrimSpace(lines[len(lines)-1])
}

var freeRe = regexp.MustCompile(`CUDA0: .*\((\d+) MiB, (\d+) MiB free\)`)

// freeMiB reads free VRAM from `llama-server --list-devices`.
func freeMiB(out string) (int, bool) {
	m := freeRe.FindStringSubmatch(out)
	if m == nil {
		return 0, false
	}
	n, err := strconv.Atoi(m[2])
	return n, err == nil
}

// killSelfFromEnv terminates this pod with the RunPod-injected pod key, without the agent config.
func killSelfFromEnv() {
	prov := os.Getenv("LOBO_PROVIDER")
	api := selfAPI(prov, os.Getenv("RUNPOD_POD_ID"), os.Getenv("RUNPOD_API_KEY"), os.Getenv("CONTAINER_ID"), os.Getenv("CONTAINER_API_KEY"))
	if api == nil {
		fmt.Fprintf(os.Stderr, "lobo-agent: no instance id/key for provider %q, cannot self-terminate\n", prov)
		return
	}
	log := zerolog.New(os.Stderr).With().Timestamp().Logger()
	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Minute)
	defer cancel()
	_ = agent.RunPodKiller{API: api, Log: log}.KillSelf(ctx)
}

// selfAPI terminates this instance with the credentials its provider injected (never our account key).
func selfAPI(prov, rpID, rpKey, vastID, vastKey string) agent.PodAPI {
	switch prov {
	case "vast":
		if vastID == "" || vastKey == "" {
			return nil
		}
		s, err := vast.NewSelf(vastID, vastKey)
		if err != nil {
			return nil
		}
		return s
	default:
		if rpID == "" || rpKey == "" {
			return nil
		}
		return runpod.NewSelf(rpID, rpKey)
	}
}

// modelSource: ssh://user@host:port (the model server, file from the catalog) or a plain HTTP(S) URL.
func modelSource(cfg config.Agent, m model.Model) (agent.Source, error) {
	u, err := url.Parse(cfg.ModelURL)
	if err != nil {
		return nil, err
	}
	if u.Scheme != "ssh" {
		return agent.HTTPSource{URL: cfg.ModelURL}, nil
	}
	pem, err := base64.StdEncoding.DecodeString(cfg.ModelSSHKey)
	if err != nil {
		return nil, fmt.Errorf("LOBO_MODEL_SSH_KEY: %w", err)
	}
	signer, err := ssh.ParsePrivateKey(pem)
	if err != nil {
		return nil, fmt.Errorf("LOBO_MODEL_SSH_KEY: %w", err)
	}
	hk, _, _, _, err := ssh.ParseAuthorizedKey([]byte(cfg.ModelHostKey))
	if err != nil {
		return nil, fmt.Errorf("LOBO_MODEL_SSH_HOSTKEY: %w", err)
	}
	host := u.Host
	if u.Port() == "" {
		host += ":22"
	}
	return agent.SSHSource{Addr: host, User: u.User.Username(), File: m.File, Size: m.Size, Signer: signer, HostKey: hk}, nil
}

// bootTimings reads the timestamps the bootstrap exported (unix seconds with fractions).
func bootTimings() agent.Timings {
	ts := func(k string) float64 { f, _ := strconv.ParseFloat(os.Getenv(k), 64); return f }
	t0, apt, zip := ts("LOBO_T_BOOT0"), ts("LOBO_T_APT"), ts("LOBO_T_ZIP")
	var t agent.Timings
	if t0 > 0 {
		t.ContainerStartedAt = time.Unix(0, int64(t0*1e9)).UTC()
		if apt > t0 {
			t.BootstrapAptS = apt - t0
		}
		if zip > apt && apt > 0 {
			t.BootstrapZipS = zip - apt
		}
	}
	return t
}

// benchCmd measures download speed without writing the model: lobo-agent bench --conns 1,4,8 [--url URL].
// No --url: the SSH source from the pod env (LOBO_MODEL_URL + key). Output: one JSON line per run.
func benchCmd() *cobra.Command {
	var conns string
	var secs int
	var rawURL, modelID string
	c := &cobra.Command{
		Use:   "bench",
		Short: "Measure model download MB/s for several connection counts (nothing is written)",
		RunE: func(cmd *cobra.Command, _ []string) error {
			m, err := model.Get(modelID)
			if err != nil {
				return err
			}
			var src agent.Source
			if rawURL != "" {
				src = agent.HTTPSource{URL: rawURL}
			} else {
				cfg := config.Agent{ModelURL: os.Getenv("LOBO_MODEL_URL"), ModelSSHKey: os.Getenv("LOBO_MODEL_SSH_KEY"), ModelHostKey: os.Getenv("LOBO_MODEL_SSH_HOSTKEY")}
				if src, err = modelSource(cfg, m); err != nil {
					return err
				}
			}
			for _, cs := range strings.Split(conns, ",") {
				n, err := strconv.Atoi(strings.TrimSpace(cs))
				if err != nil {
					return err
				}
				b, mbps, err := agent.Bench(cmd.Context(), src, m.Size, n, time.Duration(secs)*time.Second)
				out := map[string]any{"source": fmt.Sprint(src)[:min(40, len(fmt.Sprint(src)))], "conns": n, "bytes": b, "mbps": mbps}
				if err != nil {
					out["err"] = err.Error()
				}
				j, _ := json.Marshal(out)
				fmt.Println(string(j))
			}
			return nil
		},
	}
	c.Flags().StringVar(&conns, "conns", "1,4,8", "comma-separated connection counts")
	c.Flags().IntVar(&secs, "seconds", 45, "seconds per run")
	c.Flags().StringVar(&rawURL, "url", "", "HTTP(S) URL (e.g. presigned R2); empty = SSH source from pod env")
	c.Flags().StringVar(&modelID, "model", "q8", "q8 or q6 (sets file and size)")
	return c
}

// downloadFrom pulls the model from one source. After slowCheckAfter it decides once whether the speed is
// worth it; below the floor it fails with "host: download too slow" so the caller can switch or re-rent.
func downloadFrom(ctx context.Context, cfg config.Agent, m model.Model, rawURL string, switched bool, p func(agent.DownloadProgress)) error {
	c := cfg
	c.ModelURL = rawURL
	src, err := modelSource(c, m)
	if err != nil {
		return err
	}
	dctx, cancel := context.WithCancelCause(ctx)
	defer cancel(nil)
	start, checked := time.Now(), false
	floor := float64(cfg.MinMBps) // one setting for every source (user: below 100 MB/s isn't worth paying for)
	err = agent.DownloadParallel(dctx, src, modelDir+"/"+m.File, m.Size, m.SHA256, m.ChunkSHA(), cfg.DLConns, func(dp agent.DownloadProgress) {
		if switched {
			dp.Source = agent.RedactURL(rawURL)
		}
		p(dp)
		if !checked && time.Since(start) > slowCheckAfter {
			checked = true
			if dp.MBps < floor {
				cancel(fmt.Errorf("host: download too slow: %.1f MB/s after %s from %s (min %.0f)", dp.MBps, time.Since(start).Round(time.Second), agent.RedactURL(rawURL), floor))
			}
		}
	})
	if cause := context.Cause(dctx); cause != nil && ctx.Err() == nil {
		return cause
	}
	return err
}

// downloadAny tries each source in order. Any failure of a source (too slow — R2 was 12–17 MB/s for everyone
// on 2026-09-25 —, a 5xx storm, a stream that gave up after its resumes, a 403) moves on to the next source on
// this pod while the boot is still alive; only the last source's error is returned.
func downloadAny(ctx context.Context, urls []string, try func(i int, u string) error, onSwitch func(err error, next string)) error {
	var err error
	for i, u := range urls {
		err = try(i, u)
		if err == nil || ctx.Err() != nil || i == len(urls)-1 {
			return err
		}
		onSwitch(err, urls[i+1])
	}
	return err
}
