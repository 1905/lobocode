package runpod

import (
	"context"
	"encoding/json"
	"errors"
	"github.com/1905/lobocode/internal/provider"
	"io"
	"net/http"
	"net/http/httptest"
	"os"
	"sort"
	"strings"
	"testing"
	"time"
)

func opts() provider.CreateOpts {
	return provider.CreateOpts{Image: "img", ReleaseURL: "https://b/r.zip", ReleaseSHA256: "abc", ModelURL: "https://b/m.gguf",
		LoboAPIKey: "sk", CFTunnelToken: "tok", Model: "q8", Ctx: 8192, IdleMin: 30,
		ExpiresAt: time.Date(2026, 9, 23, 22, 0, 0, 0, time.UTC)}
}

func TestBuildCreatePayload(t *testing.T) {
	p := BuildCreatePayload(opts(), "COMMUNITY", 0)
	if p["volumeInGb"] != 0 || len(p["ports"].([]string)) != 0 || p["cloudType"] != "COMMUNITY" || p["name"] != "lobo" {
		t.Fatalf("%v", p)
	}
	if g := p["gpuTypeIds"].([]string); len(g) != 1 || g[0] != "NVIDIA GeForce RTX 5090" {
		t.Fatal(g)
	}
	env := p["env"].(map[string]string)
	var keys []string
	for k := range env {
		keys = append(keys, k)
	}
	sort.Strings(keys)
	want := "CF_TUNNEL_TOKEN LOBO_API_KEY LOBO_BOOT_TIMEOUT LOBO_CTX LOBO_EXPIRES_AT LOBO_IDLE_MIN LOBO_MODEL LOBO_MODEL_URL LOBO_PROVIDER LOBO_RELEASE_SHA256 LOBO_RELEASE_URL"
	if strings.Join(keys, " ") != want {
		t.Fatalf("env keys %v", keys)
	}
	if env["LOBO_EXPIRES_AT"] != "2026-09-23T22:00:00Z" {
		t.Fatal(env["LOBO_EXPIRES_AT"])
	}
	b, _ := json.Marshal(p)
	if strings.Contains(string(b), "R2_") || strings.Contains(string(b), "RUNPOD_API_KEY\":") {
		t.Fatal("payload must not carry R2 or RunPod account keys")
	}
	cmd := p["dockerStartCmd"].([]string)[0]
	for _, s := range []string{"sha256sum -c", "exec /lobo/lobo-agent", "trap die ERR", "podTerminate", "$LOBO_RELEASE_URL"} {
		if !strings.Contains(cmd, s) {
			t.Errorf("bootstrap missing %q", s)
		}
	}
}

func TestPodFixture(t *testing.T) {
	b, err := os.ReadFile("testdata/pod.json")
	if err != nil {
		t.Fatal(err)
	}
	var p Pod
	if err := json.Unmarshal(b, &p); err != nil {
		t.Fatal(err)
	}
	if p.CostPerHr != 0.69 || p.DesiredStatus != "RUNNING" || p.LastStartedAt.Hour() != 8 || p.PortMappings["22"] != 18180 {
		t.Fatalf("%+v", p)
	}
}

func TestClient(t *testing.T) {
	fixture, _ := os.ReadFile("testdata/pod.json")
	var calls []string
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		calls = append(calls, r.Method+" "+r.URL.Path)
		if r.Header.Get("Authorization") != "Bearer key" {
			w.WriteHeader(401)
			return
		}
		switch {
		case r.Method == "POST" && r.URL.Path == "/pods":
			body, _ := io.ReadAll(r.Body)
			if strings.Contains(string(body), `"SECURE"`) {
				_, _ = w.Write(fixture)
				return
			}
			w.WriteHeader(500)
			_, _ = w.Write([]byte(`{"error":"create pod: There are no instances currently available","status":500}`))
		case r.Method == "GET" && r.URL.Path == "/pods":
			_, _ = w.Write([]byte("[" + string(fixture) + "]"))
		case r.URL.Path == "/pods/gone":
			w.WriteHeader(404)
		case r.Method == "GET":
			_, _ = w.Write(fixture)
		case r.Method == "DELETE":
			w.WriteHeader(200)
		}
	}))
	defer srv.Close()
	c := New("key")
	c.BaseURL = srv.URL
	ctx := context.Background()
	if _, err := c.Create(ctx, opts(), "COMMUNITY", 0); !errors.Is(err, ErrNoCapacity) {
		t.Fatalf("want ErrNoCapacity, got %v", err)
	}
	if p, err := c.Create(ctx, opts(), "SECURE", 0); err != nil || p.ID == "" {
		t.Fatal(p, err)
	}
	if ps, err := c.List(ctx); err != nil || len(ps) != 1 {
		t.Fatal(ps, err)
	}
	if _, err := c.Get(ctx, "gone"); !errors.Is(err, ErrNotFound) {
		t.Fatal(err)
	}
	if err := c.Delete(ctx, "gone"); err != nil {
		t.Fatal("delete of a gone pod must be nil:", err)
	}
	if err := c.Delete(ctx, "x"); err != nil {
		t.Fatal(err)
	}
}

func TestSelf(t *testing.T) {
	var queries []string
	status := `{"data":{"pod":{"desiredStatus":"RUNNING"}}}`
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		var q struct{ Query string }
		_ = json.NewDecoder(r.Body).Decode(&q)
		queries = append(queries, q.Query)
		if strings.HasPrefix(q.Query, "mutation") {
			_, _ = w.Write([]byte(`{"data":{"podTerminate":null}}`))
			return
		}
		_, _ = w.Write([]byte(status))
	}))
	defer srv.Close()
	s := NewSelf("pod1", "k")
	s.URL = srv.URL
	ctx := context.Background()
	if err := s.Terminate(ctx); err != nil || !strings.Contains(queries[0], `podTerminate(input:{podId:"pod1"})`) {
		t.Fatal(err, queries)
	}
	if gone, err := s.Gone(ctx); err != nil || gone {
		t.Fatal(gone, err)
	}
	status = `{"data":{"pod":null}}`
	if gone, err := s.Gone(ctx); err != nil || !gone {
		t.Fatal(gone, err)
	}
	status = `{"errors":[{"message":"Unauthorized"}],"data":null}`
	if _, err := s.Gone(ctx); err == nil {
		t.Fatal("want error")
	}
}

func TestTimeRoundTrip(t *testing.T) {
	var p Pod
	_ = json.Unmarshal([]byte(`{"lastStartedAt":"2026-09-23 08:48:28.204 +0000 UTC"}`), &p)
	b, _ := json.Marshal(p)
	var q Pod
	if err := json.Unmarshal(b, &q); err != nil || !q.LastStartedAt.Equal(p.LastStartedAt.Time) {
		t.Fatal(err, string(b))
	}
}

func TestPayloadMinDownload(t *testing.T) {
	o := opts()
	if _, ok := BuildCreatePayload(o, "SECURE", 0)["minDownloadMbps"]; ok {
		t.Fatal("no filter by default")
	}
	if BuildCreatePayload(o, "SECURE", 5000)["minDownloadMbps"] != 5000.0 {
		t.Fatal(BuildCreatePayload(o, "SECURE", 5000)["minDownloadMbps"])
	}
}
