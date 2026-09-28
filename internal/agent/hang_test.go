package agent

import (
	"bytes"
	"context"
	"crypto/ed25519"
	"crypto/rand"
	"crypto/sha256"
	"encoding/hex"
	"net"
	"net/http"
	"net/http/httptest"
	"path/filepath"
	"strconv"
	"strings"
	"sync/atomic"
	"testing"
	"time"

	"golang.org/x/crypto/ssh"
)

func shortStall(t *testing.T) {
	old := StallTimeout
	StallTimeout = 200 * time.Millisecond
	t.Cleanup(func() { StallTimeout = old })
}

func within(t *testing.T, d time.Duration, f func() error) error {
	t.Helper()
	done := make(chan error, 1)
	go func() { done <- f() }()
	select {
	case err := <-done:
		return err
	case <-time.After(d):
		t.Fatal("hung")
		return nil
	}
}

// A server that accepts a range request and never sends headers must not hang the download.
func TestDownloadParallelNoHeaders(t *testing.T) {
	shortStall(t)
	data := make([]byte, ChunkSize+4096)
	_, _ = rand.Read(data)
	var calls atomic.Int32
	block := make(chan struct{})
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if calls.Add(1) == 1 {
			select { // silent before headers
			case <-block:
			case <-r.Context().Done():
			}
			return
		}
		http.ServeContent(w, r, "m", time.Time{}, bytes.NewReader(data))
	}))
	defer srv.Close()
	defer close(block)
	err := within(t, 20*time.Second, func() error {
		return DownloadParallel(context.Background(), HTTPSource{srv.URL}, filepath.Join(t.TempDir(), "m"), int64(len(data)), "unused", chunkTable(data), 2, nil)
	})
	if err != nil {
		t.Fatal(err)
	}
}

// Single-stream path: a stream that goes silent mid-body is closed and resumed.
func TestDownloadSingleStalledStream(t *testing.T) {
	shortStall(t)
	data := make([]byte, 64<<10)
	_, _ = rand.Read(data)
	sum := sha256.Sum256(data)
	var calls atomic.Int32
	block := make(chan struct{})
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if calls.Add(1) == 1 {
			w.Header().Set("Content-Length", strconv.Itoa(len(data)))
			w.WriteHeader(http.StatusOK)
			_, _ = w.Write(data[:100])
			w.(http.Flusher).Flush()
			select {
			case <-block:
			case <-r.Context().Done():
			}
			return
		}
		http.ServeContent(w, r, "m", time.Time{}, bytes.NewReader(data))
	}))
	defer srv.Close()
	defer close(block)
	dst := filepath.Join(t.TempDir(), "m")
	err := within(t, 20*time.Second, func() error {
		return DownloadParallel(context.Background(), HTTPSource{srv.URL}, dst, int64(len(data)), hex.EncodeToString(sum[:]), nil, 1, nil)
	})
	if err != nil || calls.Load() < 2 {
		t.Fatal(err, calls.Load())
	}
}

// An SSH server that accepts TCP and never speaks must fail the stream, not hang it.
func TestSSHSourceSilentHandshake(t *testing.T) {
	shortStall(t)
	ln, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		t.Fatal(err)
	}
	defer func() { _ = ln.Close() }()
	held := make(chan net.Conn, 4)
	t.Cleanup(func() {
		for {
			select {
			case c := <-held:
				_ = c.Close()
			default:
				return
			}
		}
	})
	go func() {
		for {
			c, err := ln.Accept()
			if err != nil {
				return
			}
			held <- c // hold it open, say nothing
		}
	}()
	_, cpriv, _ := ed25519.GenerateKey(rand.Reader)
	signer, _ := ssh.NewSignerFromKey(cpriv)
	src := SSHSource{Addr: ln.Addr().String(), User: "lobo", File: "m", Size: 10, Signer: signer, HostKey: signer.PublicKey()}
	err = within(t, 10*time.Second, func() error {
		_, _, err := src.Open(context.Background(), 0, 10)
		return err
	})
	if err == nil {
		t.Fatal("want error")
	}
}

// A bad chunk table fails at once and leaves no progress ticker running.
func TestDownloadParallelBadTableNoTickerLeak(t *testing.T) {
	var calls atomic.Int32
	err := DownloadParallel(context.Background(), HTTPSource{"http://127.0.0.1:1"}, filepath.Join(t.TempDir(), "m"), 10, "", []string{"a", "b"}, 2, func(DownloadProgress) { calls.Add(1) })
	if err == nil || !strings.Contains(err.Error(), "chunk table") {
		t.Fatal(err)
	}
	time.Sleep(700 * time.Millisecond)
	if n := calls.Load(); n != 0 {
		t.Fatalf("progress called %d times after return", n)
	}
}
