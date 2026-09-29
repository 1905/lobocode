package local

import (
	"archive/tar"
	"compress/gzip"
	"context"
	"errors"
	"fmt"
	"io"
	"io/fs"
	"os"
	"path/filepath"

	"github.com/1905/lobocode/internal/agent"
)

// Pinned llama.cpp macOS arm64 build (Metal). A bump changes RuntimeVersion, runtimeSize and runtimeSHA.
const RuntimeVersion = "b11118"

// Vars: tests swap them.
var (
	RuntimeURL        = "https://github.com/ggml-org/llama.cpp/releases/download/" + RuntimeVersion + "/llama-" + RuntimeVersion + "-bin-macos-arm64.tar.gz"
	runtimeSize int64 = 11205140
	runtimeSHA        = "ca0ea3156257b21eeb11d0628f2baecd3928013a3d060e2e192042276e5b1f35"
)

const serverName = "llama-server"

// RuntimeDir is <weights>/runtime/llama-<version>.
func RuntimeDir(weights string) string {
	return filepath.Join(weights, "runtime", "llama-"+RuntimeVersion)
}

func runtimeNote(size int64) string {
	return fmt.Sprintf("llama.cpp %s %d MB", RuntimeVersion, size/1e6)
}

// EnsureRuntime returns the llama-server path in RuntimeDir, downloading and unpacking the pinned
// tarball first if it is missing. The tarball layout is not fixed, so llama-server is found by name;
// its dylibs ship in the same dir. note gets one line when a download starts.
func EnsureRuntime(ctx context.Context, weights string, note func(string)) (string, error) {
	dir := RuntimeDir(weights)
	if bin, err := findServer(dir); err == nil {
		return bin, nil
	}
	root := filepath.Dir(dir)
	if err := os.MkdirAll(root, 0o755); err != nil {
		return "", err
	}
	note(runtimeNote(runtimeSize))
	tgz, err := fetchRuntime(ctx, root)
	if err != nil {
		return "", err
	}
	defer func() { _ = os.Remove(tgz) }()

	tmp, err := os.MkdirTemp(root, ".llama-*")
	if err != nil {
		return "", err
	}
	defer func() { _ = os.RemoveAll(tmp) }() // no-op after a successful rename
	if err := untar(tgz, tmp); err != nil {
		return "", fmt.Errorf("unpack %s: %w", RuntimeURL, err)
	}
	if _, err := findServer(tmp); err != nil {
		return "", err
	}
	if err := os.Chmod(tmp, 0o755); err != nil { // MkdirTemp makes it 0700
		return "", err
	}
	if err := os.Rename(tmp, dir); err != nil {
		// A concurrent ensure may have won the rename; use its tree if complete.
		if bin, ferr := findServer(dir); ferr == nil {
			return bin, nil
		}
		return "", err
	}
	return findServer(dir)
}

// fetchRuntime downloads RuntimeURL into a temp file under dir and checks size + sha256.
func fetchRuntime(ctx context.Context, dir string) (string, error) {
	var name string
	err := agent.FetchFile(ctx, RuntimeURL, func() (*os.File, error) {
		f, err := os.CreateTemp(dir, ".llama-*.tar.gz")
		if err == nil {
			name = f.Name()
		}
		return f, err
	}, runtimeSize, runtimeSHA)
	if err != nil {
		if name != "" {
			_ = os.Remove(name)
		}
		return "", err
	}
	return name, nil
}

// untar unpacks a .tar.gz into dst. Paths must stay inside dst; symlinks must be relative and
// point inside. Mode bits are kept. Anything but dirs, files and symlinks is refused.
func untar(src, dst string) error {
	f, err := os.Open(src)
	if err != nil {
		return err
	}
	defer f.Close()
	gz, err := gzip.NewReader(f)
	if err != nil {
		return err
	}
	tr := tar.NewReader(gz)
	for {
		h, err := tr.Next()
		if errors.Is(err, io.EOF) {
			return nil
		}
		if err != nil {
			return err
		}
		if !filepath.IsLocal(h.Name) {
			return fmt.Errorf("unsafe path %q", h.Name)
		}
		p := filepath.Join(dst, h.Name)
		mode := fs.FileMode(h.Mode).Perm()
		switch h.Typeflag {
		case tar.TypeDir:
			if err := os.MkdirAll(p, 0o755); err != nil {
				return err
			}
			if err := os.Chmod(p, mode|0o700); err != nil {
				return err
			}
		case tar.TypeReg:
			if err := os.MkdirAll(filepath.Dir(p), 0o755); err != nil {
				return err
			}
			// O_EXCL: never write through an existing symlink.
			out, err := os.OpenFile(p, os.O_CREATE|os.O_EXCL|os.O_WRONLY, 0o600)
			if err != nil {
				return err
			}
			_, err = io.Copy(out, tr)
			if cerr := out.Close(); err == nil {
				err = cerr
			}
			if err != nil {
				return err
			}
			if err := os.Chmod(p, mode); err != nil { // umask-free
				return err
			}
		case tar.TypeSymlink:
			if filepath.IsAbs(h.Linkname) || !filepath.IsLocal(filepath.Join(filepath.Dir(h.Name), h.Linkname)) {
				return fmt.Errorf("unsafe symlink %q -> %q", h.Name, h.Linkname)
			}
			if err := os.MkdirAll(filepath.Dir(p), 0o755); err != nil {
				return err
			}
			if err := os.Symlink(h.Linkname, p); err != nil {
				return err
			}
		case tar.TypeXGlobalHeader:
		default:
			return fmt.Errorf("unsupported tar entry %q (type %q)", h.Name, h.Typeflag)
		}
	}
}

// findServer walks dir (symlinks not followed) for a regular executable file named llama-server.
func findServer(dir string) (string, error) {
	var found string
	err := filepath.WalkDir(dir, func(p string, d fs.DirEntry, err error) error {
		if err != nil {
			return err
		}
		if d.Name() == serverName && d.Type().IsRegular() {
			if fi, err := d.Info(); err == nil && fi.Mode().Perm()&0o111 != 0 {
				found = p
				return fs.SkipAll
			}
		}
		return nil
	})
	if err != nil {
		return "", err
	}
	if found == "" {
		return "", fmt.Errorf("no %s in %s", serverName, dir)
	}
	return found, nil
}
