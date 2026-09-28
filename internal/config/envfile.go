package config

import (
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strings"
)

// Layout is the key order and grouping of a fresh config file. `lobo config` edits these keys.
var Layout = []struct {
	Title string
	Keys  []string
}{
	{"providers (at least one)", []string{"RUNPOD_API_KEY", "VASTAI_API_KEY"}},
	{"access", []string{"LOBO_DOMAIN", "LOBO_API_KEY", "CF_TUNNEL_TOKEN", "LOBO_BUCKET_URL"}},
	{"defaults for `lobo up` (flags override; empty or 0 = built-in / release default)", []string{
		"LOBO_PROVIDER", "LOBO_MIN_MBPS", "LOBO_MODEL", "LOBO_CTX", "LOBO_IDLE_MIN", "LOBO_MAX_HOURS", "LOBO_CLOUD", "LOBO_VAST_MAX_DPH"}},
}

const header = `# lobo config. Edit by hand or run ` + "`lobo config`" + `.
# Keys here never go into a pod image or a release. Flags on ` + "`lobo up`" + ` override the defaults.
# Advanced keys (edit by hand): LOBO_MODEL_SOURCE, LOBO_MODEL_SSH_KEY_FILE, LOBO_MODEL_SSH_HOSTKEY,
# LOBO_FEESH_HTTP_URL; R2_ACCOUNT_ID, R2_ACCESS_KEY, R2_SECRET_KEY, R2_ENDPOINT (fast presigned model
# download; ` + "`lobo release`" + ` needs them too).
`

// Save sets keys in a dotenv file and keeps every other line (comments, order, unknown keys) as is.
// An empty value removes the key. A new file gets the header and the grouped Layout.
// The write is atomic (temp file + rename), mode 0600, dir 0700.
func Save(path string, set map[string]string) error {
	b, err := os.ReadFile(path)
	if err != nil && !os.IsNotExist(err) {
		return err
	}
	var lines []string
	done := map[string]bool{}
	if len(b) == 0 {
		lines = strings.Split(strings.TrimRight(header, "\n"), "\n")
		for _, g := range Layout {
			lines = append(lines, "", "# "+g.Title)
			for _, k := range g.Keys {
				if v := set[k]; v != "" {
					lines = append(lines, k+"="+quote(v))
				}
				done[k] = true
			}
		}
	} else {
		for _, l := range strings.Split(strings.TrimRight(string(b), "\n"), "\n") {
			k, _, ok := strings.Cut(strings.TrimPrefix(strings.TrimSpace(l), "export "), "=")
			k = strings.TrimSpace(k)
			if v, want := set[k]; ok && want && !strings.HasPrefix(strings.TrimSpace(l), "#") {
				if !done[k] && v != "" {
					lines = append(lines, k+"="+quote(v))
				}
				done[k] = true
				continue
			}
			lines = append(lines, l)
		}
	}
	var rest []string
	for k := range set {
		if !done[k] && set[k] != "" {
			rest = append(rest, k)
		}
	}
	sortByLayout(rest)
	for _, k := range rest {
		lines = append(lines, k+"="+quote(set[k]))
	}
	if err := os.MkdirAll(filepath.Dir(path), 0o700); err != nil {
		return err
	}
	tmp, err := os.CreateTemp(filepath.Dir(path), ".lobo-config-*")
	if err != nil {
		return err
	}
	defer func() { _ = os.Remove(tmp.Name()) }() // no-op after a successful rename
	if err := tmp.Chmod(0o600); err != nil {
		_ = tmp.Close()
		return err
	}
	if _, err := tmp.WriteString(strings.Join(lines, "\n") + "\n"); err != nil {
		_ = tmp.Close()
		return err
	}
	if err := tmp.Close(); err != nil {
		return err
	}
	if err := os.Rename(tmp.Name(), path); err != nil {
		return fmt.Errorf("write %s: %w", path, err)
	}
	return nil
}

// SetEnvValue sets one KEY=value (see Save).
func SetEnvValue(path, key, value string) error { return Save(path, map[string]string{key: value}) }

// quote writes v so godotenv reads it back byte for byte: unquoted `$X` is expanded and `#` starts a
// comment, so anything special goes in double quotes with \\ \" \$ \n escaped.
func quote(v string) string {
	if !strings.ContainsAny(v, " \t#\"'\\$\n\r") {
		return v
	}
	return `"` + strings.NewReplacer(`\`, `\\`, `"`, `\"`, `$`, `\$`, "\n", `\n`, "\r", `\r`).Replace(v) + `"`
}

func sortByLayout(keys []string) {
	pos := map[string]int{}
	i := 0
	for _, g := range Layout {
		for _, k := range g.Keys {
			pos[k] = i
			i++
		}
	}
	rank := func(k string) int {
		if p, ok := pos[k]; ok {
			return p
		}
		return i
	}
	sort.Slice(keys, func(a, b int) bool {
		if ra, rb := rank(keys[a]), rank(keys[b]); ra != rb {
			return ra < rb
		}
		return keys[a] < keys[b]
	})
}
