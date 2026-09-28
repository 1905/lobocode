package vast

import (
	"context"
	"errors"
	"fmt"
	"strconv"
	"sync"
	"time"

	"github.com/1905/lobocode/internal/bootstrap"
	"github.com/1905/lobocode/internal/provider"
)

// DiskGB is the container disk: 28.6 GB Q8 model + release + headroom.
const DiskGB = 80

// Provider rents the fastest-network verified 5090 offer. Offers already tried in this process are
// skipped, so a bad host is never rented twice by one `up`.
type Provider struct {
	C      *Client
	MaxDPH float64
	mu     sync.Mutex
	tried  map[int64]bool
}

func (*Provider) Name() string { return "vast" }

// CreateBody is the PUT /asks body. runtype must be "ssh": "args" never runs the onstart.
func CreateBody(o provider.CreateOpts) map[string]any {
	return map[string]any{
		"client_id": "me",
		"image":     o.Image,
		"disk":      DiskGB,
		"label":     provider.Name,
		"runtype":   "ssh",
		"onstart":   "#!/bin/bash\n" + bootstrap.Script("vast"),
		"env":       bootstrap.Env(o, "vast"),
	}
}

func (p *Provider) Rent(ctx context.Context, o provider.CreateOpts, note func(string)) (provider.Instance, error) {
	maxDPH := p.MaxDPH
	if maxDPH <= 0 {
		maxDPH = 1.20
	}
	offers, err := p.C.SearchOffers(ctx, maxDPH)
	if err != nil {
		return provider.Instance{}, err
	}
	p.mu.Lock()
	if p.tried == nil {
		p.tried = map[int64]bool{}
	}
	p.mu.Unlock()
	before, err := p.lobo(ctx)
	if err != nil {
		return provider.Instance{}, err
	}
	body := CreateBody(o)
	for _, off := range offers {
		p.mu.Lock()
		seen := p.tried[off.ID]
		p.tried[off.ID] = true
		p.mu.Unlock()
		if seen {
			continue
		}
		id, err := p.C.Create(ctx, off.ID, body)
		if err != nil && !errors.Is(err, ErrRejected) {
			// The create may have gone through (timeout, 5xx, cut body). Renting another offer could
			// leave two billed instances on one tunnel token, so adopt the new one or stop here.
			if in, ok := p.adopt(ctx, before); ok {
				in.Detail = fmt.Sprintf("offer %d, %.0f Mbps down, %s", off.ID, off.InetDown, off.Geo)
				return in, nil
			}
			return provider.Instance{}, fmt.Errorf("vast create offer %d: %w (not retrying another offer: it may have been rented — check `lobo status`)", off.ID, err)
		}
		if err != nil {
			if note != nil {
				note(fmt.Sprintf("offer %d unavailable: %v", off.ID, err))
			}
			continue
		}
		return provider.Instance{
			Provider: "vast", ID: strconv.FormatInt(id, 10), Status: "created", CostPerHr: off.DPH,
			StartedAt: time.Now(), HostDownloadMbps: int(off.InetDown),
			Detail: fmt.Sprintf("offer %d, %.0f Mbps down, %s", off.ID, off.InetDown, off.Geo),
		}, nil
	}
	return provider.Instance{}, fmt.Errorf("%w: no untried 1× RTX 5090 offer (verified, reliability ≥0.98, ≤$%.2f/h)", provider.ErrNoCapacity, maxDPH)
}

// lobo returns the ids of lobo instances that exist now.
func (p *Provider) lobo(ctx context.Context) (map[string]bool, error) {
	l, err := p.List(ctx)
	if err != nil {
		return nil, err
	}
	m := map[string]bool{}
	for _, in := range l {
		m[in.ID] = true
	}
	return m, nil
}

// adopt looks for a lobo instance that was not there before an uncertain create.
func (p *Provider) adopt(ctx context.Context, before map[string]bool) (provider.Instance, bool) {
	for i := 0; i < 3; i++ {
		if i > 0 {
			select {
			case <-ctx.Done():
				return provider.Instance{}, false
			case <-time.After(adoptWait):
			}
		}
		l, err := p.List(ctx)
		if err != nil {
			continue
		}
		for _, in := range l {
			if !before[in.ID] {
				return in, true
			}
		}
	}
	return provider.Instance{}, false
}

var adoptWait = 3 * time.Second

func (p *Provider) List(ctx context.Context) ([]provider.Instance, error) {
	l, err := p.C.List(ctx)
	if err != nil {
		return nil, err
	}
	var out []provider.Instance
	for _, in := range l {
		if in.Label == provider.Name {
			out = append(out, toInstance(in))
		}
	}
	return out, nil
}

func (p *Provider) Get(ctx context.Context, id string) (provider.Instance, error) {
	n, err := strconv.ParseInt(id, 10, 64)
	if err != nil {
		return provider.Instance{}, err
	}
	in, err := p.C.Get(ctx, n)
	if err != nil {
		return provider.Instance{}, err
	}
	return toInstance(in), nil
}

func (p *Provider) Delete(ctx context.Context, id string) error {
	n, err := strconv.ParseInt(id, 10, 64)
	if err != nil {
		return err
	}
	return p.C.Destroy(ctx, n)
}

func toInstance(in Inst) provider.Instance {
	out := provider.Instance{Provider: "vast", ID: strconv.FormatInt(in.ID, 10), Status: in.Status, CostPerHr: in.DPH,
		HostDownloadMbps: int(in.InetDown), Detail: fmt.Sprintf("%.0f Mbps down, %s", in.InetDown, in.Geo)}
	if in.Start > 0 {
		out.StartedAt = time.Unix(int64(in.Start), 0)
	}
	return out
}
