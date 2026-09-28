package metrics

import (
	"fmt"
	"strconv"
	"strings"
)

type Host struct {
	Load1      float64 `json:"load1"`
	Load5      float64 `json:"load5"`
	Load15     float64 `json:"load15"`
	MemUsedMB  int     `json:"mem_used_mb"`
	MemTotalMB int     `json:"mem_total_mb"`
}

// ParseHost reads /proc/loadavg and /proc/meminfo. Used = MemTotal − MemAvailable.
func ParseHost(loadavg, meminfo string) (Host, error) {
	f := strings.Fields(loadavg)
	if len(f) < 3 {
		return Host{}, fmt.Errorf("loadavg: bad %q", loadavg)
	}
	var h Host
	for i, dst := range []*float64{&h.Load1, &h.Load5, &h.Load15} {
		v, err := strconv.ParseFloat(f[i], 64)
		if err != nil {
			return Host{}, fmt.Errorf("loadavg: bad %q", loadavg)
		}
		*dst = v
	}
	kb := map[string]int{}
	for _, line := range strings.Split(meminfo, "\n") {
		p := strings.Fields(line)
		if len(p) >= 2 {
			if v, err := strconv.Atoi(p[1]); err == nil {
				kb[strings.TrimSuffix(p[0], ":")] = v
			}
		}
	}
	total, ok1 := kb["MemTotal"]
	avail, ok2 := kb["MemAvailable"]
	if !ok1 || !ok2 {
		return Host{}, fmt.Errorf("meminfo: missing MemTotal/MemAvailable")
	}
	h.MemTotalMB, h.MemUsedMB = total/1024, (total-avail)/1024
	return h, nil
}
