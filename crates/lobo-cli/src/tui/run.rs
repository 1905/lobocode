use super::{
    Flow,
    status::{StatusModel, StatusMsg},
    up::{UpModel, UpMsg},
};
use chrono::{Local, TimeZone};
use crossterm::{
    cursor,
    event::{self, Event},
    execute,
    terminal::{self, Clear, ClearType},
};
use lobo_core::{
    clock::Clock,
    control::{self, Deps},
};
use lobo_proto::{ReadyInfo, UpEvent};
use ratatui::{Frame, Terminal, TerminalOptions, Viewport, backend::CrosstermBackend, text::Text};
use std::{
    fmt::Display,
    io::{Stdout, Write},
    sync::Arc,
    time::Duration,
};
use tokio::{
    sync::mpsc,
    time::{Instant, MissedTickBehavior},
};
use tokio_util::sync::CancellationToken;

/// Owns raw mode even when terminal initialization or drawing fails.
struct Inline {
    terminal: Option<Terminal<CrosstermBackend<Stdout>>>,
    height: u16,
    last_row: Option<u16>,
    no_color: bool,
}
impl Inline {
    fn new(no_color: bool) -> std::io::Result<Self> {
        terminal::enable_raw_mode()?;
        Ok(Self {
            terminal: None,
            height: 0,
            last_row: None,
            no_color,
        })
    }
    fn draw(&mut self, mut text: Text<'static>) -> std::io::Result<()> {
        let height = u16::try_from(text.lines.len()).unwrap_or(u16::MAX).max(1);
        if self.terminal.is_none() || self.height != height {
            if let Some(mut old) = self.terminal.take() {
                old.autoresize()?;
                let area = old.get_frame().area();
                execute!(
                    std::io::stdout(),
                    cursor::MoveTo(area.x, area.y),
                    Clear(ClearType::FromCursorDown)
                )?;
            }
            self.terminal = Some(Terminal::with_options(
                CrosstermBackend::new(std::io::stdout()),
                TerminalOptions {
                    viewport: Viewport::Inline(height),
                },
            )?);
            self.height = height;
        }
        if self.no_color {
            text.style = Default::default();
            for line in &mut text.lines {
                line.style = Default::default();
                for span in &mut line.spans {
                    span.style = Default::default();
                }
            }
        }
        let terminal = self.terminal.as_mut().unwrap();
        let frame = terminal.draw(|frame| frame.render_widget(text, frame.area()))?;
        self.last_row = Some(frame.area.bottom().saturating_sub(1));
        Ok(())
    }
}
impl Drop for Inline {
    fn drop(&mut self) {
        // Leave the final dashboard in scrollback and put the prompt below it.
        self.terminal.take();
        let mut out = std::io::stdout();
        if let Some(row) = self.last_row {
            let _ = execute!(out, cursor::MoveTo(0, row));
        }
        let _ = execute!(out, cursor::Show);
        let _ = terminal::disable_raw_mode();
        let _ = write!(out, "\r\n");
        let _ = out.flush();
    }
}
pub fn draw_up(frame: &mut Frame, model: &UpModel) {
    frame.render_widget(model.view(), frame.area());
}
pub fn draw_status<Tz: TimeZone>(frame: &mut Frame, model: &StatusModel, tz: &Tz)
where
    Tz::Offset: Display,
{
    frame.render_widget(model.view(tz), frame.area());
}

pub async fn run_up(
    rx: &mut mpsc::Receiver<UpEvent>,
    ready: &mut Option<ReadyInfo>,
    clock: Arc<dyn Clock>,
    cancel: &CancellationToken,
    no_color: bool,
) -> anyhow::Result<()> {
    let mut terminal = Inline::new(no_color)?;
    let mut model = UpModel::new();
    let mut tick = tokio::time::interval(Duration::from_millis(100));
    tick.set_missed_tick_behavior(MissedTickBehavior::Skip);
    terminal.draw(model.view())?;
    loop {
        let flow = tokio::select! {
            biased;
            _ = cancel.cancelled() => return Err(lobo_core::Error::Cancelled.into()),
            event = rx.recv() => {
                if let Some(info) = event.as_ref().and_then(|e| e.ready.as_ref()) {*ready = Some(info.clone());}
                model.update(UpMsg::Event(event), clock.now())
            }
            _ = tick.tick() => {
                let mut flow = Flow::Continue;
                // poll(0) + read stay together. No detached input task survives this command.
                while event::poll(Duration::ZERO)? {
                    if let Event::Key(key) = event::read()? {flow = model.update(UpMsg::Key(key), clock.now()); if flow == Flow::Quit {break;}}
                }
                if flow != Flow::Quit {model.update(UpMsg::Tick(clock.now()), clock.now());}
                flow
            }
        };
        terminal.draw(model.view())?;
        if flow == Flow::Quit {
            break;
        }
    }
    if model.interrupted {
        return Err(lobo_core::Error::Cancelled.into());
    }
    if let Some(err) = model.err() {
        anyhow::bail!("{err}");
    }
    Ok(())
}

pub async fn run_status(
    deps: &Deps,
    cancel: &CancellationToken,
    no_color: bool,
) -> anyhow::Result<()> {
    let mut terminal = Inline::new(no_color)?;
    let mut model = StatusModel::default();
    let mut tick = tokio::time::interval(Duration::from_millis(100));
    tick.set_missed_tick_behavior(MissedTickBehavior::Skip);
    let mut request = Box::pin(control::snapshot(deps));
    let mut fetching = true;
    let mut next_fetch = Instant::now();
    terminal.draw(model.view(&Local))?;
    loop {
        let flow = tokio::select! {
            biased;
            _ = cancel.cancelled() => return Ok(()),
            snap = &mut request, if fetching => {
                fetching = false;
                next_fetch = Instant::now() + Duration::from_secs(2);
                model.update(StatusMsg::Snap(snap.map_err(|e|e.to_string())))
            }
            _ = tick.tick() => {
                let mut flow = Flow::Continue;
                while event::poll(Duration::ZERO)? {
                    if let Event::Key(key) = event::read()? {flow = model.update(StatusMsg::Key(key)); if flow == Flow::Quit {break;}}
                }
                if !fetching && Instant::now() >= next_fetch {request = Box::pin(control::snapshot(deps)); fetching = true;}
                flow
            }
        };
        terminal.draw(model.view(&Local))?;
        if flow == Flow::Quit {
            return Ok(());
        }
    }
}
