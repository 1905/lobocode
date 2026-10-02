pub use crate::opencode::PreparedOpenCode;
use crate::{
    opencode,
    types::{AppError, OpenCodeResult},
};
use async_trait::async_trait;
use lobo_core::{config, control, local};
use lobo_proto::{ConfigShow, Readiness, Snap, Status, UpRequest};
use std::{
    collections::BTreeMap,
    io::{Read, Write},
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
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
    fn opencode_path(&self) -> Result<PathBuf> {
        opencode::discover()
    }
    async fn prepare_opencode(&self, owner: control::RuntimeTarget) -> Result<PreparedOpenCode> {
        opencode::prepare(self, owner, &opencode::client()?).await
    }
    fn configure_opencode(
        &self,
        path: &Path,
        prepared: &PreparedOpenCode,
        make_default: bool,
        validate: &dyn Fn() -> Result<()>,
    ) -> Result<OpenCodeResult> {
        opencode::configure(self, path, prepared, make_default, validate)
    }
    async fn config(&self) -> Result<(ConfigShow, Readiness)>;
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
    legacy_owner: Arc<Mutex<Option<control::RuntimeTarget>>>,
    owner_lock: Arc<Mutex<()>>,
    setup_client: reqwest::Client,
}
impl CoreBackend {
    pub fn new(path: PathBuf) -> std::io::Result<Self> {
        let path = std::path::absolute(path)?;
        let mut wiring =
            control::Wiring::new(path.clone(), local::Spawner::app(std::env::current_exe()?));
        // Shared core retains CLI local support. The app never constructs that provider.
        wiring.supported = || {
            Err(lobo_core::Error::Local(
                "The Mac app supports cloud GPUs only.".into(),
            ))
        };
        Ok(Self {
            path,
            wiring,
            legacy_owner: Arc::new(Mutex::new(None)),
            owner_lock: Arc::new(Mutex::new(())),
            setup_client: opencode::client().map_err(std::io::Error::other)?,
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
            cloud_config(config::load_laptop(&self.path)?),
            &self.wiring,
        )?)
    }
    fn owner_path(&self) -> PathBuf {
        self.path.with_extension("app-runtime.json")
    }
    fn expected_owner(
        &self,
        expected: Option<control::RuntimeTarget>,
    ) -> Option<control::RuntimeTarget> {
        expected.or_else(|| self.legacy_owner.lock().unwrap().clone())
    }
    fn clear_owner(&self, target: &control::RuntimeTarget) -> Result<()> {
        let _guard = self.owner_lock.lock().unwrap();
        if read_owner(&self.owner_path())?.as_ref() == Some(target) {
            std::fs::remove_file(self.owner_path()).map_err(owner_error)?;
            *self.legacy_owner.lock().unwrap() = None;
        }
        Ok(())
    }
}
pub(crate) fn require_cloud_provider(provider: &str) -> Result<()> {
    if matches!(provider, "runpod" | "vast") {
        Ok(())
    } else {
        Err(AppError {
            kind: "invalid".into(),
            message: "The Mac app supports RunPod and Vast cloud providers only.".into(),
        })
    }
}
const LOCAL_KEYS: &[&str] = &[
    "LOBO_WEIGHTS_DIR",
    "LOBO_LOCAL_PORT",
    "LOBO_MODEL_SOURCE",
    "LOBO_MODEL_SSH_KEY_FILE",
    "LOBO_MODEL_SSH_HOSTKEY",
    "LOBO_FEESH_HTTP_URL",
];
fn cloud_config(mut cfg: config::Laptop) -> config::Laptop {
    let available = cfg.providers();
    if !matches!(cfg.provider.as_str(), "runpod" | "vast")
        || (!available.is_empty() && !available.contains(&cfg.provider))
    {
        cfg.provider = available
            .first()
            .cloned()
            .unwrap_or_else(|| "runpod".into());
    }
    if cfg.model.is_empty() {
        cfg.model = "q6".into();
    }
    // Legacy CLI settings stay on disk and do not participate in app admission.
    cfg.local_port.clear();
    cfg
}

