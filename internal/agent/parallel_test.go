package agent

import (
	"bytes"
	"context"
	"crypto/ed25519"
	"crypto/rand"
	"crypto/sha256"
	"encoding/hex"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"sync/atomic"
	"testing"
	"time"

	"golang.org/x/crypto/ssh"
)

func rangeServer(t *testing.T, data []byte, dropFirst bool) (*httptest.Server, *atomic.Int32) {
	var calls atomic.Int32
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if calls.Add(1) == 1 && dropFirst {
			conn, _, _ := w.(http.Hijacker).Hijack()
			_ = conn.Close()
			return
		}
		http.ServeContent(w, r, "m", time.Time{}, bytes.NewReader(data))
	}))
	t.Cleanup(srv.Close)
	return srv, &calls
}

func TestDownloadParallelHTTP(t *testing.T) {
	data := make([]byte, ChunkSize*2+12345) // 3 chunks, last one short
	_, _ = rand.Read(data)
	sum := sha256.Sum256(data)
	srv, calls := rangeServer(t, data, true)
	dst := filepath.Join(t.TempDir(), "m")
	var last DownloadProgress
	sawVerify := false
	err := DownloadParallel(context.Background(), HTTPSource{srv.URL}, dst, int64(len(data)), hex.EncodeToString(sum[:]), nil, 3, func(p DownloadProgress) {
		last = p
		sawVerify = sawVerify || p.Verifying
	})
	if err != nil {
		t.Fatal(err)
	}
	got, _ := os.ReadFile(dst)
	if !bytes.Equal(got, data) || !sawVerify || last.Verifying || last.Bytes != int64(len(data)) || calls.Load() < 4 {
		t.Fatalf("equal %v verify %v last %+v calls %d", bytes.Equal(got, data), sawVerify, last, calls.Load())
	}
}

func TestDownloadParallelBadSHA(t *testing.T) {
	data := make([]byte, 1<<20)
	srv, _ := rangeServer(t, data, false)
	dst := filepath.Join(t.TempDir(), "m")
	err := DownloadParallel(context.Background(), HTTPSource{srv.URL}, dst, int64(len(data)), strings.Repeat("0", 64), nil, 4, nil)
	if err == nil || !strings.Contains(err.Error(), "sha256") {
		t.Fatal(err)
	}
	if _, err := os.Stat(dst + ".bad"); err != nil {
		t.Fatal(err)
	}
}

func TestDownloadParallelSSH(t *testing.T) {
	data := make([]byte, ChunkSize+777)
	_, _ = rand.Read(data)
	sum := sha256.Sum256(data)
	_, cpriv, _ := ed25519.GenerateKey(rand.Reader)
	signer, _ := ssh.NewSignerFromKey(cpriv)
	addr, hk, sessions := fakeFeesh(t, data, signer.PublicKey())
	src := SSHSource{Addr: addr, User: "lobo", File: "m.gguf", Size: int64(len(data)), Signer: signer, HostKey: hk}
	dst := filepath.Join(t.TempDir(), "m")
	if err := DownloadParallel(context.Background(), src, dst, int64(len(data)), hex.EncodeToString(sum[:]), nil, 2, nil); err != nil {
		t.Fatal(err)
	}
	got, _ := os.ReadFile(dst)
	if !bytes.Equal(got, data) || sessions.Load() < 3 { // 2 chunks + 1 resume after the dropped first session
		t.Fatalf("equal %v sessions %d", bytes.Equal(got, data), sessions.Load())
	}
}

func TestBench(t *testing.T) {
	data := make([]byte, 4<<20)
	srv, _ := rangeServer(t, data, false)
	n, mbps, err := Bench(context.Background(), HTTPSource{srv.URL}, int64(len(data)), 2, 5*time.Second)
	if err != nil || n != int64(len(data)) || mbps <= 0 {
		t.Fatal(n, mbps, err)
	}
}

func chunkTable(data []byte) []string {
	var hs []string
	for off := 0; off < len(data); off += ChunkSize {
		s := sha256.Sum256(data[off:min(off+ChunkSize, len(data))])
		hs = append(hs, hex.EncodeToString(s[:]))
	}
	return hs
}

// Per-chunk verification: first request is dropped mid-chunk, the chunk restarts and still matches.
func TestDownloadParallelChunkSHA(t *testing.T) {
	data := make([]byte, ChunkSize+999)
	_, _ = rand.Read(data)
	srv, _ := rangeServer(t, data, true)
	dst := filepath.Join(t.TempDir(), "m")
	var last DownloadProgress
	if err := DownloadParallel(context.Background(), HTTPSource{srv.URL}, dst, int64(len(data)), "unused", chunkTable(data), 2, func(p DownloadProgress) { last = p }); err != nil {
		t.Fatal(err)
	}
	got, _ := os.ReadFile(dst)
	if !bytes.Equal(got, data) || last.Bytes != int64(len(data)) || last.Verifying {
		t.Fatalf("equal %v last %+v", bytes.Equal(got, data), last)
	}
}

// A chunk whose bytes don't match the table is retried, then the download fails.
func TestDownloadParallelChunkSHAMismatch(t *testing.T) {
	data := make([]byte, 1<<20)
	srv, _ := rangeServer(t, data, false)
	table := []string{strings.Repeat("0", 64)}
	err := DownloadParallel(context.Background(), HTTPSource{srv.URL}, filepath.Join(t.TempDir(), "m"), int64(len(data)), "unused", table, 2, nil)
	if err == nil || !strings.Contains(err.Error(), "gave up") {
		t.Fatal(err)
	}
}

// One range request stalls forever after a few bytes; the stall timeout must drop it and retry.
func TestDownloadParallelStalledStream(t *testing.T) {
	old := StallTimeout
	// 1 s, not less: a verified 256 MB chunk restarts from byte 0 on any read gap longer than this, and
	// GC or disk flushes on a busy laptop hit 200 ms (the test flaked at 200 ms, 1 run in 3).
	StallTimeout = time.Second
	defer func() { StallTimeout = old }()
	data := make([]byte, ChunkSize+4096)
	_, _ = rand.Read(data)
	var calls atomic.Int32
	block := make(chan struct{})
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if calls.Add(1) == 1 {
			w.Header().Set("Content-Range", "bytes 0-"+strconv.Itoa(ChunkSize-1)+"/"+strconv.Itoa(len(data)))
			w.WriteHeader(http.StatusPartialContent)
			_, _ = w.Write(data[:100])
			w.(http.Flusher).Flush()
			<-block // go silent
			return
		}
		http.ServeContent(w, r, "m", time.Time{}, bytes.NewReader(data))
	}))
	defer srv.Close()
	defer close(block) // runs before srv.Close: release the stalled handler or Close waits forever
	done := make(chan error, 1)
	go func() {
		done <- DownloadParallel(context.Background(), HTTPSource{srv.URL}, filepath.Join(t.TempDir(), "m"), int64(len(data)), "unused", chunkTable(data), 2, nil)
	}()
	select {
	case err := <-done:
		if err != nil {
			t.Fatal(err)
		}
	case <-time.After(60 * time.Second):
		t.Fatal("download hung on a stalled stream")
	}
	if calls.Load() < 3 {
		t.Fatalf("want the stalled stream retried (≥3 requests), got %d", calls.Load())
	}
}
