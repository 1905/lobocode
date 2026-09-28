package vast

import (
	"context"
	"strconv"

	"github.com/1905/lobocode/internal/provider"
)

// Self is the instance's own view: CONTAINER_ID + CONTAINER_API_KEY (instance-scoped; verified live
// 2026-09-25 that it can GET and DELETE its own instance). Satisfies agent.PodAPI.
type Self struct {
	ID int64
	C  *Client
}

func NewSelf(id, key string) (*Self, error) {
	n, err := strconv.ParseInt(id, 10, 64)
	if err != nil {
		return nil, err
	}
	return &Self{ID: n, C: New(key)}, nil
}

func (s *Self) Terminate(ctx context.Context) error { return s.C.Destroy(ctx, s.ID) }

func (s *Self) Gone(ctx context.Context) (bool, error) {
	_, err := s.C.Get(ctx, s.ID)
	if err == provider.ErrNotFound {
		return true, nil
	}
	return false, err
}
