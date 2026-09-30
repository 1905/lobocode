use super::{Flow, styles::*};
use chrono::{TimeDelta, TimeZone};
use lobo_proto::{Snap, Stage};
use ratatui::{
    style::Style,
    text::{Line, Span, Text},
};
use std::fmt::Display;
fn text(s: impl Into<String>) -> Vec<Span<'static>> {
    vec![Span::raw(s.into())]
}
pub fn render_status<Tz: TimeZone>(s: &Snap, tz: &Tz) -> Text<'static>
where
    Tz::Offset: Display,
{
    let at =
        s.at.0
            .map(|t| t.with_timezone(tz).format("%H:%M:%S").to_string())
            .unwrap_or_else(|| "00:00:00".into());
    let mut lines = vec![Line::from(vec![
        span("lobo status", TITLE),
        dim(format!("  ·  {at}")),
    ])];
    let Some(p) = s.pod.as_ref().filter(|_| !s.down) else {
        lines.extend([
            Line::default(),
            Line::from(dim("lobo: down (no lobo instance on any provider)")),
        ]);
        return Text::from(lines);
    };
    let up = match (s.at.0, p.started_at.0) {
        (Some(at), Some(start)) => at - start,
        _ => TimeDelta::zero(),
    };
    let local = p.provider == "local";
    lines.extend([
        Line::default(),
        Line::from(span("Pod", TITLE)),
        row(
            "id",
            vec![
                Span::raw(format!("{}  ", p.id)),
                dim(format!("{} · {}", p.provider, p.status)),
            ],
        ),
    ]);
    lines.push(row(
        "cost",
        text(if local {
            format!("free  ·  up {}", dur(up))
        } else {
            format!(
                "${:.2}/h  ·  up {}  ·  spent ${:.2}",
                p.cost_per_hr,
                dur(up),
                p.cost_per_hr * up.num_milliseconds() as f64 / 3_600_000.0
            )
        }),
    ));
    lines.extend([Line::default(), Line::from(span("Release", TITLE))]);
    if let Some(v) = &s.version {
        let mut value = text(format!("{}  ·  git {}", v.version, v.git_sha));
        if v.git_dirty {
            value.push(span("  ⚠ dirty tree", Style::new().fg(WARN)));
        }
        lines.push(row("version", value));
        if !v.llama_image.is_empty() {
            lines.push(row("image", vec![dim(v.llama_image.clone())]));
        }
    } else {
        lines.push(row("version", vec![dim("n/a (agent not reachable yet)")]));
    }
    lines.extend([Line::default(), Line::from(span("Agent", TITLE))]);
    let Some(st) = &s.status else {
        lines.push(row(
            "stage",
            vec![dim("n/a (container booting or tunnel down)")],
        ));
        return Text::from(lines);
    };
    let color = match st.stage {
        Stage::Ready => OK,
        Stage::Failed | Stage::Terminating => ERR,
        _ => WARN,
    };
    lines.push(row(
        "stage",
        vec![
            span(format!("● {}", st.stage.as_str()), Style::new().fg(color)),
            Span::raw("  "),
            dim(format!("{} · ctx {}", st.model, st.ctx)),
        ],
    ));
    if !st.stage_detail.is_empty() {
        lines.push(row(
            "detail",
            vec![span(st.stage_detail.clone(), Style::new().fg(ERR))],
        ));
    }
    if st.stage == Stage::Download && st.download.total > 0 {
        let dl = &st.download;
        let f = dl.bytes as f64 / dl.total as f64;
        let mut value = bar(f, 24, Style::new().fg(OK));
        value.push(Span::raw(format!(
            " {:5.1}%  {} / {}  {:.0} MB/s",
            100.0 * f,
            gb(dl.bytes),
            gb(dl.total),
            dl.mbps
        )));
        lines.push(row("download", value));
    }
    lines.extend([
        Line::default(),
        Line::from(span(if local { "Mac" } else { "GPU" }, TITLE)),
    ]);
    let memory = |used: i64, total: i64| {
        let f = if total > 0 {
            used as f64 / total as f64
        } else {
            0.0
        };
        let mut v = bar(f, 24, load_style(f));
        v.push(Span::raw(format!(" {} / {} MB", num(used), num(total))));
        v
    };
    if let Some(g) = &st.gpu {
        lines.push(row("device", text(g.name.clone())));
        if local {
            lines.push(row("memory", memory(g.vram_used_mb, g.vram_total_mb)));
        } else {
            let f = g.util_pct as f64 / 100.0;
            let mut v = bar(f, 24, load_style(f));
            v.push(Span::raw(format!(" {:3}%", g.util_pct)));
            lines.push(row("load", v));
            lines.push(row("vram", memory(g.vram_used_mb, g.vram_total_mb)));
        }
    } else {
        lines.push(row("gpu", vec![dim("n/a")]));
    }
    if !local {
        lines.extend([Line::default(), Line::from(span("Host", TITLE))]);
        if let Some(h) = &st.host {
            lines.push(row(
                "cpu load",
                vec![
                    Span::raw(format!("{:.2}  {:.2}  {:.2}  ", h.load1, h.load5, h.load15)),
                    dim("(1/5/15 min)"),
                ],
            ));
            lines.push(row("ram", memory(h.mem_used_mb, h.mem_total_mb)));
        } else {
            lines.push(row("host", vec![dim("n/a")]));
        }
    }
    lines.extend([
        Line::default(),
        Line::from(vec![
            span("LLM", TITLE),
            dim("  (totals since llama-server start)"),
        ]),
    ]);
    if let Some(l) = &st.llama {
        lines.push(row(
            "requests",
            text(format!(
                "{} running · {} queued",
                l.requests_processing, l.requests_deferred
            )),
        ));
        lines.push(row(
            "tokens in",
            vec![
                Span::raw(num(l.prompt_tokens_total)),
                dim(format!("  ·  {:.0} tok/s prompt", l.prompt_tps)),
            ],
        ));
        lines.push(row(
            "tokens out",
            vec![
                Span::raw(num(l.gen_tokens_total)),
                dim(format!("  ·  {:.1} tok/s gen", l.gen_tps)),
            ],
        ));
    } else {
        lines.push(row("llama", vec![dim("n/a")]));
    }
    lines.extend([Line::default(), Line::from(span("Watchdog", TITLE))]);
    if st.stage == Stage::Ready {
        lines.push(row("idle", text(dur(secs(st.idle_s as f64)))));
    }
    let kill = format!("in {} ({})", dur(secs(st.kill_in_s as f64)), st.kill_reason);
    lines.push(row(
        "auto-kill",
        vec![span(
            kill,
            if st.kill_in_s < 300 {
                Style::new().fg(WARN)
            } else {
                Style::default()
            },
        )],
    ));
    if !local {
        lines.push(row(
            "expires",
            text(
                st.expires_at
                    .0
                    .map(|t| t.with_timezone(tz).format("%H:%M %a").to_string())
                    .unwrap_or_else(|| "00:00 Mon".into()),
            ),
        ));
    }
    if st.metrics_failures > 0 {
        lines.push(row(
            "metrics",
            vec![span(
                format!("⚠ {} failed reads in a row", st.metrics_failures),
                Style::new().fg(WARN),
            )],
        ));
    }
    Text::from(lines)
}
// Reducer inputs are consumed immediately, not retained in a queue.
#[allow(clippy::large_enum_variant)]
pub enum StatusMsg {
    Snap(Result<Snap, String>),
    Key(crossterm::event::KeyEvent),
}
#[derive(Default)]
pub struct StatusModel {
    pub snap: Option<Snap>,
    pub err: Option<String>,
}
impl StatusModel {
    pub fn update(&mut self, m: StatusMsg) -> Flow {
        match m {
            StatusMsg::Snap(Ok(s)) => {
                let down = s.down;
                self.snap = Some(s);
                self.err = None;
                if down {
                    return Flow::Quit;
                }
            }
            StatusMsg::Snap(Err(e)) => self.err = Some(e),
            StatusMsg::Key(k) if super::quit_key(k, true) => return Flow::Quit,
            _ => {}
        }
        Flow::Continue
    }
    pub fn view<Tz: TimeZone>(&self, tz: &Tz) -> Text<'static>
    where
        Tz::Offset: Display,
    {
        let Some(s) = &self.snap else {
            return Text::from(if let Some(e) = &self.err {
                vec![
                    Line::from(span(format!("status failed: {e}"), Style::new().fg(ERR))),
                    Line::from(dim("retrying every 2s · q quit")),
                ]
            } else {
                vec![Line::from(dim("loading…"))]
            });
        };
        let mut out = render_status(s, tz);
        if let Some(e) = &self.err {
            out.lines.extend([
                Line::default(),
                Line::from(span(format!("refresh failed: {e}"), Style::new().fg(ERR))),
            ]);
        }
        out.lines
            .extend([Line::default(), Line::from(dim("q quit · refresh 2s"))]);
        out
    }
}
