package main

import (
	"flag"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/spf13/cobra"
)

var update = flag.Bool("update", false, "rewrite golden files")

func TestRootHelpGolden(t *testing.T) {
	root := &cobra.Command{Use: "lobo"}
	root.AddCommand(versionCmd(), configCmd(), genKeyCmd(), releaseCmd(), upCmd(), downCmd(), statusCmd(), logsCmd(), testCmd())
	cfgPath = "/home/u/.config/lobo/config.env"
	for name, have := range map[string]bool{"help_config": true, "help_noconfig": false} {
		var b strings.Builder
		rootHelp(&b, root, false, have)
		p := filepath.Join("testdata", name+".golden")
		if *update {
			_ = os.MkdirAll("testdata", 0o755)
			if err := os.WriteFile(p, []byte(b.String()), 0o644); err != nil {
				t.Fatal(err)
			}
		}
		want, err := os.ReadFile(p)
		if err != nil {
			t.Fatal(err)
		}
		if b.String() != string(want) {
			t.Fatalf("%s differs:\n%s", name, b.String())
		}
		if strings.Contains(b.String(), "release") || strings.Contains(b.String(), "\x1b[") {
			t.Fatalf("%s: shows the hidden release command or colour codes", name)
		}
	}
}
