// Package config loads laptop config from the lobo config file (DefaultPath, or any .env via --config)
// and pod config from the process env.
package config

import (
	"encoding/base64"
	"fmt"
	"os"
	"reflect"
	"strings"

	"github.com/go-playground/validator/v10"
	"github.com/joho/godotenv"
)

// Laptop is the control-side config. It only ever comes from the config file.
type Laptop struct {
	RunPodAPIKey  string `env:"RUNPOD_API_KEY"` // at least one of RUNPOD_API_KEY / VASTAI_API_KEY
	LoboAPIKey    string `env:"LOBO_API_KEY" validate:"required"`
	CFTunnelToken string `env:"CF_TUNNEL_TOKEN" validate:"required"`
	Domain        string `env:"LOBO_DOMAIN" validate:"required"`
	BucketURL     string `env:"LOBO_BUCKET_URL" validate:"required,url"`
	// Optional model source over SSH (the model server). Empty = public bucket URL.
	ModelSource     string `env:"LOBO_MODEL_SOURCE"`       // ssh://lobo@203.0.113.10:22
	ModelSSHKeyFile string `env:"LOBO_MODEL_SSH_KEY_FILE"` // private key of the restricted lobo user
	ModelSSHHostKey string `env:"LOBO_MODEL_SSH_HOSTKEY"`  // pinned "ssh-ed25519 AAAA…"
	MinMBps         string `env:"LOBO_MIN_MBPS"`           // optional: minimum model download MB/s (default 100)
	FeeshHTTPURL    string `env:"LOBO_FEESH_HTTP_URL"`     // http://<model-server>:8088/<token> (nginx, secret path); fallback source
	VastAPIKey      string `env:"VASTAI_API_KEY"`          // optional: enables --provider vast
	VastMaxDPH      string `env:"LOBO_VAST_MAX_DPH"`       // optional: max $/h for a Vast 5090 offer (default 1.20)
	PodImage        string `env:"LOBO_POD_IMAGE"`          // optional: image with lobo-agent baked in (ghcr.io/1905/lobocode@sha256:…); skips the release zip
	// Defaults for `lobo up` (flags override). Parsed by Defaults().
	Provider string `env:"LOBO_PROVIDER"`  // runpod | vast, used when both keys are set
	Model    string `env:"LOBO_MODEL"`     // q8 | q6
	Cloud    string `env:"LOBO_CLOUD"`     // runpod: secure | community first
	Ctx      string `env:"LOBO_CTX"`       // 0 = release default
	IdleMin  string `env:"LOBO_IDLE_MIN"`  // 0 = release default
	MaxHours string `env:"LOBO_MAX_HOURS"` // 0 = release default
	R2       R2Creds
}

// R2Creds are the S3 upload keys. Laptop-only: never put them in a pod or a release.
type R2Creds struct {
	AccountID string `env:"R2_ACCOUNT_ID" validate:"required"`
	AccessKey string `env:"R2_ACCESS_KEY" validate:"required"`
	SecretKey string `env:"R2_SECRET_KEY" validate:"required"`
	Endpoint  string `env:"R2_ENDPOINT" validate:"required,url"`
}

// LoadLaptop reads envPath only. Launch defaults are checked by `up` (Defaults), never here:
// a bad LOBO_CTX must not stop `lobo down`. The OS environment is ignored on purpose:
// a shell-exported key must never leak into lobo.
func LoadLaptop(envPath string) (Laptop, error) {
	m, err := godotenv.Read(envPath)
	if err != nil {
		return Laptop{}, fmt.Errorf("read %s: %w", envPath, err)
	}
	var l Laptop
	fill(&l, m)
	fill(&l.R2, m)
	if err := validate(l, "R2"); err != nil {
		return Laptop{}, err
	}
	if l.RunPodAPIKey == "" && l.VastAPIKey == "" {
		return Laptop{}, fmt.Errorf("config: set RUNPOD_API_KEY or VASTAI_API_KEY")
	}
	if strings.HasPrefix(l.ModelSource, "ssh://") && (l.ModelSSHKeyFile == "" || l.ModelSSHHostKey == "") {
		return Laptop{}, fmt.Errorf("config: LOBO_MODEL_SOURCE is ssh://: LOBO_MODEL_SSH_KEY_FILE and LOBO_MODEL_SSH_HOSTKEY are required")
	}
	if s := l.ModelSource; s != "" && s != "r2" && !strings.HasPrefix(s, "ssh://") {
		return Laptop{}, fmt.Errorf("config: LOBO_MODEL_SOURCE: want r2 or ssh://user@host:port, got %q", s)
	}
	return l, nil
}

// RequireR2 checks the upload keys. Only `lobo release` needs them.
func (l Laptop) RequireR2() error { return validate(l.R2) }

// SecretValues maps env names to secret values, for the release secret scan.
func (l Laptop) SecretValues() map[string]string {
	m := map[string]string{
		"RUNPOD_API_KEY":  l.RunPodAPIKey,
		"LOBO_API_KEY":    l.LoboAPIKey,
		"CF_TUNNEL_TOKEN": l.CFTunnelToken,
		"R2_ACCESS_KEY":   l.R2.AccessKey,
		"R2_SECRET_KEY":   l.R2.SecretKey,
		"VASTAI_API_KEY":  l.VastAPIKey,
	}
	if l.ModelSSHKeyFile != "" {
		if b, err := os.ReadFile(l.ModelSSHKeyFile); err == nil {
			m["LOBO_MODEL_SSH_KEY_FILE"] = string(b)
			m["LOBO_MODEL_SSH_KEY_FILE (base64)"] = base64.StdEncoding.EncodeToString(b)
		}
	}
	return m
}

func fill(dst any, m map[string]string) {
	v := reflect.ValueOf(dst).Elem()
	t := v.Type()
	for i := 0; i < t.NumField(); i++ {
		if name := t.Field(i).Tag.Get("env"); name != "" && t.Field(i).Type.Kind() == reflect.String {
			v.Field(i).SetString(m[name])
		}
	}
}

var validate1 = validator.New()

// validate runs struct validation and turns errors into "set ENV_NAME" messages.
func validate(s any, skip ...string) error {
	err := validate1.StructExcept(s, skip...)
	if err == nil {
		return nil
	}
	verrs, ok := err.(validator.ValidationErrors)
	if !ok {
		return err
	}
	t := reflect.TypeOf(s)
	var msgs []string
	for _, fe := range verrs {
		name := fe.Field()
		if f, ok := t.FieldByName(fe.StructField()); ok && f.Tag.Get("env") != "" {
			name = f.Tag.Get("env")
		}
		msgs = append(msgs, fmt.Sprintf("%s: %s", name, fe.Tag()))
	}
	return fmt.Errorf("config: %s", strings.Join(msgs, "; "))
}
