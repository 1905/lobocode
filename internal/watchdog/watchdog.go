// Package watchdog decides when the pod must delete itself: idle too long, or expired.
package watchdog

import "time"

// Sample is one llama-server metrics read. OK=false means the read failed.
type Sample struct {
	At           time.Time
	OK           bool
	Processing   int
	Deferred     int
	PromptTokens int64
	GenTokens    int64
}

// Config holds the kill thresholds.
type Config struct {
	Idle      time.Duration
	ExpiresAt time.Time
}

// Decision is what the agent should do now.
type Decision struct {
	Kill   bool
	Reason string // "idle" | "expired" | ""
	KillIn time.Duration
}

// State tracks activity. Idle = now − last OK sample that showed activity (or the ready time).
// Failed samples never count as activity, so a dead scraper can't keep a pod alive.
type State struct {
	start        time.Time
	ready        bool
	lastActive   time.Time
	havePrev     bool
	prevPrompt   int64
	prevGen      int64
	failedInARow int
}

func NewState(start time.Time) *State { return &State{start: start} }

// SetReady starts the idle clock. Before ready only expiry applies.
func (s *State) SetReady(at time.Time) {
	s.ready = true
	s.lastActive = at
}

func (s *State) Observe(sm Sample) {
	if !sm.OK {
		s.failedInARow++
		return
	}
	s.failedInARow = 0
	moved := s.havePrev && (sm.PromptTokens != s.prevPrompt || sm.GenTokens != s.prevGen)
	s.prevPrompt, s.prevGen, s.havePrev = sm.PromptTokens, sm.GenTokens, true
	if s.ready && (sm.Processing > 0 || sm.Deferred > 0 || moved) && sm.At.After(s.lastActive) {
		s.lastActive = sm.At
	}
}

func (s *State) IdleFor(now time.Time) time.Duration {
	if !s.ready {
		return 0
	}
	return now.Sub(s.lastActive)
}

func (s *State) FailedSamples() int { return s.failedInARow }

func (s *State) Decide(now time.Time, cfg Config) Decision {
	expIn := cfg.ExpiresAt.Sub(now)
	if expIn <= 0 {
		return Decision{Kill: true, Reason: "expired"}
	}
	if !s.ready {
		return Decision{KillIn: expIn, Reason: "expired"}
	}
	idleIn := cfg.Idle - s.IdleFor(now)
	if idleIn <= 0 {
		return Decision{Kill: true, Reason: "idle"}
	}
	if idleIn < expIn {
		return Decision{KillIn: idleIn, Reason: "idle"}
	}
	return Decision{KillIn: expIn, Reason: "expired"}
}
