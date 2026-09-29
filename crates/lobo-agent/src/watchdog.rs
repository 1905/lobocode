use chrono::{DateTime, Utc};
use std::time::Duration;

#[derive(Debug, Clone, Default)]
pub struct Sample {
    pub at: DateTime<Utc>,
    pub ok: bool,
    pub processing: i64,
    pub deferred: i64,
    pub prompt_tokens: i64,
    pub gen_tokens: i64,
}

pub struct Config {
    pub idle: Duration,
    pub expires_at: DateTime<Utc>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Reason {
    Idle,
    Expired,
}

impl Reason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Expired => "expired",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Decision {
    Kill { reason: Reason },
    Wait { reason: Reason, kill_in: Duration },
}

impl Decision {
    pub fn reason(&self) -> Reason {
        match *self {
            Self::Kill { reason } | Self::Wait { reason, .. } => reason,
        }
    }
    pub fn kill_in(&self) -> Duration {
        match *self {
            Self::Kill { .. } => Duration::ZERO,
            Self::Wait { kill_in, .. } => kill_in,
        }
    }
    pub fn is_kill(&self) -> bool {
        matches!(self, Self::Kill { .. })
    }
}

pub struct State {
    ready: bool,
    last_active: DateTime<Utc>,
    previous: Option<(i64, i64)>,
    failed: i64,
}

impl State {
    pub fn new(start: DateTime<Utc>) -> Self {
        Self {
            ready: false,
            last_active: start,
            previous: None,
            failed: 0,
        }
    }
    pub fn set_ready(&mut self, at: DateTime<Utc>) {
        self.ready = true;
        self.last_active = at;
    }
    pub fn observe(&mut self, s: Sample) {
        if !s.ok {
            self.failed += 1;
            return;
        }
        self.failed = 0;
        let totals = (s.prompt_tokens, s.gen_tokens);
        let moved = self.previous.is_some_and(|previous| previous != totals);
        self.previous = Some(totals);
        if self.ready && (s.processing > 0 || s.deferred > 0 || moved) && s.at > self.last_active {
            self.last_active = s.at;
        }
    }
    pub fn idle_for(&self, now: DateTime<Utc>) -> Duration {
        if self.ready {
            (now - self.last_active).to_std().unwrap_or_default()
        } else {
            Duration::ZERO
        }
    }
    pub fn failed_samples(&self) -> i64 {
        self.failed
    }
    pub fn decide(&self, now: DateTime<Utc>, cfg: &Config) -> Decision {
        let expiry = (cfg.expires_at - now).to_std().unwrap_or_default();
        if expiry.is_zero() {
            return Decision::Kill {
                reason: Reason::Expired,
            };
        }
        if self.ready {
            let idle = cfg.idle.saturating_sub(self.idle_for(now));
            if idle.is_zero() {
                return Decision::Kill {
                    reason: Reason::Idle,
                };
            }
            if idle < expiry {
                return Decision::Wait {
                    reason: Reason::Idle,
                    kill_in: idle,
                };
            }
        }
        Decision::Wait {
            reason: Reason::Expired,
            kill_in: expiry,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeDelta;

    fn start() -> DateTime<Utc> {
        "2026-09-23T10:00:00Z".parse().unwrap()
    }
    fn min(n: u64) -> Duration {
        Duration::from_secs(n * 60)
    }
    fn sample(at: i64, processing: i64, deferred: i64, tokens: i64, ok: bool) -> Sample {
        Sample {
            at: start() + TimeDelta::minutes(at),
            ok,
            processing,
            deferred,
            prompt_tokens: tokens,
            gen_tokens: 0,
        }
    }
    #[test]
    fn decide() {
        let cases = [
            (
                "not ready never idles",
                false,
                vec![],
                300,
                false,
                Reason::Expired,
                420,
            ),
            ("idle 29m", true, vec![], 29, false, Reason::Idle, 1),
            ("idle 31m", true, vec![], 31, true, Reason::Idle, 0),
            (
                "busy keeps alive",
                true,
                vec![sample(25, 1, 0, 0, true)],
                31,
                false,
                Reason::Idle,
                24,
            ),
            (
                "deferred keeps alive",
                true,
                vec![sample(25, 0, 2, 0, true)],
                31,
                false,
                Reason::Idle,
                24,
            ),
            (
                "tokens moved reset",
                true,
                vec![sample(1, 0, 0, 10, true), sample(20, 0, 0, 50, true)],
                40,
                false,
                Reason::Idle,
                10,
            ),
            (
                "failed samples add nothing",
                true,
                vec![
                    sample(5, 0, 0, 0, false),
                    sample(20, 0, 0, 0, false),
                    sample(30, 0, 0, 0, false),
                ],
                31,
                true,
                Reason::Idle,
                0,
            ),
            (
                "expired while busy",
                true,
                vec![Sample {
                    at: start() + TimeDelta::hours(12) - TimeDelta::seconds(1),
                    ok: true,
                    processing: 1,
                    ..Default::default()
                }],
                720,
                true,
                Reason::Expired,
                0,
            ),
        ];
        for (name, ready, samples, now, kill, reason, remaining) in cases {
            let mut state = State::new(start());
            if ready {
                state.set_ready(start());
            }
            for sm in samples {
                state.observe(sm);
            }
            let d = state.decide(
                start() + TimeDelta::minutes(now),
                &Config {
                    idle: min(30),
                    expires_at: start() + TimeDelta::hours(12),
                },
            );
            assert_eq!(
                (d.is_kill(), d.reason(), d.kill_in()),
                (kill, reason, min(remaining)),
                "{name}"
            );
        }
    }
    #[test]
    fn expired_at_start() {
        assert_eq!(
            State::new(start()).decide(
                start(),
                &Config {
                    idle: min(30),
                    expires_at: start() - TimeDelta::minutes(1)
                }
            ),
            Decision::Kill {
                reason: Reason::Expired
            }
        );
    }
    #[test]
    fn failed_samples() {
        let mut s = State::new(start());
        s.observe(Sample::default());
        s.observe(Sample::default());
        assert_eq!(s.failed_samples(), 2);
        s.observe(Sample {
            ok: true,
            ..Default::default()
        });
        assert_eq!(s.failed_samples(), 0);
    }
}
