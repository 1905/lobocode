// lobo controls the on-demand RunPod 5090 LLM pod. The laptop is control only.
package main

import (
	"context"
	"encoding/json"
	"fmt"
	"os"
	"os/exec"
	"os/signal"
	"os/user"
	"path/filepath"
	"strconv"
	"strings"
	"time"

	"github.com/rs/zerolog"
	"github.com/spf13/cobra"

	"github.com/1905/lobocode/internal/checks"
	"github.com/1905/lobocode/internal/config"
	"github.com/1905/lobocode/internal/control"
	"github.com/1905/lobocode/internal/local"
	"github.com/1905/lobocode/internal/model"
	"github.com/1905/lobocode/internal/provider"
	"github.com/1905/lobocode/internal/release"
	"github.com/1905/lobocode/internal/runpod"
	"github.com/1905/lobocode/internal/vast"
)

// Set by -ldflags at build time (make install, goreleaser).
var (
	version = "dev"
	commit  = "none"
	date    = "unknown"
)

var (
	cfgPath string
	log     = zerolog.New(zerolog.ConsoleWriter{Out: os.Stderr, TimeFormat: "15:04:05"}).With().Timestamp().Logger()
)

func main() {
	root := &cobra.Command{Use: "lobo", Short: "Rent an RTX 5090 and serve Qwen3.5-27B", SilenceUsage: true, SilenceErrors: true}
	root.PersistentFlags().StringVar(&cfgPath, "config", config.DefaultPath(), "config file (dotenv; the OS env is never read)")
	root.PersistentFlags().StringVar(&cfgPath, "env", config.DefaultPath(), "alias of --config")
	_ = root.PersistentFlags().MarkHidden("env")
	root.AddCommand(versionCmd(), configCmd(), genKeyCmd(), releaseCmd(), upCmd(), downCmd(), statusCmd(), logsCmd(), testCmd(), modelsCmd(), localCmd(runLocal))
	root.CompletionOptions.HiddenDefaultCmd = true
	installHelp(root)
	ctx, stop := signal.NotifyContext(context.Background(), os.Interrupt)
	defer stop()
	if err := root.ExecuteContext(ctx); err != nil {
		fmt.Fprintln(os.Stderr, "error:", err)
		os.Exit(1)
	}
}

func versionCmd() *cobra.Command {
	return &cobra.Command{Use: "version", Short: "Print version, commit and build date",
		Run: func(*cobra.Command, []string) { fmt.Printf("lobo %s (%s, %s)\n", version, commit, date) }}
}

func loadCfg() (config.Laptop, error) {
	if _, err := os.Stat(cfgPath); os.IsNotExist(err) {
		return config.Laptop{}, fmt.Errorf("no config at %s. Run `lobo config` first", cfgPath)
	}
	if config.LooseMode(cfgPath) {
		log.Warn().Msgf("%s holds API keys and other users can read it: chmod 600 %s", cfgPath, cfgPath)
	}
	return config.LoadLaptop(cfgPath)
}

func deps(cfg config.Laptop) control.Deps {
	port := cfg.Port()
	return control.Deps{
		Providers:  providers(cfg),
		Releases:   control.BucketReleases{BucketURL: cfg.BucketURL},
		Agent:      control.NewHTTPAgent(cfg.Domain, cfg.LoboAPIKey),
		LocalAgent: control.NewHTTPAgentURL(fmt.Sprintf("http://127.0.0.1:%d", port+1), cfg.LoboAPIKey),
		LocalURL:   fmt.Sprintf("http://127.0.0.1:%d/v1", port),
		Cfg:        cfg,
	}
}

// checkTarget runs before `up` touches anything: local needs Apple Silicon, runpod/vast need the cloud keys.
func checkTarget(cfg config.Laptop, provider string) error {
	if provider == "local" {
		return localSupported()
	}
	return cfg.RequireCloud()
}

// checkProviders: down and status only list and delete, so they need a provider, not the tunnel or bucket.
// A missing CF_TUNNEL_TOKEN must never block deleting a billing pod.
func checkProviders(cfg config.Laptop) error {
	if localSupported() == nil {
		return nil // local is always there; a cloud provider joins only with its key
	}
	return cfg.RequireProviderKey()
}

// checkRelease: `lobo release` uploads to R2 and logs the zip URL against LOBO_BUCKET_URL.
func checkRelease(cfg config.Laptop) error {
	if err := cfg.RequireR2(); err != nil {
		return err
	}
	return cfg.RequireBucket()
}

var localSupported = local.Supported // swapped in tests

