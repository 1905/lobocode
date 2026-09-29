package local

import (
	"bufio"
	"context"
	"errors"
	"fmt"
	"net"
	"os"
	"os/exec"
	"path/filepath"
	"strconv"
	"strings"
	"syscall"
	"time"

	"github.com/1905/lobocode/internal/model"
	"github.com/1905/lobocode/internal/provider"
)

// Provider runs lobo on this Mac: Rent spawns the detached `lobo local run` supervisor.
type Provider struct {
	Exe        string // the lobo binary (os.Executable())
	ConfigPath string // passed to the child as --config; empty = its default
	Weights    string
	Port       int // llama-server; the agent API is Port+1
}

// Swapped in tests.
var (
	supported     = Supported
	ensureRuntime = EnsureRuntime
	listModels    = List
	psCommand     = commandOf
	stateWait     = 15 * time.Second // child must write its state file within this
	stopWait      = 10 * time.Second // SIGTERM grace before SIGKILL
)

func (Provider) Name() string { return "local" }

// LogPath is the supervisor's stdout+stderr, next to the state file.
func LogPath() string { return filepath.Join(filepath.Dir(StatePath()), "local.log") }

// Rent checks this Mac can run o.Model, fetches the runtime, then spawns the supervisor and waits for its state file.
// Checks run before any download: platform, weights writable, disk space, ports, then the runtime.
func (p Provider) Rent(ctx context.Context, o provider.CreateOpts, note func(string)) (provider.Instance, error) {
	if note == nil {
		note = func(string) {}
	}
	if err := supported(); err != nil {
		return provider.Instance{}, err
	}
	m, err := model.Get(o.Model)
	if err != nil {
		return provider.Instance{}, err
	}
	if s, ok, err := ReadState(); err != nil {
		return provider.Instance{}, err
	} else if ok {
		return provider.Instance{}, fmt.Errorf("lobo already running: local pid %d", s.PID)
	}
	if err := writable(p.Weights); err != nil {
		return provider.Instance{}, err
	}
	if err := p.checkSpace(m); err != nil {
		return provider.Instance{}, err
	}
	for _, c := range []struct {
		port int
		why  string
	}{{p.Port, "LOBO_LOCAL_PORT"}, {p.Port + 1, "agent API = LOBO_LOCAL_PORT+1"}} {
		if err := portFree(c.port); err != nil {
			return provider.Instance{}, fmt.Errorf("port %d in use (%s): %w", c.port, c.why, err)
		}
	}
	if _, err := ensureRuntime(ctx, p.Weights, note); err != nil {
		return provider.Instance{}, err
	}
	pid, err := p.spawn(o)
	if err != nil {
		return provider.Instance{}, err
	}
	return p.Get(ctx, strconv.Itoa(pid))
}

// writable: the weights folder exists or can be created, and a file can be written in it.
func writable(dir string) error {
	if err := os.MkdirAll(dir, 0o755); err != nil {
		return fmt.Errorf("weights folder %s is not writable: %w", dir, err)
	}
	f, err := os.CreateTemp(dir, ".lobo-write-*")
	if err != nil {
		return fmt.Errorf("weights folder %s is not writable: %w", dir, err)
	}
	_ = f.Close()
	return os.Remove(f.Name())
}

// checkSpace: the bytes still missing for m must fit in the free space. An oversize file is refetched after
// truncation, which frees more than it needs, so it counts as 0.
func (p Provider) checkSpace(m model.Model) error {
	l, err := listModels(p.Weights)
	if err != nil {
		return err
	}
	var need int64
	for _, s := range l.Models {
		if s.ID == m.ID && s.OnDisk < s.Size {
			need = s.Size - s.OnDisk
		}
	}
	if need > 0 && uint64(need) > l.FreeBytes {
		return fmt.Errorf("not enough space in %s for %s: need %d bytes, %d free", p.Weights, m.ID, need, l.FreeBytes)
	}
	return nil
}

func portFree(port int) error {
	ln, err := net.Listen("tcp", "127.0.0.1:"+strconv.Itoa(port))
	if err != nil {
		return err
	}
	return ln.Close()
}

