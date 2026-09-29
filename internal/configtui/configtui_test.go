package configtui

import (
	"strings"
	"testing"
)

func TestMask(t *testing.T) {
	for in, want := range map[string]string{"": "(not set)", "short": "••••", "rpa_ABCDEFGHIJKLMNOP": "rpa_…MNOP"} {
		if got := Mask(in); got != want {
			t.Fatalf("%q: %q", in, got)
		}
	}
}

func TestResultKeepClearAndDefaults(t *testing.T) {
	cur := map[string]string{"RUNPOD_API_KEY": "rp-old-key-123456", "VASTAI_API_KEY": "vast-old-key-1234", "LOBO_API_KEY": "sk-old",
		"CF_TUNNEL_TOKEN": "tok", "LOBO_DOMAIN": "lobo.x.cc", "LOBO_BUCKET_URL": "https://b", "LOBO_PROVIDER": "vast", "LOBO_MIN_MBPS": "150"}
	s := newState(cur, false)
	// untouched: every value kept, built-in defaults dropped
	r := s.result("sk-new")
	if r["RUNPOD_API_KEY"] != cur["RUNPOD_API_KEY"] || r["LOBO_API_KEY"] != "sk-old" || r["LOBO_PROVIDER"] != "vast" ||
		r["LOBO_MIN_MBPS"] != "150" || r["LOBO_MODEL"] != "" || r["LOBO_CLOUD"] != "" {
		t.Fatalf("%v", r)
	}
	// clear Vast with "-": provider pick and Vast price go away; new API key
	s.Vast, s.APIKey, s.MinMBps, s.Ctx = "-", "new", "100", "0"
	r = s.result("sk-new")
	if r["VASTAI_API_KEY"] != "" || r["LOBO_PROVIDER"] != "" || r["LOBO_API_KEY"] != "sk-new" || r["LOBO_MIN_MBPS"] != "" || r["LOBO_CTX"] != "" {
		t.Fatalf("%v", r)
	}
	// new RunPod key typed
	s.Runpod, s.Cloud = " rp-new ", "secure"
	if r = s.result(""); r["RUNPOD_API_KEY"] != "rp-new" || r["LOBO_CLOUD"] != "secure" {
		t.Fatalf("%v", r)
	}
}

func TestNewStateFreshFile(t *testing.T) {
	s := newState(map[string]string{}, false)
	if s.APIKey != "new" || s.Provider != "runpod" || s.Model != "q8" || s.Cloud != "community" {
		t.Fatalf("%+v", s)
	}
	if k := NewAPIKey(); !strings.HasPrefix(k, "sk-") || len(k) != 51 {
		t.Fatal(k)
	}
}

func TestValidators(t *testing.T) {
	if wholeNumber(512)("100") == nil || wholeNumber(512)("") != nil || wholeNumber(512)("0") != nil || wholeNumber(1)("abc") == nil {
		t.Fatal("wholeNumber")
	}
	if positiveFloat("0") == nil || positiveFloat("1.2") != nil {
		t.Fatal("positiveFloat")
	}
	if hostname("https://x.cc") == nil || hostname("lobo.x.cc") != nil {
		t.Fatal("hostname")
	}
	if httpsURL("pub.r2.dev") == nil || httpsURL("https://pub.r2.dev") != nil {
		t.Fatal("httpsURL")
	}
}

func TestSummaryMasksSecrets(t *testing.T) {
	s := newState(map[string]string{"RUNPOD_API_KEY": "rpa_SECRETSECRETSECRET", "CF_TUNNEL_TOKEN": "eyJTOKENTOKENTOKEN"}, false)
	sum := s.summary()
	if strings.Contains(sum, "SECRETSECRET") || strings.Contains(sum, "TOKENTOKEN") || !strings.Contains(sum, "rpa_…CRET") {
		t.Fatal(sum)
	}
}

func TestNewStateLocal(t *testing.T) {
	tests := []struct {
		name     string
		cur      map[string]string
		localOK  bool
		provider string
	}{
		{name: "fresh file, Apple Silicon: local first", cur: map[string]string{}, localOK: true, provider: "local"},
		{name: "fresh file, not Apple Silicon", cur: map[string]string{}, provider: "runpod"},
		{name: "runpod key, Apple Silicon: keep runpod", cur: map[string]string{"RUNPOD_API_KEY": "rp"}, localOK: true, provider: "runpod"},
		{name: "saved local", cur: map[string]string{"RUNPOD_API_KEY": "rp", "LOBO_PROVIDER": "local"}, localOK: true, provider: "local"},
		{name: "saved local, not Apple Silicon", cur: map[string]string{"LOBO_PROVIDER": "local"}, provider: "runpod"},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if s := newState(tt.cur, tt.localOK); s.Provider != tt.provider {
				t.Fatalf("provider %q want %q", s.Provider, tt.provider)
			}
		})
	}
}

