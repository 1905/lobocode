package agent

import (
	"context"
	"time"

	"github.com/rs/zerolog"
)

// Killer deletes the pod this agent runs on.
type Killer interface {
	KillSelf(ctx context.Context) error
}

// PodAPI is satisfied by *runpod.Self.
type PodAPI interface {
	Terminate(ctx context.Context) error
	Gone(ctx context.Context) (bool, error)
}

// RunPodKiller retries terminate until RunPod confirms the pod is gone, or ctx ends.
type RunPodKiller struct {
	API   PodAPI
	Sleep func(time.Duration)
	Log   zerolog.Logger
}

func (k RunPodKiller) KillSelf(ctx context.Context) error {
	sleep := k.Sleep
	if sleep == nil {
		sleep = time.Sleep
	}
	backoff := 2 * time.Second
	for attempt := 1; ; attempt++ {
		if err := ctx.Err(); err != nil {
			return err
		}
		tctx, cancel := context.WithTimeout(ctx, 10*time.Second)
		err := k.API.Terminate(tctx)
		gone := false
		if err == nil {
			gone, err = k.API.Gone(tctx)
		}
		cancel()
		if gone {
			k.Log.Info().Int("attempt", attempt).Msg("pod terminated")
			return nil
		}
		k.Log.Warn().Err(err).Int("attempt", attempt).Dur("retry_in", backoff).Msg("terminate not confirmed")
		sleep(backoff)
		if backoff *= 2; backoff > time.Minute {
			backoff = time.Minute
		}
	}
}
