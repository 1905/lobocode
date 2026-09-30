use crate::{app::App, cli::UpArgs};
use lobo_core::{
    config::Laptop,
    control::{self, UpOpts},
};
use std::{collections::BTreeSet, path::Path};

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
