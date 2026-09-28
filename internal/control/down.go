package control

import (
	"context"
	"errors"
	"fmt"
	"time"
)

// Down deletes every lobo instance on every configured provider and returns their estimated spend.
// One provider failing (list or delete) never stops the others: every error is collected and returned.
func Down(ctx context.Context, d Deps) (float64, error) {
	l, listErr := listAll(ctx, d)
	var errs []error
	if listErr != nil {
		errs = append(errs, fmt.Errorf("list: %w", listErr))
	}
	spent := 0.0
	for _, in := range l {
		if !in.StartedAt.IsZero() {
			spent += in.CostPerHr * d.now().Sub(in.StartedAt).Hours()
		}
		if err := d.Providers[in.Provider].Delete(ctx, in.ID); err != nil {
			errs = append(errs, fmt.Errorf("delete %s %s: %w", in.Provider, in.ID, err))
		}
	}
	// Providers can list a deleted instance for a moment: wait between checks instead of back-to-back Lists.
	wait := d.Poll
	if wait == 0 {
		wait = 2 * time.Second
	}
	var left []error
	for i := 0; i < 5; i++ {
		if i > 0 {
			select {
			case <-ctx.Done():
				return spent, errors.Join(append(errs, ctx.Err())...)
			case <-time.After(wait):
			}
		}
		l, err := listAll(ctx, d)
		left = nil
		if err != nil {
			left = append(left, fmt.Errorf("list after delete: %w", err))
		}
		if len(l) > 0 {
			left = append(left, fmt.Errorf("lobo instances still listed after delete: %d", len(l)))
		}
		if len(left) == 0 {
			break
		}
	}
	return spent, errors.Join(append(errs, left...)...)
}
