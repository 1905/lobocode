package main

import (
	"encoding/json"
	"fmt"
	"io"
	"os"
	"regexp"
	"sort"
	"strings"

	"github.com/spf13/cobra"

	"github.com/1905/lobocode/internal/config"
	"github.com/1905/lobocode/internal/configtui"
)

func configCmd() *cobra.Command {
	c := &cobra.Command{
		Use:   "config",
		Short: "Set API keys and defaults for `lobo up` (form in a terminal)",
		Long: "Opens a form for the provider keys, access keys and the defaults `lobo up` uses.\n" +
			"Everything is stored as plain KEY=value lines in one file (see `lobo config path`).\n" +
			"You can edit that file by hand; the form keeps keys it does not show.",
		RunE: func(cmd *cobra.Command, _ []string) error {
			cur, err := config.Values(cfgPath)
			if err != nil {
				return err
			}
			if !isTTY() || !isTTYIn() {
				fmt.Fprintf(os.Stderr, "not a terminal: showing %s instead of the form\n", cfgPath)
				return showConfig(cur)
			}
			set, saved, err := configtui.Run(cfgPath, cur)
			if err != nil {
				return err
			}
			if !saved {
				fmt.Println("nothing saved")
				return nil
			}
			if err := config.Save(cfgPath, set); err != nil {
				return err
			}
			fmt.Printf("saved %s\n", cfgPath)
			if set["LOBO_API_KEY"] != cur["LOBO_API_KEY"] && cur["LOBO_API_KEY"] != "" {
				fmt.Println("new LOBO_API_KEY: a running pod keeps the old one until the next `lobo up`; `lobo gen-api-key` rewrites opencode.lobo.json")
			}
			if _, err := config.LoadLaptop(cfgPath); err != nil {
				fmt.Printf("still missing before `lobo up` works: %v\n", err)
			}
			return nil
		},
	}
	var showJSON bool
	show := &cobra.Command{
		Use:   "show",
		Short: "Print the config with API keys masked",
		RunE: func(cmd *cobra.Command, _ []string) error {
			cur, err := config.Values(cfgPath)
			if err != nil {
				return err
			}
			if showJSON {
				return showConfigJSON(cmd.OutOrStdout(), cur)
			}
			return showConfig(cur)
		},
	}
	show.Flags().BoolVar(&showJSON, "json", false, "machine-readable: path, exists, masked values, which keys are set")
	var setStdin bool
	set := &cobra.Command{
		Use:   "set KEY=value...",
		Short: "Set keys in the config file (KEY= removes it); keeps every other line",
		Long: "Set keys in the config file. KEY= removes a key. With --stdin, reads one JSON object\n" +
			`{"KEY": "value"} from stdin instead, so secrets never show up in the process list.`,
		RunE: func(cmd *cobra.Command, args []string) error {
			var kv map[string]string
			var err error
			switch {
			case setStdin && len(args) > 0:
				return fmt.Errorf("use KEY=value arguments or --stdin, not both")
			case setStdin:
				kv, err = parseSetJSON(cmd.InOrStdin())
			case len(args) == 0:
				return fmt.Errorf("want KEY=value arguments or --stdin")
			default:
				kv, err = parseSetArgs(args)
			}
			if err != nil {
				return err
			}
			return config.Save(cfgPath, kv)
		},
	}
	set.Flags().BoolVar(&setStdin, "stdin", false, `read {"KEY": "value"} JSON from stdin (keeps secrets out of argv)`)
	c.AddCommand(show, set, &cobra.Command{
		Use:   "get KEY",
		Short: "Print one value in clear (e.g. LOBO_API_KEY for a client config)",
		Args:  cobra.ExactArgs(1),
		RunE: func(_ *cobra.Command, args []string) error {
			cur, err := config.Values(cfgPath)
			if err != nil {
				return err
			}
			v, ok := cur[args[0]]
			if !ok {
				return fmt.Errorf("%s is not set in %s", args[0], cfgPath)
			}
			fmt.Println(v)
			return nil
		},
	}, &cobra.Command{
		Use:   "path",
		Short: "Print the config file path",
		Run:   func(*cobra.Command, []string) { fmt.Println(cfgPath) },
	})
	return c
}

