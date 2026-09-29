package local

import (
	"archive/tar"
	"bytes"
	"compress/gzip"
	"context"
	"crypto/sha256"
	"encoding/hex"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"sync/atomic"
	"testing"
)

type tarEntry struct {
	name, body, link string
	typ              byte
	mode             int64
}

func makeTarGz(t *testing.T, entries []tarEntry) []byte {
	t.Helper()
	var buf bytes.Buffer
	gz := gzip.NewWriter(&buf)
	tw := tar.NewWriter(gz)
	for _, e := range entries {
		typ := e.typ
		if typ == 0 {
			typ = tar.TypeReg
		}
		h := &tar.Header{Name: e.name, Typeflag: typ, Mode: e.mode, Size: int64(len(e.body)), Linkname: e.link}
		if typ != tar.TypeReg {
			h.Size = 0
		}
		if err := tw.WriteHeader(h); err != nil {
			t.Fatal(err)
		}
		if typ == tar.TypeReg {
			if _, err := tw.Write([]byte(e.body)); err != nil {
				t.Fatal(err)
			}
		}
	}
	if err := tw.Close(); err != nil {
		t.Fatal(err)
	}
	if err := gz.Close(); err != nil {
		t.Fatal(err)
	}
	return buf.Bytes()
}

// serveRuntime points RuntimeURL/size/sha at a test server with body. Returns the request counter.
func serveRuntime(t *testing.T, body []byte, sha string) *atomic.Int32 {
	t.Helper()
	var hits atomic.Int32
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		hits.Add(1)
		_, _ = w.Write(body)
	}))
	t.Cleanup(srv.Close)
	oldURL, oldSize, oldSHA := RuntimeURL, runtimeSize, runtimeSHA
	t.Cleanup(func() { RuntimeURL, runtimeSize, runtimeSHA = oldURL, oldSize, oldSHA })
	RuntimeURL, runtimeSize = srv.URL+"/llama.tar.gz", int64(len(body))
	if sha == "" {
		s := sha256.Sum256(body)
		sha = hex.EncodeToString(s[:])
	}
	runtimeSHA = sha
	return &hits
}

var goodTree = []tarEntry{
	{name: "build/", typ: tar.TypeDir, mode: 0o755},
	{name: "build/bin/", typ: tar.TypeDir, mode: 0o755},
	{name: "build/bin/llama-server", body: "#!/bin/sh\n", mode: 0o755},
	{name: "build/bin/libllama.0.dylib", body: "lib", mode: 0o644},
	{name: "build/bin/libllama.dylib", typ: tar.TypeSymlink, link: "libllama.0.dylib"},
}

func TestRuntimeEnsure(t *testing.T) {
	hits := serveRuntime(t, makeTarGz(t, goodTree), "")
	w := t.TempDir()
	var notes []string
	bin, err := EnsureRuntime(context.Background(), w, func(s string) { notes = append(notes, s) })
	if err != nil {
		t.Fatal(err)
	}
	want := filepath.Join(w, "runtime", "llama-"+RuntimeVersion, "build", "bin", "llama-server")
	if bin != want {
		t.Fatalf("bin %s, want %s", bin, want)
	}
	fi, err := os.Stat(bin)
	if err != nil || fi.Mode().Perm()&0o100 == 0 {
		t.Fatalf("not executable: %v %v", fi, err)
	}
	if l, err := os.Readlink(filepath.Join(filepath.Dir(bin), "libllama.dylib")); err != nil || l != "libllama.0.dylib" {
		t.Fatalf("symlink %q %v", l, err)
	}
	if len(notes) != 1 || !strings.HasPrefix(notes[0], "llama.cpp "+RuntimeVersion+" ") || !strings.HasSuffix(notes[0], " MB") {
		t.Fatalf("notes %q", notes)
	}
	// Second call: present, no request, no note.
	bin2, err := EnsureRuntime(context.Background(), w, func(s string) { notes = append(notes, s) })
	if err != nil || bin2 != bin || hits.Load() != 1 || len(notes) != 1 {
		t.Fatalf("second: %s %v hits=%d notes=%q", bin2, err, hits.Load(), notes)
	}
	// Only the final dir is left in runtime/: no temp files or dirs.
	ents, _ := os.ReadDir(filepath.Join(w, "runtime"))
	if len(ents) != 1 {
		t.Fatalf("runtime dir: %v", ents)
	}
}

func TestRuntimeNote(t *testing.T) {
	if got := runtimeNote(runtimeSize); got != "llama.cpp b11118 11 MB" {
		t.Fatal(got)
	}
}

func TestRuntimeRejects(t *testing.T) {
	tests := []struct {
		name    string
		entries []tarEntry
		sha     string
		want    string
	}{
		{"sha mismatch", goodTree, strings.Repeat("0", 64), "sha256"},
		{"traversal", []tarEntry{{name: "../evil", body: "x", mode: 0o644}}, "", "unsafe path"},
		{"absolute", []tarEntry{{name: "/tmp/evil", body: "x", mode: 0o644}}, "", "unsafe path"},
		{"symlink out", []tarEntry{{name: "a", typ: tar.TypeSymlink, link: "../../x"}}, "", "unsafe symlink"},
		{"symlink abs", []tarEntry{{name: "a", typ: tar.TypeSymlink, link: "/etc/passwd"}}, "", "unsafe symlink"},
		{"no server", []tarEntry{{name: "bin/other", body: "x", mode: 0o755}}, "", "no llama-server"},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			serveRuntime(t, makeTarGz(t, tt.entries), tt.sha)
			w := t.TempDir()
			_, err := EnsureRuntime(context.Background(), w, func(string) {})
			if err == nil || !strings.Contains(err.Error(), tt.want) {
				t.Fatalf("err %v, want %q", err, tt.want)
			}
			ents, _ := os.ReadDir(filepath.Join(w, "runtime"))
			if len(ents) != 0 {
				t.Fatalf("left behind: %v", ents)
			}
		})
	}
}

func TestRuntimeShort(t *testing.T) {
	body := makeTarGz(t, goodTree)
	serveRuntime(t, body, "")
	runtimeSize++ // server sends one byte less than pinned
	if _, err := EnsureRuntime(context.Background(), t.TempDir(), func(string) {}); err == nil || !strings.Contains(err.Error(), "size") {
		t.Fatal(err)
	}
}
