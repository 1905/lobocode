package main

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net"
	"net/http"
	"os"
	"os/signal"
	"strconv"
	"syscall"
	"time"

	"github.com/rs/zerolog"
	"github.com/spf13/cobra"

	"github.com/1905/lobocode/internal/agent"
	"github.com/1905/lobocode/internal/config"
	"github.com/1905/lobocode/internal/local"
	"github.com/1905/lobocode/internal/model"
)

// Local supervisor timings. BootTimeout covers a first HF download: 28 GB at ~7 MB/s is over an hour.
const (
	localBootTimeout = 8 * time.Hour
	localLifetime    = 100 * 365 * 24 * time.Hour // no hard lifetime locally; idle stops it
	localLlamaStop   = 10 * time.Second           // wait for llama-server to exit on shutdown
)

// runOpts are the `lobo local run` flags; Rent in internal/local builds them.
type runOpts struct {
	Model   string
	Ctx     int
	IdleMin int
	BootID  string
	Port    int // llama-server
	APIPort int // agent API
}

func (o runOpts) validate() error {
	if _, err := model.Get(o.Model); err != nil {
		return err
	}
	switch {
	case o.Ctx <= 0:
		return fmt.Errorf("--ctx: want > 0, got %d", o.Ctx)
	case o.IdleMin <= 0:
		return fmt.Errorf("--idle-min: want > 0, got %d", o.IdleMin)
	case o.Port < 1 || o.Port > 65535:
		return fmt.Errorf("--port: want 1-65535, got %d", o.Port)
	case o.APIPort < 1 || o.APIPort > 65535 || o.APIPort == o.Port:
		return fmt.Errorf("--api-port: want 1-65535 and not --port, got %d", o.APIPort)
	}
	return nil
}

// localCmd is the hidden `lobo local` group. run is swapped in tests.
func localCmd(run func(ctx context.Context, o runOpts) error) *cobra.Command {
	c := &cobra.Command{Use: "local", Hidden: true, Short: "Local-mode internals (spawned by `lobo up --provider local`)"}
	var o runOpts
	r := &cobra.Command{
		Use:   "run",
		Short: "Supervise llama-server on this Mac and serve the agent API on 127.0.0.1:--api-port",
		Args:  cobra.NoArgs,
		RunE: func(cmd *cobra.Command, _ []string) error {
			if err := o.validate(); err != nil {
				return err
			}
			return run(cmd.Context(), o)
		},
	}
	r.Flags().StringVar(&o.Model, "model", "q6", "catalog model id")
	r.Flags().IntVar(&o.Ctx, "ctx", 0, "context size")
	r.Flags().IntVar(&o.IdleMin, "idle-min", 0, "minutes without requests before it stops")
	r.Flags().StringVar(&o.BootID, "boot-id", "", "echoed in /api/status")
	r.Flags().IntVar(&o.Port, "port", config.DefaultLocalPort, "llama-server port")
	r.Flags().IntVar(&o.APIPort, "api-port", config.DefaultLocalPort+1, "agent API port")
	c.AddCommand(r)
	return c
}

