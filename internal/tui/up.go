package tui

import (
	"fmt"
	"strings"
	"time"

	"charm.land/bubbles/v2/spinner"
	tea "charm.land/bubbletea/v2"

	"github.com/1905/lobocode/internal/control"
)

var upPhases = []struct{ id, name string }{
	{"create", "rent pod"},
	{"image", "boot container"},
	{"tunnel", "tunnel"},
	{"gpu", "gpu check"},
	{"download", "download model"},
	{"verify", "sha256 verify"},
	{"load", "load model"},
	{"ready", "ready"},
}

// UpState is everything the up view shows. Apply folds events into it.
type UpState struct {
	Phase   string
	Details map[string]string
	Event   control.Event // last event (download progress, ready info)
	Err     error
	Done    bool
	At      time.Time                // render time (spinner tick), for live elapsed timers
	started map[string]time.Time     // when each phase was first seen
	took    map[string]time.Duration // finished phases
}

func (s *UpState) Apply(e control.Event) { s.ApplyAt(e, time.Now()) }

// ApplyAt folds an event that arrived at at. Timers come from arrival times: a phase that finished
// between two polls shows no duration. Going back to an earlier phase (re-rent) resets later timers.
func (s *UpState) ApplyAt(e control.Event, at time.Time) {
	if s.Details == nil {
		s.Details, s.started, s.took = map[string]string{}, map[string]time.Time{}, map[string]time.Duration{}
	}
	if s.At.Before(at) {
		s.At = at
	}
	if e.Phase != "failed" && e.Phase != "terminated" && e.Phase != s.Phase {
		ni, oi := phaseIndex(e.Phase), phaseIndex(s.Phase)
		if s.Phase != "" && ni >= 0 && ni <= oi {
			for _, p := range upPhases[ni:] {
				delete(s.started, p.id)
				delete(s.took, p.id)
			}
		} else if st, ok := s.started[s.Phase]; ok {
			s.took[s.Phase] = at.Sub(st)
		}
		s.started[e.Phase] = at
	}
	if e.Phase != "failed" && e.Phase != "terminated" {
		s.Phase = e.Phase
		if e.Detail != "" {
			s.Details[e.Phase] = e.Detail
		}
	}
	s.Event = e
	if e.Err != nil {
		s.Err = e.Err
	}
	s.Done = s.Done || e.Done
}

func phaseIndex(id string) int {
	for i, p := range upPhases {
		if p.id == id {
			return i
		}
	}
	return -1
}

// RenderUp draws the stage list; spin is the current spinner frame.
func RenderUp(s UpState, spin string) string {
	var b strings.Builder
	b.WriteString(title.Render("lobo up") + dimS.Render("  ·  Qwen3.5-27B on a rented RTX 5090") + "\n\n")
	cur := phaseIndex(s.Phase)
	for i, p := range upPhases {
		var mark, name string
		switch {
		case i < cur || (i == cur && s.Phase == "ready"):
			mark, name = okS.Render("✓"), p.name
		case i == cur && s.Err != nil:
			mark, name = errS.Render("✗"), errS.Render(p.name)
		case i == cur:
			mark, name = spin, boldS.Render(p.name)
		default:
			mark, name = dimS.Render("·"), dimS.Render(p.name)
		}
		line := fmt.Sprintf(" %s %s", mark, lipglossPad(name, 16))
		switch {
		case i < cur || (i == cur && s.Phase == "ready"):
			if d, ok := s.took[p.id]; ok {
				line += dimS.Render(fmt.Sprintf("%-6s", dur(d))) + " "
			}
		case i == cur && s.Err == nil:
			if st, ok := s.started[p.id]; ok && !s.At.Before(st) {
				line += boldS.Render(fmt.Sprintf("%-6s", clock(s.At.Sub(st)))) + " "
			}
			if p.id == "image" {
				line += dimS.Render(fmt.Sprintf("usually 15–30 s · re-rent at %s ", clock(control.ContainerTimeout)))
			}
		}
		if d := s.Details[p.id]; d != "" && i <= cur {
			line += dimS.Render(d)
		}
		if p.id == "download" && i == cur && s.Event.Download != nil && s.Event.Download.Total > 0 {
			dl := s.Event.Download
			frac := float64(dl.Bytes) / float64(dl.Total)
			eta := "?"
			if dl.MBps > 0 {
				eta = dur(secs(float64(dl.Total-dl.Bytes) / (dl.MBps * 1e6)))
			}
			line += fmt.Sprintf("%s %5.1f%%  %s / %s  %.0f MB/s  ETA %s", bar(frac, 24, okS), 100*frac, gb(dl.Bytes), gb(dl.Total), dl.MBps, eta)
		}
		b.WriteString(line + "\n")
	}
	if r := s.Event.Ready; r != nil {
		sha := r.GitSHA
		if sha == "" {
			sha = "?"
		}
		b.WriteString("\n" + box.Render(strings.Join([]string{
			okS.Bold(true).Render("✓ lobo is up"),
			row("endpoint", boldS.Render(r.URL)),
			row("release", fmt.Sprintf("%s (%s)", r.Version, sha)),
			row("cost", fmt.Sprintf("$%.2f/h", r.CostPerHr)),
			row("boot time", dur(r.Elapsed)),
			dimS.Render("lobo status · lobo test · lobo down"),
		}, "\n")) + "\n")
	}
	if s.Err != nil {
		b.WriteString("\n" + errBox.Render(errS.Bold(true).Render("✗ up failed")+"\n"+s.Err.Error()) + "\n")
	}
	return b.String()
}

func lipglossPad(s string, w int) string {
	if n := w - visibleLen(s); n > 0 {
		return s + strings.Repeat(" ", n)
	}
	return s + " "
}

type eventMsg struct {
	e  control.Event
	ok bool
}

func next(ch <-chan control.Event) tea.Cmd {
	return func() tea.Msg {
		e, ok := <-ch
		return eventMsg{e, ok}
	}
}

// UpModel is the bubbletea model for `lobo up`.
type UpModel struct {
	ch    <-chan control.Event
	state UpState
	spin  spinner.Model
}

func NewUp(ch <-chan control.Event) UpModel {
	return UpModel{ch: ch, spin: spinner.New(spinner.WithSpinner(spinner.Dot), spinner.WithStyle(okS))}
}

func (m UpModel) Err() error { return m.state.Err }

func (m UpModel) Init() tea.Cmd { return tea.Batch(next(m.ch), m.spin.Tick) }

func (m UpModel) Update(msg tea.Msg) (tea.Model, tea.Cmd) {
	switch msg := msg.(type) {
	case eventMsg:
		if !msg.ok {
			return m, tea.Quit
		}
		m.state.Apply(msg.e)
		if m.state.Done {
			return m, tea.Quit
		}
		return m, next(m.ch)
	case tea.KeyPressMsg:
		if msg.String() == "ctrl+c" || msg.String() == "q" {
			m.state.Err = fmt.Errorf("interrupted: the pod keeps booting; check with `lobo status`, stop with `lobo down`")
			return m, tea.Quit
		}
	case spinner.TickMsg:
		m.state.At = time.Now()
		var cmd tea.Cmd
		m.spin, cmd = m.spin.Update(msg)
		return m, cmd
	}
	return m, nil
}

func (m UpModel) View() tea.View { return tea.NewView(RenderUp(m.state, m.spin.View())) }
