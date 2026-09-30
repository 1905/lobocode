use crate::{
    app::{App, Io, load_cfg},
    cli::StatusArgs,
    tui::{
        run::run_status,
        status::render_status,
        styles::{to_ansi, to_plain},
    },
};
use lobo_core::control;
use std::{io::Write, path::Path};

pub async fn run(app: &App, a: &StatusArgs, path: &Path, io: &mut Io) -> anyhow::Result<()> {
    let cfg = load_cfg(path)?;
    control::check_providers(&cfg, app.local_supported)?;
    let deps = app.deps(cfg, path)?;
    if !a.json && !a.once && app.term.stdout_tty {
        return run_status(&deps, &app.cancel, app.term.no_color).await;
    }
    let snap = tokio::select! {
        biased;
        _ = app.cancel.cancelled() => return Err(lobo_core::Error::Cancelled.into()),
        result = control::snapshot(&deps) => result?,
    };
    if a.json {
        serde_json::to_writer(&mut io.out, &snap)?;
        writeln!(io.out)?;
    } else {
        let text = render_status(&snap, &chrono::Local);
        write!(
            io.out,
            "{}",
            if app.term.stdout_tty && !app.term.no_color {
                to_ansi(&text)
            } else {
                to_plain(&text)
            }
        )?;
    }
    Ok(())
}
