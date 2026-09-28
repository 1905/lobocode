package vast

import (
	"context"
	"encoding/json"
	"errors"
	"io"
	"net/http"
	"net/http/httptest"
	"strings"
	"sync"
	"testing"
	"time"

	"github.com/1905/lobocode/internal/provider"
)

// fakeVast mimics the shapes seen live on 2026-09-25.
type fakeVast struct {
	mu        sync.Mutex
	offers    []Offer
	taken     map[int64]bool // PUT on these fails like a gone offer
	lostReply map[int64]bool // PUT creates the instance, then answers 502 (a cut response)
	fail5xx   map[int64]bool // PUT answers 502 and creates nothing
	puts      []int64
	putBody   map[string]any
	instances map[int64]Inst
	deleted   []int64
	nextID    int64
}

func (f *fakeVast) handler(t *testing.T) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		f.mu.Lock()
		defer f.mu.Unlock()
		if r.Header.Get("Authorization") != "Bearer key" {
			w.WriteHeader(401)
			return
		}
		switch {
		case r.Method == "POST" && r.URL.Path == "/bundles":
			var q map[string]any
			_ = json.NewDecoder(r.Body).Decode(&q)
			if q["order"] == nil || q["verified"] == nil {
				t.Errorf("query missing order/verified: %v", q)
			}
			_ = json.NewEncoder(w).Encode(map[string]any{"offers": f.offers})
		case r.Method == "PUT" && strings.HasPrefix(r.URL.Path, "/asks/"):
			var id int64
			_, _ = fmtSscan(strings.TrimSuffix(strings.TrimPrefix(r.URL.Path, "/asks/"), "/"), &id)
			f.puts = append(f.puts, id)
			b, _ := io.ReadAll(r.Body)
			_ = json.Unmarshal(b, &f.putBody)
			if f.taken[id] {
				w.WriteHeader(400)
				_, _ = w.Write([]byte(`{"success": false, "error": "invalid_args", "msg": "offer no longer available"}`))
				return
			}
			if f.fail5xx[id] {
				w.WriteHeader(502)
				return
			}
			f.nextID++
			f.instances[f.nextID] = Inst{ID: f.nextID, Label: "lobo", Status: "loading", DPH: 0.73, InetDown: 20313}
			if f.lostReply[id] {
				w.WriteHeader(502)
				return
			}
			_ = json.NewEncoder(w).Encode(map[string]any{"success": true, "new_contract": f.nextID, "instance_api_key": "x"})
		case r.Method == "GET" && r.URL.Path == "/instances/":
			var l []Inst
			for _, in := range f.instances {
				l = append(l, in)
			}
			l = append(l, Inst{ID: 999, Label: "someone-else"})
			_ = json.NewEncoder(w).Encode(map[string]any{"instances": l})
		case r.Method == "GET" && strings.HasPrefix(r.URL.Path, "/instances/"):
			var id int64
			_, _ = fmtSscan(strings.Trim(strings.TrimPrefix(r.URL.Path, "/instances/"), "/"), &id)
			if in, ok := f.instances[id]; ok {
				_ = json.NewEncoder(w).Encode(map[string]any{"instances": in})
				return
			}
			_, _ = w.Write([]byte(`{"instances": null}`)) // live: 200 + null right after a destroy
		case r.Method == "DELETE":
			var id int64
			_, _ = fmtSscan(strings.Trim(strings.TrimPrefix(r.URL.Path, "/instances/"), "/"), &id)
			if _, ok := f.instances[id]; !ok {
				w.WriteHeader(404)
				_, _ = w.Write([]byte(`{"success": false, "error": "no_such_instance"}`))
				return
			}
			delete(f.instances, id)
			f.deleted = append(f.deleted, id)
			_, _ = w.Write([]byte(`{"success": true}`))
		default:
			w.WriteHeader(404)
		}
	})
}

func fmtSscan(s string, id *int64) (int, error) {
	var n int64
	for _, c := range s {
		if c < '0' || c > '9' {
			return 0, errors.New("nan")
		}
		n = n*10 + int64(c-'0')
	}
	*id = n
	return 1, nil
}

func setup(t *testing.T, f *fakeVast) *Provider {
	f.instances = map[int64]Inst{}
	f.nextID = 52607649
	srv := httptest.NewServer(f.handler(t))
	t.Cleanup(srv.Close)
	c := New("key")
	c.BaseURL = srv.URL
	return &Provider{C: c, MaxDPH: 1.2}
}

func opts() provider.CreateOpts {
	return provider.CreateOpts{Image: "ghcr.io/ggml-org/llama.cpp:server-cuda-b11118", ReleaseURL: "r", ReleaseSHA256: "s", ModelURL: "m",
		LoboAPIKey: "sk", CFTunnelToken: "tok", Model: "q8", Ctx: 65536, IdleMin: 30, ExpiresAt: time.Now().Add(time.Hour)}
}

