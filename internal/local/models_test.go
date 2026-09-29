package local

import (
	"encoding/json"
	"os"
	"path/filepath"
	"reflect"
	"testing"

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
	if err := os.WriteFile(MarkerPath(w, q6.File), []byte(q6.SHA256), 0o644); err != nil {
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
