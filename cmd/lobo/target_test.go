package main

import (
	"encoding/json"
	"errors"
	"os"
	"path/filepath"
	"runtime"
	"strings"
	"testing"

	"github.com/1905/lobocode/internal/config"
	"github.com/1905/lobocode/internal/local"
)

var cloudCfg = config.Laptop{RunPodAPIKey: "r", LoboAPIKey: "sk", CFTunnelToken: "tok", Domain: "lobo.example.com", BucketURL: "https://pub-x.r2.dev"}

func swapSupported(t *testing.T, err error) {
	old := localSupported
	localSupported = func() error { return err }
	t.Cleanup(func() { localSupported = old })
}

func TestCheckTarget(t *testing.T) {
	keyOnly := config.Laptop{LoboAPIKey: "sk", WeightsDir: "/w"}
	tests := []struct {
		name      string
		cfg       config.Laptop
		provider  string
		supported error
		wantErr   string
	}{
		{name: "local, key-only config", cfg: keyOnly, provider: "local"},
		{name: "local on an unsupported machine", cfg: cloudCfg, provider: "local", supported: errors.New("local mode needs macOS on Apple Silicon"), wantErr: "Apple Silicon"},
		{name: "runpod, key-only config", cfg: keyOnly, provider: "runpod", wantErr: "CF_TUNNEL_TOKEN"},
		{name: "vast without domain", cfg: config.Laptop{VastAPIKey: "v", CFTunnelToken: "tok", BucketURL: "https://b.dev"}, provider: "vast", wantErr: "LOBO_DOMAIN"},
		{name: "runpod, full cloud config", cfg: cloudCfg, provider: "runpod"},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			swapSupported(t, tt.supported)
			err := checkTarget(tt.cfg, tt.provider)
			if tt.wantErr == "" && err != nil || tt.wantErr != "" && (err == nil || !strings.Contains(err.Error(), tt.wantErr)) {
				t.Fatalf("err %v, want %q", err, tt.wantErr)
			}
		})
	}
}

func TestCheckProviders(t *testing.T) {
	tests := []struct {
		name      string
		cfg       config.Laptop
		supported error
		wantErr   string
	}{
		{name: "local only, Apple Silicon", cfg: config.Laptop{LoboAPIKey: "sk"}},
		{name: "runpod key, no tunnel, domain or bucket", cfg: config.Laptop{LoboAPIKey: "sk", RunPodAPIKey: "r"}, supported: errors.New("no")},
		{name: "vast key only", cfg: config.Laptop{LoboAPIKey: "sk", VastAPIKey: "v"}, supported: errors.New("no")},
		{name: "no keys, not Apple Silicon", cfg: config.Laptop{LoboAPIKey: "sk"}, supported: errors.New("no"), wantErr: "RUNPOD_API_KEY or VASTAI_API_KEY"},
		{name: "full cloud config", cfg: cloudCfg},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			swapSupported(t, tt.supported)
			err := checkProviders(tt.cfg)
			if tt.wantErr == "" && err != nil || tt.wantErr != "" && (err == nil || !strings.Contains(err.Error(), tt.wantErr)) {
				t.Fatalf("err %v, want %q", err, tt.wantErr)
			}
		})
	}
}

func TestCheckRelease(t *testing.T) {
	r2 := config.R2Creds{AccountID: "a", AccessKey: "k", SecretKey: "s", Endpoint: "https://r2.example.com"}
	tests := []struct {
		name    string
		cfg     config.Laptop
		wantErr string
	}{
		{name: "r2 + bucket, no provider or tunnel", cfg: config.Laptop{LoboAPIKey: "sk", BucketURL: "https://pub-x.r2.dev", R2: r2}},
		{name: "no r2", cfg: cloudCfg, wantErr: "R2_"},
		{name: "no bucket", cfg: config.Laptop{LoboAPIKey: "sk", R2: r2}, wantErr: "LOBO_BUCKET_URL"},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			err := checkRelease(tt.cfg)
			if tt.wantErr == "" && err != nil || tt.wantErr != "" && (err == nil || !strings.Contains(err.Error(), tt.wantErr)) {
				t.Fatalf("err %v, want %q", err, tt.wantErr)
			}
		})
	}
}

