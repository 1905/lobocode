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
// A dead pid means a crashed run: that stale file is removed (only if it is still the same run) and ok is false.
func ReadState() (State, bool, error) {
	s, err := readStateFile()
	if errors.Is(err, os.ErrNotExist) {
		return State{}, false, nil
	}
	if err != nil {
		return State{}, false, err
	}
	if !alive(s.PID) {
		return State{}, false, RemoveStateIf(s.PID, s.BootID)
	}
	return s, true, nil
}

func readStateFile() (State, error) {
	b, err := os.ReadFile(StatePath())
	if err != nil {
		return State{}, err
	}
	var s State
	if err := json.Unmarshal(b, &s); err != nil {
		return State{}, fmt.Errorf("read %s: %w", StatePath(), err)
	}
	return s, nil
}

// ClaimState makes s the one running local instance, mode 0600. The state file is created exclusively: a
// complete temp file is hard-linked into place, which fails like O_CREATE|O_EXCL when the file exists, and a
// reader never sees half a file. An existing state whose pid is a live, verified supervisor fails the claim.
// A stale one (dead pid, or a pid that is no longer our supervisor) is replaced.
func ClaimState(s State) error {
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
	defer func() { _ = os.Remove(tmp.Name()) }() // the claim is the link; the temp name always goes
	if _, err := tmp.Write(append(b, '\n')); err != nil {
		_ = tmp.Close()
		return err
	}
	if err := tmp.Close(); err != nil {
		return err
	}
	return withStateLock(func() error {
		err := os.Link(tmp.Name(), p)
		if !errors.Is(err, os.ErrExist) {
			return err
		}
		old, err := readStateFile()
		if err != nil {
			return err
		}
		if alive(old.PID) && isSupervisor(old.PID, old.BootID) {
			return fmt.Errorf("local already running (pid %d)", old.PID)
		}
		if err := os.Remove(p); err != nil && !errors.Is(err, os.ErrNotExist) {
			return err
		}
		return os.Link(tmp.Name(), p)
	})
}

// RemoveStateIf deletes the state file only if it belongs to the run with this pid and boot id. A missing
// file or another run's file is left alone and returns nil.
func RemoveStateIf(pid int, bootID string) error {
	return withStateLock(func() error {
		s, err := readStateFile()
		if errors.Is(err, os.ErrNotExist) {
			return nil
		}
		if err != nil {
			return err
		}
		if s.PID != pid || s.BootID != bootID {
			return nil
		}
		if err := os.Remove(StatePath()); err != nil && !errors.Is(err, os.ErrNotExist) {
			return err
		}
		return nil
	})
}

// withStateLock runs fn under an exclusive flock on local.json.lock, so a check and the change it decides
// (claim, replace, remove) cannot interleave with another lobo process.
func withStateLock(fn func() error) error {
	p := StatePath() + ".lock"
	if err := os.MkdirAll(filepath.Dir(p), 0o700); err != nil {
		return err
	}
	f, err := os.OpenFile(p, os.O_RDWR|os.O_CREATE, 0o600)
	if err != nil {
		return err
	}
	defer f.Close() // closing drops the lock
	if err := syscall.Flock(int(f.Fd()), syscall.LOCK_EX); err != nil {
		return fmt.Errorf("lock %s: %w", p, err)
	}
	return fn()
}

// ActiveEndpoints is the llama-server and agent API ports to talk to: a running instance's saved ports, else
// cfgPort and cfgPort+1. The configured port only applies to the next run. A state read error counts as none.
func ActiveEndpoints(cfgPort int) (llamaPort, apiPort int) {
	if s, ok, err := ReadState(); err == nil && ok {
		return s.Port, s.APIPort
	}
	return cfgPort, cfgPort + 1
}

// alive: signal 0 checks the pid exists. EPERM = exists but not ours, still alive.
func alive(pid int) bool {
	if pid <= 0 {
		return false
	}
	err := syscall.Kill(pid, 0)
	return err == nil || errors.Is(err, syscall.EPERM)
}
