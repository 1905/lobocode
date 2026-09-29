package model

import (
	"strings"
	"testing"
)

func TestGet(t *testing.T) {
	tests := []struct {
		id, file, alias string
		size            int64
	}{
		{"q8", "Qwen3.5-27B-Uncensored-HauhauCS-Aggressive-Q8_0.gguf", "qwen3.5-27b-uncensored-q8", 28595762272},
		{"q6", "Qwen3.5-27B-Uncensored-HauhauCS-Aggressive-Q6_K.gguf", "qwen3.5-27b-uncensored-q6", 22082528352},
	}
	for _, tt := range tests {
		t.Run(tt.id, func(t *testing.T) {
			m, err := Get(tt.id)
			if err != nil {
				t.Fatal(err)
			}
			if m.File != tt.file || m.Alias != tt.alias || m.Size != tt.size || len(m.SHA256) != 64 {
				t.Fatalf("got %+v", m)
			}
			if got := m.URL("https://pub-x.r2.dev/"); got != "https://pub-x.r2.dev/models/"+tt.file {
				t.Fatalf("url %s", got)
			}
		})
	}
	_, err := Get("x")
	if err == nil || !strings.Contains(err.Error(), "q6, q8") {
		t.Fatalf("err %v", err)
	}
}

func TestChunkSHA(t *testing.T) {
	for _, id := range []string{"q8", "q6"} {
		m, _ := Get(id)
		want := int((m.Size + ChunkSize - 1) / ChunkSize)
		if got := len(m.ChunkSHA()); got != want {
			t.Fatalf("%s: %d chunk hashes, want %d", id, got, want)
		}
	}
}

func TestAll(t *testing.T) {
	all := All()
	if len(all) != 2 || all[0].ID != "q6" || all[1].ID != "q8" {
		t.Fatalf("%+v", all)
	}
}

func TestMinFreeMiB(t *testing.T) {
	// Q8 @ 64K measured 29,274 MiB used: need more than that, but a clean 5090 (31,602 free) must pass.
	if need := MinFreeMiB(28595762272); need <= 29274 || need >= 31602 {
		t.Fatal(need)
	}
}