var keyRe = regexp.MustCompile(`^[A-Z][A-Z0-9_]*$`)

// parseSetArgs turns KEY=value arguments into a Save set. "KEY=" removes the key.
func parseSetArgs(args []string) (map[string]string, error) {
	set := map[string]string{}
	for _, a := range args {
		k, v, ok := strings.Cut(a, "=")
		if !ok || !keyRe.MatchString(k) {
			return nil, fmt.Errorf("want KEY=value with KEY like LOBO_MIN_MBPS, got %q", a)
		}
		set[k] = v
	}
	return set, nil
}

// parseSetJSON reads {"KEY": "value"} for `config set --stdin`.
func parseSetJSON(r io.Reader) (map[string]string, error) {
	var kv map[string]string
	if err := json.NewDecoder(io.LimitReader(r, 1<<20)).Decode(&kv); err != nil {
		return nil, fmt.Errorf("config set --stdin: want a JSON object of strings: %w", err)
	}
	if len(kv) == 0 {
		return nil, fmt.Errorf("config set --stdin: empty object")
	}
	for k := range kv {
		if !keyRe.MatchString(k) {
			return nil, fmt.Errorf("config set --stdin: bad key %q (want like LOBO_MIN_MBPS)", k)
		}
	}
	return kv, nil
}

// showConfigJSON is `config show --json` for the macOS app: never prints a secret in clear.
func showConfigJSON(w io.Writer, cur map[string]string) error {
	_, statErr := os.Stat(cfgPath)
	out := struct {
		Path   string            `json:"path"`
		Exists bool              `json:"exists"`
		Values map[string]string `json:"values"`
		Set    map[string]bool   `json:"set"`
	}{Path: cfgPath, Exists: statErr == nil, Values: map[string]string{}, Set: map[string]bool{}}
	for k, v := range cur {
		out.Values[k] = masked(k, v)
		out.Set[k] = v != ""
	}
	return json.NewEncoder(w).Encode(out)
}

// showConfig prints every key, secrets masked, in the file layout order.
func showConfig(cur map[string]string) error {
	fmt.Printf("# %s\n", cfgPath)
	if len(cur) == 0 {
		fmt.Println("# no config yet: run `lobo config` in a terminal")
		return nil
	}
	seen := map[string]bool{}
	for _, g := range config.Layout {
		fmt.Printf("\n# %s\n", g.Title)
		for _, k := range g.Keys {
			seen[k] = true
			if v, ok := cur[k]; ok {
				fmt.Printf("%s=%s\n", k, masked(k, v))
			}
		}
	}
	var rest []string
	for k := range cur {
		if !seen[k] {
			rest = append(rest, k)
		}
	}
	sort.Strings(rest)
	if len(rest) > 0 {
		fmt.Println("\n# other")
	}
	for _, k := range rest {
		fmt.Printf("%s=%s\n", k, masked(k, cur[k]))
	}
	return nil
}

// plainKeys are the only values `config show` prints in clear; everything else, including keys lobo
// does not know, is masked (a hand-added FOO_TOKEN is still a secret).
var plainKeys = map[string]bool{
	"LOBO_DOMAIN": true, "LOBO_BUCKET_URL": true, "LOBO_PROVIDER": true, "LOBO_MODEL": true, "LOBO_CTX": true,
	"LOBO_IDLE_MIN": true, "LOBO_MAX_HOURS": true, "LOBO_MIN_MBPS": true, "LOBO_CLOUD": true, "LOBO_VAST_MAX_DPH": true, "LOBO_POD_IMAGE": true,
	"LOBO_MODEL_SOURCE": true, "LOBO_MODEL_SSH_KEY_FILE": true, "LOBO_MODEL_SSH_HOSTKEY": true, "R2_ACCOUNT_ID": true, "R2_ENDPOINT": true,
}

func masked(k, v string) string {
	if plainKeys[k] {
		return v
	}
	return configtui.Mask(v)
}

func isTTYIn() bool {
	fi, err := os.Stdin.Stat()
	return err == nil && fi.Mode()&os.ModeCharDevice != 0
}