func TestRentFastestOfferSkipsTakenAndTried(t *testing.T) {
	f := &fakeVast{offers: []Offer{{ID: 1, InetDown: 20313, DPH: 0.73, Geo: "California, US"}, {ID: 2, InetDown: 17805, DPH: 0.73}, {ID: 3, InetDown: 9129, DPH: 0.81}}, taken: map[int64]bool{1: true}}
	p := setup(t, f)
	var notes []string
	in, err := p.Rent(context.Background(), opts(), func(s string) { notes = append(notes, s) })
	if err != nil || in.Provider != "vast" || in.HostDownloadMbps != 17805 || len(notes) != 1 {
		t.Fatalf("%+v %v %v", in, err, notes)
	}
	// second Rent in the same process (bad-host re-rent) must not retry offers 1 or 2
	in2, err := p.Rent(context.Background(), opts(), nil)
	if err != nil || !strings.Contains(in2.Detail, "offer 3") {
		t.Fatalf("%+v %v", in2, err)
	}
	if _, err := p.Rent(context.Background(), opts(), nil); !errors.Is(err, provider.ErrNoCapacity) {
		t.Fatal(err)
	}
	if strings.Join(func() []string {
		var s []string
		for _, id := range f.puts {
			s = append(s, string(rune('0'+id)))
		}
		return s
	}(), ",") != "1,2,3" {
		t.Fatal(f.puts)
	}
}

func TestCreateBody(t *testing.T) {
	f := &fakeVast{offers: []Offer{{ID: 7, InetDown: 1000}}}
	p := setup(t, f)
	if _, err := p.Rent(context.Background(), opts(), nil); err != nil {
		t.Fatal(err)
	}
	b := f.putBody
	env, _ := b["env"].(map[string]any)
	if b["runtype"] != "ssh" || b["label"] != "lobo" || b["disk"] != 80.0 || env["LOBO_PROVIDER"] != "vast" || env["LOBO_CTX"] != "65536" {
		t.Fatalf("%v", b)
	}
	on, _ := b["onstart"].(string)
	if !strings.HasPrefix(on, "#!/bin/bash\n") || !strings.Contains(on, "CONTAINER_API_KEY") || strings.Contains(on, "RUNPOD") {
		t.Fatal(on)
	}
	raw, _ := json.Marshal(b)
	if strings.Contains(string(raw), "R2_") || strings.Contains(string(raw), "VASTAI_API_KEY") {
		t.Fatal("create body must not carry R2 or account keys")
	}
}

func TestListGetDelete(t *testing.T) {
	f := &fakeVast{offers: []Offer{{ID: 7, InetDown: 1000}}}
	p := setup(t, f)
	in, _ := p.Rent(context.Background(), opts(), nil)
	l, err := p.List(context.Background())
	if err != nil || len(l) != 1 || l[0].ID != in.ID {
		t.Fatalf("%+v %v (someone-else's instance must be filtered out)", l, err)
	}
	if _, err := p.Get(context.Background(), in.ID); err != nil {
		t.Fatal(err)
	}
	if err := p.Delete(context.Background(), in.ID); err != nil {
		t.Fatal(err)
	}
	if _, err := p.Get(context.Background(), in.ID); !errors.Is(err, provider.ErrNotFound) {
		t.Fatalf("200 + instances:null must be ErrNotFound, got %v", err)
	}
	if err := p.Delete(context.Background(), in.ID); err != nil {
		t.Fatal("deleting a gone instance must be nil:", err)
	}
}

func TestSelfTerminateAndGone(t *testing.T) {
	f := &fakeVast{offers: []Offer{{ID: 7, InetDown: 1000}}}
	p := setup(t, f)
	in, _ := p.Rent(context.Background(), opts(), nil)
	s, err := NewSelf(in.ID, "key")
	if err != nil {
		t.Fatal(err)
	}
	s.C.BaseURL = p.C.BaseURL
	if gone, _ := s.Gone(context.Background()); gone {
		t.Fatal("not gone yet")
	}
	if err := s.Terminate(context.Background()); err != nil {
		t.Fatal(err)
	}
	if gone, err := s.Gone(context.Background()); !gone || err != nil {
		t.Fatal(gone, err)
	}
}

func TestBadKey(t *testing.T) {
	f := &fakeVast{}
	p := setup(t, f)
	p.C.key = "wrong"
	if _, err := p.C.SearchOffers(context.Background(), 1.2); err == nil || !strings.Contains(err.Error(), "VASTAI_API_KEY") {
		t.Fatal(err)
	}
}

func TestRentUncertainCreateNeverRentsTwice(t *testing.T) {
	adoptWait = time.Millisecond
	offers := []Offer{{ID: 1, InetDown: 20313, DPH: 0.73}, {ID: 2, InetDown: 17805, DPH: 0.73}}

	// created but the reply was lost: adopt that instance, no second PUT
	f := &fakeVast{offers: offers, lostReply: map[int64]bool{1: true}}
	p := setup(t, f)
	in, err := p.Rent(context.Background(), opts(), nil)
	if err != nil || in.ID != "52607650" || len(f.puts) != 1 || !strings.Contains(in.Detail, "offer 1") {
		t.Fatalf("%+v %v puts=%v", in, err, f.puts)
	}

	// 5xx and nothing created: stop with an error, still no second PUT
	f = &fakeVast{offers: offers, fail5xx: map[int64]bool{1: true}}
	p = setup(t, f)
	if _, err := p.Rent(context.Background(), opts(), nil); err == nil || errors.Is(err, provider.ErrNoCapacity) || len(f.puts) != 1 {
		t.Fatalf("%v puts=%v", err, f.puts)
	}
}
