package local

import (
	"encoding/json"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"sync"
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
	if err := ClaimState(want); err != nil {
		t.Fatal(err)
	}
	if fi, err := os.Stat(StatePath()); err != nil || fi.Mode().Perm() != 0o600 {
		t.Fatalf("state file %v %v", fi, err)
	}
	got, ok, err := ReadState()
	if err != nil || !ok || !got.StartedAt.Equal(want.StartedAt) {
		t.Fatalf("%+v %v %v", got, ok, err)
	}
	got.StartedAt = want.StartedAt
	if got != want {
		t.Fatalf("got %+v want %+v", got, want)
	}
	if err := RemoveStateIf(want.PID, want.BootID); err != nil {
		t.Fatal(err)
	}
	if err := RemoveStateIf(want.PID, want.BootID); err != nil {
		t.Fatalf("second remove: %v", err)
	}
	if _, ok, _ := ReadState(); ok {
		t.Fatal("state after remove")
	}
	if tmps, _ := filepath.Glob(filepath.Join(filepath.Dir(StatePath()), ".local-*")); len(tmps) != 0 {
		t.Fatalf("temp files left: %v", tmps)
	}
}

func TestStateDeadPID(t *testing.T) {
	t.Setenv("XDG_STATE_HOME", t.TempDir())
	if err := ClaimState(State{PID: deadPID(t), Port: 8931, BootID: "b0"}); err != nil {
		t.Fatal(err)
	}
	if _, ok, err := ReadState(); ok || err != nil {
		t.Fatalf("dead pid: ok=%v err=%v", ok, err)
	}
	if _, err := os.Stat(StatePath()); !os.IsNotExist(err) {
		t.Fatalf("state file kept: %v", err)
	}
}

func deadPID(t *testing.T) int {
	t.Helper()
	cmd := exec.Command("true")
	if err := cmd.Run(); err != nil {
		t.Fatal(err)
	}
	return cmd.Process.Pid
}

// psSupervisor makes every pid look like the supervisor of boot b1.
func psSupervisor(t *testing.T) {
	swap(t, &psCommand, func(int) (string, error) { return "lobo local run --boot-id b1", nil })
}

func TestClaimState(t *testing.T) {
	tests := []struct {
		name    string
		old     func(t *testing.T) State
		wantErr string
	}{
		{name: "live supervisor", old: func(t *testing.T) State { psSupervisor(t); return State{PID: os.Getpid(), BootID: "b1"} },
			wantErr: fmt.Sprintf("local already running (pid %d)", os.Getpid())},
		{name: "stale dead pid", old: func(t *testing.T) State { psSupervisor(t); return State{PID: deadPID(t), BootID: "b1"} }},
		{name: "live pid of another boot", old: func(t *testing.T) State { psSupervisor(t); return State{PID: os.Getpid(), BootID: "b0"} }},
		{name: "live pid, not a supervisor", old: func(t *testing.T) State { return State{PID: os.Getpid(), BootID: "b1"} }},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Setenv("XDG_STATE_HOME", t.TempDir())
			old := tt.old(t)
			if err := os.MkdirAll(filepath.Dir(StatePath()), 0o700); err != nil {
				t.Fatal(err)
			}
			b, _ := json.Marshal(old)
			if err := os.WriteFile(StatePath(), b, 0o600); err != nil {
				t.Fatal(err)
			}
			mine := State{PID: os.Getpid(), Port: 9000, BootID: "b2"}
			err := ClaimState(mine)
			if tt.wantErr == "" && err != nil || tt.wantErr != "" && (err == nil || err.Error() != tt.wantErr) {
				t.Fatalf("err %v, want %q", err, tt.wantErr)
			}
			got, _ := readStateFile()
			if want := map[bool]State{true: old, false: mine}[tt.wantErr != ""]; got.PID != want.PID || got.BootID != want.BootID {
				t.Fatalf("state %+v, want %+v", got, want)
			}
		})
	}
}

// Concurrent claims: exactly one wins, the file is that one's.
func TestClaimStateConcurrent(t *testing.T) {
	t.Setenv("XDG_STATE_HOME", t.TempDir())
	psSupervisor(t)
	const n = 16
	errs := make([]error, n)
	var wg sync.WaitGroup
	for i := 0; i < n; i++ {
		wg.Add(1)
		go func(i int) {
			defer wg.Done()
			errs[i] = ClaimState(State{PID: os.Getpid(), Port: 9000 + i, BootID: "b1"})
		}(i)
	}
	wg.Wait()
	won := -1
	for i, err := range errs {
		if err == nil {
			if won >= 0 {
				t.Fatalf("claims %d and %d both won", won, i)
			}
			won = i
		} else if !strings.Contains(err.Error(), "already running") {
			t.Fatal(err)
		}
	}
	if s, _ := readStateFile(); won < 0 || s.Port != 9000+won {
		t.Fatalf("winner %d, state %+v", won, s)
	}
}

func TestRemoveStateIf(t *testing.T) {
	tests := []struct {
		name     string
		pid      int
		bootID   string
		wantKept bool
	}{
		{name: "ours", pid: 111, bootID: "b1"},
		{name: "wrong pid", pid: 112, bootID: "b1", wantKept: true},
		{name: "wrong boot id", pid: 111, bootID: "b2", wantKept: true},
		{name: "empty boot id", pid: 111, bootID: "", wantKept: true},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Setenv("XDG_STATE_HOME", t.TempDir())
			if err := ClaimState(State{PID: 111, BootID: "b1"}); err != nil {
				t.Fatal(err)
			}
			if err := RemoveStateIf(tt.pid, tt.bootID); err != nil {
				t.Fatal(err)
			}
			if _, err := os.Stat(StatePath()); (err == nil) != tt.wantKept {
				t.Fatalf("stat %v, want kept %v", err, tt.wantKept)
			}
		})
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

func TestActiveEndpoints(t *testing.T) {
	tests := []struct {
		name      string
		state     *State
		wantLlama int
		wantAPI   int
	}{
		{name: "no state: config", wantLlama: 9000, wantAPI: 9001},
		{name: "running: saved ports", state: &State{PID: os.Getpid(), Port: 8931, APIPort: 8940, BootID: "b1"}, wantLlama: 8931, wantAPI: 8940},
		{name: "dead run: config", state: &State{PID: -1, Port: 8931, APIPort: 8940, BootID: "b1"}, wantLlama: 9000, wantAPI: 9001},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Setenv("XDG_STATE_HOME", t.TempDir())
			if tt.state != nil {
				if tt.state.PID < 0 {
					tt.state.PID = deadPID(t)
				}
				if err := ClaimState(*tt.state); err != nil {
					t.Fatal(err)
				}
			}
			if l, a := ActiveEndpoints(9000); l != tt.wantLlama || a != tt.wantAPI {
				t.Fatalf("got %d %d, want %d %d", l, a, tt.wantLlama, tt.wantAPI)
			}
		})
	}
}
