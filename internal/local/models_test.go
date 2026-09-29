package local

import (
	"encoding/json"
	"os"
	"path/filepath"
	"reflect"
	"strings"
	"testing"
	"time"

	"github.com/1905/lobocode/internal/model"
)

func TestList(t *testing.T) {
	w := t.TempDir()
	q6, _ := model.Get("q6")
	q8, _ := model.Get("q8")
	// q6: full-size sparse file + marker. q8: short file, no marker.
	f, err := os.Create(filepath.Join(w, q6.File))
	if err != nil {
		t.Fatal(err)
	}
	if err := f.Truncate(q6.Size); err != nil {
		t.Fatal(err)
	}
	f.Close()
	if err := writeMarker(w, q6); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(w, q8.File), []byte("short"), 0o644); err != nil {
		t.Fatal(err)
	}

	l, err := List(w)
	if err != nil {
		t.Fatal(err)
	}
	want := []ModelState{
		{ID: "q6", File: q6.File, Size: q6.Size, OnDisk: q6.Size, Verified: true},
		{ID: "q8", File: q8.File, Size: q8.Size, OnDisk: 5, Verified: false},
	}
	if !reflect.DeepEqual(l.Models, want) {
		t.Fatalf("models %+v", l.Models)
	}
	if l.Weights != w || l.FreeBytes == 0 || l.Runtime.Version != RuntimeVersion || l.Runtime.Present {
		t.Fatalf("listing %+v", l)
	}

	// Missing file → 0; marker without the full file is not verified.
	if err := os.Rename(filepath.Join(w, q8.File), filepath.Join(t.TempDir(), "x")); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(MarkerPath(w, q8.File), nil, 0o644); err != nil {
		t.Fatal(err)
	}
	l, _ = List(w)
	if l.Models[1].OnDisk != 0 || l.Models[1].Verified {
		t.Fatalf("missing: %+v", l.Models[1])
	}

	// Runtime present once llama-server is in RuntimeDir.
	bin := filepath.Join(RuntimeDir(w), "build", "bin", "llama-server")
	if err := os.MkdirAll(filepath.Dir(bin), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(bin, nil, 0o755); err != nil {
		t.Fatal(err)
	}
	if l, _ = List(w); !l.Runtime.Present {
		t.Fatal("runtime not present")
	}
}

func TestListMissingWeights(t *testing.T) {
	w := filepath.Join(t.TempDir(), "nope")
	l, err := List(w)
	if err != nil {
		t.Fatal(err)
	}
	if l.FreeBytes != 0 || len(l.Models) != 2 || l.Models[0].OnDisk != 0 {
		t.Fatalf("%+v", l)
	}
}

func TestListJSON(t *testing.T) {
	l := Listing{Weights: "/w", FreeBytes: 7, Models: []ModelState{{ID: "q6", File: "f", Size: 2, OnDisk: 1, Verified: true}}}
	l.Runtime.Version, l.Runtime.Present = "b1", true
	b, err := json.Marshal(l)
	if err != nil {
		t.Fatal(err)
	}
	want := `{"weights":"/w","free_bytes":7,"models":[{"id":"q6","file":"f","size":2,"on_disk":1,"verified":true}],"runtime":{"version":"b1","present":true}}`
	if string(b) != want {
		t.Fatalf("got  %s\nwant %s", b, want)
	}
}

func TestMarkerPath(t *testing.T) {
	if got := MarkerPath("/w", "a.gguf"); got != "/w/a.gguf.sha256-ok" {
		t.Fatal(got)
	}
}

func TestMarkerValid(t *testing.T) {
	body := []byte("gguf bytes")
	m := model.Model{ID: "t", File: "t.gguf", SHA256: strings.Repeat("b", 64), Size: int64(len(body))}
	tests := []struct {
		name   string
		marker string // testMarker kind; "" = none, "written" = writeMarker
		change func(t *testing.T, path string)
		want   bool
	}{
		{name: "written", marker: "written", want: true},
		{name: "valid", marker: "valid", want: true},
		{name: "no marker"},
		{name: "stale mtime", marker: "stale mtime"},
		{name: "old format", marker: "old format"},
		{name: "wrong sha", marker: "wrong sha"},
		{name: "wrong size", marker: "wrong size"},
		{name: "file touched after marking", marker: "written", change: func(t *testing.T, path string) {
			later := time.Now().Add(time.Hour)
			if err := os.Chtimes(path, later, later); err != nil {
				t.Fatal(err)
			}
		}},
		{name: "same length, new bytes", marker: "written", change: func(t *testing.T, path string) {
			later := time.Now().Add(time.Hour)
			if err := os.WriteFile(path, []byte("GGUF BYTES"), 0o644); err != nil {
				t.Fatal(err)
			}
			if err := os.Chtimes(path, later, later); err != nil { // coarse clocks: force an mtime change
				t.Fatal(err)
			}
		}},
		{name: "file gone", marker: "written", change: func(t *testing.T, path string) {
			if err := os.Rename(path, path+".x"); err != nil {
				t.Fatal(err)
			}
		}},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			w := t.TempDir()
			path := filepath.Join(w, m.File)
			if err := os.WriteFile(path, body, 0o644); err != nil {
				t.Fatal(err)
			}
			switch tt.marker {
			case "":
			case "written":
				if err := writeMarker(w, m); err != nil {
					t.Fatal(err)
				}
			default:
				if err := os.WriteFile(MarkerPath(w, m.File), []byte(testMarker(t, w, m, tt.marker)), 0o644); err != nil {
					t.Fatal(err)
				}
			}
			if tt.change != nil {
				tt.change(t, path)
			}
			if got := markerValid(w, m); got != tt.want {
				b, _ := os.ReadFile(MarkerPath(w, m.File))
				t.Fatalf("markerValid = %v, want %v; marker %q", got, tt.want, b)
			}
			if tmps, _ := filepath.Glob(filepath.Join(w, ".sha256-ok-*")); len(tmps) != 0 {
				t.Fatalf("temp markers left: %v", tmps)
			}
		})
	}
}
