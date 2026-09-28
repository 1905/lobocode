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
	s := newState(cur)
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
	s := newState(map[string]string{})
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
	s := newState(map[string]string{"RUNPOD_API_KEY": "rpa_SECRETSECRETSECRET", "CF_TUNNEL_TOKEN": "eyJTOKENTOKENTOKEN"})
	sum := s.summary()
	if strings.Contains(sum, "SECRETSECRET") || strings.Contains(sum, "TOKENTOKEN") || !strings.Contains(sum, "rpa_…CRET") {
		t.Fatal(sum)
	}
}
