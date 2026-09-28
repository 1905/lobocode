package config

import (
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strconv"
	"strings"

	"github.com/joho/godotenv"
)

// DefaultPath is where `lobo` keeps its config: $XDG_CONFIG_HOME/lobo/config.env, else ~/.config/lobo/config.env.
// Same dotenv format and key names as the repo .env, so either file works with --config.
func DefaultPath() string {
	if x := os.Getenv("XDG_CONFIG_HOME"); x != "" {
		return filepath.Join(x, "lobo", "config.env")
	}
	home, err := os.UserHomeDir()
	if err != nil {
		return filepath.Join(".config", "lobo", "config.env")
	}
	return filepath.Join(home, ".config", "lobo", "config.env")
}

// Values returns the raw KEY=value map of a config file; a missing file is an empty map.
func Values(path string) (map[string]string, error) {
	m, err := godotenv.Read(path)
	if os.IsNotExist(err) {
		return map[string]string{}, nil
	}
	if err != nil {
		return nil, fmt.Errorf("read %s: %w", path, err)
	}
	return m, nil
}

// LooseMode reports a config file that other users can read (it holds API keys).
func LooseMode(path string) bool {
	fi, err := os.Stat(path)
	return err == nil && fi.Mode().Perm()&0o077 != 0
}

// Defaults are the user's defaults for `lobo up`. Zero values mean "use the built-in / release default".
type Defaults struct {
	Provider   string // runpod | vast
	Model      string // q8 | q6
	Cloud      string // secure | community
	Ctx        int
	IdleMin    int
	MaxHours   int
	MinMBps    int
	VastMaxDPH float64
}

// DefaultsError lists bad default keys (env name → problem). Defaults still returns the good ones.
type DefaultsError struct{ Bad map[string]string }

func (e *DefaultsError) Error() string {
	keys := make([]string, 0, len(e.Bad))
	for k := range e.Bad {
		keys = append(keys, k)
	}
	sort.Strings(keys)
	var msgs []string
	for _, k := range keys {
		msgs = append(msgs, k+": "+e.Bad[k])
	}
	return "config: " + strings.Join(msgs, "; ")
}

// Defaults parses the LOBO_* default keys. Bad keys are left zero and reported in a *DefaultsError.
func (l Laptop) Defaults() (Defaults, error) {
	var d Defaults
	bad := map[string]string{}
	oneOf := func(key, v string, ok ...string) string {
		if v == "" {
			return ""
		}
		for _, o := range ok {
			if v == o {
				return v
			}
		}
		bad[key] = fmt.Sprintf("want %s, got %q", strings.Join(ok, " or "), v)
		return ""
	}
	num := func(key, v string, min int) int {
		if v == "" || v == "0" {
			return 0
		}
		n, err := strconv.Atoi(v)
		if err != nil || n < min {
			bad[key] = fmt.Sprintf("want a whole number ≥ %d (or empty), got %q", min, v)
			return 0
		}
		return n
	}
	d.Provider = oneOf("LOBO_PROVIDER", l.Provider, "runpod", "vast")
	d.Model = oneOf("LOBO_MODEL", l.Model, "q8", "q6")
	d.Cloud = oneOf("LOBO_CLOUD", l.Cloud, "secure", "community")
	d.Ctx = num("LOBO_CTX", l.Ctx, 512)
	d.IdleMin = num("LOBO_IDLE_MIN", l.IdleMin, 1)
	d.MaxHours = num("LOBO_MAX_HOURS", l.MaxHours, 1)
	d.MinMBps = num("LOBO_MIN_MBPS", l.MinMBps, 1)
	if l.VastMaxDPH != "" {
		if f, err := strconv.ParseFloat(l.VastMaxDPH, 64); err != nil || f <= 0 {
			bad["LOBO_VAST_MAX_DPH"] = fmt.Sprintf("want $/h > 0, got %q", l.VastMaxDPH)
		} else {
			d.VastMaxDPH = f
		}
	}
	if len(bad) > 0 {
		return d, &DefaultsError{Bad: bad}
	}
	return d, nil
}

// Providers lists providers that have a key, in a stable order.
func (l Laptop) Providers() []string {
	var out []string
	if l.RunPodAPIKey != "" {
		out = append(out, "runpod")
	}
	if l.VastAPIKey != "" {
		out = append(out, "vast")
	}
	return out
}

// DefaultProvider is LOBO_PROVIDER when that provider has a key, else runpod if keyed, else vast.
func (l Laptop) DefaultProvider() string {
	keyed := l.Providers()
	for _, p := range keyed {
		if p == l.Provider {
			return p
		}
	}
	if len(keyed) > 0 {
		return keyed[0]
	}
	return "runpod"
}
