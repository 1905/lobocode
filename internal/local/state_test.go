package local

import (
	"os"
	"os/exec"
	"path/filepath"
	"testing"
	"time"
)

func TestStatePath(t *testing.T) {
	t.Setenv("XDG_STATE_HOME", "/x")
	if StatePath() != "/x/lobo/local.json" {
		t.Fatal(StatePath())
	}
	t.Setenv("XDG_STATE_HOME", "")
	home, _ := os.UserHomeDir()
	if StatePath() != filepath.Join(home, ".local/state/lobo/local.json") {
		t.Fatal(StatePath())
	}
}

func TestStateRoundTrip(t *testing.T) {
	t.Setenv("XDG_STATE_HOME", t.TempDir())
	if _, ok, err := ReadState(); ok || err != nil {
		t.Fatalf("missing file: ok=%v err=%v", ok, err)
	}
	want := State{PID: os.Getpid(), Port: 8931, APIPort: 8932, Model: "q6", Weights: "/w",
		StartedAt: time.Date(2026, 9, 29, 12, 0, 0, 0, time.UTC), BootID: "b1"}
	if err := WriteState(want); err != nil {
		t.Fatal(err)
	}
	got, ok, err := ReadState()
	if err != nil || !ok || !got.StartedAt.Equal(want.StartedAt) {
		t.Fatalf("%+v %v %v", got, ok, err)
	}
	got.StartedAt = want.StartedAt
	if got != want {
		t.Fatalf("got %+v want %+v", got, want)
	}
	if err := RemoveState(); err != nil {
		t.Fatal(err)
	}
	if err := RemoveState(); err != nil {
		t.Fatalf("second remove: %v", err)
	}
	if _, ok, _ := ReadState(); ok {
		t.Fatal("state after remove")
	}
}

func TestStateDeadPID(t *testing.T) {
	t.Setenv("XDG_STATE_HOME", t.TempDir())
	cmd := exec.Command("true")
	if err := cmd.Run(); err != nil {
		t.Fatal(err)
	}
	if err := WriteState(State{PID: cmd.Process.Pid, Port: 8931}); err != nil {
		t.Fatal(err)
	}
	if _, ok, err := ReadState(); ok || err != nil {
		t.Fatalf("dead pid: ok=%v err=%v", ok, err)
	}
	if _, err := os.Stat(StatePath()); !os.IsNotExist(err) {
		t.Fatalf("state file kept: %v", err)
	}
}

func TestStateCorrupt(t *testing.T) {
	t.Setenv("XDG_STATE_HOME", t.TempDir())
	if err := os.MkdirAll(filepath.Dir(StatePath()), 0o700); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(StatePath(), []byte("{"), 0o600); err != nil {
		t.Fatal(err)
	}
	if _, ok, err := ReadState(); ok || err == nil {
		t.Fatalf("corrupt: ok=%v err=%v", ok, err)
	}
}
