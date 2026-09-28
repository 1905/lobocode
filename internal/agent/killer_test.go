package agent

import (
	"context"
	"errors"
	"testing"
	"time"

	"github.com/rs/zerolog"
)

type fakePod struct {
	termErrs []error
	gone     []bool
	terms    int
}

func (f *fakePod) Terminate(context.Context) error {
	f.terms++
	if len(f.termErrs) > 0 {
		e := f.termErrs[0]
		f.termErrs = f.termErrs[1:]
		return e
	}
	return nil
}

func (f *fakePod) Gone(context.Context) (bool, error) {
	if len(f.gone) > 0 {
		g := f.gone[0]
		f.gone = f.gone[1:]
		return g, nil
	}
	return true, nil
}

func TestKillSelfRetries(t *testing.T) {
	var slept []time.Duration
	f := &fakePod{termErrs: []error{errors.New("500"), errors.New("timeout")}}
	k := RunPodKiller{API: f, Sleep: func(d time.Duration) { slept = append(slept, d) }, Log: zerolog.Nop()}
	if err := k.KillSelf(context.Background()); err != nil {
		t.Fatal(err)
	}
	if f.terms != 3 || len(slept) != 2 || slept[1] != 4*time.Second {
		t.Fatal(f.terms, slept)
	}
}

func TestKillSelfWaitsForGone(t *testing.T) {
	f := &fakePod{gone: []bool{false, false, true}}
	k := RunPodKiller{API: f, Sleep: func(time.Duration) {}, Log: zerolog.Nop()}
	if err := k.KillSelf(context.Background()); err != nil || f.terms != 3 {
		t.Fatal(err, f.terms)
	}
}

func TestKillSelfCtx(t *testing.T) {
	ctx, cancel := context.WithCancel(context.Background())
	f := &fakePod{gone: []bool{false, false, false, false}}
	k := RunPodKiller{API: f, Sleep: func(time.Duration) { cancel() }, Log: zerolog.Nop()}
	if err := k.KillSelf(ctx); !errors.Is(err, context.Canceled) {
		t.Fatal(err)
	}
}
