package main

import (
	"bytes"
	"context"
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/1905/lobocode/internal/local"
	"github.com/1905/lobocode/internal/model"
)

func TestLocalRunFlags(t *testing.T) {
	tests := []struct {
		name    string
		args    []string
		want    runOpts
		wantErr string
	}{
		{name: "all", args: []string{"--model", "q8", "--ctx", "65536", "--idle-min", "20", "--boot-id", "b1", "--port", "9000", "--api-port", "9001"},
			want: runOpts{Model: "q8", Ctx: 65536, IdleMin: 20, BootID: "b1", Port: 9000, APIPort: 9001}},
		{name: "default ports", args: []string{"--ctx", "4096", "--idle-min", "5"},
			want: runOpts{Model: "q6", Ctx: 4096, IdleMin: 5, Port: 8931, APIPort: 8932}},
		{name: "bad model", args: []string{"--model", "q2", "--ctx", "1", "--idle-min", "1"}, wantErr: "q2"},
		{name: "no ctx", args: []string{"--idle-min", "1"}, wantErr: "--ctx"},
		{name: "no idle", args: []string{"--ctx", "1"}, wantErr: "--idle-min"},
		{name: "same ports", args: []string{"--ctx", "1", "--idle-min", "1", "--port", "9000", "--api-port", "9000"}, wantErr: "--api-port"},
		{name: "extra arg", args: []string{"--ctx", "1", "--idle-min", "1", "x"}, wantErr: "unknown command"},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			var got runOpts
			called := false
			c := localCmd(func(_ context.Context, o runOpts) error { got, called = o, true; return nil })
			c.SetArgs(append([]string{"run"}, tt.args...))
			c.SetOut(&bytes.Buffer{})
			c.SetErr(&bytes.Buffer{})
			err := c.Execute()
			if tt.wantErr != "" {
				if err == nil || !strings.Contains(err.Error(), tt.wantErr) || called {
					t.Fatalf("err %v, called %v, want %q", err, called, tt.wantErr)
				}
				return
			}
			if err != nil || got != tt.want {
				t.Fatalf("err %v, got %+v, want %+v", err, got, tt.want)
			}
		})
	}
	if !localCmd(nil).Hidden {
		t.Fatal("local must be hidden")
	}
}

func TestModelsOutput(t *testing.T) {
	w := t.TempDir()
	all := model.All()
	if len(all) < 2 {
		t.Fatal("catalog needs 2 models for this test")
	}
	// all[0]: half on disk. all[1]: missing. The files are sparse: Truncate allocates no blocks.
	f, err := os.Create(filepath.Join(w, all[0].File))
	if err != nil {
		t.Fatal(err)
	}
	if err := f.Truncate(all[0].Size / 2); err != nil {
		t.Fatal(err)
	}
	f.Close()

	var b bytes.Buffer
	if err := writeModels(&b, w, true); err != nil {
		t.Fatal(err)
	}
	var l local.Listing
	if err := json.Unmarshal(b.Bytes(), &l); err != nil {
		t.Fatal(err)
	}
	var raw map[string]any
	_ = json.Unmarshal(b.Bytes(), &raw)
	for _, k := range []string{"weights", "free_bytes", "models", "runtime"} {
		if _, ok := raw[k]; !ok {
			t.Fatalf("json has no %q: %s", k, b.String())
		}
	}
	if l.Weights != w || len(l.Models) != len(all) || l.Models[0].OnDisk != all[0].Size/2 || l.Models[1].OnDisk != 0 || l.Runtime.Present {
		t.Fatalf("listing %+v", l)
	}

	b.Reset()
	if err := writeModels(&b, w, false); err != nil {
		t.Fatal(err)
	}
	out := b.String()
	for _, want := range []string{all[0].ID, "partial 50%", all[1].ID, "missing", "weights " + w, "GB free"} {
		if !strings.Contains(out, want) {
			t.Fatalf("plain output lacks %q:\n%s", want, out)
		}
	}
}
