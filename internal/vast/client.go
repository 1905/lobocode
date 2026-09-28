// Package vast rents lobo instances on Vast.ai (REST, Bearer key).
package vast

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/http"
	"strconv"
	"strings"
	"time"

	"github.com/1905/lobocode/internal/provider"
)

const DefaultBaseURL = "https://console.vast.ai/api/v0"

// Client talks to the Vast REST API. The same code serves the laptop (account key) and the
// instance itself (CONTAINER_API_KEY, which may only GET/DELETE its own instance).
type Client struct {
	BaseURL string
	key     string
	HTTP    *http.Client
}

func New(key string) *Client {
	return &Client{BaseURL: DefaultBaseURL, key: key, HTTP: &http.Client{Timeout: 30 * time.Second}}
}

// Offer is one rentable machine from POST /bundles.
type Offer struct {
	ID          int64   `json:"id"`
	GPUName     string  `json:"gpu_name"`
	DPH         float64 `json:"dph_total"`
	InetDown    float64 `json:"inet_down"`
	Geo         string  `json:"geolocation"`
	Reliability float64 `json:"reliability2"`
}

// Inst is one instance from GET /instances.
type Inst struct {
	ID       int64   `json:"id"`
	Label    string  `json:"label"`
	Status   string  `json:"actual_status"`
	DPH      float64 `json:"dph_total"`
	InetDown float64 `json:"inet_down"`
	Geo      string  `json:"geolocation"`
	Start    float64 `json:"start_date"` // unix seconds
}

// APIError is a non-2xx answer. Code is Vast's "error" field (e.g. insufficient_credit), "" if absent.
type APIError struct {
	Method, Path, Code, Body string
	Status                   int
}

func (e *APIError) Error() string {
	return fmt.Sprintf("vast %s %s: HTTP %d: %s", e.Method, e.Path, e.Status, e.Body)
}

func (c *Client) do(ctx context.Context, method, path string, body, out any) (int, error) {
	var rd io.Reader
	if body != nil {
		b, err := json.Marshal(body)
		if err != nil {
			return 0, err
		}
		rd = bytes.NewReader(b)
	}
	req, err := http.NewRequestWithContext(ctx, method, c.BaseURL+path, rd)
	if err != nil {
		return 0, err
	}
	req.Header.Set("Authorization", "Bearer "+c.key)
	req.Header.Set("Content-Type", "application/json")
	resp, err := c.HTTP.Do(req)
	if err != nil {
		return 0, err
	}
	defer resp.Body.Close()
	b, _ := io.ReadAll(resp.Body)
	if resp.StatusCode == http.StatusNotFound {
		return resp.StatusCode, provider.ErrNotFound
	}
	if resp.StatusCode == http.StatusUnauthorized || resp.StatusCode == http.StatusForbidden {
		return resp.StatusCode, fmt.Errorf("vast %s %s: HTTP %d (check VASTAI_API_KEY)", method, path, resp.StatusCode)
	}
	if resp.StatusCode >= 300 {
		e := &APIError{Method: method, Path: path, Status: resp.StatusCode, Body: strings.TrimSpace(string(b))}
		var body struct {
			Error string `json:"error"`
		}
		if json.Unmarshal(b, &body) == nil {
			e.Code = body.Error
		}
		return resp.StatusCode, e
	}
	if out != nil {
		if err := json.Unmarshal(b, out); err != nil {
			return resp.StatusCode, fmt.Errorf("vast %s %s: %w", method, path, err)
		}
	}
	return resp.StatusCode, nil
}

// SearchQuery is the offer filter: 1× RTX 5090, verified, reliable, enough disk, CUDA and network, price cap.
func SearchQuery(maxDPH float64, minMBps int) map[string]any {
	return map[string]any{
		"gpu_name":      map[string]any{"in": []string{"RTX 5090"}},
		"num_gpus":      map[string]any{"eq": 1},
		"rentable":      map[string]any{"eq": true},
		"verified":      map[string]any{"eq": true},
		"reliability2":  map[string]any{"gte": 0.98},
		"disk_space":    map[string]any{"gte": 80},
		"cuda_max_good": map[string]any{"gte": 12.8},
		"dph_total":     map[string]any{"lte": maxDPH},
		"inet_down":     map[string]any{"gte": minMBps * 8}, // drop hosts that would fail LOBO_MIN_MBPS before paying for them
		"order":         [][]string{{"dph_total", "asc"}},   // cheapest first (user rule 2026-09-29)
		"limit":         10,
	}
}

func (c *Client) SearchOffers(ctx context.Context, maxDPH float64, minMBps int) ([]Offer, error) {
	var r struct {
		Offers []Offer `json:"offers"`
	}
	_, err := c.do(ctx, http.MethodPost, "/bundles", SearchQuery(maxDPH, minMBps), &r)
	return r.Offers, err
}

// Create rents offerID. Proven live 2026-09-25: PUT /asks/{id}/ → {"success":true,"new_contract":<instance id>}.
// ErrRejected: Vast answered the create and rented nothing, so trying another offer is safe.
var ErrRejected = errors.New("vast: offer rejected")

// ErrNoCredit: the account can't rent anything, so walking the other offers only hides the cause.
var ErrNoCredit = errors.New("vast: account has no credit (top up at https://cloud.vast.ai/billing/)")

func (c *Client) Create(ctx context.Context, offerID int64, body map[string]any) (int64, error) {
	var r struct {
		Success     bool   `json:"success"`
		NewContract int64  `json:"new_contract"`
		Msg         string `json:"msg"`
		Error       string `json:"error"`
	}
	code, err := c.do(ctx, http.MethodPut, "/asks/"+strconv.FormatInt(offerID, 10)+"/", body, &r)
	if err != nil {
		if code >= 400 && code < 500 { // Vast answered and said no: nothing was rented
			if ae := (*APIError)(nil); errors.As(err, &ae) && ae.Code == "insufficient_credit" {
				return 0, ErrNoCredit
			}
			return 0, fmt.Errorf("%w: %v", ErrRejected, err)
		}
		return 0, err // transport error, 5xx or bad body: the instance may exist
	}
	if !r.Success || r.NewContract == 0 {
		return 0, fmt.Errorf("%w: vast create offer %d: %s %s", ErrRejected, offerID, r.Error, r.Msg)
	}
	return r.NewContract, nil
}

func (c *Client) List(ctx context.Context) ([]Inst, error) {
	var r struct {
		Instances []Inst `json:"instances"`
	}
	_, err := c.do(ctx, http.MethodGet, "/instances/", nil, &r)
	return r.Instances, err
}

// Get returns provider.ErrNotFound when the instance is gone: 404, or 200 with "instances": null
// (both seen live right after a destroy).
func (c *Client) Get(ctx context.Context, id int64) (Inst, error) {
	var r struct {
		Instances *Inst `json:"instances"`
	}
	if _, err := c.do(ctx, http.MethodGet, "/instances/"+strconv.FormatInt(id, 10)+"/", nil, &r); err != nil {
		return Inst{}, err
	}
	if r.Instances == nil {
		return Inst{}, provider.ErrNotFound
	}
	return *r.Instances, nil
}

// Destroy deletes the instance (stops all billing). Already gone = nil.
func (c *Client) Destroy(ctx context.Context, id int64) error {
	_, err := c.do(ctx, http.MethodDelete, "/instances/"+strconv.FormatInt(id, 10)+"/", nil, nil)
	if err == provider.ErrNotFound {
		return nil
	}
	return err
}
