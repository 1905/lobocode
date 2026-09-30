use crate::types::AppError;
use async_trait::async_trait;
use lobo_core::{config, control, local};
use lobo_proto::{ConfigShow, Listing, Readiness, Snap, UpRequest};
use local::memory::{self, MemoryAssessment, MemoryProbe};
use std::{collections::BTreeMap, path::PathBuf};
use tokio_util::sync::CancellationToken;

pub type Result<T> = std::result::Result<T, AppError>;
#[async_trait]
pub trait Backend: Send + Sync {
    fn config_path(&self) -> PathBuf;
    async fn config(&self) -> Result<(ConfigShow, Readiness)>;
    async fn models(&self) -> Result<Listing>;
    async fn snapshot(&self) -> Result<Snap>;
    async fn local_memory(&self, model: &str) -> Result<MemoryAssessment>;
    fn up(&self, req: UpRequest, cancel: CancellationToken) -> Result<control::UpOperation>;
    async fn down(&self) -> Result<f64>;
    async fn api_key(&self) -> Result<String>;
    async fn save(&self, set: BTreeMap<String, String>) -> Result<()>;
}
pub struct CoreBackend {
    pub path: PathBuf,
    pub wiring: control::Wiring,
    memory: MemoryProbe,
}
impl CoreBackend {
    pub fn new(path: PathBuf) -> std::io::Result<Self> {
        let path = std::path::absolute(path)?;
        let wiring =
            control::Wiring::new(path.clone(), local::Spawner::app(std::env::current_exe()?));
        Ok(Self {
            path,
            wiring,
            memory: std::sync::Arc::new(memory::snapshot),
        })
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
    fn local_options(&self, model: &str) -> Result<control::UpOpts> {
        let cfg = config::Laptop::from_values(&config::values(&self.path)?);
        self.resolve(
            &cfg,
            &UpRequest {
                provider: Some("local".into()),
                model: Some(model.into()),
                ..Default::default()
            },
        )
    }
    fn resolve(&self, cfg: &config::Laptop, req: &UpRequest) -> Result<control::UpOpts> {
        let mut opts = control::resolve_up(cfg, &self.path, req, self.wiring.supported)?;
        if opts.provider == "local" && opts.ctx == 0 {
            opts.ctx = lobo_core::release::DEFAULT_DEFAULTS.ctx;
        }
        Ok(opts)
    }
    fn probe(&self, display: bool) -> MemoryProbe {
        #[cfg(feature = "e2e")]
        if let Some(probe) = crate::e2e_memory::probe(&self.path, display) {
            return probe;
        }
        let _ = display;
        self.memory.clone()
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
    async fn local_memory(&self, model: &str) -> Result<MemoryAssessment> {
        let opts = self.local_options(model)?;
        let probe = self.probe(true);
        tokio::task::spawn_blocking(move || memory::inspect_with(&opts.model, opts.ctx, &probe))
            .await
            .map_err(|e| AppError {
                kind: "local".into(),
                message: format!("Mac memory check failed: {e}. Retry or use Cloud."),
            })?
            .map_err(AppError::from)
    }
    fn up(&self, req: UpRequest, cancel: CancellationToken) -> Result<control::UpOperation> {
        // Admit before constructing dependencies, creating files or starting workers.
        let cfg = config::load_laptop(&self.path)?;
        let opts = self.resolve(&cfg, &req)?;
        if opts.provider == "local" {
            memory::inspect_with(&opts.model, opts.ctx, &self.probe(false))?.ensure_fit()?;
        }
        let d = control::deps_from_config(cfg, &self.wiring)?;
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
    fn memory_backend(mode: usize) -> (tempfile::TempDir, CoreBackend) {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("config.env");
        std::fs::write(
            &path,
            "LOBO_API_KEY=fixture\nLOBO_PROVIDER=local\nLOBO_CTX=8192\n",
        )
        .unwrap();
        let mut backend = CoreBackend::new(path).unwrap();
        backend.wiring.supported = || Ok(());
        backend.memory = std::sync::Arc::new(move || {
            if mode == 2 {
                return Err(lobo_core::Error::Local("probe unavailable".into()));
            }
            Ok(memory::MemorySnapshot {
                total_bytes: 64 << 30,
                available_bytes: (if mode == 1 { 8 } else { 60 }) << 30,
                metal_limit_bytes: 48 << 30,
            })
        });
        (root, backend)
    }
    #[tokio::test]
    async fn memory_uses_selected_model_and_saved_context() {
        let (_root, backend) = memory_backend(0);
        let memory = backend.local_memory("q6").await.unwrap();
        assert_eq!((memory.model.as_str(), memory.ctx), ("q6", 8192));
        assert!(memory.fits());
        backend
            .save(BTreeMap::from([("LOBO_CTX".into(), "0".into())]))
            .await
            .unwrap();
        assert_eq!(
            backend.local_memory("q8").await.unwrap().ctx,
            lobo_core::release::DEFAULT_DEFAULTS.ctx
        );
        let (_root, backend) = memory_backend(1);
        assert!(!backend.local_memory("q8").await.unwrap().fits());
        let (_root, backend) = memory_backend(2);
        assert!(
            backend
                .local_memory("q8")
                .await
                .unwrap_err()
                .message
                .contains("probe unavailable")
        );
    }
    #[tokio::test]
    async fn fresh_start_denies_before_creating_an_operation() {
        let (root, mut backend) = memory_backend(0);
        assert!(backend.local_memory("q8").await.unwrap().fits());
        backend.memory = memory_backend(1).1.memory;
        let result = backend.up(
            UpRequest {
                provider: Some("local".into()),
                model: Some("q8".into()),
                ..Default::default()
            },
            CancellationToken::new(),
        );
        let Err(error) = result else {
            panic!("Start must deny");
        };
        assert!(error.message.contains("GiB"));
        assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 1);
        backend.memory = memory_backend(2).1.memory;
        assert!(
            backend
                .up(
                    UpRequest {
                        provider: Some("local".into()),
                        model: Some("q8".into()),
                        ..Default::default()
                    },
                    CancellationToken::new()
                )
                .is_err()
        );
    }
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