// runLocal is the supervisor: state file, agent.Runner with the Mac deps, agent API. SIGTERM stops llama-server
// and exits 0. So do the idle watchdog and a failed boot (after FailGrace), through the Killer.
func runLocal(ctx context.Context, o runOpts) error {
	cfg, err := loadCfg()
	if err != nil {
		return err
	}
	m, _ := model.Get(o.Model) // validated
	logs := agent.NewLogRing(5000)
	log := zerolog.New(io.MultiWriter(os.Stdout, logs)).With().Timestamp().Str("service", "lobo-local").Logger()

	ctx, cancel := signal.NotifyContext(ctx, syscall.SIGTERM, os.Interrupt)
	defer cancel()
	weights := cfg.Weights()
	bin, err := local.EnsureRuntime(ctx, weights, func(s string) { log.Info().Msg("runtime: " + s) })
	if err != nil {
		return err
	}
	// Bind before the state file: a busy port must fail the child before Rent sees it as up.
	ln, err := net.Listen("tcp", "127.0.0.1:"+strconv.Itoa(o.APIPort))
	if err != nil {
		return fmt.Errorf("agent API: %w", err)
	}
	st := local.State{PID: os.Getpid(), Port: o.Port, APIPort: o.APIPort, Model: m.ID, Weights: weights,
		StartedAt: time.Now().UTC(), BootID: o.BootID}
	if err := local.ClaimState(st); err != nil { // fails while another supervisor is live
		_ = ln.Close()
		return err
	}
	defer func() {
		if err := local.RemoveStateIf(st.PID, st.BootID); err != nil { // never another run's state
			log.Error().Err(err).Msg("remove state")
		}
	}()

	deps, waitLlama := local.NewDeps(local.RunConfig{Weights: weights, LlamaServer: bin, APIKey: cfg.LoboAPIKey, Port: o.Port, Ctx: o.Ctx, Model: m},
		io.MultiWriter(os.Stdout, logs), cancel)
	log.Info().Str("model", m.File).Int("ctx", o.Ctx).Int("port", o.Port).Int("api_port", o.APIPort).Str("weights", weights).Msg("start")
	r := agent.NewRunner(deps, agent.RunnerConfig{
		BootID: o.BootID, Model: m.ID, Ctx: o.Ctx, Idle: time.Duration(o.IdleMin) * time.Minute,
		ExpiresAt: time.Now().Add(localLifetime), BootTimeout: localBootTimeout, Tick: 30 * time.Second, FailGrace: 2 * time.Minute,
	}, log)

	ver, _ := json.Marshal(map[string]string{"version": version, "git_sha": commit})
	srv := &http.Server{Handler: agent.NewAPI(cfg.LoboAPIKey, ver, r.Status, logs), ReadHeaderTimeout: 10 * time.Second}
	go func() {
		if err := srv.Serve(ln); err != nil && !errors.Is(err, http.ErrServerClosed) {
			log.Error().Err(err).Msg("api server")
		}
	}()

	err = r.Run(ctx)
	cancel() // CommandContext kills llama-server; wait so it does not outlive the supervisor
	if !waitLlama(localLlamaStop) {
		log.Warn().Msg("llama-server did not exit in time")
	}
	_ = srv.Close()
	log.Info().Err(err).Msg("stopped")
	if errors.Is(err, context.Canceled) { // SIGTERM or the Killer: a normal stop
		return nil
	}
	return err
}

func modelsCmd() *cobra.Command {
	var asJSON bool
	c := &cobra.Command{
		Use:   "models",
		Short: "Catalog models and their state in the local weights folder",
		Args:  cobra.NoArgs,
		RunE: func(cmd *cobra.Command, _ []string) error {
			cfg, err := loadCfg()
			if err != nil {
				return err
			}
			return writeModels(cmd.OutOrStdout(), cfg.Weights(), asJSON)
		},
	}
	c.Flags().BoolVar(&asJSON, "json", false, "print the listing as JSON (weights, free_bytes, models, runtime)")
	return c
}

func writeModels(w io.Writer, weights string, asJSON bool) error {
	l, err := local.List(weights)
	if err != nil {
		return err
	}
	if asJSON {
		return json.NewEncoder(w).Encode(l)
	}
	for _, m := range l.Models {
		state, verified := "missing", ""
		switch {
		case m.OnDisk == m.Size:
			state, verified = "on disk", "not verified"
			if m.Verified {
				verified = "verified"
			}
		case m.OnDisk > m.Size:
			state = "oversize" // Download starts it over
		case m.OnDisk > 0:
			state = fmt.Sprintf("partial %d%%", m.OnDisk*100/m.Size)
		}
		fmt.Fprintf(w, "%-4s %6.1f GB  %-12s %s\n", m.ID, float64(m.Size)/1e9, state, verified)
	}
	fmt.Fprintf(w, "weights %s (%.1f GB free)\n", l.Weights, float64(l.FreeBytes)/1e9)
	return nil
}
