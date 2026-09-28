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

// Rent tries each cloud at every network tier. Default (community): COMMUNITY only, the cheapest
// ($0.69/h, user rule 2026-09-29). secure: SECURE first, COMMUNITY as fallback. Community hosts sometimes
// never start the container (2026-09-24/25); `up` replaces those as bad hosts.
func (p Provider) Rent(ctx context.Context, o provider.CreateOpts, note func(string)) (provider.Instance, error) {
	clouds := []string{"COMMUNITY"}
	if o.Cloud == "secure" {
		clouds = []string{"SECURE", "COMMUNITY"}
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
