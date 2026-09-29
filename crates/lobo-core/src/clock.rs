use chrono::{DateTime, Utc};
use std::{sync::Mutex, time::Duration};
pub trait Clock: Send + Sync {
    fn now(&self) -> DateTime<Utc>;
}
pub struct SystemClock;
impl Clock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}
pub struct FixedClock(pub DateTime<Utc>);
impl Clock for FixedClock {
    fn now(&self) -> DateTime<Utc> {
        self.0
    }
}
pub struct StepClock {
    time: Mutex<DateTime<Utc>>,
    step: chrono::TimeDelta,
}
impl StepClock {
    pub fn new(t0: DateTime<Utc>, step: Duration) -> Self {
        Self {
            time: Mutex::new(t0),
            step: chrono::TimeDelta::from_std(step).expect("clock step in range"),
        }
    }
}
impl Clock for StepClock {
    fn now(&self) -> DateTime<Utc> {
        let mut time = self.time.lock().unwrap();
        *time += self.step;
        *time
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn step_clock_advances_per_call() {
        let t0 = "2026-09-29T12:00:00Z".parse().unwrap();
        let c = StepClock::new(t0, Duration::from_secs(1));
        assert_eq!(c.now(), t0 + chrono::TimeDelta::seconds(1));
        assert_eq!(c.now(), t0 + chrono::TimeDelta::seconds(2));
    }
}
