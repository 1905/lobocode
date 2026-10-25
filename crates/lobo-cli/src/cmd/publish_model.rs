use crate::{
    app::{App, Io, load_cfg},
    cli::PublishModelArgs,
};
use lobo_core::release::{Store, models::publish_model};
use lobo_proto::catalog::{self, CHUNK_SIZE};
use std::{io::Write, path::Path};

pub async fn run(
    app: &App,
    args: &PublishModelArgs,
    path: &Path,
    io: &mut Io,
) -> anyhow::Result<()> {
    let cfg = load_cfg(path)?;
    cfg.require_r2()?;
    let store = Store::models(&cfg.r2)?;
    let model = catalog::get(&args.id)?;
    let mut note = |line: String| eprintln!("{line}");
    // Keys are immutable and a re-run skips finished chunks, so cancelling is safe.
    let table = tokio::select! {
        biased;
        _ = app.cancel.cancelled() => return Err(lobo_core::Error::Cancelled.into()),
        table = publish_model(&store, model, &args.file, CHUNK_SIZE, &mut note) => table?,
    };
    writeln!(io.out, "{}", serde_json::to_string(&table)?)?;
    Ok(())
}
