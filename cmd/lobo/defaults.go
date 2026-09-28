package main

import (
	"errors"
	"fmt"
	"time"

	"github.com/spf13/pflag"

	"github.com/1905/lobocode/internal/config"
	"github.com/1905/lobocode/internal/control"
)

// applyDefaults fills UpOpts from the config defaults for every flag the user did not set:
// flag > config (LOBO_*) > built-in / release default.
func applyDefaults(fs *pflag.FlagSet, o *control.UpOpts, cfg config.Laptop, cfgPath string) error {
	set := fs.Changed
	d, err := cfg.Defaults()
	var de *config.DefaultsError
	if errors.As(err, &de) {
		// A bad key only matters when its flag did not override it.
		flagOf := map[string]string{"LOBO_PROVIDER": "provider", "LOBO_MODEL": "q6", "LOBO_CLOUD": "cloud", "LOBO_CTX": "ctx",
			"LOBO_IDLE_MIN": "idle-min", "LOBO_MAX_HOURS": "max-life", "LOBO_MIN_MBPS": "min-mbps"}
		for k := range de.Bad {
			if f, ok := flagOf[k]; !ok || !set(f) {
				return fmt.Errorf("%w (in %s; fix it with `lobo config` or by hand)", err, cfgPath)
			}
		}
	} else if err != nil {
		return err
	}
	if !set("provider") {
		o.Provider = cfg.DefaultProvider()
	}
	if !set("q6") && d.Model != "" {
		o.Model = d.Model
	}
	if !set("ctx") && d.Ctx > 0 {
		o.Ctx = d.Ctx
	}
	if !set("idle-min") && d.IdleMin > 0 {
		o.IdleMin = d.IdleMin
	}
	if !set("max-life") && d.MaxHours > 0 {
		o.MaxLife = time.Duration(d.MaxHours) * time.Hour
	}
	if !set("min-mbps") && d.MinMBps > 0 {
		o.MinMBps = d.MinMBps
	}
	if !set("cloud") && d.Cloud != "" {
		o.Cloud = d.Cloud
	}
	keyed := false
	for _, p := range cfg.Providers() {
		keyed = keyed || p == o.Provider
	}
	if !keyed {
		key := map[string]string{"runpod": "RUNPOD_API_KEY", "vast": "VASTAI_API_KEY"}[o.Provider]
		if key == "" {
			return fmt.Errorf("--provider: want runpod or vast, got %q", o.Provider)
		}
		return fmt.Errorf("no %s in %s. Run `lobo config` to add it", key, cfgPath)
	}
	return nil
}
