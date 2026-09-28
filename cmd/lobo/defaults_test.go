package main

import (
	"strings"
	"testing"
	"time"

	"github.com/1905/lobocode/internal/config"
	"github.com/1905/lobocode/internal/control"
)

func TestApplyDefaults(t *testing.T) {
	both := config.Laptop{RunPodAPIKey: "r", VastAPIKey: "v"}
	for _, c := range []struct {
		name string
		cfg  config.Laptop
		args []string
		want control.UpOpts
		err  string
	}{
		{"builtin", config.Laptop{RunPodAPIKey: "r"}, nil, control.UpOpts{Provider: "runpod", Cloud: "community"}, ""},
		{"vast only", config.Laptop{VastAPIKey: "v"}, nil, control.UpOpts{Provider: "vast", Cloud: "community"}, ""},
		{"config provider", config.Laptop{RunPodAPIKey: "r", VastAPIKey: "v", Provider: "vast"}, nil, control.UpOpts{Provider: "vast", Cloud: "community"}, ""},
		{"flag beats config", config.Laptop{RunPodAPIKey: "r", VastAPIKey: "v", Provider: "vast"}, []string{"--provider", "runpod"}, control.UpOpts{Provider: "runpod", Cloud: "community"}, ""},
		{"config defaults", config.Laptop{RunPodAPIKey: "r", Model: "q6", Ctx: "32768", IdleMin: "10", MaxHours: "2", MinMBps: "200", Cloud: "community"}, nil,
			control.UpOpts{Provider: "runpod", Model: "q6", Ctx: 32768, IdleMin: 10, MaxLife: 2 * time.Hour, MinMBps: 200, Cloud: "community"}, ""},
		{"flags beat config", config.Laptop{RunPodAPIKey: "r", Ctx: "32768", MinMBps: "200", Cloud: "community"}, []string{"--ctx", "8192", "--min-mbps", "50", "--cloud", "secure"},
			control.UpOpts{Provider: "runpod", Ctx: 8192, MinMBps: 50, Cloud: "secure"}, ""},
		{"unkeyed provider", config.Laptop{RunPodAPIKey: "r"}, []string{"--provider", "vast"}, control.UpOpts{}, "no VASTAI_API_KEY in /cfg"},
		{"bad provider", both, []string{"--provider", "aws"}, control.UpOpts{}, "want runpod or vast"},
		{"bad config ctx", config.Laptop{RunPodAPIKey: "r", Ctx: "100"}, nil, control.UpOpts{}, "LOBO_CTX"},
		{"flag overrides bad config ctx", config.Laptop{RunPodAPIKey: "r", Ctx: "100"}, []string{"--ctx", "8192"},
			control.UpOpts{Provider: "runpod", Ctx: 8192, Cloud: "community"}, ""},
	} {
		t.Run(c.name, func(t *testing.T) {
			cmd := upCmd()
			if err := cmd.Flags().Parse(c.args); err != nil {
				t.Fatal(err)
			}
			var o control.UpOpts
			o.Provider, _ = cmd.Flags().GetString("provider")
			o.Cloud, _ = cmd.Flags().GetString("cloud")
			o.Ctx, _ = cmd.Flags().GetInt("ctx")
			o.MinMBps, _ = cmd.Flags().GetInt("min-mbps")
			err := applyDefaults(cmd.Flags(), &o, c.cfg, "/cfg")
			if c.err != "" {
				if err == nil || !strings.Contains(err.Error(), c.err) {
					t.Fatalf("want %q, got %v", c.err, err)
				}
				return
			}
			if err != nil || o != c.want {
				t.Fatalf("got %+v %v\nwant %+v", o, err, c.want)
			}
		})
	}
}

func TestMaskedUnknownKeys(t *testing.T) {
	for k, clear := range map[string]bool{"CUSTOM_API_KEY": false, "DATABASE_PASSWORD": false, "RUNPOD_API_KEY": false, "R2_SECRET_KEY": false,
		"LOBO_FEESH_HTTP_URL": false, "LOBO_DOMAIN": true, "LOBO_CTX": true} {
		v := "value-that-is-long-enough"
		if got := masked(k, v); (got == v) != clear {
			t.Fatalf("%s: got %q", k, got)
		}
	}
}

func TestParseSetArgs(t *testing.T) {
	set, err := parseSetArgs([]string{"LOBO_MIN_MBPS=150", "LOBO_CTX=", "CF_TUNNEL_TOKEN=a=b"})
	if err != nil || set["LOBO_MIN_MBPS"] != "150" || set["LOBO_CTX"] != "" || set["CF_TUNNEL_TOKEN"] != "a=b" || len(set) != 3 {
		t.Fatalf("%v %v", set, err)
	}
	for _, bad := range []string{"lower=1", "NOEQUALS", "=x", "A-B=1"} {
		if _, err := parseSetArgs([]string{bad}); err == nil {
			t.Fatalf("%q: want error", bad)
		}
	}
}

func TestShowConfigJSONMasks(t *testing.T) {
	var b strings.Builder
	cfgPath = "/nonexistent/config.env"
	if err := showConfigJSON(&b, map[string]string{"RUNPOD_API_KEY": "rpa_SECRETSECRETSECRET", "LOBO_DOMAIN": "lobo.x.cc", "LOBO_CTX": ""}); err != nil {
		t.Fatal(err)
	}
	s := b.String()
	if strings.Contains(s, "SECRETSECRET") || !strings.Contains(s, `"LOBO_DOMAIN":"lobo.x.cc"`) || !strings.Contains(s, `"LOBO_CTX":false`) || !strings.Contains(s, `"exists":false`) {
		t.Fatal(s)
	}
}

func TestParseSetJSON(t *testing.T) {
	kv, err := parseSetJSON(strings.NewReader(`{"RUNPOD_API_KEY":"rpa_x","LOBO_CTX":""}`))
	if err != nil || kv["RUNPOD_API_KEY"] != "rpa_x" || kv["LOBO_CTX"] != "" || len(kv) != 2 {
		t.Fatalf("%v %v", kv, err)
	}
	for _, bad := range []string{`{"lower":"1"}`, `{}`, `[1]`, `{"A":1}`, `nope`} {
		if _, err := parseSetJSON(strings.NewReader(bad)); err == nil {
			t.Fatalf("%s: want error", bad)
		}
	}
}
