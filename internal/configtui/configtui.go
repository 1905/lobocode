// Package configtui is the `lobo config` form: provider keys, access keys and `lobo up` defaults,
// written back to the config file by config.Save.
package configtui

import (
	"crypto/rand"
	"encoding/hex"
	"errors"
	"fmt"
	"net/url"
	"strconv"
	"strings"

	"charm.land/huh/v2"
	"charm.land/lipgloss/v2"
)

// Mask shows enough of a secret to recognise it: "abcd…wxyz". Short values are fully hidden.
func Mask(s string) string {
	switch {
	case s == "":
		return "(not set)"
	case len(s) < 12:
		return "••••"
	}
	return s[:4] + "…" + s[len(s)-4:]
}

// Secrets are shown masked everywhere and edited with keep/clear semantics.
var Secrets = map[string]bool{"RUNPOD_API_KEY": true, "VASTAI_API_KEY": true, "LOBO_API_KEY": true, "CF_TUNNEL_TOKEN": true}

// resolveSecret turns a password field into the new value: empty keeps the old one, "-" clears it.
func resolveSecret(old, typed string) string {
	switch t := strings.TrimSpace(typed); t {
	case "":
		return old
	case "-":
		return ""
	default:
		return t
	}
}

// NewAPIKey is a fresh LOBO_API_KEY.
func NewAPIKey() string {
	b := make([]byte, 24)
	_, _ = rand.Read(b)
	return "sk-" + hex.EncodeToString(b)
}

// state is everything the form edits, as strings, so validation and the result are pure functions.
// Fields are exported on purpose: huh re-renders DescriptionFunc(…, s) only when the hash of s changes,
// and the hash skips unexported fields (the summary stayed stale otherwise).
type state struct {
	Cur                      map[string]string
	Runpod, Vast, Tunnel     string // typed secrets (empty = keep)
	APIKey                   string // keep | new
	Domain, Bucket           string
	Provider, Model, Cloud   string
	MinMBps, Ctx, Idle, MaxH string
	VastDPH                  string
	Save                     bool
}

func newState(cur map[string]string) *state {
	s := &state{Cur: cur, APIKey: "keep", Save: true,
		Domain: cur["LOBO_DOMAIN"], Bucket: cur["LOBO_BUCKET_URL"],
		Provider: cur["LOBO_PROVIDER"], Model: cur["LOBO_MODEL"], Cloud: cur["LOBO_CLOUD"],
		MinMBps: cur["LOBO_MIN_MBPS"], Ctx: cur["LOBO_CTX"], Idle: cur["LOBO_IDLE_MIN"], MaxH: cur["LOBO_MAX_HOURS"],
		VastDPH: cur["LOBO_VAST_MAX_DPH"]}
	if cur["LOBO_API_KEY"] == "" {
		s.APIKey = "new"
	}
	if s.Provider == "" {
		s.Provider = "runpod"
	}
	if s.Model == "" {
		s.Model = "q8"
	}
	if s.Cloud == "" {
		s.Cloud = "community"
	}
	return s
}

func (s *state) runpodKey() string { return resolveSecret(s.Cur["RUNPOD_API_KEY"], s.Runpod) }
func (s *state) vastKey() string   { return resolveSecret(s.Cur["VASTAI_API_KEY"], s.Vast) }
func (s *state) bothKeys() bool    { return s.runpodKey() != "" && s.vastKey() != "" }

// result is the KEY=value set to save. newKey is used when the user picked "generate".
func (s *state) result(newKey string) map[string]string {
	out := map[string]string{
		"RUNPOD_API_KEY":    s.runpodKey(),
		"VASTAI_API_KEY":    s.vastKey(),
		"CF_TUNNEL_TOKEN":   resolveSecret(s.Cur["CF_TUNNEL_TOKEN"], s.Tunnel),
		"LOBO_API_KEY":      s.Cur["LOBO_API_KEY"],
		"LOBO_DOMAIN":       strings.TrimSpace(s.Domain),
		"LOBO_BUCKET_URL":   strings.TrimSpace(s.Bucket),
		"LOBO_MODEL":        s.Model,
		"LOBO_MIN_MBPS":     strings.TrimSpace(s.MinMBps),
		"LOBO_CTX":          strings.TrimSpace(s.Ctx),
		"LOBO_IDLE_MIN":     strings.TrimSpace(s.Idle),
		"LOBO_MAX_HOURS":    strings.TrimSpace(s.MaxH),
		"LOBO_PROVIDER":     "",
		"LOBO_CLOUD":        "",
		"LOBO_VAST_MAX_DPH": "",
	}
	if s.APIKey == "new" {
		out["LOBO_API_KEY"] = newKey
	}
	if s.bothKeys() {
		out["LOBO_PROVIDER"] = s.Provider
	}
	if out["RUNPOD_API_KEY"] != "" {
		out["LOBO_CLOUD"] = s.Cloud
	}
	if out["VASTAI_API_KEY"] != "" {
		out["LOBO_VAST_MAX_DPH"] = strings.TrimSpace(s.VastDPH)
	}
	// "0" and the built-in values are the same as unset: keep the file short.
	for k, def := range map[string]string{"LOBO_MIN_MBPS": "100", "LOBO_CTX": "0", "LOBO_IDLE_MIN": "0", "LOBO_MAX_HOURS": "0", "LOBO_MODEL": "q8", "LOBO_CLOUD": "community", "LOBO_PROVIDER": "runpod", "LOBO_VAST_MAX_DPH": "1.20"} {
		if out[k] == def || out[k] == "0" {
			out[k] = ""
		}
	}
	return out
}

