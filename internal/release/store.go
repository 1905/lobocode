package release

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"net/url"
	"strings"
	"time"

	"github.com/minio/minio-go/v7"
	"github.com/minio/minio-go/v7/pkg/credentials"

	"github.com/1905/lobocode/internal/config"
)

const Bucket = "lobo"

// Store uploads to R2. Laptop only: it holds the R2 keys.
type Store struct{ c *minio.Client }

func NewStore(r2 config.R2Creds) (*Store, error) {
	u, err := url.Parse(r2.Endpoint)
	if err != nil {
		return nil, err
	}
	c, err := minio.New(u.Host, &minio.Options{Creds: credentials.NewStaticV4(r2.AccessKey, r2.SecretKey, ""), Secure: true, Region: "auto"})
	if err != nil {
		return nil, err
	}
	return &Store{c: c}, nil
}

func (s *Store) ListReleaseKeys(ctx context.Context) ([]string, error) {
	var keys []string
	for o := range s.c.ListObjects(ctx, Bucket, minio.ListObjectsOptions{Prefix: "releases/"}) {
		if o.Err != nil {
			return nil, o.Err
		}
		keys = append(keys, o.Key)
	}
	return keys, nil
}

// Publish uploads zip → per-version json → latest.json, in that order, so latest never points at a missing file.
func (s *Store) Publish(ctx context.Context, zipPath string, r Resolved) error {
	// Versioned objects are immutable: never overwrite an existing release (e.g. two concurrent `make release`).
	for _, key := range []string{r.ZipKey, MetaKey(r.Manifest.Version)} {
		if _, err := s.c.StatObject(ctx, Bucket, key, minio.StatObjectOptions{}); err == nil {
			return fmt.Errorf("release %s already exists (%s); run make release again", r.Manifest.Version, key)
		} else if minio.ToErrorResponse(err).Code != "NoSuchKey" {
			return fmt.Errorf("check %s: %w", key, err)
		}
	}
	if _, err := s.c.FPutObject(ctx, Bucket, r.ZipKey, zipPath, minio.PutObjectOptions{ContentType: "application/zip"}); err != nil {
		return fmt.Errorf("upload zip: %w", err)
	}
	b, _ := json.MarshalIndent(r, "", "  ")
	for _, key := range []string{MetaKey(r.Manifest.Version), LatestKey} {
		opts := minio.PutObjectOptions{ContentType: "application/json", CacheControl: "no-cache"}
		if _, err := s.c.PutObject(ctx, Bucket, key, bytes.NewReader(b), int64(len(b)), opts); err != nil {
			return fmt.Errorf("upload %s: %w", key, err)
		}
	}
	return nil
}

// Resolve reads release metadata from the public bucket URL. version "" = latest.
func Resolve(ctx context.Context, hc *http.Client, bucketURL, version string) (Resolved, error) {
	key := LatestKey
	if version != "" {
		key = MetaKey(version)
	}
	u := strings.TrimRight(bucketURL, "/") + "/" + key + "?t=" + fmt.Sprint(time.Now().UnixNano())
	req, err := http.NewRequestWithContext(ctx, http.MethodGet, u, nil)
	if err != nil {
		return Resolved{}, err
	}
	resp, err := hc.Do(req)
	if err != nil {
		return Resolved{}, err
	}
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusOK {
		name := version
		if name == "" {
			name = "latest"
		}
		return Resolved{}, fmt.Errorf("release %s: HTTP %d from %s", name, resp.StatusCode, key)
	}
	var r Resolved
	if err := json.NewDecoder(resp.Body).Decode(&r); err != nil {
		return Resolved{}, fmt.Errorf("release %s: %w", key, err)
	}
	return r, nil
}

// ZipURL is where the pod downloads the zip.
// ZipURL is "" for a baked image (no zip): the pod then runs the agent inside the image.
func (r Resolved) ZipURL(bucketURL string) string {
	if r.ZipKey == "" {
		return ""
	}
	return strings.TrimRight(bucketURL, "/") + "/" + r.ZipKey
}

// PresignGet returns a signed GET URL for a bucket key (S3 endpoint, not the public dev URL).
func (s *Store) PresignGet(ctx context.Context, key string, ttl time.Duration) (string, error) {
	u, err := s.c.PresignedGetObject(ctx, Bucket, key, ttl, nil)
	if err != nil {
		return "", err
	}
	return u.String(), nil
}
