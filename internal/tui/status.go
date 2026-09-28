package tui

import (
	"context"
	"fmt"
	"strings"
	"time"

	tea "charm.land/bubbletea/v2"

	"github.com/1905/lobocode/internal/agent"
	"github.com/1905/lobocode/internal/control"
)

// RenderStatus draws the dashboard from one snapshot. Only /api/status data and RunPod pod data are shown.
func RenderStatus(s control.Snap) string {
	var b strings.Builder
	b.WriteString(title.Render("lobo status") + dimS.Render("  ·  "+s.At.Local().Format("15:04:05")) + "\n")
	if s.Down || s.Pod == nil {
		b.WriteString("\n" + dimS.Render("lobo: down (no lobo instance on any provider)") + "\n")
		return b.String()
	}
	p := s.Pod
	b.WriteString("\n" + section.Render("Pod") + "\n")
	up := time.Duration(0)
	if !p.StartedAt.IsZero() {
		up = s.At.Sub(p.StartedAt)
	}
	b.WriteString(row("id", p.ID+"  "+dimS.Render(p.Provider+" · "+p.Status)) + "\n")
	b.WriteString(row("cost", fmt.Sprintf("$%.2f/h  ·  up %s  ·  spent $%.2f", p.CostPerHr, dur(up), p.CostPerHr*up.Hours())) + "\n")

	b.WriteString("\n" + section.Render("Release") + "\n")
	if v := s.Version; v != nil {
		dirty := ""
		if v.GitDirty {
			dirty = warnS.Render("  ⚠ dirty tree")
		}
		b.WriteString(row("version", fmt.Sprintf("%s  ·  git %s%s", v.Version, v.GitSHA, dirty)) + "\n")
		b.WriteString(row("image", dimS.Render(v.LlamaImage)) + "\n")
	} else {
		b.WriteString(row("version", dimS.Render("n/a (agent not reachable yet)")) + "\n")
	}

	st := s.Status
	if st == nil {
		b.WriteString("\n" + section.Render("Agent") + "\n" + row("stage", dimS.Render("n/a (container booting or tunnel down)")) + "\n")
		return b.String()
	}
	b.WriteString("\n" + section.Render("Agent") + "\n")
	stage := string(st.Stage)
	switch st.Stage {
	case agent.StageReady:
		stage = okS.Render("● ready")
	case agent.StageFailed, agent.StageTerminating:
		stage = errS.Render("● " + stage)
	default:
		stage = warnS.Render("● " + stage)
	}
	b.WriteString(row("stage", stage+"  "+dimS.Render(fmt.Sprintf("%s · ctx %d", st.Model, st.Ctx))) + "\n")
	if st.StageDetail != "" {
		b.WriteString(row("detail", errS.Render(st.StageDetail)) + "\n")
	}
	if st.Stage == agent.StageDownload && st.Download.Total > 0 {
		f := float64(st.Download.Bytes) / float64(st.Download.Total)
		b.WriteString(row("download", fmt.Sprintf("%s %5.1f%%  %s / %s  %.0f MB/s", bar(f, 24, okS), 100*f, gb(st.Download.Bytes), gb(st.Download.Total), st.Download.MBps)) + "\n")
	}

	b.WriteString("\n" + section.Render("GPU") + "\n")
	if g := st.GPU; g != nil {
		util := float64(g.UtilPct) / 100
		vram := 0.0
		if g.VRAMTotalMB > 0 {
			vram = float64(g.VRAMUsedMB) / float64(g.VRAMTotalMB)
		}
		b.WriteString(row("device", g.Name) + "\n")
		b.WriteString(row("load", fmt.Sprintf("%s %3d%%", bar(util, 24, loadStyle(util)), g.UtilPct)) + "\n")
		b.WriteString(row("vram", fmt.Sprintf("%s %s / %s MB", bar(vram, 24, loadStyle(vram)), num(int64(g.VRAMUsedMB)), num(int64(g.VRAMTotalMB)))) + "\n")
	} else {
		b.WriteString(row("gpu", dimS.Render("n/a")) + "\n")
	}

	b.WriteString("\n" + section.Render("Host") + "\n")
	if h := st.Host; h != nil {
		mem := 0.0
		if h.MemTotalMB > 0 {
			mem = float64(h.MemUsedMB) / float64(h.MemTotalMB)
		}
		b.WriteString(row("cpu load", fmt.Sprintf("%.2f  %.2f  %.2f  %s", h.Load1, h.Load5, h.Load15, dimS.Render("(1/5/15 min)"))) + "\n")
		b.WriteString(row("ram", fmt.Sprintf("%s %s / %s MB", bar(mem, 24, loadStyle(mem)), num(int64(h.MemUsedMB)), num(int64(h.MemTotalMB)))) + "\n")
	} else {
		b.WriteString(row("host", dimS.Render("n/a")) + "\n")
	}

	b.WriteString("\n" + section.Render("LLM") + dimS.Render("  (totals since llama-server start)") + "\n")
	if l := st.Llama; l != nil {
		b.WriteString(row("requests", fmt.Sprintf("%d running · %d queued", l.RequestsProcessing, l.RequestsDeferred)) + "\n")
		b.WriteString(row("tokens in", num(l.PromptTokensTotal)+dimS.Render(fmt.Sprintf("  ·  %.0f tok/s prompt", l.PromptTPS))) + "\n")
		b.WriteString(row("tokens out", num(l.GenTokensTotal)+dimS.Render(fmt.Sprintf("  ·  %.1f tok/s gen", l.GenTPS))) + "\n")
	} else {
		b.WriteString(row("llama", dimS.Render("n/a")) + "\n")
	}

	b.WriteString("\n" + section.Render("Watchdog") + "\n")
	if st.Stage == agent.StageReady {
		b.WriteString(row("idle", dur(time.Duration(st.IdleS)*time.Second)) + "\n")
	}
	kill := fmt.Sprintf("in %s (%s)", dur(time.Duration(st.KillInS)*time.Second), st.KillReason)
	if st.KillInS < 300 {
		kill = warnS.Render(kill)
	}
	b.WriteString(row("auto-kill", kill) + "\n")
	b.WriteString(row("expires", st.ExpiresAt.Local().Format("15:04 Mon")) + "\n")
	if st.MetricsFailures > 0 {
		b.WriteString(row("metrics", warnS.Render(fmt.Sprintf("⚠ %d failed reads in a row", st.MetricsFailures))) + "\n")
	}
	return b.String()
}

