use crate::{fmt, types::*};
use chrono::{DateTime, Utc};
use lobo_proto::{ConfigShow, Readiness, Snap, Stage, UpEvent, UpRequest, catalog};
use std::time::Duration;

/// State rules only. The controller owns work and supplies the clock.
pub struct Store {
    state: PanelState,
    pub up_running: bool,
    pub stop_running: bool,
    cleanup_failed: bool,
    user_stopped: bool,
    panel_open: bool,
    runtime: Option<lobo_core::control::RuntimeTarget>,
    runtime_generation: u64,
    config_generation: u64,
    selection_generation: u64,
    submission_id: u64,
    operation_launched: bool,
}
impl Store {
    pub fn new() -> Self {
        let mut ids: Vec<_> = catalog::all().iter().map(|m| m.id.clone()).collect();
        ids.sort_by_key(|s| (s != "q6", s.clone()));
        Self {
            state: PanelState {
                provider: "runpod".into(),
                model: "q6".into(),
                catalog_ids: ids,
                ..Default::default()
            },
            up_running: false,
            stop_running: false,
            cleanup_failed: false,
            user_stopped: false,
            panel_open: false,
            runtime: None,
            runtime_generation: 0,
            config_generation: 0,
            selection_generation: 0,
            submission_id: 0,
            operation_launched: false,
        }
    }
    pub fn view(&self, now: DateTime<Utc>) -> PanelState {
        let mut s = self.state.clone();
        s.start_allowed = !self.up_running
            && !self.stop_running
            && !self.cleanup_failed
            && matches!(s.phase, Phase::Off | Phase::Failed { .. });
        s.boot_steps = Step::steps().to_vec();
        s.current_step = (s.phase == Phase::Booting)
            .then(|| {
                s.boot_steps
                    .iter()
                    .rev()
                    .find(|step| s.steps.iter().any(|mark| mark.step == **step))
                    .copied()
            })
            .flatten();
        s.endpoint = self.endpoint();
        s.boot_progress = if s.phase == Phase::Ready {
            1.0
        } else if let Some(step) = s.current_step {
            let i = s.boot_steps.iter().position(|x| *x == step).unwrap();
            let frac = if step == Step::Download {
                s.download
                    .as_ref()
                    .filter(|d| d.total > 0)
                    .map_or(0.0, |d| (d.bytes as f64 / d.total as f64).clamp(0.0, 1.0))
            } else {
                0.0
            };
            (i as f64 + frac) / s.boot_steps.len() as f64
        } else {
            0.0
        };
        s.menu_text = match &s.phase {
            Phase::Loading => "lobo".into(),
            Phase::NoConfig => "setup".into(),
            Phase::Off => "off".into(),
            Phase::Stopping => "stop".into(),
            Phase::Failed { .. } => "FAIL".into(),
            Phase::Booting => {
                if s.current_step == Some(Step::Download)
                    && let Some(d) = s.download.as_ref().filter(|d| d.total > 0)
                {
                    format!(
                        "{}%",
                        ((d.bytes as f64 / d.total as f64) * 100.0).clamp(0.0, 100.0) as i64
                    )
                } else {
                    s.current_step.map_or("boot", |st| st.label()).into()
                }
            }
            Phase::Ready => {
                if let Some(st) = s.snap.as_ref().and_then(|s| s.status.as_ref()) {
                    if let Some(l) = st
                        .llama
                        .as_ref()
                        .filter(|l| l.requests_processing > 0 && l.gen_tps > 0.0)
                    {
                        format!("{} t/s", l.gen_tps as i64)
                    } else {
                        let processing =
                            st.llama.as_ref().is_some_and(|l| l.requests_processing > 0);
                        let elapsed = if processing {
                            0.0
                        } else {
                            s.snap
                                .as_ref()
                                .and_then(|snap| snap.at.0)
                                .map_or(0.0, |at| {
                                    now.signed_duration_since(at).num_milliseconds() as f64 / 1000.0
                                })
                        };
                        format!(
                            "{}m",
                            ((st.kill_in_s as f64 - elapsed).max(0.0) / 60.0).ceil() as u64
                        )
                    }
                } else {
                    "run".into()
                }
            }
        };

        s
    }
    fn endpoint(&self) -> Option<String> {
        if let Some(pod) = self
            .state
            .snap
            .as_ref()
            .filter(|s| !s.down)
            .and_then(|s| s.pod.as_ref())
            && !pod.api_url.is_empty()
        {
            return Some(pod.api_url.clone());
        }
        if let Some(url) = &self.state.ready_url {
            return Some(url.clone());
        }
        self.state
            .config
            .as_ref()
            .map(|c| lobo_core::config::Laptop::from_values(&c.values).cloud_url())
    }
    pub fn needs_cleanup(&self) -> bool {
        self.up_running || self.stop_running || self.cleanup_failed
    }
    pub fn poll_interval(&self) -> Duration {
        Duration::from_secs(match self.state.phase {
            Phase::Booting | Phase::Stopping | Phase::Loading => 3,
            Phase::Ready => {
                if self.panel_open {
                    5
                } else {
                    15
                }
            }
            _ => {
                if self.panel_open {
                    10
                } else {
                    30
                }
            }
        })
    }
    pub fn derive(s: &Snap, up_running: bool, current: &Phase) -> Phase {
        if matches!(current, Phase::Failed { .. }) && s.down && !up_running {
            return current.clone();
        }
        if *current == Phase::Stopping && !s.down {
            return Phase::Stopping;
        }
        if s.down {
            return if up_running {
                Phase::Booting
            } else {
                Phase::Off
            };
        }
        let Some(st) = &s.status else {
            return Phase::Booting;
        };
        match st.stage {
            Stage::Ready => Phase::Ready,
            Stage::Failed | Stage::Terminating if !up_running => Phase::Failed {
                message: if st.stage_detail.is_empty() {
                    st.stage.as_str().into()
                } else {
                    st.stage_detail.clone()
                },
            },
            _ => Phase::Booting,
        }
    }
    pub fn apply_config(&mut self, c: ConfigShow, r: Readiness) {
        let before = (self.state.provider.clone(), self.state.model.clone());
        if self
            .state
            .config
            .as_ref()
            .is_none_or(|old| old.values != c.values || old.set != c.set)
        {
            self.invalidate_config();
        }
        if !self.up_running && !self.stop_running && self.state.phase != Phase::Booting {
            self.state.provider = r.default_provider.clone();
            self.state.model = r.default_model.clone();
        }
        self.state.config = Some(c);
        self.state.readiness = Some(r);
        if before != (self.state.provider.clone(), self.state.model.clone()) {
            self.selection_generation = self.selection_generation.wrapping_add(1);
        }
    }
    pub fn apply_snap(&mut self, s: Snap, now: DateTime<Utc>) -> Vec<Note> {
        self.backend_updated(now);
        let before = self.state.phase.clone();
        if let Some(st) = &s.status
            && matches!(st.stage, Stage::Download | Stage::Verify)
        {
            self.state.download = Some(st.download.clone());
        }
        let next = if self.stop_running || self.cleanup_failed {
            before.clone()
        } else {
            Self::derive(&s, self.up_running, &before)
        };
        let mut notes = vec![];
        if before == Phase::Ready && next == Phase::Off && !self.user_stopped {
            notes.push(Note {
                title: "lobo stopped".into(),
                body: "stopped by itself (idle or expiry)".into(),
            });
        }
        if matches!(next, Phase::Off | Phase::Ready) {
            self.user_stopped = false;
        }
        if next == Phase::Booting && self.state.boot_start_ms.is_none() {
            self.state.boot_start_ms = Some(
                s.pod
                    .as_ref()
                    .and_then(|p| p.started_at.0)
                    .map_or(now.timestamp_millis(), |at| at.timestamp_millis()),
            );
        }
        if next == Phase::Booting
            && let Some(step) = s
                .status
                .as_ref()
                .and_then(|s| Step::from_up_phase(s.stage.as_str()))
        {
            for c in Step::steps().iter().filter(|c| c.index() <= step.index()) {
                self.mark(*c, now);
            }
        }
        if matches!(next, Phase::Off | Phase::Ready) && !self.up_running {
            self.clear_progress();
        }
        if next == Phase::Off {
            self.state.ready_url = None;
        }
        self.state.phase = next;
        self.state.snap = Some(s);
        notes
    }
    fn clear_progress(&mut self) {
        self.state.boot_start_ms = None;
        self.state.steps.clear();
        self.state.download = None;
    }
    fn mark(&mut self, step: Step, now: DateTime<Utc>) {
        if let Some(t0) = self.state.boot_start_ms
            && !self.state.steps.iter().any(|s| s.step == step)
        {
            self.state.steps.push(StepMark {
                step,
                at_s: (now.timestamp_millis() - t0).max(0) as f64 / 1000.0,
            });
        }
    }
    pub fn begin_up(&mut self, now: DateTime<Utc>) -> UpRequest {
        self.backend_updated(now);
        self.up_running = true;
        self.cleanup_failed = false;
        self.user_stopped = false;
        self.state.phase = Phase::Booting;
        self.clear_progress();
        self.state.boot_start_ms = Some(now.timestamp_millis());
        self.state.up_phase = None;
        self.state.log_tail.clear();
        self.state.ready_url = None;
        self.state.warning = None;
        self.state.last_detail = format!("renting {}…", self.state.provider);
        UpRequest {
            provider: Some(self.state.provider.clone()),
            model: Some(self.state.model.clone()),
            ..Default::default()
        }
    }
    pub fn handle_event(&mut self, ev: &UpEvent, now: DateTime<Utc>) -> Vec<Note> {
        self.backend_updated(now);
        self.state.up_phase = Some(ev.phase.clone());
        if let Some(step) = Step::from_up_phase(&ev.phase) {
            self.mark(step, now);
        }
        if let Some(d) = &ev.download {
            self.state.download = Some(d.clone());
        }
        if !ev.detail.is_empty() {
            self.state.last_detail = ev.detail.clone();
            self.state.log_tail.push(ev.detail.clone());
            self.tail(6);
        }
        if let Some(e) = &ev.err {
            self.state.log_tail.extend(e.lines().map(String::from));
            self.tail(8);
        }
        if let Some(r) = &ev.ready
            && !self.stop_running
            && !self.cleanup_failed
        {
            self.state.ready_url = Some(r.url.clone());
            self.state.phase = Phase::Ready;
            let cost = if r.cost_per_hr > 0.0 {
                format!("${:.2}/h", r.cost_per_hr)
            } else {
                "cloud".into()
            };
            return vec![Note {
                title: "lobo ready".into(),
                body: format!(
                    "{} · {} · {}",
                    r.url,
                    fmt::duration(r.elapsed_ns as f64 / 1e9),
                    cost
                ),
            }];
        }
        vec![]
    }
    fn tail(&mut self, n: usize) {
        let excess = self.state.log_tail.len().saturating_sub(n);
        self.state.log_tail.drain(..excess);
    }
    pub fn up_ended(&mut self, last_err: Option<&str>) -> Vec<Note> {
        self.up_running = false;
        if let Some(err) = last_err
            && !matches!(self.state.phase, Phase::Ready | Phase::Stopping)
        {
            let msg = err.to_string();
            self.state.phase = Phase::Failed {
                message: msg.clone(),
            };
            return vec![Note {
                title: "lobo boot failed".into(),
                body: msg,
            }];
        }
        vec![]
    }
    pub fn backend_updated(&mut self, now: DateTime<Utc>) {
        self.state.last_update_ms = Some(now.timestamp_millis());
    }
    pub fn set_diagnostics(&mut self, path: String, error: Option<String>) {
        self.state.log_path = Some(path);
        self.state.logging_error = error;
    }
    pub fn start_timed_out(&mut self) {
        // Keep up_running true until the retained worker finishes cleanup.
        self.cleanup_failed = true;
        self.state.phase = Phase::Failed {
            message:
                "Startup exceeded 40 minutes. Cancellation requested; cleanup is not yet confirmed."
                    .into(),
        };
        self.state.warning = Some("Cleanup is still running. Do not start another GPU.".into());
    }
    pub fn resumed_start_timed_out(&mut self) {
        self.cleanup_failed = true;
        let provider_time = self
            .state
            .snap
            .as_ref()
            .and_then(|s| s.pod.as_ref())
            .is_some_and(|p| p.started_at.0.is_some());
        self.state.phase = Phase::Failed { message: if provider_time {
            "Cloud startup exceeded 40 minutes since the provider start time. Stop the runtime before retrying."
        } else {
            "Cloud runtime did not reach Ready during 40 minutes of observation. Its original start time is unknown. Stop it before retrying."
        }.into() };
        self.state.warning = Some("Runtime cleanup has not been confirmed.".into());
    }
    pub fn begin_stop(&mut self) {
        self.stop_running = true;
        self.user_stopped = true;
        self.cleanup_failed = false;
        self.state.phase = Phase::Stopping;
        self.state.warning = None;
    }
    pub fn stop_failed(&mut self, message: String) {
        self.stop_running = false;
        self.cleanup_failed = true;
        self.state.phase = Phase::Failed { message };
    }
    pub fn stop_done(&mut self) {
        self.stop_running = false;
        self.cleanup_failed = false;
        self.state.phase = Phase::Off;
        self.state.ready_url = None;
        self.state.warning = None;
        self.clear_progress();
    }
    pub fn dismiss(&mut self) {
        if !self.up_running && !self.stop_running && !self.cleanup_failed {
            self.state.phase = Phase::Off;
        }
    }
    pub fn set_provider(&mut self, p: String) {
        if !self.up_running && !self.stop_running && matches!(p.as_str(), "runpod" | "vast") {
            self.state.provider = p;
            self.selection_generation = self.selection_generation.wrapping_add(1);
        }
    }
    pub fn set_model(&mut self, m: String) {
        if !self.up_running && !self.stop_running && self.state.catalog_ids.contains(&m) {
            self.state.model = m;
            self.selection_generation = self.selection_generation.wrapping_add(1);
        }
    }
    pub fn set_warning(&mut self, w: Option<String>) {
        self.state.warning = w;
    }
    pub fn poll_failed(&mut self, msg: String) {
        self.state.warning = Some(msg);
        if self.state.phase == Phase::Loading {
            self.state.phase = Phase::Off;
        }
    }
    pub fn set_panel_open(&mut self, open: bool) {
        self.panel_open = open;
    }
    pub fn needs_setup(&mut self) {
        if !self.up_running && !self.stop_running && !self.cleanup_failed {
            self.state.phase = Phase::NoConfig;
        }
    }
    pub fn invalidate_config(&mut self) {
        self.config_generation = self.config_generation.wrapping_add(1);
    }
    pub fn generations(&self) -> (u64, u64, u64) {
        (
            self.config_generation,
            self.selection_generation,
            self.runtime_generation,
        )
    }
    pub fn runtime(&self) -> Option<lobo_core::control::RuntimeTarget> {
        self.runtime.clone()
    }
    pub fn set_submission_id(&mut self, id: u64) {
        self.submission_id = id;
        self.operation_launched = false;
    }
    pub fn owns_submission(&self, id: u64) -> bool {
        self.submission_id == id
    }
    pub fn operation_launched(&self) -> bool {
        self.operation_launched
    }
    pub fn mark_launched(&mut self) {
        self.operation_launched = true;
    }
    pub fn set_runtime(&mut self, target: Option<lobo_core::control::RuntimeTarget>) {
        let target = target.filter(|t| matches!(t.provider.as_str(), "runpod" | "vast"));
        if self.runtime != target {
            self.runtime_generation = self.runtime_generation.wrapping_add(1);
            self.runtime = target;
        }
    }
    pub(crate) fn submit(&mut self, now: DateTime<Utc>) -> StartSubmission {
        StartSubmission {
            request: self.begin_up(now),
            config_generation: self.config_generation,
            selection_generation: self.selection_generation,
        }
    }
    pub(crate) fn submission_valid(&self, submission: &StartSubmission) -> bool {
        self.config_generation == submission.config_generation
            && self.selection_generation == submission.selection_generation
            && !self.stop_running
    }
}
impl Default for Store {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests;
