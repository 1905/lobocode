use crate::{
    app::{App, Io, load_cfg},
    cli::DownArgs,
};
use lobo_core::control;
use std::{io::Write, path::Path};

pub async fn run(app: &App, a: &DownArgs, path: &Path, io: &mut Io) -> anyhow::Result<()> {
    let cfg = load_cfg(path)?;
    control::check_providers(&cfg, app.local_supported)?;
    let deps = app.deps(cfg, path)?;
    if app.cancel.is_cancelled() {
        return Err(lobo_core::Error::Cancelled.into());
    }
    // Once deletion starts, keep ownership until its verification finishes.
    let spent = control::down(&deps).await?;
    if a.json {
        serde_json::to_writer(&mut io.out, &serde_json::json!({"spent_usd": spent}))?;
        writeln!(io.out)?;
    } else {
        tracing::info!(
            spent = format!("${spent:.2}").as_str(),
            "down: no lobo pods left"
        );
    }
    Ok(())
}
