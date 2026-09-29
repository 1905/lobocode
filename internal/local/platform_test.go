package local

import (
	"runtime"
	"testing"
)

func TestSupported(t *testing.T) {
	err := Supported()
	if want := runtime.GOOS == "darwin" && runtime.GOARCH == "arm64"; (err == nil) != want {
		t.Fatalf("%s/%s: got %v", runtime.GOOS, runtime.GOARCH, err)
	}
}

func TestUsableMiB(t *testing.T) {
	if Supported() != nil {
		t.Skip("darwin/arm64 only")
	}
	n, err := UsableMiB()
	if err != nil || n < 1024 {
		t.Fatalf("%d %v", n, err)
	}
	t.Logf("usable %d MiB", n)
}