func TestResultLocal(t *testing.T) {
	tests := []struct {
		name                    string
		cur                     map[string]string
		weights, port, provider string
		want                    map[string]string
	}{
		{name: "local only, defaults", cur: map[string]string{"LOBO_API_KEY": "sk"}, port: "8931", provider: "local",
			want: map[string]string{"LOBO_PROVIDER": "local", "LOBO_WEIGHTS_DIR": "", "LOBO_LOCAL_PORT": "", "RUNPOD_API_KEY": "", "LOBO_DOMAIN": ""}},
		{name: "local, own folder and port", cur: map[string]string{"LOBO_API_KEY": "sk"}, weights: " /Volumes/Extreme/_lobocode ", port: "9000", provider: "local",
			want: map[string]string{"LOBO_PROVIDER": "local", "LOBO_WEIGHTS_DIR": "/Volumes/Extreme/_lobocode", "LOBO_LOCAL_PORT": "9000"}},
		{name: "local wins over two cloud keys", cur: map[string]string{"LOBO_API_KEY": "sk", "RUNPOD_API_KEY": "rp", "VASTAI_API_KEY": "v"}, provider: "local",
			want: map[string]string{"LOBO_PROVIDER": "local"}},
		{name: "runpod picked, one key: no LOBO_PROVIDER", cur: map[string]string{"LOBO_API_KEY": "sk", "RUNPOD_API_KEY": "rp"}, port: "0", provider: "runpod",
			want: map[string]string{"LOBO_PROVIDER": "", "LOBO_LOCAL_PORT": ""}},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			s := newState(tt.cur, true)
			s.Weights, s.Port, s.Provider = tt.weights, tt.port, tt.provider
			r := s.result("sk-new")
			for k, v := range tt.want {
				if r[k] != v {
					t.Fatalf("%s = %q want %q (%v)", k, r[k], v, r)
				}
			}
		})
	}
}

func TestLocalValidation(t *testing.T) {
	for _, tt := range []struct {
		name     string
		provider string
		localOK  bool
		wantErr  bool
	}{
		{name: "local: cloud fields optional", provider: "local", localOK: true},
		{name: "runpod: cloud fields required", provider: "runpod", localOK: true, wantErr: true},
		{name: "local picked but not Apple Silicon", provider: "local", wantErr: true},
	} {
		t.Run(tt.name, func(t *testing.T) {
			s := newState(map[string]string{}, tt.localOK)
			s.Provider = tt.provider
			for field, err := range map[string]error{
				"provider key": s.providerKeys(""),
				"domain":       s.cloudOnly(hostname)(""),
				"tunnel":       s.tunnelToken(""),
				"bucket":       s.cloudOnly(httpsURL)(""),
			} {
				if (err != nil) != tt.wantErr {
					t.Fatalf("%s: err %v, wantErr %v", field, err, tt.wantErr)
				}
			}
			// A value that is set is still checked, local or not.
			if s.cloudOnly(hostname)("https://x.cc") == nil || s.cloudOnly(httpsURL)("pub.r2.dev") == nil {
				t.Fatal("bad values must fail")
			}
		})
	}
	if localPort("") != nil || localPort("0") != nil || localPort("9000") != nil || localPort("80") == nil || localPort("65535") == nil || localPort("x") == nil {
		t.Fatal("localPort")
	}
}

func TestSummaryLocalRows(t *testing.T) {
	s := newState(map[string]string{"LOBO_WEIGHTS_DIR": "/Volumes/Extreme/_lobocode", "LOBO_LOCAL_PORT": "9000"}, true)
	sum := s.summary()
	for _, w := range []string{"Weights folder", "/Volumes/Extreme/_lobocode", "Local port", "9000", "local"} {
		if !strings.Contains(sum, w) {
			t.Fatalf("want %q in\n%s", w, sum)
		}
	}
}

func TestProviderOptions(t *testing.T) {
	if got := len(providerOptions(true)); got != 3 {
		t.Fatalf("Apple Silicon: %d options", got)
	}
	opts := providerOptions(false)
	for _, o := range opts {
		if o.Value == "local" {
			t.Fatal("local offered on an unsupported machine")
		}
	}
}

func TestFormBuilds(t *testing.T) {
	for _, localOK := range []bool{false, true} {
		if f := newState(map[string]string{}, localOK).form("/tmp/config.env"); f == nil {
			t.Fatalf("localOK=%v: nil form", localOK)
		}
	}
}
