// Package local runs llama.cpp on this Mac: `lobo up --provider local`.
package local

import (
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"syscall"
	"time"
)

// State is the running local instance, written by `lobo local run` once it is up.
type State struct {
	PID       int       `json:"pid"`
	Port      int       `json:"port"`
	APIPort   int       `json:"api_port"`
	Model     string    `json:"model"`
	Weights   string    `json:"weights"`
	StartedAt time.Time `json:"started_at"`
	BootID    string    `json:"boot_id"`
}

// StatePath is $XDG_STATE_HOME/lobo/local.json, else ~/.local/state/lobo/local.json.
func StatePath() string {
	dir := os.Getenv("XDG_STATE_HOME")
	if dir == "" {
		home, err := os.UserHomeDir()
		if err != nil {
			home = "."
		}
		dir = filepath.Join(home, ".local", "state")
	}
	return filepath.Join(dir, "lobo", "local.json")
}

// ReadState returns the state if the file exists and its pid is alive.
// A dead pid means a crashed run: the stale file is removed and ok is false.
func ReadState() (State, bool, error) {
	b, err := os.ReadFile(StatePath())
	if errors.Is(err, os.ErrNotExist) {
		return State{}, false, nil
	}
	if err != nil {
		return State{}, false, err
	}
	var s State
	if err := json.Unmarshal(b, &s); err != nil {
		return State{}, false, fmt.Errorf("read %s: %w", StatePath(), err)
	}
	if !alive(s.PID) {
		return State{}, false, RemoveState()
	}
	return s, true, nil
}

// WriteState writes the state atomically (temp file + rename), mode 0600.
func WriteState(s State) error {
	p := StatePath()
	if err := os.MkdirAll(filepath.Dir(p), 0o700); err != nil {
		return err
	}
	b, err := json.MarshalIndent(s, "", "  ")
	if err != nil {
		return err
	}
	tmp, err := os.CreateTemp(filepath.Dir(p), ".local-*.json")
	if err != nil {
		return err
	}
	defer func() { _ = os.Remove(tmp.Name()) }() // no-op after a successful rename
	if _, err := tmp.Write(append(b, '\n')); err != nil {
		_ = tmp.Close()
		return err
	}
	if err := tmp.Close(); err != nil {
		return err
	}
	return os.Rename(tmp.Name(), p)
}

// RemoveState deletes the state file; a missing file is fine.
func RemoveState() error {
	if err := os.Remove(StatePath()); err != nil && !errors.Is(err, os.ErrNotExist) {
		return err
	}
	return nil
}

// alive: signal 0 checks the pid exists. EPERM = exists but not ours, still alive.
func alive(pid int) bool {
	if pid <= 0 {
		return false
	}
	err := syscall.Kill(pid, 0)
	return err == nil || errors.Is(err, syscall.EPERM)
}
