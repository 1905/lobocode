package bootstrap

import (
	"os"
	"os/exec"
	"path/filepath"
	"sort"
	"strings"
	"testing"
	"time"

	"github.com/1905/lobocode/internal/provider"
)

func TestScriptPerProvider(t *testing.T) {
	for p, want := range map[string]string{"runpod": "podTerminate", "vast": "$CONTAINER_API_KEY"} {
		s := Script(p)
		for _, must := range []string{want, "trap die ERR", "sha256sum -c", "exec /lobo/lobo-agent", "$LOBO_RELEASE_URL"} {
			if !strings.Contains(s, must) {
				t.Errorf("%s script missing %q", p, must)
			}
		}
	}
	if strings.Contains(Script("vast"), "RUNPOD") || strings.Contains(Script("runpod"), "CONTAINER_API_KEY") {
		t.Fatal("die() must use only its own provider's credentials")
	}
}

func TestEnv(t *testing.T) {
	o := provider.CreateOpts{ReleaseURL: "r", ReleaseSHA256: "s", ModelURL: "m", LoboAPIKey: "k", CFTunnelToken: "t", Model: "q8",
		Ctx: 65536, IdleMin: 30, ExpiresAt: time.Date(2026, 9, 25, 22, 0, 0, 0, time.UTC), DLConns: 32, MinMBps: 100, ModelFallback: "f"}
	env := Env(o, "vast")
	var keys []string
	for k := range env {
		keys = append(keys, k)
	}
	sort.Strings(keys)
	want := "CF_TUNNEL_TOKEN LOBO_API_KEY LOBO_BOOT_TIMEOUT LOBO_CTX LOBO_DL_CONNS LOBO_EXPIRES_AT LOBO_IDLE_MIN LOBO_MIN_MBPS LOBO_MODEL LOBO_MODEL_URL LOBO_MODEL_URL_FALLBACK LOBO_PROVIDER LOBO_RELEASE_SHA256 LOBO_RELEASE_URL"
	if strings.Join(keys, " ") != want || env["LOBO_PROVIDER"] != "vast" || env["LOBO_EXPIRES_AT"] != "2026-09-25T22:00:00Z" {
		t.Fatalf("%v", keys)
	}
}

func TestEnvBakedOmitsRelease(t *testing.T) {
	env := Env(provider.CreateOpts{ModelURL: "m"}, "runpod")
	if _, ok := env["LOBO_RELEASE_URL"]; ok {
		t.Fatal(env)
	}
	if _, ok := env["LOBO_RELEASE_SHA256"]; ok {
		t.Fatal(env)
	}
}

// Baked image: no LOBO_RELEASE_URL and an executable agent → no apt, no curl, exec the agent.
func TestScriptBaked(t *testing.T) {
	bash, err := exec.LookPath("bash")
	if err != nil {
		t.Skip("no bash")
	}
	dir := t.TempDir()
	log := filepath.Join(dir, "calls.log")
	for name, body := range map[string]string{
		"apt-get":    "echo apt >> " + log,
		"curl":       "echo curl >> " + log,
		"lobo-agent": `echo "agent $LOBO_T_APT $LOBO_T_ZIP $LOBO_T_BOOT0" >> ` + log,
	} {
		if err := os.WriteFile(filepath.Join(dir, name), []byte("#!/bin/sh\n"+body+"\n"), 0o755); err != nil {
			t.Fatal(err)
		}
	}
	cmd := exec.Command(bash, "-c", strings.ReplaceAll(Script("runpod"), "/lobo/lobo-agent", filepath.Join(dir, "lobo-agent")))
	cmd.Env = []string{"PATH=" + dir + ":/usr/bin:/bin"}
	if out, err := cmd.CombinedOutput(); err != nil {
		t.Fatalf("%v\n%s", err, out)
	}
	b, _ := os.ReadFile(log)
	f := strings.Fields(string(b))
	if len(f) != 4 || f[0] != "agent" || f[1] != f[3] || f[2] != f[3] {
		t.Fatalf("want only the agent, timings = boot0; got %q", b)
	}
}

// runScript runs the real bootstrap under bash with faked tools. curl for the release fails when
// releaseFails; the terminate call answers with termAnswer. Returns the curl call log and stderr.
func runScript(t *testing.T, prov string, releaseFails bool, termAnswer string) (calls, stderr string) {
	t.Helper()
	bash, err := exec.LookPath("bash")
	if err != nil {
		t.Skip("no bash")
	}
	dir := t.TempDir()
	log := filepath.Join(dir, "curl.log")
	fake := map[string]string{
		"apt-get":   "exit 0",
		"timeout":   `shift; exec "$@"`,
		"sleep":     "exit 0",
		"sha256sum": "exit 0",
		"unzip":     "exit 0",
		"curl": `echo "$*" >> ` + log + `
case "$*" in *lobo-release*) [ "$RELEASE_FAILS" = 1 ] && exit 22; exit 0 ;; esac
case "$*" in *http_code*) printf '%s' "$TERM_ANSWER"; exit 0 ;; esac
printf '%s' "$TERM_ANSWER"`,
	}
	for name, body := range fake {
		if err := os.WriteFile(filepath.Join(dir, name), []byte("#!/bin/sh\n"+body+"\n"), 0o755); err != nil {
			t.Fatal(err)
		}
	}
	cmd := exec.Command(bash, "-c", Script(prov))
	rf := "0"
	if releaseFails {
		rf = "1"
	}
	cmd.Env = []string{"PATH=" + dir + ":/usr/bin:/bin", "RELEASE_FAILS=" + rf, "TERM_ANSWER=" + termAnswer,
		"LOBO_RELEASE_URL=https://x/lobo-release.zip", "LOBO_RELEASE_SHA256=abc", "RUNPOD_POD_ID=p1", "RUNPOD_API_KEY=k", "CONTAINER_ID=7", "CONTAINER_API_KEY=k"}
	var se strings.Builder
	cmd.Stderr = &se
	_ = cmd.Run()
	b, _ := os.ReadFile(log)
	return string(b), se.String()
}

func TestScriptTerminatesOnFailure(t *testing.T) {
	// release fetch fails → one accepted terminate
	calls, se := runScript(t, "runpod", true, `{"data":{"podTerminate":null}}`)
	if strings.Count(calls, "podTerminate") != 1 || !strings.Contains(se, "terminate accepted") {
		t.Fatalf("calls:\n%s\nstderr:\n%s", calls, se)
	}
	// GraphQL error → retried, never reported accepted
	calls, se = runScript(t, "runpod", true, `{"errors":[{"message":"nope"}]}`)
	if strings.Count(calls, "podTerminate") != 30 || strings.Contains(se, "terminate accepted") {
		t.Fatalf("want 30 retries, got %d\n%s", strings.Count(calls, "podTerminate"), se)
	}
	// vast: 500 then give up after retries; 404 = gone = accepted
	calls, _ = runScript(t, "vast", true, "500")
	if strings.Count(calls, "-X DELETE") != 30 {
		t.Fatalf("vast 500: %d deletes", strings.Count(calls, "-X DELETE"))
	}
	if _, se = runScript(t, "vast", true, "404"); !strings.Contains(se, "terminate accepted") {
		t.Fatal(se)
	}
	// release fine but the agent binary can't be exec'd → still terminates
	calls, se = runScript(t, "vast", false, "200")
	if strings.Count(calls, "-X DELETE") != 1 || !strings.Contains(se, "terminate accepted") {
		t.Fatalf("exec failure: calls:\n%s\nstderr:\n%s", calls, se)
	}
}
