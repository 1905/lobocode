package local

import (
	"bytes"
	"context"
	"crypto/sha256"
	"encoding/hex"
	"errors"
	"fmt"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"sync/atomic"
	"testing"
	"time"

	"github.com/1905/lobocode/internal/agent"
	"github.com/1905/lobocode/internal/model"
)

// fakeServer writes an executable llama-server script into a temp dir.
func fakeServer(t *testing.T, script string) string {
	t.Helper()
	p := filepath.Join(t.TempDir(), "llama-server")
	if err := os.WriteFile(p, []byte("#!/bin/sh\n"+script+"\n"), 0o755); err != nil {
		t.Fatal(err)
	}
	return p
}

type syncBuf struct {
	mu sync.Mutex
	b  bytes.Buffer
}

func (s *syncBuf) Write(p []byte) (int, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	return s.b.Write(p)
}

func (s *syncBuf) String() string {
	s.mu.Lock()
	defer s.mu.Unlock()
	return s.b.String()
}

func TestCheckGPU(t *testing.T) {
	q8, _ := model.Get("q8") // needs 29,831 MiB
	metal := `echo "Available devices:"; echo "  MTL0: Apple M1 Max (49152 MiB, 49151 MiB free)"`
	tests := []struct {
		name   string
		script string
		usable int
		want   string // "" = ok
	}{
		{"fits", metal, 49152, ""},
		{"exact", metal, model.MinFreeMiB(q8.Size), ""},
		{"too small", metal, model.MinFreeMiB(q8.Size) - 1, "q8 needs 29.1 GB, this Mac allows ~29 GB to the GPU"},
		{"no metal", `echo "Available devices:"; echo "ggml_metal_init: error: failed"`, 49152, "no Metal device"},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			d := newDeps(RunConfig{LlamaServer: fakeServer(t, tt.script), Model: q8}, &syncBuf{})
			d.usableMiB = func() (int, error) { return tt.usable, nil }
			err := d.checkGPU(context.Background())
			if tt.want == "" && err != nil || tt.want != "" && (err == nil || !strings.Contains(err.Error(), tt.want)) {
				t.Fatalf("err %v, want %q", err, tt.want)
			}
		})
	}
}

// hfServer serves body at /<file> and counts requests.
func hfServer(t *testing.T, file string, body []byte) (string, *atomic.Int32) {
	t.Helper()
	var hits atomic.Int32
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		hits.Add(1)
		if r.URL.Path != "/"+file {
			http.NotFound(w, r)
			return
		}
		http.ServeContent(w, r, file, time.Time{}, bytes.NewReader(body)) // Range support, like HF
	}))
	t.Cleanup(srv.Close)
	return srv.URL + "/", &hits
}

