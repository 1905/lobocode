use crate::types::AppError;
use async_trait::async_trait;
use lobo_core::{config, control, local};
use lobo_proto::{ConfigShow, Listing, Readiness, Snap, Status, UpRequest};
use local::memory::{self, MemoryAssessment, MemoryProbe};
use std::{
    collections::BTreeMap,
    io::Write,
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    sync::{Arc, Mutex},
};
use tokio_util::sync::CancellationToken;

pub type Result<T> = std::result::Result<T, AppError>;
// Captured config and admitted options stay in Rust, never IPC or logs.
pub struct PreparedUp {
    pub(crate) config: config::Laptop,
    pub(crate) options: control::UpOpts,
}
#[async_trait]
pub trait Backend: Send + Sync {
    fn config_path(&self) -> PathBuf;
    async fn config(&self) -> Result<(ConfigShow, Readiness)>;
    async fn models(&self) -> Result<Listing>;
    #[allow(dead_code)] // Rust convenience API; app polling needs the paired private owner.
    async fn snapshot(&self, provider: &str) -> Result<Snap> {
        let expected = self.load_owner()?;
        let (owner, snap) = self.snapshot_owned(provider, expected.clone()).await?;
        if let Some(owner) = owner {
            self.adopt_owner(expected, owner)?;
        }
        Ok(snap)
    }
    fn load_owner(&self) -> Result<Option<control::RuntimeTarget>>;
    fn adopt_owner(
        &self,
        expected: Option<control::RuntimeTarget>,
        target: control::RuntimeTarget,
    ) -> Result<()>;
    async fn snapshot_owned(
        &self,
        provider: &str,
        captured: Option<control::RuntimeTarget>,
    ) -> Result<(Option<control::RuntimeTarget>, Snap)>;
    async fn local_memory(&self, model: &str) -> Result<MemoryAssessment>;
    fn prepare_up(&self, req: UpRequest) -> Result<PreparedUp>;
    fn up(
        &self,
        prepared: PreparedUp,
        previous: Option<control::RuntimeTarget>,
        cancel: CancellationToken,
        owner: control::OwnerSink,
    ) -> Result<control::UpOperation>;
    async fn down(&self, target: control::RuntimeTarget) -> Result<f64>;
    #[allow(dead_code)] // The telemetry task will consume this scoped interface.
    async fn telemetry(&self, target: control::RuntimeTarget) -> Result<Status>;
    async fn api_key(&self) -> Result<String>;
    async fn save(&self, set: BTreeMap<String, String>) -> Result<()>;
}
pub struct CoreBackend {
    pub path: PathBuf,
    pub wiring: control::Wiring,
    memory: MemoryProbe,
    owner_lock: Arc<Mutex<()>>,
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
            owner_lock: Arc::new(Mutex::new(())),
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
    fn owner_path(&self) -> PathBuf {
        self.path.with_extension("app-runtime.json")
    }
    fn clear_owner(&self, target: &control::RuntimeTarget) -> Result<()> {
        let _guard = self.owner_lock.lock().unwrap();
        if read_owner(&self.owner_path())?.as_ref() == Some(target) {
            std::fs::remove_file(self.owner_path()).map_err(owner_error)?;
        }
        Ok(())
    }
}
fn owner_error(error: impl std::fmt::Display) -> AppError {
    AppError {
        kind: "ownership".into(),
        message: format!("Runtime ownership: {error}"),
    }
}
fn read_owner(path: &std::path::Path) -> Result<Option<control::RuntimeTarget>> {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(owner_error(e)),
    };
    let target: control::RuntimeTarget = serde_json::from_slice(&bytes).map_err(owner_error)?;
    if target.boot_id.is_empty() || target.provider.is_empty() {
        return Err(owner_error("runtime identity is missing"));
    }
    Ok(Some(target))
}
fn write_owner(path: &std::path::Path, target: &control::RuntimeTarget) -> Result<()> {
    if target.boot_id.is_empty() || target.provider.is_empty() {
        return Err(owner_error("runtime identity is missing"));
    }
    let parent = path
        .parent()
        .ok_or_else(|| owner_error("missing parent directory"))?;
    std::fs::create_dir_all(parent).map_err(owner_error)?;
    let mut file = tempfile::NamedTempFile::new_in(parent).map_err(owner_error)?;
    file.as_file()
        .set_permissions(std::fs::Permissions::from_mode(0o600))
        .map_err(owner_error)?;
    file.write_all(&serde_json::to_vec(target).map_err(owner_error)?)
        .map_err(owner_error)?;
    file.as_file().sync_all().map_err(owner_error)?;
    file.persist(path).map_err(owner_error)?;
    std::fs::File::open(parent)
        .and_then(|dir| dir.sync_all())
        .map_err(owner_error)?;
    Ok(())
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
    fn load_owner(&self) -> Result<Option<control::RuntimeTarget>> {
        let _guard = self.owner_lock.lock().unwrap();
        read_owner(&self.owner_path())
    }
    fn adopt_owner(
        &self,
        expected: Option<control::RuntimeTarget>,
        target: control::RuntimeTarget,
    ) -> Result<()> {
        let _guard = self.owner_lock.lock().unwrap();
        if read_owner(&self.owner_path())? != expected {
            return Err(owner_error("runtime changed before ownership commit"));
        }
        if expected.as_ref() != Some(&target) {
            write_owner(&self.owner_path(), &target)?;
        }
        Ok(())
    }
    async fn snapshot_owned(
        &self,
        provider: &str,
        captured: Option<control::RuntimeTarget>,
    ) -> Result<(Option<control::RuntimeTarget>, Snap)> {
        let d = self.deps()?;
        let recorded = self.load_owner()?;
        let target = match captured.or(recorded.clone()) {
            Some(target) => Some(target),
            None => control::discover_app(&d, provider).await?,
        };
        let snap = match &target {
            Some(target) => control::snapshot_app(&d, target).await?,
            None => Snap {
                down: true,
                ..Default::default()
            },
        };
        let _guard = self.owner_lock.lock().unwrap();
        if read_owner(&self.owner_path())? != recorded {
            return Err(owner_error("runtime changed while polling"));
        }
        // Discovery is a private candidate. Controller checks generations and
        // persists it before Store or IPC delivery. Polling never erases ownership.
        Ok((target, snap))
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
    fn prepare_up(&self, req: UpRequest) -> Result<PreparedUp> {
        // Admit before constructing dependencies, creating files or starting workers.
        self.load_owner()?;
        let cfg = config::load_laptop(&self.path)?;
        let opts = self.resolve(&cfg, &req)?;
        if opts.provider == "local" {
            memory::inspect_with(&opts.model, opts.ctx, &self.probe(false))?.ensure_fit()?;
        }
        Ok(PreparedUp {
            config: cfg,
            options: opts,
        })
    }
    fn up(
        &self,
        prepared: PreparedUp,
        previous: Option<control::RuntimeTarget>,
        cancel: CancellationToken,
        owner: control::OwnerSink,
    ) -> Result<control::UpOperation> {
        let d = control::deps_from_config(prepared.config, &self.wiring)?;
        let path = self.owner_path();
        let lock = self.owner_lock.clone();
        let owner = Arc::new(move |target: control::RuntimeTarget| {
            {
                let _guard = lock.lock().unwrap();
                write_owner(&path, &target).map_err(|e| lobo_core::Error::Other(e.message))?;
            }
            owner(target)
        });
        Ok(control::up_app(
            d,
            prepared.options,
            previous,
            cancel,
            owner,
        ))
    }
    async fn down(&self, target: control::RuntimeTarget) -> Result<f64> {
        let cost = control::down_app(&self.deps()?, &target).await?;
        self.clear_owner(&target)?;
        Ok(cost)
    }
    async fn telemetry(&self, target: control::RuntimeTarget) -> Result<Status> {
        Ok(control::sample_app(&self.deps()?, &target).await?)
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
    fn target(boot: &str) -> control::RuntimeTarget {
        control::RuntimeTarget {
            provider: "local".into(),
            boot_id: boot.into(),
            instance_id: Some("4242".into()),
            local_pid: Some(4242),
            local_start_id: Some(100),
            agent_url: None,
            api_url: None,
        }
    }
    #[test]
    fn persisted_restart_ownership_is_private_and_old_stop_cannot_erase_new_owner() {
        let (_root, backend) = memory_backend(0);
        let old = target("old");
        write_owner(&backend.owner_path(), &old).unwrap();
        let reopened = CoreBackend::new(backend.path.clone()).unwrap();
        assert_eq!(reopened.load_owner().unwrap(), Some(old.clone()));
        assert_eq!(
            std::fs::metadata(backend.owner_path())
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        let newer = target("new");
        write_owner(&backend.owner_path(), &newer).unwrap();
        backend.clear_owner(&old).unwrap();
        assert_eq!(backend.load_owner().unwrap(), Some(newer.clone()));
        backend.clear_owner(&newer).unwrap();
        assert_eq!(backend.load_owner().unwrap(), None);
    }
    #[test]
    fn malformed_or_empty_owner_fails_without_overwriting_record() {
        let (_root, backend) = memory_backend(0);
        let path = backend.owner_path();
        std::fs::write(&path, b"broken ownership").unwrap();
        assert!(backend.load_owner().is_err());
        assert!(backend.prepare_up(UpRequest::default()).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"broken ownership");
        assert!(write_owner(&path, &target("")).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"broken ownership");
    }
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
        let result = backend.prepare_up(UpRequest {
            provider: Some("local".into()),
            model: Some("q8".into()),
            ..Default::default()
        });
        let Err(error) = result else {
            panic!("Start must deny");
        };
        assert!(error.message.contains("GiB"));
        assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 1);
        backend.memory = memory_backend(2).1.memory;
        assert!(
            backend
                .prepare_up(UpRequest {
                    provider: Some("local".into()),
                    model: Some("q8".into()),
                    ..Default::default()
                })
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
