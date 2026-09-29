package local

import (
	"errors"
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
	Verified bool   `json:"verified"` // full size and the sha256 marker is present
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

// List reports the catalog models and runtime in weights. A missing weights folder lists as empty.
func List(weights string) (Listing, error) {
	l := Listing{Weights: weights, Models: []ModelState{}}
	l.Runtime.Version = RuntimeVersion
	var st syscall.Statfs_t
	if err := syscall.Statfs(weights, &st); err == nil {
		l.FreeBytes = st.Bavail * uint64(st.Bsize)
	} else if !errors.Is(err, os.ErrNotExist) {
		return Listing{}, err
	}
	for _, m := range model.All() {
		s := ModelState{ID: m.ID, File: m.File, Size: m.Size}
		if fi, err := os.Stat(filepath.Join(weights, m.File)); err == nil {
			s.OnDisk = fi.Size()
		}
		if s.OnDisk == m.Size {
			_, err := os.Stat(MarkerPath(weights, m.File))
			s.Verified = err == nil
		}
		l.Models = append(l.Models, s)
	}
	_, err := findServer(RuntimeDir(weights))
	l.Runtime.Present = err == nil
	return l, nil
}