func TestDownload(t *testing.T) {
	body := bytes.Repeat([]byte("gguf"), 4096)
	sum := sha256.Sum256(body)
	good := model.Model{ID: "t", File: "t.gguf", SHA256: hex.EncodeToString(sum[:]), Size: int64(len(body))}
	bad := good
	bad.SHA256 = strings.Repeat("0", 64)

	tests := []struct {
		name      string
		m         model.Model
		file      []byte // nil = missing
		marker    string // "" = none, else a markers key
		wantHits  int32
		wantErr   string
		wantMark  bool
		wantBad   bool
		wantVerif bool
	}{
		{name: "complete+marker", m: good, file: body, marker: "valid", wantHits: 0, wantMark: true},
		{name: "complete no marker", m: good, file: body, wantHits: 0, wantMark: true, wantVerif: true},
		{name: "complete, stale mtime", m: good, file: body, marker: "stale mtime", wantHits: 0, wantMark: true, wantVerif: true},
		{name: "complete, old format", m: good, file: body, marker: "old format", wantHits: 0, wantMark: true, wantVerif: true},
		{name: "complete, wrong sha", m: good, file: body, marker: "wrong sha", wantHits: 0, wantMark: true, wantVerif: true},
		{name: "missing", m: good, wantHits: 1, wantMark: true},
		{name: "short", m: good, file: body[:100], marker: "old format", wantHits: 2, wantMark: true}, // probe + rest
		{name: "bad sha download", m: bad, wantHits: 1, wantErr: "sha256", wantBad: true},
		{name: "bad sha on disk", m: bad, file: body, wantHits: 0, wantErr: "sha256", wantBad: true, wantVerif: true},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			w := t.TempDir()
			dst := filepath.Join(w, tt.m.File)
			if tt.file != nil {
				if err := os.WriteFile(dst, tt.file, 0o644); err != nil {
					t.Fatal(err)
				}
			}
			if tt.marker != "" {
				if err := os.WriteFile(MarkerPath(w, tt.m.File), []byte(testMarker(t, w, tt.m, tt.marker)), 0o644); err != nil {
					t.Fatal(err)
				}
			}
			base, hits := hfServer(t, tt.m.File, body)
			d := newDeps(RunConfig{Weights: w, Model: tt.m}, &syncBuf{})
			d.hfBase = base
			var verif bool
			err := d.download(context.Background(), func(p agent.DownloadProgress) { verif = verif || p.Verifying })

			if tt.wantErr == "" && err != nil || tt.wantErr != "" && (err == nil || !strings.Contains(err.Error(), tt.wantErr)) {
				t.Fatalf("err %v, want %q", err, tt.wantErr)
			}
			if hits.Load() != tt.wantHits {
				t.Fatalf("hits %d, want %d", hits.Load(), tt.wantHits)
			}
			fi, _ := os.Stat(filepath.Join(w, tt.m.File))
			if markerValid(w, tt.m, fi) != tt.wantMark {
				b, _ := os.ReadFile(MarkerPath(w, tt.m.File))
				t.Fatalf("marker valid %v, want %v: %q", !tt.wantMark, tt.wantMark, b)
			}
			if verif != tt.wantVerif {
				t.Fatalf("verifying progress %v", verif)
			}
			bads, _ := filepath.Glob(filepath.Join(w, ".bad", tt.m.File+".*"))
			if (len(bads) == 1) != tt.wantBad {
				t.Fatalf(".bad: %v", bads)
			}
			if tt.wantBad {
				if _, err := os.Stat(dst); !errors.Is(err, os.ErrNotExist) {
					t.Fatalf("bad file left at %s: %v", dst, err)
				}
				if _, err := os.Stat(dst + ".bad"); !errors.Is(err, os.ErrNotExist) {
					t.Fatalf("bad file left at %s.bad: %v", dst, err)
				}
				return
			}
			if got, _ := os.ReadFile(dst); !bytes.Equal(got, body) {
				t.Fatalf("file %d bytes", len(got))
			}
		})
	}
}

// testMarker is a marker body for <w>/<m.File>: "valid", "stale mtime", "old format" (`<sha>\n`), "wrong sha",
// "wrong size". A missing file gets its fields from m and the zero mtime.
func testMarker(t *testing.T, w string, m model.Model, kind string) string {
	t.Helper()
	size, mtime := m.Size, int64(0)
	if fi, err := os.Stat(filepath.Join(w, m.File)); err == nil {
		size, mtime = fi.Size(), fi.ModTime().UnixNano()
	}
	switch kind {
	case "valid":
		return fmt.Sprintf("%s %d %d\n", m.SHA256, size, mtime)
	case "stale mtime":
		return fmt.Sprintf("%s %d %d\n", m.SHA256, size, mtime-1)
	case "old format":
		return m.SHA256 + "\n"
	case "wrong sha":
		return fmt.Sprintf("%s %d %d\n", strings.Repeat("a", 64), size, mtime)
	case "wrong size":
		return fmt.Sprintf("%s %d %d\n", m.SHA256, size+1, mtime)
	}
	t.Fatalf("marker kind %q", kind)
	return ""
}

