package local

import (
	"context"
	"errors"
	"fmt"
	"net"
	"os"
	"os/signal"
	"path/filepath"
	"strconv"
	"strings"
	"syscall"
	"testing"
	"time"

	"github.com/1905/lobocode/internal/model"
	"github.com/1905/lobocode/internal/provider"
)

// TestMain doubles as the fake `lobo local run` child: Provider.Exe = os.Args[0] with LOBO_LOCAL_HELPER set.
func TestMain(m *testing.M) {
	if mode := os.Getenv("LOBO_LOCAL_HELPER"); mode != "" {
		helperChild(mode)
		return
	}
	os.Exit(m.Run())
}

// helperChild: "ok" writes the state file and sleeps; "stubborn" also ignores SIGTERM; "fail" logs and exits 1.
func helperChild(mode string) {
	_ = os.WriteFile(filepath.Join(filepath.Dir(StatePath()), "args"), []byte(strings.Join(os.Args[1:], " ")), 0o600)
	if mode == "fail" {
		for i := 1; i <= 25; i++ {
			fmt.Printf("line %d\n", i)
		}
		fmt.Fprintln(os.Stderr, "boom: weights gone")
		os.Exit(1)
	}
	if mode == "stubborn" {
		signal.Ignore(syscall.SIGTERM)
	}
	model := ""
	for i, a := range os.Args {
		if a == "--model" && i+1 < len(os.Args) {
			model = os.Args[i+1]
		}
	}
	if err := WriteState(State{PID: os.Getpid(), Model: model, StartedAt: time.Now().UTC()}); err != nil {
		os.Exit(2)
	}
	time.Sleep(time.Minute)
	os.Exit(3)
}

// testProvider: fake child, fake runtime, a free port pair, weights and state in temp dirs.
func testProvider(t *testing.T, mode string) (Provider, *bool) {
	t.Helper()
	state := t.TempDir()
	t.Setenv("XDG_STATE_HOME", state)
	if err := os.MkdirAll(filepath.Join(state, "lobo"), 0o700); err != nil {
		t.Fatal(err)
	}
	t.Setenv("LOBO_LOCAL_HELPER", mode)
	ran := false
	swap(t, &supported, func() error { return nil })
	swap(t, &ensureRuntime, func(context.Context, string, func(string)) (string, error) {
		ran = true
		return "/fake/llama-server", nil
	})
	swap(t, &listModels, func(w string) (Listing, error) { // real free space must not decide the test
		return Listing{Weights: w, FreeBytes: 1 << 50, Models: []ModelState{}}, nil
	})
	swap(t, &stateWait, 5*time.Second)
	swap(t, &stopWait, 500*time.Millisecond)
	return Provider{Exe: os.Args[0], ConfigPath: "/cfg/lobo.env", Weights: filepath.Join(t.TempDir(), "w"), Port: freePair(t)}, &ran
}

func swap[T any](t *testing.T, p *T, v T) {
	old := *p
	*p = v
	t.Cleanup(func() { *p = old })
}

// freePair finds p with p and p+1 free on 127.0.0.1.
func freePair(t *testing.T) int {
	t.Helper()
	for i := 0; i < 50; i++ {
		ln, err := net.Listen("tcp", "127.0.0.1:0")
		if err != nil {
			t.Fatal(err)
		}
		p := ln.Addr().(*net.TCPAddr).Port
		_ = ln.Close()
		if p < 65535 && portFree(p) == nil && portFree(p+1) == nil {
			return p
		}
	}
	t.Fatal("no free port pair")
	return 0
}

var opts = provider.CreateOpts{Model: "q6", Ctx: 4096, IdleMin: 7, BootID: "b1"}

func TestProviderLifecycle(t *testing.T) {
	p, ran := testProvider(t, "ok")
	ctx := context.Background()
	in, err := p.Rent(ctx, opts, nil)
	if err != nil {
		t.Fatal(err)
	}
	pid, _ := strconv.Atoi(in.ID)
	t.Cleanup(func() { _ = syscall.Kill(pid, syscall.SIGKILL) })
	if in.Provider != "local" || pid <= 0 || in.CostPerHr != 0 || in.Status != "running" || in.Detail != "this Mac, q6" || in.StartedAt.IsZero() || !*ran {
		t.Fatalf("instance %+v ran %v", in, *ran)
	}
	args, _ := os.ReadFile(filepath.Join(filepath.Dir(StatePath()), "args"))
	want := fmt.Sprintf("--config /cfg/lobo.env local run --model q6 --ctx 4096 --idle-min 7 --boot-id b1 --port %d --api-port %d", p.Port, p.Port+1)
	if string(args) != want {
		t.Fatalf("args\n%s\nwant\n%s", args, want)
	}
	if fi, err := os.Stat(p.Weights); err != nil || !fi.IsDir() {
		t.Fatalf("weights not created: %v", err)
	}
	if _, err := p.Rent(ctx, opts, nil); err == nil || !strings.Contains(err.Error(), "already running") {
		t.Fatalf("second rent: %v", err)
	}

	list, err := p.List(ctx)
	if err != nil || len(list) != 1 || list[0].ID != in.ID {
		t.Fatalf("list %+v %v", list, err)
	}
	if _, err := p.Get(ctx, "1"); !errors.Is(err, provider.ErrNotFound) {
		t.Fatalf("get other pid: %v", err)
	}
	if err := p.Delete(ctx, in.ID); err != nil {
		t.Fatal(err)
	}
	if alive(pid) {
		t.Fatal("child still alive")
	}
	if _, err := os.Stat(StatePath()); !errors.Is(err, os.ErrNotExist) {
		t.Fatalf("state left: %v", err)
	}
	if list, err := p.List(ctx); err != nil || len(list) != 0 {
		t.Fatalf("list after delete %+v %v", list, err)
	}
	if err := p.Delete(ctx, in.ID); !errors.Is(err, provider.ErrNotFound) {
		t.Fatalf("delete missing: %v", err)
	}
}