type snapMsg struct {
	s   control.Snap
	err error
}

// StatusModel refreshes the dashboard every 2 s. q quits.
type StatusModel struct {
	ctx  context.Context
	d    control.Deps
	snap *control.Snap
	err  error
}

func NewStatus(ctx context.Context, d control.Deps) StatusModel { return StatusModel{ctx: ctx, d: d} }

func (m StatusModel) fetch() tea.Msg {
	s, err := control.Snapshot(m.ctx, m.d)
	return snapMsg{s, err}
}

func (m StatusModel) Init() tea.Cmd { return m.fetch }

func (m StatusModel) Update(msg tea.Msg) (tea.Model, tea.Cmd) {
	switch msg := msg.(type) {
	case snapMsg:
		m.err = msg.err
		if msg.err == nil {
			m.snap = &msg.s // on error keep the last good snapshot
		}
		if msg.err == nil && msg.s.Down {
			return m, tea.Quit
		}
		return m, tea.Tick(2*time.Second, func(time.Time) tea.Msg { return m.fetch() })
	case tea.KeyPressMsg:
		if s := msg.String(); s == "q" || s == "ctrl+c" || s == "esc" {
			return m, tea.Quit
		}
	}
	return m, nil
}

func (m StatusModel) View() tea.View {
	if m.snap == nil {
		if m.err != nil {
			return tea.NewView(errS.Render("status failed: "+m.err.Error()) + "\n" + dimS.Render("retrying every 2s · q quit") + "\n")
		}
		return tea.NewView(dimS.Render("loading…") + "\n")
	}
	out := RenderStatus(*m.snap)
	if m.err != nil {
		out += "\n" + errS.Render("refresh failed: "+m.err.Error()) + "\n"
	}
	return tea.NewView(out + "\n" + dimS.Render("q quit · refresh 2s") + "\n")
}
