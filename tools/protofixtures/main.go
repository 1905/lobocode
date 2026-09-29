// protofixtures captures wire values from the Go types while both implementations exist.
// It uses fixed data only. It does not read config, call providers or inspect local state.
package main

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"reflect"
	"time"

	"github.com/1905/lobocode/internal/agent"
	"github.com/1905/lobocode/internal/control"
	"github.com/1905/lobocode/internal/local"
	"github.com/1905/lobocode/internal/model"
	"github.com/1905/lobocode/internal/provider"
	"github.com/1905/lobocode/internal/release"
)

// These two CLI-local types mirror cmd/lobo/main.go and cmd/lobo/config.go.
// Keep their embedded type and JSON tags identical to the CLI output.
type upEvent struct {
	control.Event
	Err string `json:"err,omitempty"`
}

type configShow struct {
	Path   string            `json:"path"`
	Exists bool              `json:"exists"`
	Values map[string]string `json:"values"`
	Set    map[string]bool   `json:"set"`
}

type catalogModel struct {
	ID       string   `json:"id"`
	File     string   `json:"file"`
	SHA256   string   `json:"sha256"`
	Alias    string   `json:"alias"`
	Size     int64    `json:"size"`
	ChunkSHA []string `json:"chunk_sha"`
}

var fixedTime = time.Date(2026, 9, 29, 10, 0, 0, 500000000, time.UTC)

// Fill every exported wire field, including future fields, so a missing Rust field
// cannot pass the drift check because its fixture value happened to be zero.
func fill(v reflect.Value) {
	if v.Type() == reflect.TypeFor[time.Time]() {
		v.Set(reflect.ValueOf(fixedTime))
		return
	}
	switch v.Kind() {
	case reflect.Struct:
		for i := 0; i < v.NumField(); i++ {
			if v.Field(i).CanSet() {
				fill(v.Field(i))
			}
		}
	case reflect.Pointer:
		v.Set(reflect.New(v.Type().Elem()))
		fill(v.Elem())
	case reflect.String:
		v.SetString("sample")
	case reflect.Bool:
		v.SetBool(true)
	case reflect.Int, reflect.Int64:
		v.SetInt(42)
	case reflect.Uint64:
		v.SetUint(42)
	case reflect.Float64:
		v.SetFloat(1.25)
	case reflect.Slice:
		v.Set(reflect.MakeSlice(v.Type(), 2, 2))
		for i := 0; i < v.Len(); i++ {
			fill(v.Index(i))
		}
	default:
		panic(fmt.Sprintf("fixture generator needs support for %s", v.Type()))
	}
}

func sample[T any]() T {
	var out T
	fill(reflect.ValueOf(&out).Elem())
	return out
}

func write(path string, value any) error {
	b, err := json.MarshalIndent(value, "", "  ")
	if err != nil {
		return err
	}
	if err = os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		return err
	}
	return os.WriteFile(path, append(b, '\n'), 0o644)
}

func run() error {
	if len(os.Args) != 3 {
		return fmt.Errorf("usage: protofixtures <fixture-directory> <catalog.json>")
	}
	ready := sample[agent.Status]()
	ready.Stage, ready.Model = agent.StageReady, "q8"
	ready.Timings.DownloadSource = "https://models.example.com/q8.gguf"
	boot := agent.Status{Stage: agent.StageDownload, Download: agent.DownloadProgress{Bytes: 1024, Total: 8192, MBps: 1.25, Verifying: true, Source: "fallback"}}
	offset := ready
	offset.ExpiresAt = time.Date(2026, 9, 29, 10, 0, 0, 123456789, time.FixedZone("+03", 3*3600))
	r := sample[control.ReadyInfo]()
	r.Elapsed, r.URL, r.Provider = 123456789012, "https://lobo.example.com/v1", "runpod"
	r.Timings.DownloadSource = "https://models.example.com/q8.gguf"
	manifest := sample[release.Manifest]()
	manifest.LlamaImage, manifest.Model.ID = release.DefaultLlamaImage, release.DefaultModel
	manifest.Defaults = release.DefaultDefaults
	in := sample[provider.Instance]()
	in.Provider, in.APIURL, in.AgentURL = "runpod", "https://lobo.example.com/v1", "https://lobo.example.com"
	listing := sample[local.Listing]()
	listing.Weights, listing.Models[0].ID, listing.Models[1].ID = "/weights", "q6", "q8"
	listing.Models[1].Verified = false
	state := sample[local.State]()
	state.Weights, state.Port, state.APIPort = "/weights", 8931, 8932
	state.StartedAt = time.Date(2026, 9, 29, 10, 0, 0, 0, time.FixedZone("+02", 2*3600))
	fixtures := map[string]any{
		"status_ready":       ready,
		"status_booting":     boot,
		"status_offset_time": offset,
		"up_event_progress":  upEvent{Event: control.Event{Phase: "download", Detail: "downloading", Download: &ready.Download}},
		"up_event_ready":     upEvent{Event: control.Event{Phase: "ready", Ready: &r, Done: true}},
		"up_event_error":     upEvent{Event: control.Event{Phase: "failed", Done: true}, Err: "no host"},
		"snap_running":       control.Snap{Pod: &in, Version: &manifest, Status: &ready, At: fixedTime},
		"snap_down":          control.Snap{Down: true, At: fixedTime},
		"manifest":           manifest,
		"resolved":           release.Resolved{Manifest: manifest, ZipKey: "releases/lobo-fixture.zip", ZipSHA256: "fixture-sha"},
		"listing":            listing,
		"listing_empty":      local.Listing{},
		"local_state":        state,
		"config_show": configShow{Path: "/config.env", Exists: true,
			Values: map[string]string{"Z": "last", "A": "first", "M": "middle"},
			Set:    map[string]bool{"Z": true, "A": false, "M": true}},
	}
	for name, value := range fixtures {
		if err := write(filepath.Join(os.Args[1], name+".json"), value); err != nil {
			return err
		}
	}
	var catalog []catalogModel
	for _, m := range model.All() {
		catalog = append(catalog, catalogModel{m.ID, m.File, m.SHA256, m.Alias, m.Size, m.ChunkSHA()})
	}
	return write(os.Args[2], catalog)
}

func main() {
	if err := run(); err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
}
