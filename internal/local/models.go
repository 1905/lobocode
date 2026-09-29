package local

import (
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"syscall"

	"github.com/1905/lobocode/internal/model"
)

// HFBase is the public source of the catalog GGUFs, byte-identical to our copies.
const HFBase = "https://huggingface.co/HauhauCS/Qwen3.5-27B-Uncensored-HauhauCS-Aggressive/resolve/main/"

// ModelState is one catalog model in the weights folder.
type ModelState struct {
	ID       string `json:"id"`
	File     string `json:"file"`
	Size     int64  `json:"size"`
	OnDisk   int64  `json:"on_disk"`
	Verified bool   `json:"verified"` // full size and a valid sha256 marker (markerValid)
}

// Listing is the `lobo models --json` body.
type Listing struct {
	Weights   string       `json:"weights"`
	FreeBytes uint64       `json:"free_bytes"`
	Models    []ModelState `json:"models"`
	Runtime   struct {
		Version string `json:"version"`
		Present bool   `json:"present"`
	} `json:"runtime"`
}

// MarkerPath is written once the file's sha256 matched the catalog, so later starts skip the hash.
func MarkerPath(weights, file string) string {
	return filepath.Join(weights, file+".sha256-ok")
}

// markerLine is the marker body for the file as fi describes it: `<sha256> <size> <mtime_unix_nano>`.
func markerLine(sha string, fi os.FileInfo) string {
	return fmt.Sprintf("%s %d %d\n", sha, fi.Size(), fi.ModTime().UnixNano())
}

// markerValid: <weights>/<m.File>, as fi (nil = missing) describes it, has the catalog size and its marker names
// the catalog sha256 and the file's current size and mtime. Anything else (no marker, the old `<sha>` format, a
// changed file) means hash again.
func markerValid(weights string, m model.Model, fi os.FileInfo) bool {
	if fi == nil || fi.Size() != m.Size {
		return false
	}
	b, err := os.ReadFile(MarkerPath(weights, m.File))
	return err == nil && string(b) == markerLine(m.SHA256, fi)
}

// writeMarker records that <weights>/<m.File>, as it is now, hashed to m.SHA256. Atomic: temp file + rename.
func writeMarker(weights string, m model.Model) error {
	fi, err := os.Stat(filepath.Join(weights, m.File))
	if err != nil {
		return err
	}
	tmp, err := os.CreateTemp(weights, ".sha256-ok-*")
	if err != nil {
		return err
	}
	defer func() { _ = os.Remove(tmp.Name()) }() // no-op after a successful rename
	if _, err := tmp.WriteString(markerLine(m.SHA256, fi)); err != nil {
		_ = tmp.Close()
		return err
	}
	if err := tmp.Close(); err != nil {
		return err
	}
	if err := os.Chmod(tmp.Name(), 0o644); err != nil {
		return err
	}
	return os.Rename(tmp.Name(), MarkerPath(weights, m.File))
}

// List reports the catalog models and runtime in weights. A missing weights folder lists as empty.
func List(weights string) (Listing, error) {
	l := Listing{Weights: weights, Models: []ModelState{}}
	l.Runtime.Version = RuntimeVersion
	free, err := freeSpace(weights)
	if err != nil {
		return Listing{}, err
	}
	l.FreeBytes = free
	for _, m := range model.All() {
		s := ModelState{ID: m.ID, File: m.File, Size: m.Size}
		fi, err := os.Stat(filepath.Join(weights, m.File))
		if err == nil {
			s.OnDisk = fi.Size()
		}
		s.Verified = s.OnDisk == m.Size && markerValid(weights, m, fi)
		l.Models = append(l.Models, s)
	}
	_, err = findServer(RuntimeDir(weights))
	l.Runtime.Present = err == nil
	return l, nil
}

// freeSpace is the bytes available in dir's filesystem; a missing dir has 0.
func freeSpace(dir string) (uint64, error) {
	var st syscall.Statfs_t
	if err := syscall.Statfs(dir, &st); err != nil {
		if errors.Is(err, os.ErrNotExist) {
			return 0, nil
		}
		return 0, err
	}
	return st.Bavail * uint64(st.Bsize), nil
}
