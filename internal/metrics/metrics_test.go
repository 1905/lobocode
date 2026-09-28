package metrics

import (
	"context"
	"errors"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"
)

const sample = `# HELP llamacpp:prompt_tokens_total Number of prompt tokens processed.
# TYPE llamacpp:prompt_tokens_total counter
llamacpp:prompt_tokens_total 182340
llamacpp:tokens_predicted_total 21044
llamacpp:prompt_tokens_seconds 2410.5
llamacpp:predicted_tokens_seconds 48.3
llamacpp:requests_processing 1
llamacpp:requests_deferred 2
`

func TestParseLlama(t *testing.T) {
	l, err := ParseLlama(strings.NewReader(sample))
	if err != nil {
		t.Fatal(err)
	}
	want := Llama{RequestsProcessing: 1, RequestsDeferred: 2, PromptTokensTotal: 182340, GenTokensTotal: 21044, PromptTPS: 2410.5, GenTPS: 48.3}
	if l != want {
		t.Fatalf("got %+v", l)
	}
	for name, in := range map[string]string{
		"missing required": "llamacpp:tokens_predicted_total 5\n",
		"garbage":          "llamacpp:requests_processing abc\n",
		"short line":       "llamacpp:requests_processing\n",
	} {
		if _, err := ParseLlama(strings.NewReader(in)); err == nil {
			t.Errorf("%s: want error", name)
		}
	}
}

// The fixture is a real /metrics capture from the pod (P1). It must parse.
func TestParseLlamaFixture(t *testing.T) {
	b, err := os.ReadFile("testdata/llama_metrics.txt")
	if errors.Is(err, os.ErrNotExist) {
		t.Skip("fixture not captured yet")
	}
	if err != nil {
		t.Fatal(err)
	}
	if _, err := ParseLlama(strings.NewReader(string(b))); err != nil {
		t.Fatal(err)
	}
}

func TestParseNvidiaSMI(t *testing.T) {
	g, err := ParseNvidiaSMI("NVIDIA GeForce RTX 5090, 30112, 32607, 3\n")
	if err != nil || g != (GPU{"NVIDIA GeForce RTX 5090", 30112, 32607, 3}) {
		t.Fatalf("%+v %v", g, err)
	}
	if _, err := ParseNvidiaSMI("NVIDIA, x, 1, 2"); err == nil {
		t.Fatal("want error")
	}
}

const meminfo = "MemTotal:       461373440 kB\nMemFree:        271000000 kB\nMemAvailable:   391000000 kB\n"

func TestParseHost(t *testing.T) {
	h, err := ParseHost("1.20 0.90 0.70 2/1234 5678\n", meminfo)
	if err != nil {
		t.Fatal(err)
	}
	if h.Load1 != 1.2 || h.Load15 != 0.7 || h.MemTotalMB != 461373440/1024 || h.MemUsedMB != (461373440-391000000)/1024 {
		t.Fatalf("%+v", h)
	}
	if _, err := ParseHost("x", meminfo); err == nil {
		t.Fatal("want error")
	}
	if _, err := ParseHost("1 1 1", "MemTotal: 5 kB\n"); err == nil {
		t.Fatal("want error")
	}
}

func TestCollector(t *testing.T) {
	var gotAuth string
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		gotAuth = r.Header.Get("Authorization")
		if r.URL.Path == "/metrics" && gotAuth == "Bearer k" {
			_, _ = w.Write([]byte(sample))
			return
		}
		w.WriteHeader(http.StatusUnauthorized)
	}))
	defer srv.Close()
	dir := t.TempDir()
	_ = os.WriteFile(filepath.Join(dir, "loadavg"), []byte("0.5 0.4 0.3 1/2 3"), 0o644)
	_ = os.WriteFile(filepath.Join(dir, "meminfo"), []byte(meminfo), 0o644)
	c := Collector{LlamaURL: srv.URL, APIKey: "k", ProcDir: dir,
		SMI: func(context.Context) (string, error) { return "NVIDIA GeForce RTX 5090, 1, 2, 3", nil }}
	ctx := context.Background()
	if l, err := c.Llama(ctx); err != nil || l.PromptTokensTotal != 182340 || gotAuth != "Bearer k" {
		t.Fatalf("%+v %v %q", l, err, gotAuth)
	}
	if _, err := (Collector{LlamaURL: srv.URL, APIKey: "bad"}).Llama(ctx); err == nil {
		t.Fatal("want 401 error")
	}
	if g, err := c.GPU(ctx); err != nil || g.UtilPct != 3 {
		t.Fatal(g, err)
	}
	if h, err := c.Host(ctx); err != nil || h.Load1 != 0.5 {
		t.Fatal(h, err)
	}
}

func TestCollectorTimeout(t *testing.T) {
	block := make(chan struct{})
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) { <-block }))
	defer srv.Close()
	defer close(block)
	start := time.Now()
	if _, err := (Collector{LlamaURL: srv.URL}).Llama(context.Background()); err == nil {
		t.Fatal("want timeout")
	}
	if time.Since(start) > 5*time.Second {
		t.Fatal("timeout not applied")
	}
}
