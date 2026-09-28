package main

import (
	"context"
	"fmt"

	tea "charm.land/bubbletea/v2"
	"github.com/charmbracelet/x/ansi"

	"github.com/1905/lobocode/internal/control"
	"github.com/1905/lobocode/internal/tui"
)

func tuiUp(events <-chan control.Event) error {
	m, err := tea.NewProgram(tui.NewUp(events)).Run()
	if err != nil {
		return err
	}
	return m.(tui.UpModel).Err()
}

func tuiStatus(ctx context.Context, d control.Deps) error {
	_, err := tea.NewProgram(tui.NewStatus(ctx, d)).Run()
	return err
}

// renderStatus is the one-shot (--once / piped) dashboard, plain text when not a TTY.
func renderStatus(s control.Snap, _ int) string {
	out := tui.RenderStatus(s)
	if !isTTY() {
		out = ansi.Strip(out)
	}
	return fmt.Sprint(out)
}