fn owner_error(error: impl std::fmt::Display) -> AppError {
    AppError {
        kind: "ownership".into(),
        message: format!("Runtime ownership: {error}"),
    }
}
fn read_owner(path: &std::path::Path) -> Result<Option<control::RuntimeTarget>> {
    const OWNER_LIMIT: u64 = 64 << 10;
    let file = match std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
    {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(owner_error(e)),
    };
    let metadata = file.metadata().map_err(owner_error)?;
    if !metadata.is_file() || metadata.len() > OWNER_LIMIT {
        return Err(owner_error(
            "ownership file must be regular and below 64 KiB",
        ));
    }
    let mut bytes = Vec::new();
    file.take(OWNER_LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(owner_error)?;
    if bytes.len() as u64 > OWNER_LIMIT {
        return Err(owner_error("ownership file exceeds 64 KiB"));
    }
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
    async fn prepare_opencode(&self, owner: control::RuntimeTarget) -> Result<PreparedOpenCode> {
        opencode::prepare(self, owner, &self.setup_client).await
    }
    async fn config(&self) -> Result<(ConfigShow, Readiness)> {
        let mut shown = config::show(&self.path)?;
        for key in LOCAL_KEYS {
            shown.values.remove(*key);
            shown.set.remove(*key);
        }
        let cfg = cloud_config(config::Laptop::from_values(&config::values(&self.path)?));
        shown
            .values
            .insert("LOBO_PROVIDER".into(), cfg.provider.clone());
        shown.values.insert("LOBO_MODEL".into(), cfg.model.clone());
        let loaded = config::load_laptop(&self.path).map(cloud_config);
        let error = loaded.as_ref().err().map(ToString::to_string);
        let cloud_ready = loaded.is_ok() && cfg.require_cloud().is_ok();
        let readiness = Readiness {
            exists: self.path.exists(),
            providers: cfg.providers(),
            default_provider: cfg.provider,
            default_model: cfg.model,
            ready: cloud_ready,
            cloud_ready,
            error,
            ..Default::default()
        };
        Ok((shown, readiness))
    }
    fn load_owner(&self) -> Result<Option<control::RuntimeTarget>> {
        let _guard = self.owner_lock.lock().unwrap();
        let owner = read_owner(&self.owner_path())?;
        if owner.as_ref().is_some_and(|t| t.provider == "local") {
            let mut legacy = self.legacy_owner.lock().unwrap();
            // Capture once. A changed legacy record must fail the later comparison.
            if legacy.is_none() {
                *legacy = owner;
            }
            return Ok(None);
        }
        if let Some(owner) = &owner {
            require_cloud_provider(&owner.provider)?;
        }
        Ok(owner)
    }
    fn adopt_owner(
        &self,
        expected: Option<control::RuntimeTarget>,
        target: control::RuntimeTarget,
    ) -> Result<()> {
        require_cloud_provider(&target.provider)?;
        let _guard = self.owner_lock.lock().unwrap();
        let expected = self.expected_owner(expected);
        if read_owner(&self.owner_path())? != expected {
            return Err(owner_error("runtime changed before ownership commit"));
        }
        if expected.as_ref() != Some(&target) {
            write_owner(&self.owner_path(), &target)?;
            *self.legacy_owner.lock().unwrap() = None;
        }
        Ok(())
    }
    async fn snapshot_owned(
        &self,
        provider: &str,
        captured: Option<control::RuntimeTarget>,
    ) -> Result<(Option<control::RuntimeTarget>, Snap)> {
        require_cloud_provider(provider)?;
        if let Some(target) = &captured {
            require_cloud_provider(&target.provider)?;
        }
        let recorded = self.load_owner()?;
        let expected = self.expected_owner(recorded.clone());
        let d = self.deps()?;
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
        if read_owner(&self.owner_path())? != expected {
            return Err(owner_error("runtime changed while polling"));
        }
        // Discovery is a private candidate. Controller checks generations and
        // persists it before Store or IPC delivery. Polling never erases ownership.
        Ok((target, snap))
    }
    fn prepare_up(&self, req: UpRequest) -> Result<PreparedUp> {
        if let Some(provider) = &req.provider {
            require_cloud_provider(provider)?;
        }
        // Reject before dependencies, ownership writes, or workers exist.
        self.load_owner()?;
        let cfg = cloud_config(config::load_laptop(&self.path)?);
        let opts = control::resolve_up(&cfg, &self.path, &req, self.wiring.supported)?;
        require_cloud_provider(&opts.provider)?;
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
        require_cloud_provider(&prepared.options.provider)?;
        if cfg!(feature = "e2e") {
            return Err(AppError {
                kind: "e2e".into(),
                message: "Cloud rentals are disabled in the native test build.".into(),
            });
        }
        if let Some(previous) = &previous {
            require_cloud_provider(&previous.provider)?;
        }
        let expected = self.expected_owner(previous.clone());
        {
            let _guard = self.owner_lock.lock().unwrap();
            if read_owner(&self.owner_path())? != expected {
                return Err(owner_error("runtime changed before Start"));
            }
        }
        let d = control::deps_from_config(prepared.config, &self.wiring)?;
        let path = self.owner_path();
        let lock = self.owner_lock.clone();
        let expected = Mutex::new(expected);
        let legacy_owner = self.legacy_owner.clone();
        let owner = Arc::new(move |target: control::RuntimeTarget| {
            require_cloud_provider(&target.provider)
                .map_err(|e| lobo_core::Error::Other(e.message))?;
            {
                let _guard = lock.lock().unwrap();
                let mut expected = expected.lock().unwrap();
                let current = read_owner(&path).map_err(|e| lobo_core::Error::Other(e.message))?;
                if current != *expected {
                    return Err(lobo_core::Error::Other(
                        "runtime changed before ownership commit".into(),
                    ));
                }
                write_owner(&path, &target).map_err(|e| lobo_core::Error::Other(e.message))?;
                *expected = Some(target.clone());
                *legacy_owner.lock().unwrap() = None;
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
        require_cloud_provider(&target.provider)?;
        let cost = control::down_app(&self.deps()?, &target).await?;
        self.clear_owner(&target)?;
        Ok(cost)
    }
    async fn telemetry(&self, target: control::RuntimeTarget) -> Result<Status> {
        require_cloud_provider(&target.provider)?;
        Ok(control::sample_app(&self.deps()?, &target).await?)
    }
    async fn api_key(&self) -> Result<String> {
        Ok(config::values(&self.path)?
            .remove("LOBO_API_KEY")
            .unwrap_or_default())
    }
    async fn save(&self, set: BTreeMap<String, String>) -> Result<()> {
        if let Some(provider) = set.get("LOBO_PROVIDER").filter(|p| !p.is_empty()) {
            require_cloud_provider(provider)?;
        }
        if set.keys().any(|key| LOCAL_KEYS.contains(&key.as_str())) {
            return Err(AppError {
                kind: "invalid".into(),
                message: "Local model settings are unavailable in the Mac app.".into(),
            });
        }
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
            provider: "runpod".into(),
            boot_id: boot.into(),
            instance_id: Some("4242".into()),
            local_pid: None,
            local_start_id: None,
            agent_url: None,
            api_url: None,
        }
    }
    #[test]
    fn persisted_restart_ownership_is_private_and_old_stop_cannot_erase_new_owner() {
        let (_root, backend) = fixture_backend();
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
        let (_root, backend) = fixture_backend();
        let path = backend.owner_path();
        std::fs::write(&path, b"broken ownership").unwrap();
        assert!(backend.load_owner().is_err());
        assert!(backend.prepare_up(UpRequest::default()).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"broken ownership");
        assert!(write_owner(&path, &target("")).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"broken ownership");
    }
    #[test]
    fn owner_read_is_bounded_and_rejects_links_or_special_files() {
        let (_root, backend) = fixture_backend();
        let path = backend.owner_path();
        std::fs::write(&path, vec![b'x'; (64 << 10) + 1]).unwrap();
        assert!(backend.load_owner().is_err());
        std::fs::remove_file(&path).unwrap();
        std::os::unix::fs::symlink(&backend.path, &path).unwrap();
        assert!(backend.load_owner().is_err());
        std::fs::remove_file(&path).unwrap();
        std::fs::create_dir(&path).unwrap();
        assert!(backend.load_owner().is_err());
    }
    fn fixture_backend() -> (tempfile::TempDir, CoreBackend) {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("config.env");
        std::fs::write(&path, "# keep\nLOBO_API_KEY=fixture\nLOBO_PROVIDER=local\nLOBO_CTX=8192\nRUNPOD_API_KEY=fixture\nLOBO_WEIGHTS_DIR=/untouched/model\nLOBO_LOCAL_PORT=invalid-legacy-port\n").unwrap();
        (root, CoreBackend::new(path).unwrap())
    }
    #[tokio::test]
    async fn cloud_defaults_and_saves_preserve_legacy_config() {
        let (_root, backend) = fixture_backend();
        let original = std::fs::read(&backend.path).unwrap();
        let (shown, readiness) = backend.config().await.unwrap();
        assert_eq!(readiness.default_provider, "runpod");
        assert_eq!(readiness.default_model, "q6");
        assert!(readiness.cloud_ready);
        assert!(!readiness.local_supported);
        assert!(!shown.values.contains_key("LOBO_WEIGHTS_DIR"));
        assert_eq!(shown.values["LOBO_PROVIDER"], "runpod");
        assert_eq!(std::fs::read(&backend.path).unwrap(), original);
        assert!((backend.wiring.supported)().is_err());
        let cfg = cloud_config(config::load_laptop(&backend.path).unwrap());
        assert!(control::local_provider_from_config(&cfg, &backend.wiring).is_none());
        backend
            .save(BTreeMap::from([("LOBO_CTX".into(), "65536".into())]))
            .await
            .unwrap();
        let saved = std::fs::read_to_string(&backend.path).unwrap();
        assert!(saved.contains("LOBO_PROVIDER=local"));
        assert!(saved.contains("LOBO_WEIGHTS_DIR=/untouched/model"));
        assert!(saved.contains("LOBO_LOCAL_PORT=invalid-legacy-port"));
        assert!(saved.starts_with("# keep\n"));
        let prepared = backend.prepare_up(UpRequest::default()).unwrap();
        assert_eq!(prepared.options.provider, "runpod");
        assert_eq!(prepared.options.model, "q6");
        backend
            .save(BTreeMap::from([
                ("LOBO_MODEL".into(), "q8".into()),
                ("LOBO_PROVIDER".into(), "vast".into()),
                ("VASTAI_API_KEY".into(), "fixture".into()),
            ]))
            .await
            .unwrap();
        let (_, readiness) = backend.config().await.unwrap();
        assert_eq!(
            (
                readiness.default_provider.as_str(),
                readiness.default_model.as_str()
            ),
            ("vast", "q8")
        );
    }
    #[tokio::test]
    async fn local_boundaries_reject_before_config_or_dependencies() {
        let root = tempfile::tempdir().unwrap();
        let backend = CoreBackend::new(root.path().join("missing.env")).unwrap();
        let mut local = target("legacy");
        local.provider = "local".into();
        assert_eq!(
            backend
                .prepare_up(UpRequest {
                    provider: Some("local".into()),
                    ..Default::default()
                })
                .err()
                .unwrap()
                .kind,
            "invalid"
        );
        assert_eq!(
            backend.down(local.clone()).await.unwrap_err().kind,
            "invalid"
        );
        assert_eq!(
            backend.telemetry(local.clone()).await.unwrap_err().kind,
            "invalid"
        );
        assert_eq!(
            backend
                .snapshot_owned("local", None)
                .await
                .unwrap_err()
                .kind,
            "invalid"
        );
        assert_eq!(
            backend
                .snapshot_owned("runpod", Some(local.clone()))
                .await
                .unwrap_err()
                .kind,
            "invalid"
        );
        assert_eq!(
            backend
                .prepare_opencode(local.clone())
                .await
                .err()
                .unwrap()
                .kind,
            "invalid"
        );
        assert!(backend.adopt_owner(None, local).is_err());
        for (key, value) in [
            ("LOBO_PROVIDER", "local"),
            ("LOBO_LOCAL_PORT", "9000"),
            ("LOBO_WEIGHTS_DIR", "/model"),
        ] {
            assert_eq!(
                backend
                    .save(BTreeMap::from([(key.into(), value.into())]))
                    .await
                    .unwrap_err()
                    .kind,
                "invalid"
            );
        }
        assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
    }
    #[test]
    fn legacy_owner_is_ignored_and_replaced_only_after_exact_comparison() {
        let (_root, backend) = fixture_backend();
        let mut legacy = target("legacy");
        legacy.provider = "local".into();
        legacy.local_pid = Some(4242);
        legacy.local_start_id = Some(100);
        write_owner(&backend.owner_path(), &legacy).unwrap();
        let original = std::fs::read(backend.owner_path()).unwrap();
        assert_eq!(backend.load_owner().unwrap(), None);
        assert_eq!(std::fs::read(backend.owner_path()).unwrap(), original);
        let mut changed = legacy.clone();
        changed.boot_id = "new-local".into();
        write_owner(&backend.owner_path(), &changed).unwrap();
        assert!(backend.adopt_owner(None, target("cloud")).is_err());
        assert_eq!(read_owner(&backend.owner_path()).unwrap(), Some(changed));
        write_owner(&backend.owner_path(), &legacy).unwrap();
        let cloud = target("cloud");
        backend.adopt_owner(None, cloud.clone()).unwrap();
        assert_eq!(backend.load_owner().unwrap(), Some(cloud));
    }
    #[test]
    fn cloud_selection_preserves_configured_choice_and_uses_available_alternate() {
        for (provider, runpod, vast, expected) in [
            ("runpod", "key", "key", "runpod"),
            ("vast", "key", "key", "vast"),
            ("runpod", "", "key", "vast"),
            ("vast", "key", "", "runpod"),
            ("vast", "", "", "vast"),
            ("local", "", "key", "vast"),
            ("local", "", "", "runpod"),
        ] {
            let cfg = cloud_config(config::Laptop {
                provider: provider.into(),
                runpod_api_key: runpod.into(),
                vast_api_key: vast.into(),
                model: "q8".into(),
                local_port: "invalid".into(),
                ..Default::default()
            });
            assert_eq!(
                cfg.provider, expected,
                "{provider} / runpod={runpod} / vast={vast}"
            );
            assert_eq!(cfg.model, "q8");
            assert!(cfg.local_port.is_empty());
        }
    }
    #[tokio::test]
    async fn local_config_without_cloud_keys_requires_setup() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("config.env");
        std::fs::write(
            &path,
            "LOBO_API_KEY=fixture\nLOBO_PROVIDER=local\nVASTAI_API_KEY=fixture\n",
        )
        .unwrap();
        let backend = CoreBackend::new(path.clone()).unwrap();
        assert_eq!(backend.config().await.unwrap().1.default_provider, "vast");
        std::fs::write(&path, "LOBO_API_KEY=fixture\nLOBO_PROVIDER=local\n").unwrap();
        let (_, readiness) = backend.config().await.unwrap();
        assert!(!readiness.ready);
        assert!(!readiness.cloud_ready);
        assert_eq!(readiness.default_provider, "runpod");
    }
}
