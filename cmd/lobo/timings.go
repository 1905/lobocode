package main

import (
	"encoding/json"
	"fmt"
	"os"
	"text/tabwriter"
	"time"

	"github.com/1905/lobocode/internal/control"
)

const bootsLog = "boots.jsonl" // gitignored; one line per successful boot

// teeReady passes events through and remembers the ready info for the timing report.
func teeReady(in <-chan control.Event, ready **control.ReadyInfo) <-chan control.Event {
	out := make(chan control.Event)
	go func() {
		defer close(out)
		for e := range in {
			if e.Ready != nil {
				*ready = e.Ready
			}
			out <- e
		}
	}()
	return out
}

// reportBoot prints where the boot time went and appends it to boots.jsonl.
func reportBoot(r *control.ReadyInfo, source string, conns int) {
	if r == nil || r.Timings == nil {
		return
	}
	t := r.Timings
	w := tabwriter.NewWriter(os.Stderr, 0, 0, 2, ' ', 0)
	fmt.Fprintln(os.Stderr, "\nboot timings:")
	row := func(k string, v float64, extra string) { fmt.Fprintf(w, "  %s\t%6.1fs\t%s\t\n", k, v, extra) }
	hostNet := "n/a" // not every provider reports it (RunPod's pod GET often omits it)
	if r.HostDownloadMbps > 0 {
		hostNet = fmt.Sprintf("%d Mbps", r.HostDownloadMbps)
	}
	row("rent → container start", r.RentS, fmt.Sprintf("%s %s (%s), attempt %d, host net %s", r.Provider, r.PodID, r.Detail, r.Attempts, hostNet))
	row("bootstrap apt", t.BootstrapAptS, "")
	row("bootstrap release zip", t.BootstrapZipS, "")
	row("tunnel", t.TunnelS, "")
	row("gpu check", t.GPUCheckS, "")
	row("model download", t.DownloadS, fmt.Sprintf("%.0f MB/s, %d conns, %s", t.DownloadMBps, t.DownloadConns, t.DownloadSource))
	row("sha256 verify", t.VerifyS, "")
	row("load into VRAM", t.LoadS, "")
	row("total (incl. replaced pods)", r.Elapsed.Seconds(), "")
	_ = w.Flush()

	line := map[string]any{"at": time.Now().UTC(), "source": source, "conns": conns, "ready": r}
	b, _ := json.Marshal(line)
	f, err := os.OpenFile(bootsLog, os.O_CREATE|os.O_APPEND|os.O_WRONLY, 0o644)
	if err == nil {
		_, _ = f.Write(append(b, '\n'))
		_ = f.Close()
	}
}
