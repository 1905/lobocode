package agent

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"fmt"
	"hash"
	"io"
	"os"
	"time"
)

type DownloadProgress struct {
	Bytes     int64   `json:"bytes"`
	Total     int64   `json:"total"`
	MBps      float64 `json:"mbps"`
	Verifying bool    `json:"verifying,omitempty"` // bytes are in; sha256 pass running
	Source    string  `json:"source,omitempty"`    // set when the agent switches to a fallback source
}

// maxResumes bounds how often a dropped stream is resumed from the current offset.
// r2.dev dropped a 28.6 GB GET at 92.8% on the first live boot (2026-09-23), so this is needed.
const maxResumes = 8

// Download streams src into dst, hashing while writing. A dropped stream resumes from the current
// offset (single connection). A partial dst left by an earlier run is resumed too: its bytes are hashed
// and only the rest is fetched. A checksum mismatch moves the file to dst+".bad".
func Download(ctx context.Context, src Source, dst, sha string, onProgress func(DownloadProgress)) error {
	f, err := os.OpenFile(dst, os.O_RDWR|os.O_CREATE, 0o666)
	if err != nil {
		return err
	}
	h := sha256.New()
	pw := &progressWriter{total: -1, start: time.Now(), fn: onProgress}
	done, err := resumePartial(ctx, src, f, h, pw)
	if err != nil {
		f.Close()
		return err
	}
	var lastErr error
	for attempt := 0; attempt <= maxResumes && !done; attempt++ {
		if attempt > 0 {
			select {
			case <-ctx.Done():
				f.Close()
				return ctx.Err()
			case <-time.After(time.Duration(attempt) * 2 * time.Second):
			}
		}
		done, lastErr = copyFrom(ctx, src, f, h, pw)
		if ctx.Err() != nil {
			f.Close()
			return ctx.Err()
		}
		if pe, ok := lastErr.(permanentErr); ok {
			f.Close()
			return pe.error
		}
	}
	if cerr := f.Close(); lastErr == nil {
		lastErr = cerr
	}
	if !done || lastErr != nil {
		return fmt.Errorf("download %s: %v (after %d resumes)", src, lastErr, maxResumes)
	}
	pw.report(true)
	if got := hex.EncodeToString(h.Sum(nil)); got != sha {
		_ = os.Rename(dst, dst+".bad")
		return fmt.Errorf("download %s: sha256 %s, want %s", src, got, sha)
	}
	return nil
}

// resumePartial picks up an existing dst: hashes its bytes and moves pw to its end. It asks src for the
// total first (a 1-byte read): a file longer than the source, or a source with no known total, starts over.
// done=true: dst already holds every byte.
func resumePartial(ctx context.Context, src Source, f *os.File, h hash.Hash, pw *progressWriter) (bool, error) {
	fi, err := f.Stat()
	if err != nil || fi.Size() == 0 {
		return false, err
	}
	body, total, err := src.Open(ctx, 0, 1)
	if err != nil {
		if pe, ok := err.(permanentErr); ok {
			return false, pe.error
		}
		return false, err
	}
	_ = body.Close()
	if total < 0 || fi.Size() > total {
		if err := f.Truncate(0); err != nil {
			return false, err
		}
		_, err := f.Seek(0, io.SeekStart)
		return false, err
	}
	if _, err := io.CopyBuffer(h, f, make([]byte, 4<<20)); err != nil { // leaves f at its end
		return false, err
	}
	pw.n, pw.base, pw.total = fi.Size(), fi.Size(), total
	return pw.n == total, nil
}

// copyFrom streams src from the current offset. done=true means the full length arrived.
func copyFrom(ctx context.Context, src Source, f io.Writer, h hash.Hash, pw *progressWriter) (bool, error) {
	body, total, err := src.Open(ctx, pw.n, -1)
	if err != nil {
		return false, err
	}
	defer body.Close()
	if pw.total < 0 && total >= 0 {
		pw.total = total
	}
	// Same guard as the parallel path: a silent stream is closed after StallTimeout and resumed.
	stall := time.AfterFunc(StallTimeout, func() { _ = body.Close() })
	defer stall.Stop()
	if _, err := io.Copy(io.MultiWriter(f, h, pw, stallReset{stall}), body); err != nil {
		return false, err
	}
	if pw.total >= 0 && pw.n != pw.total {
		return false, fmt.Errorf("short body: %d of %d bytes", pw.n, pw.total)
	}
	if pw.total < 0 {
		return false, permanentErr{fmt.Errorf("unknown total size from %s", src)}
	}
	return true, nil
}

type progressWriter struct {
	n, total  int64
	base      int64 // bytes already on disk at start: not counted in MBps
	start     time.Time
	last      time.Time
	fn        func(DownloadProgress)
	verifying bool // a hash pass over a file on disk, not a download
}

func (p *progressWriter) Write(b []byte) (int, error) {
	p.n += int64(len(b))
	p.report(false)
	return len(b), nil
}

func (p *progressWriter) report(force bool) {
	if p.fn == nil || (!force && time.Since(p.last) < 500*time.Millisecond) {
		return
	}
	p.last = time.Now()
	mbps := 0.0
	if s := time.Since(p.start).Seconds(); s > 0 {
		mbps = float64(p.n-p.base) / s / 1e6
	}
	p.fn(DownloadProgress{Bytes: p.n, Total: p.total, MBps: mbps, Verifying: p.verifying})
}

// HashFile sha256s path, reporting Verifying progress at the start, at most every 500 ms, and at the end.
func HashFile(ctx context.Context, path string, total int64, onProgress func(DownloadProgress)) (string, error) {
	f, err := os.Open(path)
	if err != nil {
		return "", err
	}
	defer f.Close()
	h := sha256.New()
	pw := &progressWriter{total: total, start: time.Now(), fn: onProgress, verifying: true}
	pw.report(true)
	if _, err := io.CopyBuffer(io.MultiWriter(h, pw), ctxReader{ctx, f}, make([]byte, 4<<20)); err != nil {
		return "", err
	}
	pw.report(true)
	return hex.EncodeToString(h.Sum(nil)), nil
}

// ctxReader fails the next Read once ctx is done.
type ctxReader struct {
	ctx context.Context
	r   io.Reader
}

func (c ctxReader) Read(b []byte) (int, error) {
	if err := c.ctx.Err(); err != nil {
		return 0, err
	}
	return c.r.Read(b)
}

// stallReset pushes the stall deadline back on every write.
type stallReset struct{ t *time.Timer }

func (s stallReset) Write(b []byte) (int, error) {
	s.t.Reset(StallTimeout)
	return len(b), nil
}
