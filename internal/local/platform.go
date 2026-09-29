package local

import (
	"fmt"
	"runtime"
)

// Supported is nil only on Apple Silicon macOS: the runtime is the macos-arm64 llama.cpp build (Metal).
func Supported() error {
	if runtime.GOOS != "darwin" || runtime.GOARCH != "arm64" {
		return fmt.Errorf("local mode needs macOS on Apple Silicon, this is %s/%s", runtime.GOOS, runtime.GOARCH)
	}
	return nil
}

// UsableMiB is the memory Metal can wire for the model: iogpu.wired_limit_mb if set (>0),
// else hw.memsize*3/4. The 3/4 is a guess at the macOS default GPU wired limit, not measured.
func UsableMiB() (int, error) {
	if err := Supported(); err != nil {
		return 0, err
	}
	if lim, err := wiredLimitMiB(); err == nil && lim > 0 {
		return lim, nil
	}
	mem, err := memBytes()
	if err != nil {
		return 0, fmt.Errorf("sysctl hw.memsize: %w", err)
	}
	return int(mem * 3 / 4 >> 20), nil
}
