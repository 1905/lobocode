package agent

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"fmt"
	"hash"
	"io"
	"net/http"
	"os"
	"time"
)

// FetchFile GETs url into the file create opens once the server answered 200. The whole fetch has 5 min: a
// stuck download must not eat the boot. size >= 0 caps the copy and checks the length; sha != "" checks the
// sha256. On error the file is closed and left for the caller.
func FetchFile(ctx context.Context, url string, create func() (*os.File, error), size int64, sha string) error {
	ctx, cancel := context.WithTimeout(ctx, 5*time.Minute)
	defer cancel()
	req, err := http.NewRequestWithContext(ctx, http.MethodGet, url, nil)
	if err != nil {
		return err
	}
	resp, err := http.DefaultClient.Do(req)
	if err != nil {
		return err
	}
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusOK {
		return fmt.Errorf("fetch %s: HTTP %d", url, resp.StatusCode)
	}
	f, err := create()
	if err != nil {
		return err
	}
	var body io.Reader = resp.Body
	if size >= 0 {
		body = io.LimitReader(body, size+1)
	}
	var h hash.Hash
	var w io.Writer = f
	if sha != "" {
		h = sha256.New()
		w = io.MultiWriter(f, h)
	}
	n, err := io.Copy(w, body)
	if err != nil {
		f.Close()
		return err
	}
	if err := f.Close(); err != nil {
		return err
	}
	if size >= 0 && n != size {
		return fmt.Errorf("fetch %s: size %d, want %d", url, n, size)
	}
	if h != nil {
		if got := hex.EncodeToString(h.Sum(nil)); got != sha {
			return fmt.Errorf("fetch %s: sha256 %s, want %s", url, got, sha)
		}
	}
	return nil
}
