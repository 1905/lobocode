use crate::types::AppError;
use async_trait::async_trait;
use lobo_core::{config, control, local};
use lobo_proto::{ConfigShow, Listing, Readiness, Snap, UpRequest};
use std::{collections::BTreeMap, path::PathBuf};
use tokio_util::sync::CancellationToken;

pub type Result<T> = std::result::Result<T, AppError>;
#[async_trait]
pub trait Backend: Send + Sync {
    fn config_path(&self) -> PathBuf;
    async fn config(&self) -> Result<(ConfigShow, Readiness)>;
    async fn models(&self) -> Result<Listing>;
    async fn snapshot(&self) -> Result<Snap>;
    fn up(&self, req: UpRequest, cancel: CancellationToken) -> Result<control::UpOperation>;
    async fn down(&self) -> Result<f64>;
    async fn api_key(&self) -> Result<String>;
    async fn save(&self, set: BTreeMap<String, String>) -> Result<()>;
}
pub struct CoreBackend {
    pub path: PathBuf,
    pub wiring: control::Wiring,
}
impl CoreBackend {
    pub fn new(path: PathBuf) -> std::io::Result<Self> {
        let path = std::path::absolute(path)?;
        let wiring =
            control::Wiring::new(path.clone(), local::Spawner::app(std::env::current_exe()?));
        Ok(Self { path, wiring })
    }
    pub fn configured_path() -> PathBuf {
        std::env::var_os("LOBO_APP_CONFIG")
            .filter(|s| !s.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(config::default_path)
    }
    fn deps(&self) -> Result<control::Deps> {
        Ok(control::deps_from_config(
            config::load_laptop(&self.path)?,
            &self.wiring,
        )?)
    }
}
#[async_trait]
impl Backend for CoreBackend {
    fn config_path(&self) -> PathBuf {
        self.path.clone()
    }
    async fn config(&self) -> Result<(ConfigShow, Readiness)> {
        Ok((config::show(&self.path)?, config::readiness(&self.path)))
    }
    async fn models(&self) -> Result<Listing> {
        // Listing files does not need an API key. Settings also works before setup.
        let cfg = config::Laptop::from_values(&config::values(&self.path)?);
        Ok(local::list(&cfg.weights())?)
    }
    async fn snapshot(&self) -> Result<Snap> {
        Ok(control::snapshot(&self.deps()?).await?)
    }
    fn up(&self, req: UpRequest, cancel: CancellationToken) -> Result<control::UpOperation> {
        let mut d = self.deps()?;
        let opts = control::resolve_up(&d.cfg, &self.path, &req, self.wiring.supported)?;
        if opts.provider == "local" {
            d.presign = None;
        }
        Ok(control::up(d, opts, cancel))
    }
    async fn down(&self) -> Result<f64> {
        Ok(control::down(&self.deps()?).await?)
    }
    async fn api_key(&self) -> Result<String> {
        Ok(config::values(&self.path)?
            .remove("LOBO_API_KEY")
            .unwrap_or_default())
    }
    async fn save(&self, set: BTreeMap<String, String>) -> Result<()> {
        config::validate_set(&set).map_err(|message| AppError {
            kind: "invalid".into(),
            message,
        })?;
        config::save(&self.path, &set)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn save_validates_before_writing_and_preserves_comments() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("override.env");
        std::fs::write(&p, "# keep\nLOBO_API_KEY=fixture\n").unwrap();
        let b = CoreBackend::new(p.clone()).unwrap();
        let e = b
            .save(BTreeMap::from([("LOBO_LOCAL_PORT".into(), "80".into())]))
            .await
            .unwrap_err();
        assert_eq!(e.kind, "invalid");
        assert_eq!(
            e.message,
            "LOBO_LOCAL_PORT: whole number 1024-65534, or empty"
        );
        assert_eq!(
            std::fs::read_to_string(&p).unwrap(),
            "# keep\nLOBO_API_KEY=fixture\n"
        );
        b.save(BTreeMap::from([("LOBO_LOCAL_PORT".into(), "9000".into())]))
            .await
            .unwrap();
        assert!(std::fs::read_to_string(&p).unwrap().starts_with("# keep\n"));
        assert_eq!(b.config().await.unwrap().1.local_port, 9000);
        assert_eq!(b.wiring.config_path, p);
        assert_eq!(
            b.wiring.spawner.args_prefix,
            [local::SUPERVISOR_ARG, "local", "run"]
        );
    }
}
