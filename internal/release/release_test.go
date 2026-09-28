package release

import (
	"archive/zip"
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"testing"
	"time"
)

func TestNextVersion(t *testing.T) {
	day := time.Date(2026, 9, 23, 23, 0, 0, 0, time.UTC)
	tests := []struct {
		keys []string
		want string
	}{
		{nil, "2026.09.23-1"},
		{[]string{"releases/lobo-2026.09.23-1.zip", "releases/lobo-2026.09.23-2.zip", "releases/lobo-2026.09.23-2.json"}, "2026.09.23-3"},
		{[]string{"releases/lobo-2026.09.22-7.zip", "releases/latest.json"}, "2026.09.23-1"},
		{[]string{"releases/lobo-2026.09.23-10.zip", "releases/lobo-2026.09.23-9.zip"}, "2026.09.23-11"},
	}
	for _, tt := range tests {
		if got := NextVersion(tt.keys, day); got != tt.want {
			t.Errorf("%v: got %s want %s", tt.keys, got, tt.want)
		}
	}
}

func TestBuildZipAndScan(t *testing.T) {
	dir := t.TempDir()
	bin := filepath.Join(dir, "lobo-agent")
	_ = os.WriteFile(bin, []byte("ELF binary bytes"), 0o755)
	m := Manifest{Version: "2026.09.23-1", GitSHA: "abc1234", LlamaImage: "img", Model: ModelRef{ID: "q8"}, Defaults: Defaults{Ctx: 8192}}
	out := filepath.Join(dir, "r.zip")
	sha, err := BuildZip(bin, m, out)
	if err != nil {
		t.Fatal(err)
	}
	b, _ := os.ReadFile(out)
	sum := sha256.Sum256(b)
	if hex.EncodeToString(sum[:]) != sha {
		t.Fatal("sha mismatch")
	}
	zr, _ := zip.OpenReader(out)
	var names []string
	for _, f := range zr.File {
		names = append(names, f.Name)
		if f.Name == "lobo-agent" && f.Mode().Perm() != 0o755 {
			t.Fatal(f.Mode())
		}
		if f.Name == "release.json" {
			rc, _ := f.Open()
			var got map[string]any
			_ = json.NewDecoder(rc).Decode(&got)
			rc.Close()
			var keys []string
			for k := range got {
				keys = append(keys, k)
			}
			sort.Strings(keys)
			if strings.Join(keys, " ") != "built_at built_by defaults git_dirty git_sha llama_image model version" {
				t.Fatal(keys)
			}
		}
	}
	zr.Close()
	sort.Strings(names)
	if strings.Join(names, ",") != "lobo-agent,release.json" {
		t.Fatal(names)
	}
	if err := ScanForSecrets(out, map[string]string{"LOBO_API_KEY": "sk-notinzip-123", "SHORT": "ELF"}); err != nil {
		t.Fatal(err)
	}
	err = ScanForSecrets(out, map[string]string{"R2_SECRET_KEY": "binary bytes"})
	if err == nil || !strings.Contains(err.Error(), "R2_SECRET_KEY") || strings.Contains(err.Error(), "binary bytes") {
		t.Fatal(err)
	}
}

func TestResolve(t *testing.T) {
	latest := Resolved{Manifest: Manifest{Version: "2026.09.23-2", LlamaImage: "new", Defaults: Defaults{Ctx: 16384}}, ZipKey: ZipKey("2026.09.23-2")}
	old := Resolved{Manifest: Manifest{Version: "2026.09.22-1", LlamaImage: "old", Defaults: Defaults{Ctx: 8192}}, ZipKey: ZipKey("2026.09.22-1")}
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		switch r.URL.Path {
		case "/" + LatestKey:
			_ = json.NewEncoder(w).Encode(latest)
		case "/" + MetaKey("2026.09.22-1"):
			_ = json.NewEncoder(w).Encode(old)
		default:
			w.WriteHeader(404)
		}
	}))
	defer srv.Close()
	ctx := context.Background()
	r, err := Resolve(ctx, srv.Client(), srv.URL, "")
	if err != nil || r.Manifest.LlamaImage != "new" {
		t.Fatal(r, err)
	}
	r, err = Resolve(ctx, srv.Client(), srv.URL, "2026.09.22-1")
	if err != nil || r.Manifest.LlamaImage != "old" || r.Manifest.Defaults.Ctx != 8192 {
		t.Fatal(r, err)
	}
	if r.ZipURL(srv.URL+"/") != srv.URL+"/releases/lobo-2026.09.22-1.zip" {
		t.Fatal(r.ZipURL(srv.URL))
	}
	if _, err := Resolve(ctx, srv.Client(), srv.URL, "1999.01.01-1"); err == nil || !strings.Contains(err.Error(), "1999.01.01-1") {
		t.Fatal(err)
	}
}