func TestProviders(t *testing.T) {
	old := cfgPath
	cfgPath = "rel/config.env"
	t.Cleanup(func() { cfgPath = old })
	abs, _ := filepath.Abs(cfgPath)
	tests := []struct {
		name      string
		cfg       config.Laptop
		supported error
		want      string
	}{
		{name: "key only, Apple Silicon", cfg: config.Laptop{LoboAPIKey: "sk", LocalPort: "9000", WeightsDir: "/w"}, want: "local"},
		{name: "key only, unsupported", cfg: config.Laptop{LoboAPIKey: "sk"}, supported: errors.New("no"), want: ""},
		{name: "cloud keys, Apple Silicon", cfg: config.Laptop{RunPodAPIKey: "r", VastAPIKey: "v", LocalPort: "9000", WeightsDir: "/w"}, want: "local,runpod,vast"},
		{name: "cloud keys, unsupported", cfg: config.Laptop{RunPodAPIKey: "r"}, supported: errors.New("no"), want: "runpod"},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			swapSupported(t, tt.supported)
			m := providers(tt.cfg)
			var names []string
			for _, n := range []string{"local", "runpod", "vast"} {
				if m[n] != nil {
					names = append(names, n)
				}
			}
			if got := strings.Join(names, ","); got != tt.want {
				t.Fatalf("got %q want %q", got, tt.want)
			}
			if lp, ok := m["local"].(local.Provider); ok {
				if lp.Port != 9000 || lp.Weights != "/w" || lp.ConfigPath != abs || lp.Exe == "" {
					t.Fatalf("%+v", lp)
				}
			}
		})
	}
	// Unswapped: local exactly on darwin/arm64.
	_, ok := providers(config.Laptop{LoboAPIKey: "sk"})["local"]
	if want := runtime.GOOS == "darwin" && runtime.GOARCH == "arm64"; ok != want {
		t.Fatalf("local registered %v on %s/%s", ok, runtime.GOOS, runtime.GOARCH)
	}
}

func TestDepsLocal(t *testing.T) {
	d := deps(config.Laptop{LoboAPIKey: "sk", LocalPort: "9000"})
	if d.LocalURL != "http://127.0.0.1:9000/v1" || d.LocalAgent == nil {
		t.Fatalf("%+v", d)
	}
	if d = deps(config.Laptop{LoboAPIKey: "sk"}); d.LocalURL != "http://127.0.0.1:8931/v1" {
		t.Fatal(d.LocalURL)
	}
}

func TestWriteOpencode(t *testing.T) {
	type prov struct {
		Options struct{ BaseURL, APIKey string } `json:"options"`
		Models  map[string]any                   `json:"models"`
	}
	tests := []struct {
		name, domain string
		port         int
		want         map[string]string // provider → baseURL
		agentModel   string
	}{
		{name: "cloud + local", domain: "lobo.example.com", port: 8931,
			want: map[string]string{"lobo": "https://lobo.example.com/v1", "lobo-local": "http://127.0.0.1:8931/v1"}, agentModel: "lobo/"},
		{name: "no domain = local only", port: 9000,
			want: map[string]string{"lobo-local": "http://127.0.0.1:9000/v1"}, agentModel: "lobo-local/"},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			p := filepath.Join(t.TempDir(), "opencode.lobo.json")
			if err := writeOpencode(p, tt.domain, "sk-test", tt.port); err != nil {
				t.Fatal(err)
			}
			b, _ := os.ReadFile(p)
			var out struct {
				Provider map[string]prov                   `json:"provider"`
				Agent    map[string]struct{ Model string } `json:"agent"`
			}
			if err := json.Unmarshal(b, &out); err != nil {
				t.Fatal(err)
			}
			if len(out.Provider) != len(tt.want) {
				t.Fatalf("providers %v", out.Provider)
			}
			for name, url := range tt.want {
				pr := out.Provider[name]
				if pr.Options.BaseURL != url || pr.Options.APIKey != "sk-test" || len(pr.Models) != 1 {
					t.Fatalf("%s: %+v", name, pr)
				}
			}
			if !strings.HasPrefix(out.Agent["lobo"].Model, tt.agentModel) {
				t.Fatalf("agent model %q", out.Agent["lobo"].Model)
			}
		})
	}
}
