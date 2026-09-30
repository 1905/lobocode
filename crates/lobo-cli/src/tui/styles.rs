use chrono::TimeDelta;
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
};
use unicode_width::UnicodeWidthStr;

pub const ACCENT: Color = Color::Rgb(125, 86, 244);
pub const OK: Color = Color::Rgb(63, 185, 80);
pub const WARN: Color = Color::Rgb(210, 153, 34);
pub const ERR: Color = Color::Rgb(248, 81, 73);
pub const DIM: Color = Color::Rgb(139, 148, 158);
pub const TITLE: Style = Style::new().fg(ACCENT).add_modifier(Modifier::BOLD);
pub const BOLD: Style = Style::new().add_modifier(Modifier::BOLD);
pub fn span(s: impl Into<String>, style: Style) -> Span<'static> {
    Span::styled(s.into(), style)
}
pub fn dim(s: impl Into<String>) -> Span<'static> {
    span(s, Style::new().fg(DIM))
}
pub fn bar(frac: f64, width: usize, style: Style) -> Vec<Span<'static>> {
    let full = (frac.clamp(0.0, 1.0) * width as f64 + 0.5) as usize;
    vec![span("█".repeat(full), style), dim("░".repeat(width - full))]
}
pub fn load_style(frac: f64) -> Style {
    Style::new().fg(if frac >= 0.95 {
        ERR
    } else if frac >= 0.8 {
        WARN
    } else {
        OK
    })
}
fn rounded_seconds(d: TimeDelta) -> i64 {
    let secs = d.num_seconds();
    let remainder = d - TimeDelta::seconds(secs);
    secs + if remainder >= TimeDelta::milliseconds(500) {
        1
    } else if remainder <= TimeDelta::milliseconds(-500) {
        -1
    } else {
        0
    }
}
pub fn dur(d: TimeDelta) -> String {
    let s = rounded_seconds(d);
    if s >= 3600 {
        format!("{}h{:02}m", s / 3600, s / 60 % 60)
    } else if s >= 60 {
        format!("{}m{:02}s", s / 60, s % 60)
    } else {
        format!("{s}s")
    }
}
pub fn clock(d: TimeDelta) -> String {
    let s = rounded_seconds(d);
    format!("{}:{:02}", s / 60, s % 60)
}
pub fn num(n: i64) -> String {
    let raw = n.to_string();
    let mut out = String::new();
    for (i, c) in raw.chars().enumerate() {
        if i > 0 && (raw.len() - i).is_multiple_of(3) && raw.as_bytes()[i - 1] != b'-' {
            out.push(',');
        }
        out.push(c);
    }
    out
}
pub fn gb(bytes: i64) -> String {
    format!("{:.1} GB", bytes as f64 / 1e9)
}
pub fn secs(s: f64) -> TimeDelta {
    TimeDelta::nanoseconds((s * 1e9) as i64)
}
pub fn row(k: &str, mut v: Vec<Span<'static>>) -> Line<'static> {
    let mut out = vec![dim(format!(
        "{k}{}",
        " ".repeat(14usize.saturating_sub(k.width()))
    ))];
    out.append(&mut v);
    Line::from(out)
}
pub fn pad(mut spans: Vec<Span<'static>>, width: usize) -> Vec<Span<'static>> {
    let len: usize = spans.iter().map(Span::width).sum();
    spans.push(Span::raw(" ".repeat(width.saturating_sub(len).max(1))));
    spans
}
pub fn boxed(lines: Vec<Line<'static>>, border: Color) -> Vec<Line<'static>> {
    let width = lines.iter().map(Line::width).max().unwrap_or(0);
    let style = Style::new().fg(border);
    let mut out = vec![Line::from(span(
        format!("╭{}╮", "─".repeat(width + 2)),
        style,
    ))];
    for line in lines {
        let len = line.width();
        let mut spans = vec![span("│ ", style)];
        spans.extend(line.spans);
        spans.push(span(format!("{} │", " ".repeat(width - len)), style));
        out.push(Line::from(spans));
    }
    out.push(Line::from(span(
        format!("╰{}╯", "─".repeat(width + 2)),
        style,
    )));
    out
}
pub fn to_plain(text: &Text<'_>) -> String {
    let mut out = String::new();
    for line in &text.lines {
        for span in &line.spans {
            out.push_str(&span.content);
        }
        out.push('\n');
    }
    out
}
pub fn to_ansi(text: &Text<'_>) -> String {
    let mut out = String::new();
    for line in &text.lines {
        for span in &line.spans {
            let style = text.style.patch(line.style).patch(span.style);
            let mut codes = Vec::new();
            if style.add_modifier.contains(Modifier::BOLD) {
                codes.push("1".into());
            }
            if let Some(Color::Rgb(r, g, b)) = style.fg {
                codes.push(format!("38;2;{r};{g};{b}"));
            }
            if codes.is_empty() {
                out.push_str(&span.content);
            } else {
                out.push_str(&format!("\x1b[{}m{}\x1b[0m", codes.join(";"), span.content));
            }
        }
        out.push('\n');
    }
    out
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn durations_numbers_and_bars() {
        for (s, want) in [
            (0, "0s"),
            (59, "59s"),
            (60, "1m00s"),
            (312, "5m12s"),
            (5400, "1h30m"),
            (42960, "11h56m"),
        ] {
            assert_eq!(dur(TimeDelta::seconds(s)), want);
        }
        assert_eq!(dur(TimeDelta::milliseconds(500)), "1s");
        assert_eq!(dur(TimeDelta::milliseconds(-500)), "-1s");
        assert_eq!(clock(TimeDelta::seconds(5)), "0:05");
        assert_eq!(clock(TimeDelta::seconds(360)), "6:00");
        for (n, want) in [
            (0, "0"),
            (999, "999"),
            (1000, "1,000"),
            (182340, "182,340"),
            (-100, "-100"),
            (-1000, "-1,000"),
        ] {
            assert_eq!(num(n), want);
        }
        assert_eq!(gb(12_400_000_000), "12.4 GB");
        assert_eq!(
            to_plain(&Text::from(Line::from(bar(0.432, 24, BOLD)))),
            format!("{}{}\n", "█".repeat(10), "░".repeat(14))
        );
        assert_eq!(
            to_plain(&Text::from(boxed(
                vec![Line::from("a"), Line::from("long")],
                ACCENT
            ))),
            "╭──────╮\n│ a    │\n│ long │\n╰──────╯\n"
        );
    }
}
