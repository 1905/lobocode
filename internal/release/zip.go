package release

import (
	"archive/zip"
	"bytes"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"io"
	"os"
	"sort"
)

// BuildZip writes lobo-agent (0755) + release.json into out and returns its sha256.
func BuildZip(agentBin string, m Manifest, out string) (string, error) {
	bin, err := os.ReadFile(agentBin)
	if err != nil {
		return "", err
	}
	mj, err := json.MarshalIndent(m, "", "  ")
	if err != nil {
		return "", err
	}
	var buf bytes.Buffer
	zw := zip.NewWriter(&buf)
	for _, f := range []struct {
		name string
		mode os.FileMode
		data []byte
	}{{"lobo-agent", 0o755, bin}, {"release.json", 0o644, mj}} {
		h := &zip.FileHeader{Name: f.name, Method: zip.Deflate}
		h.SetMode(f.mode)
		w, err := zw.CreateHeader(h)
		if err != nil {
			return "", err
		}
		if _, err := w.Write(f.data); err != nil {
			return "", err
		}
	}
	if err := zw.Close(); err != nil {
		return "", err
	}
	if err := os.WriteFile(out, buf.Bytes(), 0o644); err != nil {
		return "", err
	}
	sum := sha256.Sum256(buf.Bytes())
	return hex.EncodeToString(sum[:]), nil
}

// ScanForSecrets fails if any secret value appears in any file of the zip.
// The error names the env var, never the value. Values under 8 chars are skipped.
func ScanForSecrets(zipPath string, secrets map[string]string) error {
	zr, err := zip.OpenReader(zipPath)
	if err != nil {
		return err
	}
	defer zr.Close()
	names := make([]string, 0, len(secrets))
	for k := range secrets {
		names = append(names, k)
	}
	sort.Strings(names)
	for _, f := range zr.File {
		rc, err := f.Open()
		if err != nil {
			return err
		}
		data, err := io.ReadAll(rc)
		rc.Close()
		if err != nil {
			return err
		}
		for _, name := range names {
			if v := secrets[name]; len(v) >= 8 && bytes.Contains(data, []byte(v)) {
				return fmt.Errorf("release: %s found in %s, refusing to publish", name, f.Name)
			}
		}
	}
	return nil
}