func releaseCmd() *cobra.Command {
	return &cobra.Command{
		Use:    "release",
		Hidden: true, // dev-only: publishes the pod agent to the bucket
		Short:  "Build lobo-agent, zip it with release.json, scan for secrets, upload to bucket lobo",
		RunE: func(cmd *cobra.Command, _ []string) error {
			cfg, err := loadCfg()
			if err != nil {
				return err
			}
			if err := checkRelease(cfg); err != nil {
				return err
			}
			ctx := cmd.Context()
			store, err := release.NewStore(cfg.R2)
			if err != nil {
				return err
			}
			keys, err := store.ListReleaseKeys(ctx)
			if err != nil {
				return err
			}
			ver := release.NextVersion(keys, time.Now())
			sha, dirty, err := gitInfo()
			if err != nil {
				return err
			}
			dir, err := os.MkdirTemp("", "lobo-release-")
			if err != nil {
				return err
			}
			bin := filepath.Join(dir, "lobo-agent")
			build := exec.CommandContext(ctx, "go", "build", "-trimpath", "-ldflags", "-s -w -X main.version="+ver, "-o", bin, "./cmd/lobo-agent")
			build.Env = append(os.Environ(), "GOOS=linux", "GOARCH=amd64", "CGO_ENABLED=0")
			build.Stdout, build.Stderr = os.Stderr, os.Stderr
			if err := build.Run(); err != nil {
				return fmt.Errorf("build agent: %w", err)
			}
			m, _ := model.Get(release.DefaultModel)
			man := release.Manifest{
				Version: ver, GitSHA: sha, GitDirty: dirty, BuiltAt: time.Now().UTC().Truncate(time.Second), BuiltBy: whoami(),
				LlamaImage: release.DefaultLlamaImage, Model: release.ModelRef{ID: m.ID, File: m.File, SHA256: m.SHA256},
				Defaults: release.DefaultDefaults,
			}
			zipPath := filepath.Join(dir, "lobo-"+ver+".zip")
			zsha, err := release.BuildZip(bin, man, zipPath)
			if err != nil {
				return err
			}
			if err := release.ScanForSecrets(zipPath, cfg.SecretValues()); err != nil {
				return err
			}
			r := release.Resolved{Manifest: man, ZipKey: release.ZipKey(ver), ZipSHA256: zsha}
			if err := store.Publish(ctx, zipPath, r); err != nil {
				return err
			}
			log.Info().Str("version", ver).Str("git_sha", sha).Bool("dirty", dirty).Str("zip", r.ZipURL(cfg.BucketURL)).Msg("released")
			if dirty {
				log.Warn().Msg("working tree is dirty: /api/version will say git_dirty=true")
			}
			fmt.Println(ver)
			return nil
		},
	}
}

func gitInfo() (string, bool, error) {
	out, err := exec.Command("git", "rev-parse", "--short", "HEAD").Output()
	if err != nil {
		return "", false, fmt.Errorf("git rev-parse: %w", err)
	}
	st, err := exec.Command("git", "status", "--porcelain").Output()
	if err != nil {
		return "", false, fmt.Errorf("git status: %w", err)
	}
	return strings.TrimSpace(string(out)), len(strings.TrimSpace(string(st))) > 0, nil
}

func whoami() string {
	u, _ := user.Current()
	h, _ := os.Hostname()
	if u == nil {
		return h
	}
	return u.Username + "@" + h
}

