package config

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"
)

const fullEnv = `RUNPOD_API_KEY=rp
LOBO_API_KEY=sk-x
CF_TUNNEL_TOKEN=tok
LOBO_DOMAIN=lobo.example.com
LOBO_BUCKET_URL=https://pub-x.r2.dev
R2_ACCOUNT_ID=acc
R2_ACCESS_KEY=ak
R2_SECRET_KEY=sk
R2_ENDPOINT=https://acc.r2.cloudflarestorage.com
`

func writeEnv(t *testing.T, s string) string {
	p := filepath.Join(t.TempDir(), ".env")
	if err := os.WriteFile(p, []byte(s), 0o600); err != nil {
		t.Fatal(err)
	}
	return p
}

func TestLoadLaptop(t *testing.T) {
	l, err := LoadLaptop(writeEnv(t, fullEnv))
	if err != nil {
		t.Fatal(err)
	}
	if l.RunPodAPIKey != "rp" || l.R2.SecretKey != "sk" || l.Domain != "lobo.example.com" {
		t.Fatalf("%+v", l)
	}
	if err := l.RequireR2(); err != nil {
		t.Fatal(err)
	}
	sv := l.SecretValues()
	for _, k := range []string{"LOBO_API_KEY", "CF_TUNNEL_TOKEN", "RUNPOD_API_KEY", "R2_ACCESS_KEY", "R2_SECRET_KEY"} {
		if sv[k] == "" {
			t.Fatalf("missing %s", k)
		}
	}
}

func TestLoadLaptopMissing(t *testing.T) {
	t.Setenv("RUNPOD_API_KEY", "from-os")
	noRP := strings.Replace(fullEnv, "RUNPOD_API_KEY=rp\n", "", 1)
	_, err := LoadLaptop(writeEnv(t, noRP))
	if err == nil || !strings.Contains(err.Error(), "RUNPOD_API_KEY") {
		t.Fatalf("want RUNPOD_API_KEY error (OS env must be ignored), got %v", err)
	}
}

func TestLoadLaptopVastOnly(t *testing.T) {
	vastOnly := strings.Replace(fullEnv, "RUNPOD_API_KEY=rp\n", "VASTAI_API_KEY=vk\n", 1)
	l, err := LoadLaptop(writeEnv(t, vastOnly))
	if err != nil || l.VastAPIKey != "vk" || l.RunPodAPIKey != "" {
		t.Fatalf("%+v %v", l, err)
	}
}

func TestLoadLaptopNoR2(t *testing.T) {
	var keep []string
	for _, line := range strings.Split(fullEnv, "\n") {
		if !strings.HasPrefix(line, "R2_") {
			keep = append(keep, line)
		}
	}
	l, err := LoadLaptop(writeEnv(t, strings.Join(keep, "\n")))
	if err != nil {
		t.Fatal(err)
	}
	if err := l.RequireR2(); err == nil || !strings.Contains(err.Error(), "R2_ACCESS_KEY") {
		t.Fatalf("got %v", err)
	}
}

func setAgentEnv(t *testing.T) {
	for k, v := range map[string]string{
		"LOBO_API_KEY": "sk", "CF_TUNNEL_TOKEN": "tok", "LOBO_MODEL": "q8",
		"LOBO_MODEL_URL": "https://pub-x.r2.dev/models/f.gguf", "RUNPOD_POD_ID": "pod1",
		"RUNPOD_API_KEY": "podkey", "LOBO_EXPIRES_AT": "2026-09-23T20:00:00Z",
	} {
		t.Setenv(k, v)
	}
}

func TestLoadAgent(t *testing.T) {
	setAgentEnv(t)
	a, err := LoadAgent()
	if err != nil {
		t.Fatal(err)
	}
	if a.Ctx != 8192 || a.IdleMin != 30 || a.BootTimeout != 40*time.Minute || a.ExpiresAt.Hour() != 20 {
		t.Fatalf("%+v", a)
	}
}

