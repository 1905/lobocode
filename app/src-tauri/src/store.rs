use crate::{fmt, types::*};
use chrono::{DateTime, Utc};
use lobo_proto::{
    ConfigShow, DEFAULT_MODEL, Listing, Readiness, Snap, Stage, UpEvent, UpRequest, catalog,
};
use std::time::Duration;

/// State rules only. The controller owns work and supplies the clock.
pub struct Store {
    state: PanelState,
    pub up_running: bool,
    pub stop_running: bool,
    cleanup_failed: bool,
    model_auto_picked: bool,
    user_stopped: bool,
    panel_open: bool,
    saved_target: Option<Target>,
    local_supported: bool,
}
impl Store {
    pub fn new(saved_target: Option<Target>, local_supported: bool) -> Self {
        let mut ids: Vec<_> = catalog::all().iter().map(|m| m.id.clone()).collect();
        ids.sort_by_key(|s| (s != DEFAULT_MODEL, s.clone()));
        Self {
            state: PanelState {
                provider: "runpod".into(),
                model: DEFAULT_MODEL.into(),
                target: Self::default_target(saved_target, None, None, None, local_supported),
                catalog_ids: ids,
                ..Default::default()
            },
            up_running: false,
            stop_running: false,
            cleanup_failed: false,
            model_auto_picked: false,
            user_stopped: false,
            panel_open: false,
            saved_target,
            local_supported,
        }
    }
    pub fn is_local(&self) -> bool {
        self.state
            .snap
            .as_ref()
            .and_then(|s| s.pod.as_ref())
            .map_or(self.state.target == Target::Local, |p| {
                p.provider == "local"
            })
    }
    pub fn view(&self, now: DateTime<Utc>) -> PanelState {
        let mut s = self.state.clone();
        s.is_local = self.is_local();
        s.boot_steps = Step::steps(s.is_local).to_vec();
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
                    s.current_step
                        .map_or("boot", |st| st.label(s.is_local))
                        .into()
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
        if self.is_local() {
            let port = self
                .state
                .readiness
                .as_ref()
                .map_or(lobo_core::config::DEFAULT_LOCAL_PORT, |r| r.local_port);
            return Some(format!("http://127.0.0.1:{port}/v1"));
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
    pub fn default_target(
        saved: Option<Target>,
        r: Option<&Readiness>,
        cfg: Option<&ConfigShow>,
        models: Option<&Listing>,
        local_supported: bool,
    ) -> Target {
        if !local_supported {
            return Target::Cloud;
        }
        if let Some(saved) = saved {
            return saved;
        }
        if cfg
            .and_then(|c| c.values.get("LOBO_PROVIDER"))
            .is_some_and(|p| p == "local")
        {
            return Target::Local;
        }
        let no_keys = r.map_or_else(
            || {
                cfg.is_none_or(|c| {
                    !c.set.get("RUNPOD_API_KEY").copied().unwrap_or(false)
                        && !c.set.get("VASTAI_API_KEY").copied().unwrap_or(false)
                })
            },
            |r| r.providers.is_empty(),
        );
        if no_keys && models.is_some_and(|l| l.models.iter().any(on_disk)) {
            Target::Local
        } else {
            Target::Cloud
        }
    }
    fn apply_default_target(&mut self) {
        if !self.up_running
            && !self.stop_running
            && !matches!(self.state.phase, Phase::Booting | Phase::Ready)
        {
            self.state.target = Self::default_target(
                self.saved_target,
                self.state.readiness.as_ref(),
                self.state.config.as_ref(),
                self.state.models.as_ref(),
                self.local_supported,
            );
        }
    }
    pub fn apply_config(&mut self, c: ConfigShow, r: Readiness) {
        if !self.up_running && !self.stop_running && self.state.phase != Phase::Booting {
            self.state.provider = r.default_provider.clone();
            self.state.model = r.default_model.clone();
            self.model_auto_picked = false;
        }
        self.state.config = Some(c);
        self.state.readiness = Some(r);
        self.apply_default_target();
    }
    pub fn apply_models(&mut self, m: Listing) {
        let model_configured = self
            .state
            .config
            .as_ref()
            .and_then(|c| c.values.get("LOBO_MODEL"))
            .is_some_and(|s| !s.is_empty());
        if !self.model_auto_picked
            && !self.up_running
            && !self.stop_running
            && self.state.phase != Phase::Booting
            && !model_configured
            && !m
                .models
                .iter()
                .any(|m| m.id == self.state.model && on_disk(m))
            && let Some(other) = m.models.iter().find(|m| on_disk(m))
        {
            self.state.model = other.id.clone();
        }
        self.model_auto_picked = true;
        self.state.models = Some(m);
        self.apply_default_target();
    }
    pub fn apply_snap(&mut self, s: Snap, now: DateTime<Utc>) -> Vec<Note> {
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
            let local = s
                .pod
                .as_ref()
                .map_or(self.is_local(), |p| p.provider == "local");
            for c in Step::steps(local)
                .iter()
                .filter(|c| c.index() <= step.index())
            {
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
        let local = self.state.target == Target::Local;
        self.state.last_detail = if local {
            "starting llama.cpp…".into()
        } else {
            format!("renting {}…", self.state.provider)
        };
        UpRequest {
            provider: Some(if local {
                "local".into()
            } else {
                self.state.provider.clone()
            }),
            model: Some(self.state.model.clone()),
            ..Default::default()
        }
    }
    pub fn handle_event(&mut self, ev: &UpEvent, now: DateTime<Utc>) -> Vec<Note> {
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
        {
            self.state.ready_url = Some(r.url.clone());
            self.state.phase = Phase::Ready;
            let cost = if r.cost_per_hr > 0.0 {
                format!("${:.2}/h", r.cost_per_hr)
            } else {
                "local".into()
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
            let msg = self
                .state
                .log_tail
                .iter()
                .rev()
                .find(|s| !s.is_empty())
                .map(String::as_str)
                .unwrap_or(err)
                .to_string();
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
    pub fn begin_stop(&mut self) -> bool {
        let local = self.is_local();
        self.stop_running = true;
        self.user_stopped = true;
        self.cleanup_failed = false;
        self.state.phase = Phase::Stopping;
        self.state.warning = None;
        local
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
        if !self.up_running && !self.stop_running {
            self.cleanup_failed = false;
            self.state.phase = Phase::Off;
        }
    }
    pub fn choose(&mut self, t: Target) {
        if !self.up_running && !self.stop_running {
            self.saved_target = Some(t);
            self.state.target = if self.local_supported {
                t
            } else {
                Target::Cloud
            };
        }
    }
    pub fn set_provider(&mut self, p: String) {
        if !self.up_running && !self.stop_running {
            self.state.provider = p;
        }
    }
    pub fn set_model(&mut self, m: String) {
        if !self.up_running && !self.stop_running && self.state.catalog_ids.contains(&m) {
            self.state.model = m;
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
}
fn on_disk(m: &lobo_proto::ModelState) -> bool {
    m.size > 0 && m.on_disk >= m.size
}

#[cfg(test)]
mod tests;