func upCmd() *cobra.Command {
	var o control.UpOpts
	var q6, plain, asJSON bool
	var sshKey string
	c := &cobra.Command{
		Use:   "up",
		Short: "Rent a 5090 and boot lobo; shows progress until the API is ready",
		RunE: func(cmd *cobra.Command, _ []string) error {
			cfg, err := loadCfg()
			if err != nil {
				return err
			}
			if o.Cloud != "secure" && o.Cloud != "community" {
				return fmt.Errorf("--cloud: want secure or community, got %q", o.Cloud)
			}
			if q6 {
				o.Model = "q6"
			}
			if err := applyDefaults(cmd.Flags(), &o, cfg, cfgPath); err != nil {
				return err
			}
			if err := checkTarget(cfg, o.Provider); err != nil {
				return err
			}
			if sshKey != "" {
				b, err := os.ReadFile(sshKey)
				if err != nil {
					return err
				}
				o.SSHKey = strings.TrimSpace(string(b))
			}
			d := deps(cfg)
			if o.Provider != "local" && cfg.RequireR2() == nil {
				if st, err := release.NewStore(cfg.R2); err == nil {
					d.Presign = st
				}
			}
			var ready *control.ReadyInfo
			events := teeReady(control.Up(cmd.Context(), d, o), &ready)
			switch {
			case asJSON:
				err = jsonUp(events)
			case plain || !isTTY():
				err = plainUp(events)
			default:
				err = tuiUp(events)
			}
			reportBoot(ready, o.Source, o.Conns) // stderr, so --json stdout stays clean
			return err
		},
	}
	c.Flags().BoolVar(&q6, "q6", false, "serve Q6_K instead of the release default")
	c.Flags().IntVar(&o.Ctx, "ctx", 0, "context size (0 = release default)")
	c.Flags().StringVar(&o.Release, "release", "", "release version (default latest)")
	c.Flags().IntVar(&o.IdleMin, "idle-min", 0, "minutes without requests before the pod deletes itself (0 = release default)")
	c.Flags().DurationVar(&o.MaxLife, "max-life", 0, "hard pod lifetime, e.g. 12h (0 = release default)")
	c.Flags().BoolVar(&plain, "plain", false, "log lines instead of the TUI")
	c.Flags().StringVar(&o.Source, "source", "", "model source: r2 (presigned, default) | feesh (the model server HTTP) | ssh (the model server SSH) | public (r2.dev)")
	c.Flags().StringVar(&o.Image, "image", "", "pod image with lobo-agent baked in, e.g. ghcr.io/1905/lobocode@sha256:… (default: LOBO_POD_IMAGE, else the release zip)")
	c.Flags().IntVar(&o.Conns, "conns", 0, "parallel download streams on the pod (0 = agent default)")
	c.Flags().IntVar(&o.MinMBps, "min-mbps", 0, "drop the pod if the model downloads slower than this after 20 s (0 = LOBO_MIN_MBPS or 100)")
	c.Flags().StringVar(&o.Provider, "provider", "", "runpod, vast or local (this Mac) (default: LOBO_PROVIDER, else the one with a key, runpod first)")
	c.Flags().StringVar(&o.Cloud, "cloud", "community", "community ($0.69/h, default, community hosts only) or secure (datacenter $0.99/h first, community fallback)")
	c.Flags().BoolVar(&asJSON, "json", false, "one JSON object per event on stdout (for scripts and tests)")
	c.Flags().StringVar(&sshKey, "ssh", "", "debug: path to a public key; opens 22/tcp and runs sshd on the pod")
	return c
}

func plainUp(events <-chan control.Event) error {
	var last error
	for e := range events {
		ev := log.Info().Str("phase", e.Phase)
		if e.Detail != "" {
			ev = ev.Str("detail", e.Detail)
		}
		if e.Download != nil && e.Download.Total > 0 {
			ev = ev.Str("download", fmt.Sprintf("%.1f%% %.0f MB/s", 100*float64(e.Download.Bytes)/float64(e.Download.Total), e.Download.MBps))
		}
		if e.Ready != nil {
			ev = ev.Str("url", e.Ready.URL).Str("release", e.Ready.Version).Str("git_sha", e.Ready.GitSHA).
				Float64("usd_per_h", e.Ready.CostPerHr).Dur("boot", e.Ready.Elapsed.Round(time.Second))
		}
		if e.Err != nil {
			last = e.Err
			log.Error().Str("phase", e.Phase).Msg(e.Err.Error())
			continue
		}
		ev.Msg("up")
	}
	return last
}

// jsonUp prints each event as one JSON line; exit code 1 if the last event carries an error.
func jsonUp(events <-chan control.Event) error {
	enc := json.NewEncoder(os.Stdout)
	var last error
	for e := range events {
		out := struct {
			control.Event
			Err string `json:"err,omitempty"`
		}{Event: e}
		if e.Err != nil {
			out.Err, last = e.Err.Error(), e.Err
		}
		if err := enc.Encode(out); err != nil {
			return err
		}
	}
	if last != nil {
		return fmt.Errorf("up failed")
	}
	return nil
}

func downCmd() *cobra.Command {
	var asJSON bool
	c := &cobra.Command{
		Use:   "down",
		Short: "Delete every lobo pod on every provider",
		RunE: func(cmd *cobra.Command, _ []string) error {
			cfg, err := loadCfg()
			if err != nil {
				return err
			}
			if err := checkProviders(cfg); err != nil {
				return err
			}
			spent, err := control.Down(cmd.Context(), deps(cfg))
			if err != nil {
				return err
			}
			if asJSON {
				return json.NewEncoder(os.Stdout).Encode(map[string]float64{"spent_usd": spent})
			}
			log.Info().Str("spent", fmt.Sprintf("$%.2f", spent)).Msg("down: no lobo pods left")
			return nil
		},
	}
	c.Flags().BoolVar(&asJSON, "json", false, `print {"spent_usd": …} on stdout`)
	return c
}

