// Package tui renders `lobo up` progress and the `lobo status` dashboard (bubbletea + lipgloss).
package tui

import (
	"fmt"
	"strings"
	"time"

	"charm.land/lipgloss/v2"
)

var (
	cAccent = lipgloss.Color("#7D56F4")
	cOK     = lipgloss.Color("#3FB950")
	cWarn   = lipgloss.Color("#D29922")
	cErr    = lipgloss.Color("#F85149")
	cDim    = lipgloss.Color("#8B949E")

	title   = lipgloss.NewStyle().Bold(true).Foreground(cAccent)
	label   = lipgloss.NewStyle().Foreground(cDim).Width(14)
	okS     = lipgloss.NewStyle().Foreground(cOK)
	warnS   = lipgloss.NewStyle().Foreground(cWarn)
	errS    = lipgloss.NewStyle().Foreground(cErr)
	dimS    = lipgloss.NewStyle().Foreground(cDim)
	boldS   = lipgloss.NewStyle().Bold(true)
	section = lipgloss.NewStyle().Bold(true).Foreground(cAccent)
	box     = lipgloss.NewStyle().Border(lipgloss.RoundedBorder()).BorderForeground(cAccent).Padding(0, 1)
	errBox  = lipgloss.NewStyle().Border(lipgloss.RoundedBorder()).BorderForeground(cErr).Padding(0, 1)
)

// bar is a plain block bar; frac in [0,1].
func bar(frac float64, width int, st lipgloss.Style) string {
	if frac < 0 {
		frac = 0
	}
	if frac > 1 {
		frac = 1
	}
	full := int(frac*float64(width) + 0.5)
	return st.Render(strings.Repeat("█", full)) + dimS.Render(strings.Repeat("░", width-full))
}

// loadStyle colours a fraction green / yellow / red.
func loadStyle(frac float64) lipgloss.Style {
	switch {
	case frac >= 0.95:
		return errS
	case frac >= 0.8:
		return warnS
	}
	return okS
}

func gb(b int64) string { return fmt.Sprintf("%.1f GB", float64(b)/1e9) }

func dur(d time.Duration) string {
	d = d.Round(time.Second)
	if d >= time.Hour {
		return fmt.Sprintf("%dh%02dm", int(d.Hours()), int(d.Minutes())%60)
	}
	if d >= time.Minute {
		return fmt.Sprintf("%dm%02ds", int(d.Minutes()), int(d.Seconds())%60)
	}
	return fmt.Sprintf("%ds", int(d.Seconds()))
}

func num(n int64) string {
	s := fmt.Sprint(n)
	var b strings.Builder
	for i, r := range s {
		if i > 0 && (len(s)-i)%3 == 0 {
			b.WriteByte(',')
		}
		b.WriteRune(r)
	}
	return b.String()
}

func row(k, v string) string { return label.Render(k) + v }

func visibleLen(s string) int { return lipgloss.Width(s) }

func secs(f float64) time.Duration { return time.Duration(f * float64(time.Second)) }

// clock formats a running timer as m:ss.
func clock(d time.Duration) string {
	d = d.Round(time.Second)
	return fmt.Sprintf("%d:%02d", int(d.Minutes()), int(d.Seconds())%60)
}