func TestLoadAgentErrors(t *testing.T) {
	tests := []struct{ key, val, want string }{
		{"LOBO_CTX", "abc", "LOBO_CTX"},
		{"LOBO_EXPIRES_AT", "tomorrow", "LOBO_EXPIRES_AT"},
		{"LOBO_EXPIRES_AT", "", "LOBO_EXPIRES_AT"},
		{"LOBO_MODEL_URL", "", "LOBO_MODEL_URL"},
	}
	for _, tt := range tests {
		t.Run(tt.key+"="+tt.val, func(t *testing.T) {
			setAgentEnv(t)
			t.Setenv(tt.key, tt.val)
			_, err := LoadAgent()
			if err == nil || !strings.Contains(err.Error(), tt.want) {
				t.Fatalf("got %v", err)
			}
		})
	}
}

func TestSetEnvValue(t *testing.T) {
	p := writeEnv(t, "# comment\nA=1\nLOBO_API_KEY=old\nB=2\n")
	if err := SetEnvValue(p, "LOBO_API_KEY", "new"); err != nil {
		t.Fatal(err)
	}
	if err := SetEnvValue(p, "C", "3"); err != nil {
		t.Fatal(err)
	}
	b, _ := os.ReadFile(p)
	if string(b) != "# comment\nA=1\nLOBO_API_KEY=new\nB=2\nC=3\n" {
		t.Fatalf("%q", b)
	}
	if fi, _ := os.Stat(p); fi.Mode().Perm() != 0o600 {
		t.Fatal(fi.Mode())
	}
}

func TestLoadAgentPerProvider(t *testing.T) {
	setAgentEnv(t)
	t.Setenv("LOBO_PROVIDER", "vast")
	t.Setenv("RUNPOD_POD_ID", "")
	t.Setenv("RUNPOD_API_KEY", "")
	if _, err := LoadAgent(); err == nil || !strings.Contains(err.Error(), "CONTAINER_API_KEY") {
		t.Fatal(err)
	}
	t.Setenv("CONTAINER_ID", "52607650")
	t.Setenv("CONTAINER_API_KEY", "k")
	a, err := LoadAgent()
	if err != nil || a.Provider != "vast" || a.VastID != "52607650" {
		t.Fatal(a, err)
	}
	t.Setenv("LOBO_PROVIDER", "runpod")
	if _, err := LoadAgent(); err == nil || !strings.Contains(err.Error(), "RUNPOD_POD_ID") {
		t.Fatal(err)
	}
	t.Setenv("LOBO_PROVIDER", "aws")
	if _, err := LoadAgent(); err == nil || !strings.Contains(err.Error(), "LOBO_PROVIDER") {
		t.Fatal(err)
	}
}

func TestSaveKeepsHandEdits(t *testing.T) {
	p := writeEnv(t, "# my notes\nRUNPOD_API_KEY=old\nCUSTOM_THING=1\nexport LOBO_DOMAIN=a.b\n")
	if err := Save(p, map[string]string{"RUNPOD_API_KEY": "new", "LOBO_DOMAIN": "", "LOBO_MODEL_SSH_HOSTKEY": "ssh-ed25519 AAAA#x", "LOBO_MIN_MBPS": "150"}); err != nil {
		t.Fatal(err)
	}
	b, _ := os.ReadFile(p)
	want := "# my notes\nRUNPOD_API_KEY=new\nCUSTOM_THING=1\nLOBO_MIN_MBPS=150\nLOBO_MODEL_SSH_HOSTKEY=\"ssh-ed25519 AAAA#x\"\n"
	if string(b) != want {
		t.Fatalf("got:\n%s\nwant:\n%s", b, want)
	}
	v, err := Values(p)
	if err != nil || v["LOBO_MODEL_SSH_HOSTKEY"] != "ssh-ed25519 AAAA#x" || v["LOBO_DOMAIN"] != "" {
		t.Fatalf("%v %v", v, err)
	}
	if fi, _ := os.Stat(p); fi.Mode().Perm() != 0o600 {
		t.Fatal(fi.Mode())
	}
}