func wholeNumber(min int) func(string) error {
	return func(v string) error {
		v = strings.TrimSpace(v)
		if v == "" || v == "0" {
			return nil
		}
		n, err := strconv.Atoi(v)
		if err != nil || n < min {
			return fmt.Errorf("a whole number ≥ %d, or empty for the default", min)
		}
		return nil
	}
}

func positiveFloat(v string) error {
	v = strings.TrimSpace(v)
	if v == "" {
		return nil
	}
	if f, err := strconv.ParseFloat(v, 64); err != nil || f <= 0 {
		return errors.New("a price in $/h, e.g. 1.20")
	}
	return nil
}

func httpsURL(v string) error {
	v = strings.TrimSpace(v)
	if v == "" {
		return errors.New("required")
	}
	if u, err := url.Parse(v); err != nil || (u.Scheme != "https" && u.Scheme != "http") || u.Host == "" {
		return errors.New("a URL like https://pub-….r2.dev")
	}
	return nil
}

func hostname(v string) error {
	v = strings.TrimSpace(v)
	if v == "" {
		return errors.New("required")
	}
	if strings.Contains(v, "/") || strings.Contains(v, " ") || !strings.Contains(v, ".") {
		return errors.New("a bare hostname like lobo.example.com (no https://)")
	}
	return nil
}

var (
	accent = lipgloss.Color("#7D56F4")
	dim    = lipgloss.Color("#8B949E")
)

func theme() huh.Theme {
	return huh.ThemeFunc(func(isDark bool) *huh.Styles {
		t := huh.ThemeCharm(isDark)
		t.Focused.Title = t.Focused.Title.Foreground(accent).Bold(true)
		t.Focused.NoteTitle = t.Focused.NoteTitle.Foreground(accent).Bold(true)
		t.Group.Title = t.Group.Title.Foreground(accent).Bold(true)
		t.Focused.Base = t.Focused.Base.BorderForeground(accent)
		return t
	})
}

func secretInput(title, help string, cur string, dst *string) *huh.Input {
	desc := help + "\nNow: " + Mask(cur) + ". Leave empty to keep it, type - to remove it."
	return huh.NewInput().Title(title).Description(desc).EchoMode(huh.EchoModePassword).Value(dst)
}

