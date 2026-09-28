package agent

import (
	"encoding/json"
	"io"
	"net/http"
	"net/http/httptest"
	"sort"
	"strings"
	"testing"
)

func TestAPI(t *testing.T) {
	logs := NewLogRing(2000)
	for i := 0; i < 1500; i++ {
		_, _ = logs.Write([]byte("line\n"))
	}
	ver := []byte(`{"version":"2026.09.23-1"}`)
	srv := httptest.NewServer(NewAPI("sk", ver, func() Status { return Status{Stage: StageReady, Model: "q8"} }, logs))
	defer srv.Close()
	get := func(path, auth string) (int, string) {
		req, _ := http.NewRequest("GET", srv.URL+path, nil)
		if auth != "" {
			req.Header.Set("Authorization", auth)
		}
		resp, err := http.DefaultClient.Do(req)
		if err != nil {
			t.Fatal(err)
		}
		defer resp.Body.Close()
		b, _ := io.ReadAll(resp.Body)
		return resp.StatusCode, string(b)
	}
	if c, b := get("/api/version", ""); c != 200 || b != string(ver) {
		t.Fatal(c, b)
	}
	c, b := get("/api/status", "")
	if c != 200 {
		t.Fatal(c)
	}
	var m map[string]any
	_ = json.Unmarshal([]byte(b), &m)
	var keys []string
	for k := range m {
		keys = append(keys, k)
	}
	sort.Strings(keys)
	want := "ctx download expires_at gpu host idle_s kill_in_s kill_reason llama metrics_failures model stage stage_detail timings uptime_s"
	if strings.Join(keys, " ") != want {
		t.Fatalf("keys: %v", keys)
	}
	if m["gpu"] != nil || m["llama"] != nil {
		t.Fatal("nil sections must be null")
	}
	for _, auth := range []string{"", "Bearer nope"} {
		if c, _ := get("/api/logs", auth); c != 401 {
			t.Fatal(auth, c)
		}
	}
	if _, b := get("/api/logs?n=5", "Bearer sk"); strings.Count(b, "line") != 5 {
		t.Fatal(b)
	}
	if _, b := get("/api/logs?n=5000", "Bearer sk"); strings.Count(b, "line") != 1000 {
		t.Fatal(strings.Count(b, "line"))
	}
}
