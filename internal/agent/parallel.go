package agent

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"fmt"
	"hash"
	"io"
	"os"
	"sync"
	"sync/atomic"
	"time"

	"github.com/1905/lobocode/internal/model"
)

// ChunkSize is the unit of work for parallel downloads: one ranged request, one sha256 in the table.
const ChunkSize = model.ChunkSize

// StallTimeout: a stream that delivers no bytes for this long is dropped and its chunk retried.
var StallTimeout = 30 * time.Second

// fetchRanges pulls [0,size) from src with conns workers over a queue of chunks, writing with WriteAt.
// A dropped chunk resumes from where it stopped. onBytes is called with every write size.
// With chunkSHA, each chunk is hashed while it streams and retried from its start on a mismatch or a
// drop, so no full-file sha256 pass is needed afterwards. Without it, a drop resumes mid-chunk.
func fetchRanges(ctx context.Context, src Source, size int64, conns int, chunkSHA []string, w io.WriterAt, onBytes func(int)) error {
	ctx, cancel := context.WithCancelCause(ctx)
	defer cancel(nil)
	type chunk struct {
		i          int
		start, end int64
	}
	q := make(chan chunk)
	go func() {
		defer close(q)
		for i, off := 0, int64(0); off < size; i, off = i+1, off+ChunkSize {
			end := min(off+ChunkSize, size)
			select {
			case q <- chunk{i, off, end}:
			case <-ctx.Done():
				return
			}
		}
	}()
	var wg sync.WaitGroup
	for i := 0; i < conns; i++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			buf := make([]byte, 1<<20)
			for c := range q {
				pos := c.start
				for attempt := 0; pos < c.end; attempt++ {
					if ctx.Err() != nil {
						return
					}
					if attempt > maxResumes {
						cancel(fmt.Errorf("chunk %d-%d: gave up after %d resumes", c.start, c.end, maxResumes))
						return
					}
					if attempt > 0 {
						select {
						case <-ctx.Done():
							return
						case <-time.After(time.Duration(attempt) * time.Second):
						}
					}
					var h hash.Hash
					if chunkSHA != nil {
						pos, h = c.start, sha256.New() // verified chunks restart whole: the hash must see every byte
					}
					from := pos
					body, _, err := src.Open(ctx, pos, c.end-pos)
					if err != nil {
						if pe, ok := err.(permanentErr); ok {
							cancel(pe.error)
							return
						}
						continue
					}
					// A silent connection must not hang the boot: no bytes for StallTimeout → close it, the
					// read errors out and the chunk is fetched again (seen live: one of 32 streams went quiet at 99.2%).
					stall := time.AfterFunc(StallTimeout, func() { _ = body.Close() })
					for pos < c.end {
						n, rerr := body.Read(buf[:min(int64(len(buf)), c.end-pos)])
						if n > 0 {
							stall.Reset(StallTimeout)
							if _, werr := w.WriteAt(buf[:n], pos); werr != nil {
								_ = body.Close()
								cancel(werr)
								return
							}
							if h != nil {
								h.Write(buf[:n])
							}
							pos += int64(n)
							onBytes(n)
						}
						if rerr != nil {
							break
						}
					}
					stall.Stop()
					_ = body.Close()
					if h != nil && pos == c.end && hex.EncodeToString(h.Sum(nil)) != chunkSHA[c.i] {
						onBytes(-int(pos - from)) // bytes will be fetched again
						pos = c.start             // corrupt chunk: fetch it again
					} else if h != nil && pos < c.end {
						onBytes(-int(pos - from))
					}
				}
			}
		}()
	}
	wg.Wait()
	if c := context.Cause(ctx); c != nil && c != context.Canceled {
		return c
	}
	return ctx.Err()
}

// DownloadParallel fetches size bytes with conns ranged streams. With chunkSHA every chunk is checked as it
// arrives; without it a full-file sha256 pass runs at the end.
func DownloadParallel(ctx context.Context, src Source, dst string, size int64, sha string, chunkSHA []string, conns int, onProgress func(DownloadProgress)) error {
	if conns <= 1 {
		return Download(ctx, src, dst, sha, onProgress)
	}
	// Before the progress ticker starts: an early return after it would leak the ticker goroutine.
	if chunkSHA != nil && int64(len(chunkSHA)) != (size+ChunkSize-1)/ChunkSize {
		return fmt.Errorf("download %s: chunk table has %d entries for %d bytes", src, len(chunkSHA), size)
	}
	f, err := os.Create(dst)
	if err != nil {
		return err
	}
	if err := f.Truncate(size); err != nil {
		_ = f.Close()
		return err
	}
	var n atomic.Int64
	start := time.Now()
	report := func(verifying bool) {
		if onProgress == nil {
			return
		}
		b := n.Load()
		onProgress(DownloadProgress{Bytes: b, Total: size, MBps: float64(b) / time.Since(start).Seconds() / 1e6, Verifying: verifying})
	}
	done, stopped := make(chan struct{}), make(chan struct{})
	go func() {
		defer close(stopped)
		t := time.NewTicker(500 * time.Millisecond)
		defer t.Stop()
		for {
			select {
			case <-done:
				return
			case <-t.C:
				report(false)
			}
		}
	}()
	err = fetchRanges(ctx, src, size, conns, chunkSHA, f, func(k int) { n.Add(int64(k)) })
	close(done)
	<-stopped // onProgress must never run concurrently with itself
	if err != nil {
		_ = f.Close()
		return fmt.Errorf("download %s: %w", src, err)
	}
	report(false)
	mbps := float64(size) / time.Since(start).Seconds() / 1e6
	if chunkSHA != nil { // every chunk already matched the table
		if err := f.Close(); err != nil {
			return err
		}
		if onProgress != nil {
			onProgress(DownloadProgress{Bytes: size, Total: size, MBps: mbps})
		}
		return nil
	}
	if onProgress != nil {
		onProgress(DownloadProgress{Bytes: size, Total: size, MBps: mbps, Verifying: true})
	}
	if _, err := f.Seek(0, io.SeekStart); err != nil {
		_ = f.Close()
		return err
	}
	h := sha256.New()
	_, err = io.CopyBuffer(h, f, make([]byte, 8<<20))
	if cerr := f.Close(); err == nil {
		err = cerr
	}
	if err != nil {
		return err
	}
	if got := hex.EncodeToString(h.Sum(nil)); got != sha {
		_ = os.Rename(dst, dst+".bad")
		return fmt.Errorf("download %s: sha256 %s, want %s", src, got, sha)
	}
	if onProgress != nil {
		onProgress(DownloadProgress{Bytes: size, Total: size, MBps: mbps})
	}
	return nil
}

type discardAt struct{}

func (discardAt) WriteAt(p []byte, _ int64) (int, error) { return len(p), nil }

// Bench pulls from src with conns streams for up to dur and returns bytes and MB/s. Nothing is written.
func Bench(ctx context.Context, src Source, size int64, conns int, dur time.Duration) (int64, float64, error) {
	ctx, cancel := context.WithTimeout(ctx, dur)
	defer cancel()
	var n atomic.Int64
	start := time.Now()
	err := fetchRanges(ctx, src, size, conns, nil, discardAt{}, func(k int) { n.Add(int64(k)) })
	secs := time.Since(start).Seconds()
	if err == context.DeadlineExceeded {
		err = nil
	}
	return n.Load(), float64(n.Load()) / secs / 1e6, err
}
