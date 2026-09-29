// Package model is the catalog of GGUF files lobo can serve.
package model

import (
	"fmt"
	"sort"
	"strings"
)

// Model is one GGUF file stored in bucket `lobo` under models/.
type Model struct {
	ID     string
	File   string
	SHA256 string
	Alias  string // llama-server --alias, the model id clients send
	Size   int64
}

var catalog = map[string]Model{
	"q8": {
		ID:     "q8",
		File:   "Qwen3.5-27B-Uncensored-HauhauCS-Aggressive-Q8_0.gguf",
		SHA256: "78ae0800c6062b0a2fbfb3649233fd152aaf0890d3cd871965795de0119e8dd5",
		Alias:  "qwen3.5-27b-uncensored-q8",
		Size:   28595762272,
	},
	"q6": {
		ID:     "q6",
		File:   "Qwen3.5-27B-Uncensored-HauhauCS-Aggressive-Q6_K.gguf",
		SHA256: "9fc4e4768045cb187a86f42b19f3d74754406680c3688502020556a8f3b70c4b",
		Alias:  "qwen3.5-27b-uncensored-q6",
		Size:   22082528352,
	},
}

// ChunkSize is the unit of parallel download and of the per-chunk sha256 table.
const ChunkSize = 256 << 20

// ChunkSHA returns the sha256 of each ChunkSize piece of the file, or nil if unknown.
func (m Model) ChunkSHA() []string { return chunkSHA[m.File] }

// Get returns the model for id ("q8" or "q6").
func Get(id string) (Model, error) {
	m, ok := catalog[id]
	if !ok {
		ids := make([]string, 0, len(catalog))
		for k := range catalog {
			ids = append(ids, k)
		}
		sort.Strings(ids)
		return Model{}, fmt.Errorf("unknown model %q, valid: %s", id, strings.Join(ids, ", "))
	}
	return m, nil
}

// URL is the public download URL of the model in the bucket.
func (m Model) URL(bucketURL string) string {
	return strings.TrimRight(bucketURL, "/") + "/models/" + m.File
}

// All returns the catalog sorted by ID.
func All() []Model {
	out := make([]Model, 0, len(catalog))
	for _, m := range catalog {
		out = append(out, m)
	}
	sort.Slice(out, func(i, j int) bool { return out[i].ID < out[j].ID })
	return out
}

// MinFreeMiB is the GPU memory a model needs: weights + 2.5 GiB for KV cache (64K ctx, q8_0) and buffers.
// Measured 2026-09-25: Q8 @ 64K uses 29,274 MiB of VRAM; weights + 2.5 GiB = 29,831.
func MinFreeMiB(modelBytes int64) int { return int(modelBytes>>20) + 2560 }
