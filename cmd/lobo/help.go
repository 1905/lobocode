package main

import (
	"fmt"
	"io"
	"os"
	"strings"

	"charm.land/lipgloss/v2"
	"github.com/spf13/cobra"
)

var helpGroups = []struct {
	title string
	cmds  []string
}{
	{"Run", []string{"up", "status", "logs", "test", "down"}},
	{"Setup", []string{"config", "models", "gen-api-key", "version"}},
}

var helpExamples = [][2]string{
	{"lobo config", "set API keys and defaults (first run)"},
	{"lobo up", "rent a 5090 and boot the model; live progress"},
	{"lobo up --provider vast", "rent on Vast.ai instead of the default provider"},
	{"lobo up --provider local", "run on this Mac (Apple Silicon, llama.cpp); no cloud keys"},
	{"lobo up --q6 --min-mbps 200", "smaller model, drop hosts slower than 200 MB/s"},
	{"lobo status", "live dashboard: GPU, tokens/s, idle timer, cost"},
	{"lobo down", "delete every lobo pod on every provider"},
}

// rootHelp is `lobo` / `lobo help`: grouped commands, examples, and a first-run hint.
// Colour only on a terminal without NO_COLOR.
func rootHelp(w io.Writer, root *cobra.Command, color bool, haveConfig bool) {
	st := func(s lipgloss.Style) lipgloss.Style {
		if !color {
			return lipgloss.NewStyle()
		}
		return s
	}
	accent := lipgloss.Color("#7D56F4")
	var (
		title = st(lipgloss.NewStyle().Bold(true).Foreground(accent))
		head  = st(lipgloss.NewStyle().Bold(true))
		cmd   = st(lipgloss.NewStyle().Foreground(lipgloss.Color("#3FB950")))
		dim   = st(lipgloss.NewStyle().Foreground(lipgloss.Color("#8B949E")))
		warn  = st(lipgloss.NewStyle().Foreground(lipgloss.Color("#D29922")))
	)
	var b strings.Builder
	fmt.Fprintf(&b, "%s %s\n", title.Render("lobo"), dim.Render(version))
	b.WriteString("Rent an RTX 5090 on RunPod or Vast.ai and serve Qwen3.5-27B behind an OpenAI-compatible API.\n")
	b.WriteString("Pods delete themselves when idle or expired.\n")
	if !haveConfig {
		fmt.Fprintf(&b, "\n%s run %s first (no config at %s)\n", warn.Render("⚠"), cmd.Render("lobo config"), cfgPath)
	}
	for _, g := range helpGroups {
		fmt.Fprintf(&b, "\n%s\n", head.Render(g.title))
		for _, name := range g.cmds {
			c, _, err := root.Find([]string{name})
			if err != nil || c == root {
				continue
			}
			fmt.Fprintf(&b, "  %s %s\n", cmd.Render(fmt.Sprintf("%-12s", name)), c.Short)
		}
	}
	fmt.Fprintf(&b, "\n%s\n", head.Render("Examples"))
	for _, e := range helpExamples {
		fmt.Fprintf(&b, "  %s %s\n", cmd.Render(fmt.Sprintf("%-29s", e[0])), dim.Render(e[1]))
	}
	fmt.Fprintf(&b, "\n%s\n", head.Render("Config"))
	fmt.Fprintf(&b, "  %s\n  %s\n", cfgPath, dim.Render("plain KEY=value lines: edit by hand or with `lobo config`; flags override its defaults"))
	fmt.Fprintf(&b, "\n%s\n", dim.Render("`lobo <command> --help` for flags."))
	_, _ = io.WriteString(w, b.String())
}

func installHelp(root *cobra.Command) {
	def := root.HelpFunc()
	root.SetHelpFunc(func(c *cobra.Command, args []string) {
		if c != root {
			def(c, args)
			return
		}
		_, err := os.Stat(cfgPath)
		rootHelp(c.OutOrStdout(), root, isTTY() && os.Getenv("NO_COLOR") == "", err == nil)
	})
}
