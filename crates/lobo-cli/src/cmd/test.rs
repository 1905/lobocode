use crate::app::{App, Io, load_cfg};
use anyhow::Context;
use lobo_core::{checks, control};
use std::{io::Write, path::Path, time::Instant};

pub fn preview(text: &str) -> String {
    if text.len() <= 200 {
        return text.into();
    }
    let mut end = 200;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &text[..end])
}
fn took_ms(start: Instant) -> u64 {
    (start.elapsed().as_secs_f64() * 1000.0).round() as u64
}

pub async fn run(app: &App, path: &Path, io: &mut Io) -> anyhow::Result<()> {
    let cfg = load_cfg(path)?;
    let deps = app.deps(cfg.clone(), path)?;
    // Match Go's default client: requests end on completion or cancellation.
    let http = reqwest::Client::builder()
        .user_agent(lobo_core::http::USER_AGENT)
        .build()?;
    let request = async {
        let (agent, base) = control::target(&deps).await?;
        let version = agent
            .version()
            .await
            .with_context(|| format!("lobo not reachable at {base}"))?;
        let status = agent.status().await.ok();
        let id = status
            .as_ref()
            .map(|s| s.model.as_str())
            .filter(|s| !s.is_empty())
            .unwrap_or(lobo_core::release::DEFAULT_MODEL);
        let model = lobo_proto::catalog::get(id)?;
        let start = Instant::now();
        let text = checks::chat(&http, &base, &cfg.lobo_api_key, &model.alias)
            .await
            .context("chat")?;
        tracing::info!(
            took = took_ms(start),
            release = version.version.as_str(),
            "✓ streamed chat"
        );
        writeln!(io.out, "{}", preview(&text))?;
        let start = Instant::now();
        let body = checks::tool_call(&http, &base, &cfg.lobo_api_key, &model.alias)
            .await
            .context("tool call")?;
        if let Err(err) = checks::validate_tool_call(&body) {
            anyhow::bail!("{err}\n{}", String::from_utf8_lossy(&body));
        }
        tracing::info!(
            took = took_ms(start),
            "✓ tool call: arguments is a JSON string"
        );
        anyhow::Ok(())
    };
    tokio::select! {
        biased;
        _ = app.cancel.cancelled() => Err(lobo_core::Error::Cancelled.into()),
        result = request => result,
    }
}
