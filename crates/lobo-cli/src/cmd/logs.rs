use crate::{
    app::{App, Io, load_cfg},
    cli::LogsArgs,
};
use lobo_core::control;
use std::{io::Write, path::Path};

pub async fn run(app: &App, a: &LogsArgs, path: &Path, io: &mut Io) -> anyhow::Result<()> {
    let deps = app.deps(load_cfg(path)?, path)?;
    let request = async {
        let (agent, _) = control::target(&deps).await?;
        // Nonpositive Go counts select the agent's default. Zero preserves that
        // behavior through the shared unsigned AgentApi boundary.
        agent.logs(usize::try_from(a.n).unwrap_or(0)).await
    };
    let logs = tokio::select! {
        biased;
        _ = app.cancel.cancelled() => return Err(lobo_core::Error::Cancelled.into()),
        result = request => result?,
    };
    write!(io.out, "{logs}")?;
    Ok(())
}
