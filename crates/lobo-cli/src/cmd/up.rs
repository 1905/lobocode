use crate::{app::App, cli::UpArgs};
use lobo_core::{
    config::Laptop,
    control::{self, UpOpts},
};
use lobo_proto::{ReadyInfo, UpEvent};
use std::io::Write;
use std::{collections::BTreeSet, path::Path};
use tokio::sync::mpsc;

#[derive(Debug)]
struct EventFailure(String);
impl std::fmt::Display for EventFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for EventFailure {}

pub async fn json_up(
    rx: &mut mpsc::Receiver<UpEvent>,
    out: &mut dyn Write,
    ready: &mut Option<ReadyInfo>,
) -> anyhow::Result<()> {
    let mut failed = false;
    while let Some(e) = rx.recv().await {
        if let Some(info) = &e.ready {
            *ready = Some(info.clone());
        }
        failed |= e.err.is_some();
        serde_json::to_writer(&mut *out, &e)?;
        writeln!(out)?;
        out.flush()?;
    }
    if failed {
        return Err(EventFailure("up failed".into()).into());
    }
    Ok(())
}
pub async fn plain_up(
    rx: &mut mpsc::Receiver<UpEvent>,
    ready: &mut Option<ReadyInfo>,
) -> anyhow::Result<()> {
    let mut last = None;
    while let Some(e) = rx.recv().await {
        if let Some(info) = &e.ready {
            *ready = Some(info.clone());
        }
        if let Some(err) = e.err {
            tracing::error!(phase = e.phase.as_str(), "{err}");
            last = Some(err);
            continue;
        }
        let detail = (!e.detail.is_empty()).then_some(e.detail.as_str());
        let download = e.download.as_ref().filter(|d| d.total > 0).map(|d| {
            format!(
                "{:.1}% {:.0} MB/s",
                100.0 * d.bytes as f64 / d.total as f64,
                d.mbps
            )
        });
        let r = e.ready.as_ref();
        tracing::info!(
            phase = e.phase.as_str(),
            detail,
            download = download.as_deref(),
            url = r.map(|r| r.url.as_str()),
            release = r.map(|r| r.version.as_str()),
            git_sha = r.map(|r| r.git_sha.as_str()),
            usd_per_h = r.map(|r| r.cost_per_hr),
            boot = r.map(|r| ((r.elapsed_ns as f64 / 1e9).round() * 1000.0) as i64),
            "up"
        );
    }
    if let Some(err) = last {
        return Err(EventFailure(err).into());
    }
    Ok(())
}

pub async fn run(
    app: &App,
    a: &UpArgs,
    changed: &BTreeSet<String>,
    path: &Path,
    io: &mut crate::app::Io,
) -> anyhow::Result<()> {
    let cfg = crate::app::load_cfg(path)?;
    let opts = prepare(app, a, changed, &cfg, path)?;
    let deps = app.deps(cfg, path)?;
    let source = opts.source.clone();
    let conns = opts.conns;
    let mut operation = control::up(deps, opts, app.cancel.child_token());
    let mut rx = operation.take_events().expect("new operation has events");
    let mut ready = None;
    let consumed = if a.json {
        json_up(&mut rx, &mut io.out, &mut ready).await
    } else if a.plain || !app.term.stdout_tty {
        plain_up(&mut rx, &mut ready).await
    } else {
        crate::tui::run::run_up(
            &mut rx,
            &mut ready,
            app.clock.clone(),
            &app.cancel,
            app.term.no_color,
        )
        .await
    };
    if consumed.is_err() {
        operation.cancel();
    }
    // A quit or broken output must not detach a rent or stop its cleanup.
    while let Some(event) = rx.recv().await {
        if let Some(info) = event.ready {
            ready = Some(info);
        }
    }
    let completed = operation.wait().await;
    let report = crate::bootlog::report_boot(
        &mut io.err,
        ready.as_ref(),
        &source,
        conns,
        &app.boot_log,
        app.clock.now(),
    );
    match completed {
        Err(lobo_core::Error::Cancelled) => {
            if let Err(e) = consumed {
                let cancelled = matches!(
                    e.downcast_ref::<lobo_core::Error>(),
                    Some(lobo_core::Error::Cancelled)
                );
                if !cancelled && e.downcast_ref::<EventFailure>().is_none() {
                    return Err(e);
                }
            }
            anyhow::bail!("interrupted: startup cancelled and cleanup completed");
        }
        Err(e) => {
            // The terminal JSON event carries ordinary boot failures. Cleanup and
            // unresolved creates stay explicit on stderr as well.
            if a.json
                && !matches!(
                    e,
                    lobo_core::Error::Multi(_) | lobo_core::Error::UnresolvedCreate { .. }
                )
                && consumed
                    .as_ref()
                    .err()
                    .is_some_and(|e| e.downcast_ref::<EventFailure>().is_some())
            {
                return consumed;
            }
            return Err(e.into());
        }
        Ok(()) => consumed?,
    }
    report?;
    Ok(())
}

pub fn up_opts(a: &UpArgs) -> UpOpts {
    UpOpts {
        model: if a.q6 { "q6" } else { "" }.into(),
        ctx: a.ctx,
        release: a.release.clone(),
        idle_min: a.idle_min,
        max_life: a.max_life,
        source: a.source.clone(),
        conns: a.conns,
        cloud: a.cloud.clone(),
        provider: a.provider.clone(),
        min_mbps: a.min_mbps,
        image: a.image.clone(),
        ..Default::default()
    }
}
pub fn set_fn(changed: &BTreeSet<String>) -> impl Fn(&str) -> bool + '_ {
    |name| changed.contains(name)
}
pub fn prepare(
    app: &App,
    a: &UpArgs,
    changed: &BTreeSet<String>,
    cfg: &Laptop,
    path: &Path,
) -> anyhow::Result<UpOpts> {
    if a.cloud != "secure" && a.cloud != "community" {
        anyhow::bail!("--cloud: want secure or community, got {:?}", a.cloud);
    }
    let mut opts = up_opts(a);
    control::apply_defaults(&mut opts, cfg, &set_fn(changed), path)?;
    control::check_target(cfg, &opts.provider, app.local_supported)?;
    if !a.ssh.is_empty() {
        opts.ssh_key = std::fs::read_to_string(&a.ssh)?.trim().into();
    }
    Ok(opts)
}
