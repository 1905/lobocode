use super::{Flow, styles::*};
use chrono::{DateTime, TimeDelta, Utc};
use lobo_proto::UpEvent;
use ratatui::{
    style::Style,
    text::{Line, Span, Text},
};
use std::collections::BTreeMap;

pub const PHASES: [(&str, &str); 8] = [
    ("create", "rent pod"),
    ("image", "boot container"),
    ("tunnel", "tunnel"),
    ("gpu", "gpu check"),
    ("download", "download model"),
    ("verify", "sha256 verify"),
    ("load", "load model"),
    ("ready", "ready"),
];
pub const SPINNER: [&str; 8] = ["⣾", "⣽", "⣻", "⢿", "⡿", "⣟", "⣯", "⣷"];
fn index(phase: &str) -> Option<usize> {
    PHASES.iter().position(|(id, _)| *id == phase)
}
pub struct UpState {
    pub phase: String,
    pub details: BTreeMap<String, String>,
    pub event: UpEvent,
    pub err: Option<String>,
    pub done: bool,
    pub at: DateTime<Utc>,
    started: BTreeMap<String, DateTime<Utc>>,
    took: BTreeMap<String, TimeDelta>,
}
impl Default for UpState {
    fn default() -> Self {
        Self {
            phase: String::new(),
            details: BTreeMap::new(),
            event: UpEvent::default(),
            err: None,
            done: false,
            at: DateTime::<Utc>::MIN_UTC,
            started: BTreeMap::new(),
            took: BTreeMap::new(),
        }
    }
}
impl UpState {
    pub fn apply_at(&mut self, e: &UpEvent, at: DateTime<Utc>) {
        self.at = self.at.max(at);
        let terminal = ["failed", "terminated", "cancelled"].contains(&e.phase.as_str());
        if !terminal && e.phase != self.phase {
            if let (Some(ni), Some(oi)) = (index(&e.phase), index(&self.phase))
                && ni <= oi
            {
                for (id, _) in &PHASES[ni..] {
                    self.started.remove(*id);
                    self.took.remove(*id);
                }
            } else if let Some(start) = self.started.get(&self.phase) {
                self.took.insert(self.phase.clone(), at - *start);
            }
            self.started.insert(e.phase.clone(), at);
        }
        if !terminal {
            self.phase = e.phase.clone();
            if !e.detail.is_empty() {
                self.details.insert(e.phase.clone(), e.detail.clone());
            }
        }
        self.event = e.clone();
        if e.err.is_some() {
            self.err = e.err.clone();
        }
        self.done |= e.done;
    }
    pub fn took(&self, phase: &str) -> Option<TimeDelta> {
        self.took.get(phase).copied()
    }
}
pub fn render_up(s: &UpState, spin: &str) -> Text<'static> {
    let mut lines = vec![
        Line::from(vec![
            span("lobo up", TITLE),
            dim("  ·  Qwen3.5-27B on a rented RTX 5090"),
        ]),
        Line::default(),
    ];
    let cur = index(&s.phase);
    for (i, (id, name)) in PHASES.iter().enumerate() {
        let current = cur == Some(i);
        let completed = cur.is_some_and(|n| i < n) || current && s.phase == "ready";
        let (mark, style) = if completed {
            ("✓", Style::new().fg(OK))
        } else if current && s.err.is_some() {
            ("✗", Style::new().fg(ERR))
        } else if current {
            (spin, BOLD)
        } else {
            ("·", Style::new().fg(DIM))
        };
        let name_style = if completed { Style::default() } else { style };
        let mut line = vec![Span::raw(" "), span(mark, style), Span::raw(" ")];
        line.extend(pad(vec![span(*name, name_style)], 16));
        if completed {
            if let Some(d) = s.took(id) {
                line.push(dim(format!("{:<6} ", dur(d))));
            }
        } else if current && s.err.is_none() {
            if let Some(start) = s.started.get(*id)
                && s.at >= *start
            {
                line.push(span(format!("{:<6} ", clock(s.at - *start)), BOLD));
            }
            if *id == "image" {
                line.push(dim("usually 15–30 s · re-rent at 6:00 "));
            }
        }
        if cur.is_some_and(|n| i <= n)
            && let Some(detail) = s.details.get(*id)
        {
            line.push(dim(detail.clone()));
        }
        if *id == "download"
            && current
            && let Some(dl) = s.event.download.as_ref().filter(|dl| dl.total > 0)
        {
            let frac = dl.bytes as f64 / dl.total as f64;
            let eta = if dl.mbps > 0.0 {
                dur(secs((dl.total - dl.bytes) as f64 / (dl.mbps * 1e6)))
            } else {
                "?".into()
            };
            line.extend(bar(frac, 24, Style::new().fg(OK)));
            line.push(Span::raw(format!(
                " {:5.1}%  {} / {}  {:.0} MB/s  ETA {eta}",
                100.0 * frac,
                gb(dl.bytes),
                gb(dl.total),
                dl.mbps
            )));
        }
        lines.push(Line::from(line));
    }
    if let Some(r) = &s.event.ready {
        let sha = if r.git_sha.is_empty() {
            "?"
        } else {
            &r.git_sha
        };
        lines.push(Line::default());
        lines.extend(boxed(
            vec![
                Line::from(span("✓ lobo is up", BOLD.fg(OK))),
                row("endpoint", vec![span(r.url.clone(), BOLD)]),
                row("release", vec![Span::raw(format!("{} ({sha})", r.version))]),
                row("cost", vec![Span::raw(format!("${:.2}/h", r.cost_per_hr))]),
                row(
                    "boot time",
                    vec![Span::raw(dur(TimeDelta::nanoseconds(r.elapsed_ns)))],
                ),
                Line::from(dim("lobo status · lobo test · lobo down")),
            ],
            ACCENT,
        ));
    }
    if let Some(e) = &s.err {
        let mut failure = vec![Line::from(span("✗ up failed", BOLD.fg(ERR)))];
        failure.extend(e.lines().map(|s| Line::from(s.to_owned())));
        lines.push(Line::default());
        lines.extend(boxed(failure, ERR));
    }
    Text::from(lines)
}
// Reducer inputs are consumed immediately, not retained in a queue.
#[allow(clippy::large_enum_variant)]
pub enum UpMsg {
    Event(Option<UpEvent>),
    Key(crossterm::event::KeyEvent),
    Tick(DateTime<Utc>),
}
#[derive(Default)]
pub struct UpModel {
    pub state: UpState,
    frame: usize,
    pub interrupted: bool,
}
impl UpModel {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn update(&mut self, m: UpMsg, now: DateTime<Utc>) -> Flow {
        match m {
            UpMsg::Event(None) => return Flow::Quit,
            UpMsg::Event(Some(e)) => {
                self.state.apply_at(&e, now);
                if self.state.done {
                    return Flow::Quit;
                }
            }
            UpMsg::Key(key) if super::quit_key(key, false) => {
                self.interrupted = true;
                return Flow::Quit;
            }
            UpMsg::Tick(at) => {
                self.frame = (self.frame + 1) % SPINNER.len();
                self.state.at = at;
            }
            _ => {}
        }
        Flow::Continue
    }
    pub fn view(&self) -> Text<'static> {
        render_up(&self.state, SPINNER[self.frame])
    }
    pub fn err(&self) -> Option<&str> {
        self.state.err.as_deref()
    }
}
