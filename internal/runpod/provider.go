package runpod

import (
	"context"
	"errors"
	"fmt"
	"strings"

	"github.com/1905/lobocode/internal/provider"
)

// NetTiers: minDownloadMbps filters tried in order (0 = any host). RunPod can filter by network speed
// but not sort, so each cloud is walked from the fastest tier down.
var NetTiers = []float64{10000, 5000, 2500, 1000, 0}

// API is the part of the RunPod REST client the provider uses (fakes implement it in tests).
type API interface {
	Create(ctx context.Context, o provider.CreateOpts, cloud string, minDownloadMbps float64) (Pod, error)
	List(ctx context.Context) ([]Pod, error)
	Get(ctx context.Context, id string) (Pod, error)
	Delete(ctx context.Context, id string) error
}

// Provider rents lobo pods on RunPod.
type Provider struct{ C API }

func (Provider) Name() string { return "runpod" }

// Rent tries the preferred cloud at every network tier, then the other cloud.
// Datacenter (SECURE) first by default: community hosts often never started the container (2026-09-24/25).
func (p Provider) Rent(ctx context.Context, o provider.CreateOpts, note func(string)) (provider.Instance, error) {
	clouds := []string{"SECURE", "COMMUNITY"}
	if o.Cloud == "community" {
		clouds = []string{"COMMUNITY", "SECURE"}
	}
	var err error
	for _, cloud := range clouds {
		for _, mbps := range NetTiers {
			var pod Pod
			pod, err = p.C.Create(ctx, o, cloud, mbps)
			if err == nil {
				in := toInstance(pod)
				in.Detail = cloud
				if mbps > 0 {
					in.Detail += fmt.Sprintf(", host ≥%.0f Mbps", mbps)
				}
				return in, nil
			}
			if !errors.Is(err, provider.ErrNoCapacity) {
				return provider.Instance{}, err
			}
		}
		if note != nil {
			note("no 5090 in " + cloud + " at any network speed")
		}
	}
	return provider.Instance{}, err
}

func (p Provider) List(ctx context.Context) ([]provider.Instance, error) {
	ps, err := p.C.List(ctx)
	if err != nil {
		return nil, err
	}
	var out []provider.Instance
	for _, pod := range ps {
		if pod.Name == PodName {
			out = append(out, toInstance(pod))
		}
	}
	return out, nil
}

func (p Provider) Get(ctx context.Context, id string) (provider.Instance, error) {
	pod, err := p.C.Get(ctx, id)
	if err != nil {
		return provider.Instance{}, err
	}
	if strings.EqualFold(pod.DesiredStatus, "TERMINATED") {
		return provider.Instance{}, provider.ErrNotFound
	}
	return toInstance(pod), nil
}

func (p Provider) Delete(ctx context.Context, id string) error { return p.C.Delete(ctx, id) }

func toInstance(pod Pod) provider.Instance {
	in := provider.Instance{Provider: "runpod", ID: pod.ID, Status: pod.DesiredStatus, CostPerHr: pod.CostPerHr, StartedAt: pod.LastStartedAt.Time}
	if in.StartedAt.IsZero() {
		in.StartedAt = pod.CreatedAt.Time
	}
	if pod.Machine != nil {
		in.HostDownloadMbps = pod.Machine.MaxDownloadSpeedMbps
	}
	return in
}
