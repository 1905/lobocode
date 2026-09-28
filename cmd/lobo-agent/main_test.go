package main

import (
	"context"
	"errors"
	"strings"
	"testing"
)

func TestFreeMiB(t *testing.T) {
	out := "Available devices:\n  CUDA0: NVIDIA GeForce RTX 5090 (32109 MiB, 25901 MiB free)\n"
	if n, ok := freeMiB(out); !ok || n != 25901 {
		t.Fatal(n, ok)
	}
	if _, ok := freeMiB("no devices"); ok {
		t.Fatal("want !ok")
	}
	// Q8 @ 64K measured 29,274 MiB used: need more than that, but a clean 5090 (31,602 free) must pass.
	if need := minFreeMiB(28595762272); need <= 29274 || need >= 31602 {
		t.Fatal(need)
	}
}

func TestDownloadAnyFallsBackOnAnyError(t *testing.T) {
	ctx := context.Background()
	var tried []string
	err := downloadAny(ctx, []string{"r2", "feesh"}, func(_ int, u string) error {
		tried = append(tried, u)
		if u == "r2" {
			return errors.New("download r2: chunk 0-268435456: gave up after 8 resumes")
		}
		return nil
	}, func(error, string) {})
	if err != nil || strings.Join(tried, ",") != "r2,feesh" {
		t.Fatal(err, tried)
	}
	// Both fail: the last error is returned.
	err = downloadAny(ctx, []string{"a", "b"}, func(_ int, u string) error { return errors.New(u) }, func(error, string) {})
	if err == nil || err.Error() != "b" {
		t.Fatal(err)
	}
	// Boot cancelled: no switch.
	cctx, cancel := context.WithCancel(ctx)
	cancel()
	tried = nil
	_ = downloadAny(cctx, []string{"a", "b"}, func(_ int, u string) error { tried = append(tried, u); return cctx.Err() }, func(error, string) {})
	if len(tried) != 1 {
		t.Fatal(tried)
	}
}

func TestSelfAPIPerProvider(t *testing.T) {
	if selfAPI("vast", "", "", "52607650", "k") == nil || selfAPI("runpod", "pod1", "k", "", "") == nil {
		t.Fatal("expected a killer")
	}
	if selfAPI("vast", "pod1", "k", "", "") != nil || selfAPI("runpod", "", "", "1", "k") != nil {
		t.Fatal("must not cross providers")
	}
	for _, kv := range cleanEnv([]string{"CONTAINER_API_KEY=x", "RUNPOD_API_KEY=y", "PATH=/bin"}) {
		if kv != "PATH=/bin" {
			t.Fatal("secret leaked into child env:", kv)
		}
	}
}