// Form builds the `lobo config` form over cur (the current file values).
func (s *state) form(path string) *huh.Form {
	head := huh.NewNote().Title("lobo config").Description(
		"File: " + path + "\n" +
			"Plain KEY=value lines. You can edit it by hand at any time;\n" +
			"this form only rewrites the keys it shows and keeps everything else.\n\n" +
			"Enter: next · Shift+Tab: back · Ctrl+C: quit without saving")

	keys := huh.NewGroup(
		head,
		secretInput("RunPod API key", "runpod.io → Settings → API keys.", s.Cur["RUNPOD_API_KEY"], &s.Runpod),
		secretInput("Vast.ai API key", "cloud.vast.ai → Account → API keys. Optional.", s.Cur["VASTAI_API_KEY"], &s.Vast).
			Validate(func(v string) error {
				if s.runpodKey() == "" && resolveSecret(s.Cur["VASTAI_API_KEY"], v) == "" {
					return errors.New("set at least one provider key")
				}
				return nil
			}),
	).Title("1/4 · GPU providers")

	apiDesc := "Clients (OpenCode etc.) send this as the Bearer key. Now: " + Mask(s.Cur["LOBO_API_KEY"]) + "."
	apiOpts := []huh.Option[string]{huh.NewOption("keep the current key", "keep"), huh.NewOption("generate a new key", "new")}
	if s.Cur["LOBO_API_KEY"] == "" {
		apiOpts = []huh.Option[string]{huh.NewOption("generate a key", "new")}
	}
	access := huh.NewGroup(
		huh.NewInput().Title("Domain").Description("The hostname your Cloudflare tunnel serves, e.g. lobo.example.com.").
			Value(&s.Domain).Validate(hostname),
		huh.NewSelect[string]().Title("LOBO API key").Description(apiDesc).Options(apiOpts...).Value(&s.APIKey),
		secretInput("Cloudflare tunnel token", "Zero Trust → Networks → Tunnels → your tunnel → token.", s.Cur["CF_TUNNEL_TOKEN"], &s.Tunnel).
			Validate(func(v string) error {
				if resolveSecret(s.Cur["CF_TUNNEL_TOKEN"], v) == "" {
					return errors.New("required: the pod serves the API through this tunnel")
				}
				return nil
			}),
		huh.NewInput().Title("Bucket URL").Description("Public R2 URL with releases/ and models/.").
			Value(&s.Bucket).Validate(httpsURL),
	).Title("2/4 · Access")

	pick := huh.NewGroup(
		huh.NewSelect[string]().Title("Default provider").
			Description("Both keys are set. `lobo up --provider …` still overrides this.").
			Options(huh.NewOption("RunPod", "runpod"), huh.NewOption("Vast.ai (cheapest verified host)", "vast")).
			Value(&s.Provider),
	).Title("3/4 · Provider").WithHideFunc(func() bool { return !s.bothKeys() })

	defaults := huh.NewGroup(
		huh.NewInput().Title("Minimum download speed, MB/s").
			Description("A pod slower than this 20 s into the model download is dropped and replaced. Empty = 100.").
			Value(&s.MinMBps).Validate(wholeNumber(1)),
		huh.NewSelect[string]().Title("Model").
			Options(huh.NewOption("Q8_0 (28.6 GB, best quality)", "q8"), huh.NewOption("Q6_K (22 GB, faster boot)", "q6")).
			Value(&s.Model),
		huh.NewInput().Title("Context size").Description("Tokens. Empty = release default (65536).").
			Value(&s.Ctx).Validate(wholeNumber(512)),
		huh.NewInput().Title("Idle minutes").Description("The pod deletes itself after this long without requests. Empty = 30.").
			Value(&s.Idle).Validate(wholeNumber(1)),
		huh.NewInput().Title("Max hours").Description("Hard lifetime of a pod. Empty = 12.").
			Value(&s.MaxH).Validate(wholeNumber(1)),
	).Title("3/4 · Defaults for lobo up")

	perProvider := huh.NewGroup(
		huh.NewSelect[string]().Title("RunPod cloud").
			Description("Community = community hosts only. Secure = datacenter first, community fallback.").
			Options(huh.NewOption("Community (cheapest, $0.69/h)", "community"), huh.NewOption("Secure (datacenter first, $0.99/h)", "secure")).
			Value(&s.Cloud),
	).Title("3/4 · RunPod").WithHideFunc(func() bool { return s.runpodKey() == "" })

	vastGroup := huh.NewGroup(
		huh.NewInput().Title("Vast max price, $/h").Description("Offers above this are skipped. Empty = 1.20.").
			Value(&s.VastDPH).Validate(positiveFloat),
	).Title("3/4 · Vast.ai").WithHideFunc(func() bool { return s.vastKey() == "" })

	confirm := huh.NewGroup(
		huh.NewConfirm().Title("Save to "+path+"?").DescriptionFunc(func() string { return s.summary() }, s).
			Affirmative("Save").Negative("Discard").Value(&s.Save),
	).Title("4/4 · Save")

	return huh.NewForm(keys, access, pick, defaults, perProvider, vastGroup, confirm).WithTheme(theme()).WithShowHelp(true)
}

var summaryRows = [][2]string{
	{"RUNPOD_API_KEY", "RunPod key"}, {"VASTAI_API_KEY", "Vast key"}, {"LOBO_DOMAIN", "Domain"}, {"LOBO_API_KEY", "LOBO API key"},
	{"CF_TUNNEL_TOKEN", "Tunnel token"}, {"LOBO_BUCKET_URL", "Bucket URL"}, {"LOBO_PROVIDER", "Default provider"},
	{"LOBO_MIN_MBPS", "Min MB/s"}, {"LOBO_MODEL", "Model"}, {"LOBO_CTX", "Context"}, {"LOBO_IDLE_MIN", "Idle minutes"},
	{"LOBO_MAX_HOURS", "Max hours"}, {"LOBO_CLOUD", "RunPod cloud"}, {"LOBO_VAST_MAX_DPH", "Vast max $/h"},
}

// summary is the review text before saving: secrets masked.
func (s *state) summary() string {
	r := s.result("(new key)")
	var b strings.Builder
	for _, row := range summaryRows {
		k := row[0]
		v := r[k]
		switch {
		case k == "LOBO_API_KEY" && s.APIKey == "new":
			v = "(a new key is generated on save)"
		case Secrets[k]:
			v = Mask(v)
		case v == "":
			v = lipgloss.NewStyle().Foreground(dim).Render("default")
		}
		fmt.Fprintf(&b, "%-20s %s\n", row[1], v)
	}
	return strings.TrimRight(b.String(), "\n")
}

// Run shows the form for the config at path with its current values cur.
// It returns the KEY=value set to pass to config.Save, and false when the user quit or chose Discard.
func Run(path string, cur map[string]string) (map[string]string, bool, error) {
	s := newState(cur)
	if err := s.form(path).Run(); err != nil {
		if errors.Is(err, huh.ErrUserAborted) {
			return nil, false, nil
		}
		return nil, false, err
	}
	if !s.Save {
		return nil, false, nil
	}
	return s.result(NewAPIKey()), true, nil
}