func TestSaveNewFileLayout(t *testing.T) {
	p := filepath.Join(t.TempDir(), "sub", "config.env")
	if err := Save(p, map[string]string{"VASTAI_API_KEY": "vk", "LOBO_PROVIDER": "vast", "R2_ENDPOINT": "https://e"}); err != nil {
		t.Fatal(err)
	}
	b, _ := os.ReadFile(p)
	s := string(b)
	if !strings.HasPrefix(s, "# lobo config") || strings.Index(s, "VASTAI_API_KEY=vk") > strings.Index(s, "LOBO_PROVIDER=vast") ||
		!strings.HasSuffix(s, "R2_ENDPOINT=https://e\n") || strings.Contains(s, "RUNPOD_API_KEY=") {
		t.Fatal(s)
	}
	if fi, _ := os.Stat(filepath.Dir(p)); fi.Mode().Perm() != 0o700 {
		t.Fatal(fi.Mode())
	}
	if v, _ := Values(filepath.Join(t.TempDir(), "missing")); len(v) != 0 {
		t.Fatal(v)
	}
}

func TestDefaults(t *testing.T) {
	l := Laptop{Provider: "vast", Model: "q6", Cloud: "community", Ctx: "32768", MinMBps: "200", VastMaxDPH: "0.9"}
	d, err := l.Defaults()
	if err != nil || d != (Defaults{Provider: "vast", Model: "q6", Cloud: "community", Ctx: 32768, MinMBps: 200, VastMaxDPH: 0.9}) {
		t.Fatalf("%+v %v", d, err)
	}
	for _, bad := range []Laptop{{Provider: "aws"}, {Model: "q4"}, {Cloud: "x"}, {Ctx: "100"}, {IdleMin: "-1"}, {VastMaxDPH: "0"}} {
		if _, err := bad.Defaults(); err == nil {
			t.Fatalf("%+v: want error", bad)
		}
	}
}

func TestDefaultProvider(t *testing.T) {
	for _, c := range []struct {
		l    Laptop
		want string
	}{
		{Laptop{RunPodAPIKey: "r"}, "runpod"},
		{Laptop{VastAPIKey: "v"}, "vast"},
		{Laptop{RunPodAPIKey: "r", VastAPIKey: "v"}, "runpod"},
		{Laptop{RunPodAPIKey: "r", VastAPIKey: "v", Provider: "vast"}, "vast"},
		{Laptop{RunPodAPIKey: "r", Provider: "vast"}, "runpod"}, // no Vast key: fall back
	} {
		if got := c.l.DefaultProvider(); got != c.want {
			t.Fatalf("%+v: got %s want %s", c.l, got, c.want)
		}
	}
}

func TestDefaultPath(t *testing.T) {
	t.Setenv("XDG_CONFIG_HOME", "/x")
	if DefaultPath() != "/x/lobo/config.env" {
		t.Fatal(DefaultPath())
	}
	t.Setenv("XDG_CONFIG_HOME", "")
	if !strings.HasSuffix(DefaultPath(), "/.config/lobo/config.env") {
		t.Fatal(DefaultPath())
	}
}

func TestSaveRoundTripSpecialValues(t *testing.T) {
	vals := map[string]string{"A": "sk-$MISSING", "B": `a\b`, "C": "x\ny", "D": `q"uo'te`, "E": "p #q", "F": "plain-123"}
	p := filepath.Join(t.TempDir(), "c.env")
	if err := Save(p, vals); err != nil {
		t.Fatal(err)
	}
	got, err := Values(p)
	if err != nil {
		t.Fatal(err)
	}
	for k, v := range vals {
		if got[k] != v {
			t.Fatalf("%s: wrote %q, read back %q", k, v, got[k])
		}
	}
}

// Bad launch defaults must not block loading: `lobo down` has to work with them.
func TestLoadLaptopIgnoresBadDefaults(t *testing.T) {
	if _, err := LoadLaptop(writeEnv(t, fullEnv+"LOBO_CTX=100\nLOBO_MODEL=q4\n")); err != nil {
		t.Fatal(err)
	}
}