// spawn starts `Exe local run …` in its own session with output appended to LogPath, and waits until
// ReadState shows its pid. An early exit or a timeout returns the tail of the log.
func (p Provider) spawn(o provider.CreateOpts) (int, error) {
	logPath := LogPath()
	if err := os.MkdirAll(filepath.Dir(logPath), 0o700); err != nil {
		return 0, err
	}
	logf, err := os.OpenFile(logPath, os.O_WRONLY|os.O_CREATE|os.O_APPEND, 0o600)
	if err != nil {
		return 0, err
	}
	defer logf.Close() // the child holds its own copy
	start, _ := logf.Seek(0, 2)
	var args []string
	if p.ConfigPath != "" {
		args = append(args, "--config", p.ConfigPath)
	}
	args = append(args, "local", "run", "--model", o.Model, "--ctx", strconv.Itoa(o.Ctx), "--idle-min", strconv.Itoa(o.IdleMin),
		"--boot-id", o.BootID, "--port", strconv.Itoa(p.Port), "--api-port", strconv.Itoa(p.Port+1))
	cmd := exec.Command(p.Exe, args...)
	cmd.Stdout, cmd.Stderr = logf, logf // Stdin nil = /dev/null
	cmd.SysProcAttr = &syscall.SysProcAttr{Setsid: true}
	if err := cmd.Start(); err != nil {
		return 0, err
	}
	pid := cmd.Process.Pid
	exited := make(chan error, 1)
	go func() { exited <- cmd.Wait() }() // reaps the child; `lobo up` exits long before it matters
	deadline := time.After(stateWait)
	for {
		if s, ok, err := ReadState(); err == nil && ok && s.PID == pid {
			return pid, nil
		}
		select {
		case err := <-exited:
			return 0, fmt.Errorf("lobo local run exited early (%v); %s:\n%s", err, logPath, logTail(logPath, start, 20))
		case <-deadline:
			_ = cmd.Process.Kill()
			return 0, fmt.Errorf("lobo local run wrote no state in %s; %s:\n%s", stateWait, logPath, logTail(logPath, start, 20))
		case <-time.After(100 * time.Millisecond):
		}
	}
}

// logTail is the last n lines written to path after offset from.
func logTail(path string, from int64, n int) string {
	f, err := os.Open(path)
	if err != nil {
		return ""
	}
	defer f.Close()
	if _, err := f.Seek(from, 0); err != nil {
		return ""
	}
	var lines []string
	sc := bufio.NewScanner(f)
	sc.Buffer(make([]byte, 64*1024), 1<<20)
	for sc.Scan() {
		lines = append(lines, sc.Text())
		if len(lines) > n {
			lines = lines[1:]
		}
	}
	return strings.Join(lines, "\n")
}

func (p Provider) List(ctx context.Context) ([]provider.Instance, error) {
	s, ok, err := ReadState()
	if err != nil || !ok {
		return nil, err
	}
	return []provider.Instance{instance(s)}, nil
}

// Get: ErrNotFound unless the running supervisor has this pid.
func (p Provider) Get(_ context.Context, id string) (provider.Instance, error) {
	s, ok, err := ReadState()
	if err != nil {
		return provider.Instance{}, err
	}
	if !ok || strconv.Itoa(s.PID) != id {
		return provider.Instance{}, provider.ErrNotFound
	}
	return instance(s), nil
}

// Delete stops the supervisor: SIGTERM, stopWait, then SIGKILL. The state file is removed either way.
// Already gone = nil (provider contract). A live pid that is not our supervisor (reused after a crash)
// is never signalled: the state is stale, so it is removed and Delete returns nil.
func (p Provider) Delete(ctx context.Context, id string) error {
	s, err := p.Get(ctx, id)
	if errors.Is(err, provider.ErrNotFound) {
		return nil
	}
	if err != nil {
		return err
	}
	pid, _ := strconv.Atoi(s.ID)
	if cmd, err := psCommand(pid); err != nil || !strings.Contains(cmd, "local run") {
		return RemoveState()
	}
	if err := syscall.Kill(pid, syscall.SIGTERM); err != nil && !errors.Is(err, syscall.ESRCH) {
		return fmt.Errorf("stop local pid %d: %w", pid, err)
	}
	if !waitGone(pid, stopWait) {
		_ = syscall.Kill(pid, syscall.SIGKILL)
		if !waitGone(pid, 2*time.Second) {
			return fmt.Errorf("local pid %d survived SIGKILL", pid)
		}
	}
	return RemoveState()
}

// commandOf is the full command line of pid (`ps -o command=`); an error means no such process.
func commandOf(pid int) (string, error) {
	out, err := exec.Command("ps", "-o", "command=", "-p", strconv.Itoa(pid)).Output()
	return strings.TrimSpace(string(out)), err
}

func waitGone(pid int, d time.Duration) bool {
	end := time.Now().Add(d)
	for alive(pid) {
		if time.Now().After(end) {
			return false
		}
		time.Sleep(50 * time.Millisecond)
	}
	return true
}

func instance(s State) provider.Instance {
	return provider.Instance{Provider: "local", ID: strconv.Itoa(s.PID), Status: "running", CostPerHr: 0,
		StartedAt: s.StartedAt, Detail: "this Mac, " + s.Model}
}