func TestStartLlama(t *testing.T) {
	logs := &syncBuf{}
	q6, _ := model.Get("q6")
	d := newDeps(RunConfig{Weights: "/w", LlamaServer: fakeServer(t, `echo "args: $*"; echo "key: $LLAMA_API_KEY"; echo "arg: $LLAMA_ARG_HOST"`),
		APIKey: "sk-x", Port: 8931, Ctx: 65536, Model: q6}, logs)
	t.Setenv("LLAMA_ARG_HOST", "0.0.0.0") // must not reach the child
	exited, err := d.startLlama(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	if err := <-exited; err != nil {
		t.Fatal(err)
	}
	out := logs.String()
	for _, want := range []string{"args: -m /w/" + q6.File + " --alias " + q6.Alias + " --host 127.0.0.1 --port 8931 ", "-c 65536", "key: sk-x", "arg: \n"} {
		if !strings.Contains(out, want) {
			t.Fatalf("missing %q in:\n%s", want, out)
		}
	}
	if d.pid.Load() <= 0 {
		t.Fatal("pid not recorded")
	}
	if !d.waitLlama(5 * time.Second) {
		t.Fatal("waitLlama: llama-server exited, want true")
	}
	if !newDeps(RunConfig{}, logs).waitLlama(0) {
		t.Fatal("waitLlama: never started, want true")
	}
}

func TestGPUMetrics(t *testing.T) {
	d := newDeps(RunConfig{}, &syncBuf{})
	d.host, d.hostErr = hostGPU(func(name string) (string, error) {
		if name != "machdep.cpu.brand_string" {
			return "", errors.New(name)
		}
		return "Apple M1 Max", nil
	}, func() (uint64, error) { return 64 << 30, nil })
	d.vmstat = func(context.Context) (string, error) {
		return "Mach Virtual Memory Statistics: (page size of 16384 bytes)\nPages free: 19024.\nPages active: 1000000.\n" +
			"Pages wired down: 300000.\nPages occupied by compressor: 214000.\n", nil
	}
	g, err := d.gpu(context.Background())
	// (1000000+300000+214000) pages × 16 KiB = 23656 MiB
	if err != nil || g.Name != "Apple M1 Max" || g.VRAMUsedMB != 23656 || g.VRAMTotalMB != 65536 || g.UtilPct != 0 {
		t.Fatalf("%+v %v", g, err)
	}
	for _, bad := range []string{"garbage", "Mach (page size of 16384 bytes)\nPages active: 1.\n"} {
		d.vmstat = func(context.Context) (string, error) { return bad, nil }
		if _, err := d.gpu(context.Background()); err == nil {
			t.Fatalf("want parse error for %q", bad)
		}
	}
}

func TestWaitHealthy(t *testing.T) {
	var n atomic.Int32
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/health" || n.Add(1) < 2 {
			w.WriteHeader(http.StatusServiceUnavailable)
		}
	}))
	defer srv.Close()
	d := newDeps(RunConfig{}, &syncBuf{})
	d.llamaURL, d.poll = srv.URL, 0
	if err := d.waitHealthy(context.Background()); err != nil || n.Load() != 2 {
		t.Fatal(err, n.Load())
	}
}

func TestNewDepsWiring(t *testing.T) {
	var stopped bool
	deps, _ := NewDeps(RunConfig{Port: 8931}, &syncBuf{}, func() { stopped = true })
	exited, err := deps.StartTunnel(context.Background(), context.Background())
	if err != nil {
		t.Fatal(err)
	}
	select {
	case e := <-exited:
		t.Fatalf("tunnel exited: %v", e)
	default:
	}
	if err := deps.Killer.KillSelf(context.Background()); err != nil || !stopped {
		t.Fatal(err, stopped)
	}
	if deps.CheckGPU == nil || deps.Download == nil || deps.StartLlama == nil || deps.WaitHealthy == nil ||
		deps.Llama == nil || deps.GPU == nil || deps.Host == nil {
		t.Fatal("unset hook")
	}
}
