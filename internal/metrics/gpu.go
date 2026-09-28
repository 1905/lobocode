package metrics

import (
	"fmt"
	"strconv"
	"strings"
)

type GPU struct {
	Name        string `json:"name"`
	VRAMUsedMB  int    `json:"vram_used_mb"`
	VRAMTotalMB int    `json:"vram_total_mb"`
	UtilPct     int    `json:"util_pct"`
}

// SMIArgs is the nvidia-smi query whose output ParseNvidiaSMI reads.
var SMIArgs = []string{"--query-gpu=name,memory.used,memory.total,utilization.gpu", "--format=csv,noheader,nounits"}

// ParseNvidiaSMI reads the first GPU line of SMIArgs output.
func ParseNvidiaSMI(csv string) (GPU, error) {
	line := strings.TrimSpace(strings.SplitN(strings.TrimSpace(csv), "\n", 2)[0])
	f := strings.Split(line, ",")
	if len(f) != 4 {
		return GPU{}, fmt.Errorf("nvidia-smi: want 4 fields, got %q", line)
	}
	n := make([]int, 3)
	for i := range n {
		v, err := strconv.Atoi(strings.TrimSpace(f[i+1]))
		if err != nil {
			return GPU{}, fmt.Errorf("nvidia-smi: bad number in %q", line)
		}
		n[i] = v
	}
	return GPU{Name: strings.TrimSpace(f[0]), VRAMUsedMB: n[0], VRAMTotalMB: n[1], UtilPct: n[2]}, nil
}
