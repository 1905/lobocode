use crate::app::Io;
use lobo_core::{
    config::{self, Laptop},
    genkey,
};
use std::{io::Write, path::Path};

pub fn run(path: &Path, rotate: bool, io: &mut Io) -> anyhow::Result<()> {
    let (key, written) = genkey::ensure_api_key(path, rotate)?;
    if written {
        tracing::info!(
            "new LOBO_API_KEY written to the config (a running pod keeps the old key until the next `lobo up`)"
        );
    } else {
        tracing::info!("keeping existing LOBO_API_KEY (use --rotate for a new one)");
    }
    let cfg = Laptop::from_values(&config::values(path)?);
    if cfg.providers().is_empty() && cfg.domain.is_empty() {
        tracing::info!("no cloud provider configured: writing only the local provider");
    }
    let output = Path::new(genkey::OPENCODE_OUT);
    genkey::write_opencode_for_laptop(output, &cfg, &key)?;
    writeln!(io.out, "wrote {}", std::path::absolute(output)?.display())?;
    Ok(())
}
