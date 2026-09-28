// Package release builds, scans, publishes and resolves lobo-agent release zips in bucket `lobo`.
package release

import (
	"fmt"
	"regexp"
	"sort"
	"strconv"
	"time"
)

type ModelRef struct {
	ID     string `json:"id"`
	File   string `json:"file"`
	SHA256 string `json:"sha256"`
}

type Defaults struct {
	Ctx      int `json:"ctx"`
	IdleMin  int `json:"idle_min"`
	MaxHours int `json:"max_hours"`
}

// Manifest is release.json, the body of /api/version.
type Manifest struct {
	Version    string    `json:"version"`
	GitSHA     string    `json:"git_sha"`
	GitDirty   bool      `json:"git_dirty"`
	BuiltAt    time.Time `json:"built_at"`
	BuiltBy    string    `json:"built_by"`
	LlamaImage string    `json:"llama_image"`
	Model      ModelRef  `json:"model"`
	Defaults   Defaults  `json:"defaults"`
}

// Resolved is releases/lobo-<ver>.json and releases/latest.json: everything `up` needs.
type Resolved struct {
	Manifest  Manifest `json:"manifest"`
	ZipKey    string   `json:"zip_key"`
	ZipSHA256 string   `json:"zip_sha256"`
}

func ZipKey(version string) string  { return "releases/lobo-" + version + ".zip" }
func MetaKey(version string) string { return "releases/lobo-" + version + ".json" }

const LatestKey = "releases/latest.json"

var verRe = regexp.MustCompile(`^releases/lobo-(\d{4}\.\d{2}\.\d{2})-(\d+)\.zip$`)

// NextVersion returns "YYYY.MM.DD-N", N = 1 + highest N already used today.
func NextVersion(existingKeys []string, today time.Time) string {
	day := today.UTC().Format("2006.01.02")
	var ns []int
	for _, k := range existingKeys {
		if m := verRe.FindStringSubmatch(k); m != nil && m[1] == day {
			n, _ := strconv.Atoi(m[2])
			ns = append(ns, n)
		}
	}
	sort.Ints(ns)
	next := 1
	if len(ns) > 0 {
		next = ns[len(ns)-1] + 1
	}
	return fmt.Sprintf("%s-%d", day, next)
}

// Current pins for new releases. The image tag is the one verified on a 5090 in P1.
const (
	DefaultLlamaImage = "ghcr.io/ggml-org/llama.cpp:server-cuda-b11118"
	DefaultModel      = "q8"
)

// 64K: OpenCode alone sends ~43K tokens (system prompt + tools) on the first request. Q8 @ 64K fits (measured).
var DefaultDefaults = Defaults{Ctx: 65536, IdleMin: 30, MaxHours: 12}
