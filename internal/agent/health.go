package agent

import (
	"context"
	"net/http"
	"time"
)

// WaitHealthy polls base+"/health" every poll until llama-server answers 200 or ctx ends.
func WaitHealthy(ctx context.Context, base string, poll time.Duration) error {
	for {
		req, _ := http.NewRequestWithContext(ctx, http.MethodGet, base+"/health", nil)
		if resp, err := http.DefaultClient.Do(req); err == nil {
			resp.Body.Close()
			if resp.StatusCode == http.StatusOK {
				return nil
			}
		}
		select {
		case <-ctx.Done():
			return ctx.Err()
		case <-time.After(poll):
		}
	}
}
