package agent

import (
	"bytes"
	"context"
	"crypto/rand"
	"crypto/sha256"
	"encoding/hex"
	"errors"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"sync/atomic"
	"testing"
	"time"
)

func TestDownload(t *testing.T) {
	data := make([]byte, 3<<20)
	_, _ = rand.Read(data)
	sum := sha256.Sum256(data)
	sha := hex.EncodeToString(sum[:])
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path == "/missing" {
			w.WriteHeader(404)
			return
		}
		http.ServeContent(w, r, "m", time.Time{}, strings.NewReader(string(data)))
	}))
	defer srv.Close()
	dir := t.TempDir()
	ctx := context.Background()

	var last DownloadProgress
	dst := filepath.Join(dir, "m.gguf")
	if err := Download(ctx, HTTPSource{srv.URL + "/m"}, dst, sha, func(p DownloadProgress) { last = p }); err != nil {
		t.Fatal(err)
	}
	got, _ := os.ReadFile(dst)
	if string(got) != string(data) || last.Bytes != int64(len(data)) || last.Total != int64(len(data)) {
		t.Fatalf("progress %+v", last)
	}

	bad := filepath.Join(dir, "bad.gguf")
	if err := Download(ctx, HTTPSource{srv.URL + "/m"}, bad, strings.Repeat("0", 64), nil); err == nil || !strings.Contains(err.Error(), "sha256") {
		t.Fatal(err)
	}
	if _, err := os.Stat(bad); !errors.Is(err, os.ErrNotExist) {
		t.Fatal("bad file left at target name")
	}
	if _, err := os.Stat(bad + ".bad"); err != nil {
		t.Fatal(err)
	}
	if err := Download(ctx, HTTPSource{srv.URL + "/missing"}, filepath.Join(dir, "x"), sha, nil); err == nil || !strings.Contains(err.Error(), "404") {
		t.Fatal(err)
	}
}

// The server drops the first response halfway; Download must resume with Range and still hash right.
func TestDownloadResumesAfterDrop(t *testing.T) {
	data := make([]byte, 2<<20)
	_, _ = rand.Read(data)
	sum := sha256.Sum256(data)
	var calls atomic.Int32
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if calls.Add(1) == 1 {
			w.Header().Set("Content-Length", strconv.Itoa(len(data)))
			w.WriteHeader(200)
			_, _ = w.Write(data[:len(data)/2])
			w.(http.Flusher).Flush()
			conn, _, _ := w.(http.Hijacker).Hijack()
			_ = conn.Close()
			return
		}
		if r.Header.Get("Range") == "" {
			t.Error("resume without Range")
		}
		http.ServeContent(w, r, "m", time.Time{}, bytes.NewReader(data))
	}))
	defer srv.Close()
	dst := filepath.Join(t.TempDir(), "m")
	if err := Download(context.Background(), HTTPSource{srv.URL}, dst, hex.EncodeToString(sum[:]), nil); err != nil {
		t.Fatal(err)
	}
	got, _ := os.ReadFile(dst)
	if !bytes.Equal(got, data) || calls.Load() != 2 {
		t.Fatalf("calls %d equal %v", calls.Load(), bytes.Equal(got, data))
	}
}

func TestDownloadStallRespectsDeadline(t *testing.T) {
	block := make(chan struct{})
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Length", "100")
		w.WriteHeader(200)
		_, _ = w.Write([]byte("abc"))
		w.(http.Flusher).Flush()
		<-block
	}))
	defer srv.Close()
	defer close(block)
	ctx, cancel := context.WithTimeout(context.Background(), 50*time.Millisecond)
	defer cancel()
	err := Download(ctx, HTTPSource{srv.URL}, filepath.Join(t.TempDir(), "x"), "", nil)
	if !errors.Is(err, context.DeadlineExceeded) {
		t.Fatal(err)
	}
}

func TestLogRing(t *testing.T) {
	r := NewLogRing(3)
	_, _ = r.Write([]byte("a\nb\nc"))
	_, _ = r.Write([]byte("d\ne\n"))
	if got := strings.Join(r.Tail(10), ","); got != "b,cd,e" {
		t.Fatal(got)
	}
	if got := strings.Join(r.Tail(1), ","); got != "e" {
		t.Fatal(got)
	}
	done := make(chan bool)
	for i := 0; i < 8; i++ {
		go func() {
			for j := 0; j < 100; j++ {
				_, _ = r.Write([]byte("x\n"))
				_ = r.Tail(2)
			}
			done <- true
		}()
	}
	for i := 0; i < 8; i++ {
		<-done
	}
}

func TestHTTPSourceErrorsHideSecretURL(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) { w.WriteHeader(http.StatusForbidden) }))
	defer srv.Close()
	for _, u := range []string{srv.URL + "/tok3n/model.gguf?X-Amz-Signature=s3cret", "http://127.0.0.1:1/tok3n/m?X-Amz-Signature=s3cret"} {
		_, _, err := HTTPSource{URL: u}.Open(context.Background(), 0, -1)
		if err == nil || strings.Contains(err.Error(), "s3cret") || strings.Contains(err.Error(), "tok3n") {
			t.Fatalf("%s: err %v leaks the secret URL", u, err)
		}
	}
}