func statusCmd() *cobra.Command {
	var once, asJSON bool
	c := &cobra.Command{
		Use:   "status",
		Short: "Live dashboard of the running pod",
		RunE: func(cmd *cobra.Command, _ []string) error {
			cfg, err := loadCfg()
			if err != nil {
				return err
			}
			if err := checkProviders(cfg); err != nil {
				return err
			}
			d := deps(cfg)
			if asJSON {
				s, err := control.Snapshot(cmd.Context(), d)
				if err != nil {
					return err
				}
				return json.NewEncoder(os.Stdout).Encode(s)
			}
			if once || !isTTY() {
				s, err := control.Snapshot(cmd.Context(), d)
				if err != nil {
					return err
				}
				fmt.Print(renderStatus(s, 80))
				return nil
			}
			return tuiStatus(cmd.Context(), d)
		},
	}
	c.Flags().BoolVar(&once, "once", false, "print one snapshot and exit")
	c.Flags().BoolVar(&asJSON, "json", false, "print one snapshot as JSON (pod, release, agent status) and exit")
	return c
}

func logsCmd() *cobra.Command {
	var n int
	c := &cobra.Command{
		Use:   "logs",
		Short: "Print the pod's recent agent, llama-server and cloudflared logs",
		RunE: func(cmd *cobra.Command, _ []string) error {
			cfg, err := loadCfg()
			if err != nil {
				return err
			}
			ag, _, err := control.Target(cmd.Context(), deps(cfg))
			if err != nil {
				return err
			}
			s, err := ag.Logs(cmd.Context(), n)
			if err != nil {
				return err
			}
			fmt.Print(s)
			return nil
		},
	}
	c.Flags().IntVarP(&n, "n", "n", 200, "number of lines (max 1000)")
	return c
}

func testCmd() *cobra.Command {
	return &cobra.Command{
		Use:   "test",
		Short: "Smoke test the live API: streamed chat + tool call",
		RunE: func(cmd *cobra.Command, _ []string) error {
			cfg, err := loadCfg()
			if err != nil {
				return err
			}
			ctx := cmd.Context()
			ag, base, err := control.Target(ctx, deps(cfg))
			if err != nil {
				return err
			}
			ver, err := ag.Version(ctx)
			if err != nil {
				return fmt.Errorf("lobo not reachable at %s: %w", base, err)
			}
			st, _ := ag.Status(ctx)
			id := release.DefaultModel
			if st != nil && st.Model != "" {
				id = st.Model
			}
			m, err := model.Get(id)
			if err != nil {
				return err
			}
			t0 := time.Now()
			text, err := checks.Chat(ctx, base, cfg.LoboAPIKey, m.Alias)
			if err != nil {
				return fmt.Errorf("chat: %w", err)
			}
			if len(text) > 200 {
				text = text[:200] + "…"
			}
			log.Info().Dur("took", time.Since(t0).Round(time.Millisecond)).Str("release", ver.Version).Msg("✓ streamed chat")
			fmt.Println(text)
			t0 = time.Now()
			body, err := checks.ToolCall(ctx, base, cfg.LoboAPIKey, m.Alias)
			if err != nil {
				return fmt.Errorf("tool call: %w", err)
			}
			if err := checks.ValidateToolCall(body); err != nil {
				return fmt.Errorf("%w\n%s", err, body)
			}
			log.Info().Dur("took", time.Since(t0).Round(time.Millisecond)).Msg("✓ tool call: arguments is a JSON string")
			return nil
		},
	}
}

func isTTY() bool {
	fi, err := os.Stdout.Stat()
	return err == nil && fi.Mode()&os.ModeCharDevice != 0
}

// providers builds every provider that has a key in the config, plus local on Apple Silicon.
func providers(cfg config.Laptop) map[string]provider.Provider {
	m := map[string]provider.Provider{}
	if localSupported() == nil {
		exe, _ := os.Executable()
		conf, err := filepath.Abs(cfgPath) // the detached child may not share our idea of a relative path
		if err != nil {
			conf = cfgPath
		}
		m["local"] = local.Provider{Exe: exe, ConfigPath: conf, Weights: cfg.Weights(), Port: cfg.Port()}
	}
	if cfg.RunPodAPIKey != "" {
		m["runpod"] = runpod.Provider{C: runpod.New(cfg.RunPodAPIKey)}
	}
	if cfg.VastAPIKey != "" {
		maxDPH, _ := strconv.ParseFloat(cfg.VastMaxDPH, 64)
		m["vast"] = &vast.Provider{C: vast.New(cfg.VastAPIKey), MaxDPH: maxDPH}
	}
	return m
}
