package runpod

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"time"
)

// Self is the pod-side client. It uses the pod-scoped RUNPOD_API_KEY that RunPod injects.
// That key gets 403 on REST but may query and terminate its own pod over GraphQL (verified in P1).
type Self struct {
	PodID string
	URL   string
	key   string
	HTTP  *http.Client
}

func NewSelf(podID, key string) *Self {
	return &Self{PodID: podID, URL: "https://api.runpod.io/graphql", key: key, HTTP: &http.Client{Timeout: 10 * time.Second}}
}

func (s *Self) gql(ctx context.Context, query string, out any) error {
	b, _ := json.Marshal(map[string]string{"query": query})
	req, err := http.NewRequestWithContext(ctx, http.MethodPost, s.URL, bytes.NewReader(b))
	if err != nil {
		return err
	}
	req.Header.Set("Authorization", "Bearer "+s.key)
	req.Header.Set("Content-Type", "application/json")
	resp, err := s.HTTP.Do(req)
	if err != nil {
		return err
	}
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusOK {
		return fmt.Errorf("runpod graphql: HTTP %d", resp.StatusCode)
	}
	var r struct {
		Data   json.RawMessage `json:"data"`
		Errors []struct {
			Message string `json:"message"`
		} `json:"errors"`
	}
	if err := json.NewDecoder(resp.Body).Decode(&r); err != nil {
		return err
	}
	if len(r.Errors) > 0 {
		return fmt.Errorf("runpod graphql: %s", r.Errors[0].Message)
	}
	if out != nil {
		return json.Unmarshal(r.Data, out)
	}
	return nil
}

// Terminate asks RunPod to delete this pod.
func (s *Self) Terminate(ctx context.Context) error {
	return s.gql(ctx, fmt.Sprintf(`mutation { podTerminate(input:{podId:%q}) }`, s.PodID), nil)
}

// Gone reports whether this pod no longer exists (or is terminated).
func (s *Self) Gone(ctx context.Context) (bool, error) {
	var d struct {
		Pod *struct {
			DesiredStatus string `json:"desiredStatus"`
		} `json:"pod"`
	}
	if err := s.gql(ctx, fmt.Sprintf(`{ pod(input:{podId:%q}) { desiredStatus } }`, s.PodID), &d); err != nil {
		return false, err
	}
	return d.Pod == nil || d.Pod.DesiredStatus == "TERMINATED", nil
}