// A child that ignores SIGTERM is SIGKILLed after stopWait.
func TestProviderDeleteKills(t *testing.T) {
	p, _ := testProvider(t, "stubborn")
	in, err := p.Rent(context.Background(), opts, nil)
	if err != nil {
		t.Fatal(err)
	}
	pid, _ := strconv.Atoi(in.ID)
	t.Cleanup(func() { _ = syscall.Kill(pid, syscall.SIGKILL) })
	start := time.Now()
	if err := p.Delete(context.Background(), in.ID); err != nil {
		t.Fatal(err)
	}
	if alive(pid) || time.Since(start) < stopWait {
		t.Fatalf("alive %v after %s", alive(pid), time.Since(start))
	}
}

func TestProviderChildFails(t *testing.T) {
	p, _ := testProvider(t, "fail")
	_, err := p.Rent(context.Background(), opts, nil)
	if err == nil || !strings.Contains(err.Error(), "exited early") || !strings.Contains(err.Error(), "boom: weights gone") ||
		!strings.Contains(err.Error(), "line 25") || strings.Contains(err.Error(), "line 5\n") {
		t.Fatalf("err %v", err)
	}
	if _, err := os.Stat(LogPath()); err != nil {
		t.Fatal(err)
	}
}

// Every pre-check fails before the runtime is fetched or a child is spawned.
func TestProviderPrechecks(t *testing.T) {
	tests := []struct {
		name    string
		setup   func(t *testing.T, p *Provider)
		opts    provider.CreateOpts
		wantErr string
	}{
		{name: "unsupported", setup: func(t *testing.T, _ *Provider) {
			swap(t, &supported, func() error { return errors.New("local mode needs macOS on Apple Silicon") })
		}, wantErr: "Apple Silicon"},
		{name: "bad model", opts: provider.CreateOpts{Model: "q2"}, wantErr: "q2"},
		{name: "weights not writable", setup: func(t *testing.T, p *Provider) {
			if os.Geteuid() == 0 {
				t.Skip("root ignores file modes")
			}
			ro := filepath.Join(t.TempDir(), "ro")
			if err := os.Mkdir(ro, 0o500); err != nil {
				t.Fatal(err)
			}
			t.Cleanup(func() { _ = os.Chmod(ro, 0o700) })
			p.Weights = ro
		}, wantErr: "is not writable"},
		{name: "no space", setup: func(t *testing.T, _ *Provider) {
			swap(t, &listModels, func(w string) (Listing, error) {
				m, _ := model.Get("q6")
				return Listing{Weights: w, FreeBytes: 1000, Models: []ModelState{{ID: "q6", Size: m.Size, OnDisk: 400}}}, nil
			})
		}, wantErr: fmt.Sprintf("need %d bytes, 1000 free", mustModel("q6").Size-400)},
		{name: "llama port busy", setup: func(t *testing.T, p *Provider) { hold(t, p.Port) }, wantErr: "LOBO_LOCAL_PORT"},
		{name: "api port busy", setup: func(t *testing.T, p *Provider) { hold(t, p.Port+1) }, wantErr: "LOBO_LOCAL_PORT+1"},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			p, ran := testProvider(t, "ok")
			if tt.setup != nil {
				tt.setup(t, &p)
			}
			o := tt.opts
			if o.Model == "" {
				o = opts
			}
			_, err := p.Rent(context.Background(), o, nil)
			if err == nil || !strings.Contains(err.Error(), tt.wantErr) {
				t.Fatalf("err %v, want %q", err, tt.wantErr)
			}
			if *ran {
				t.Fatal("runtime fetched before the pre-checks passed")
			}
			if _, err := os.Stat(StatePath()); !errors.Is(err, os.ErrNotExist) {
				t.Fatal("child spawned")
			}
		})
	}
}

func hold(t *testing.T, port int) {
	ln, err := net.Listen("tcp", "127.0.0.1:"+strconv.Itoa(port))
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = ln.Close() })
}

func mustModel(id string) model.Model {
	m, err := model.Get(id)
	if err != nil {
		panic(err)
	}
	return m
}
