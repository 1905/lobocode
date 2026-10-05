use super::*;
use crate::backend::PreparedUp;
use async_trait::async_trait;
use lobo_core::{
    clock::FixedClock,
    control::{
        self,
        testkit::{self, FakeAgent, FakeRunPod},
    },
    provider::{CreateOpts, Provider},
};
use lobo_proto::{ConfigShow, Instance, Manifest, Readiness, Snap, Stage, Status, UpRequest};
use std::{
    collections::BTreeMap,
    sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
};

struct SlowProvider {
    running: Mutex<Vec<Instance>>,
    rents: AtomicUsize,
    deletes: AtomicUsize,
    delay: Duration,
    fail_delete: AtomicBool,
    panic_create: bool,
}
struct OwnedAgent {
    owner: Arc<Mutex<Option<RuntimeTarget>>>,
}
#[async_trait]
impl control::AgentApi for OwnedAgent {
    async fn status(&self) -> lobo_core::Result<Status> {
        Ok(Status {
            stage: Stage::Ready,
            boot_id: self
                .owner
                .lock()
                .unwrap()
                .as_ref()
                .map_or(String::new(), |owner| owner.boot_id.clone()),
            ..Default::default()
        })
    }
    async fn version(&self) -> lobo_core::Result<Manifest> {
        Ok(Manifest::default())
    }
    async fn logs(&self, _: usize) -> lobo_core::Result<String> {
        Ok(String::new())
    }
}
#[async_trait]
impl Provider for SlowProvider {
    fn name(&self) -> &'static str {
        "runpod"
    }
    fn replaceable(&self) -> bool {
        true
    }
    async fn rent(
        &self,
        _: &CreateOpts,
        _: CancellationToken,
        note: &(dyn Fn(String) + Sync),
    ) -> lobo_core::Result<Instance> {
        self.rents.fetch_add(1, Ordering::SeqCst);
        for i in 0..1000 {
            note(format!("preparing {i}"));
        }
        tokio::time::sleep(self.delay).await;
        let i = Instance {
            provider: "runpod".into(),
            id: "slow".into(),
            agent_url: "http://127.0.0.1:8932".into(),
            api_url: "http://127.0.0.1:8931/v1".into(),
            ..Default::default()
        };
        self.running.lock().unwrap().push(i.clone());
        assert!(!self.panic_create, "fixture create panic");
        Ok(i)
    }
    async fn list(&self) -> lobo_core::Result<Vec<Instance>> {
        Ok(self.running.lock().unwrap().clone())
    }
    async fn get(&self, id: &str) -> lobo_core::Result<Instance> {
        self.running
            .lock()
            .unwrap()
            .iter()
            .find(|i| i.id == id)
            .cloned()
            .ok_or(lobo_core::Error::NotFound)
    }
    async fn delete(&self, _: &str) -> lobo_core::Result<()> {
        self.deletes.fetch_add(1, Ordering::SeqCst);
        if self.fail_delete.load(Ordering::SeqCst) {
            return Err(lobo_core::Error::Other("delete denied".into()));
        }
        self.running.lock().unwrap().clear();
        Ok(())
    }
}
type PreparationGate = Arc<(Mutex<bool>, std::sync::Condvar)>;

struct FakeBackend {
    log_root: tempfile::TempDir,
    panic_prepare: AtomicBool,
    panic_up: AtomicBool,
    d: control::Deps,
    calls: Mutex<Vec<&'static str>>,
    ready: AtomicBool,
    up_calls: AtomicUsize,
    down_calls: AtomicUsize,
    start_deny: AtomicBool,
    owner: Arc<Mutex<Option<RuntimeTarget>>>,
    prepare_gate: Mutex<Option<PreparationGate>>,
    prepare_started: tokio::sync::Notify,
    prepare_calls: AtomicUsize,
    down_targets: Mutex<Vec<RuntimeTarget>>,
    discovery: Mutex<Option<RuntimeTarget>>,
    snapshot_delay_ms: AtomicU64,
    snapshot_started: tokio::sync::Notify,
}
#[async_trait]
impl Backend for FakeBackend {
    fn config_path(&self) -> PathBuf {
        self.log_root.path().join("config.env")
    }
    async fn config(&self) -> Result<(ConfigShow, Readiness)> {
        self.calls.lock().unwrap().push("config");
        Ok((
            ConfigShow {
                exists: true,
                ..Default::default()
            },
            Readiness {
                exists: true,
                ready: self.ready.load(Ordering::SeqCst),
                cloud_ready: self.ready.load(Ordering::SeqCst),
                default_provider: "runpod".into(),
                default_model: "q8".into(),
                local_port: 8931,
                ..Default::default()
            },
        ))
    }
    fn load_owner(&self) -> Result<Option<RuntimeTarget>> {
        Ok(self.owner.lock().unwrap().clone())
    }
    fn adopt_owner(&self, expected: Option<RuntimeTarget>, target: RuntimeTarget) -> Result<()> {
        let mut owner = self.owner.lock().unwrap();
        if *owner != expected {
            return Err(AppError {
                kind: "ownership".into(),
                message: "ownership changed".into(),
            });
        }
        *owner = Some(target);
        Ok(())
    }
    async fn snapshot_owned(
        &self,
        provider: &str,
        captured: Option<RuntimeTarget>,
    ) -> Result<(Option<RuntimeTarget>, Snap)> {
        self.calls.lock().unwrap().push("snapshot");
        let owner = captured
            .or(self.load_owner()?)
            .or(self.discovery.lock().unwrap().clone());
        let target = match owner {
            Some(owner) => Some(owner),
            None => control::discover_app(&self.d, provider).await?,
        };
        let snap = match &target {
            Some(target) => control::snapshot_app(&self.d, target).await?,
            None => Snap {
                down: true,
                ..Default::default()
            },
        };
        let delay = self.snapshot_delay_ms.load(Ordering::SeqCst);
        if delay > 0 {
            self.snapshot_started.notify_one();
            tokio::time::sleep(Duration::from_millis(delay)).await;
        }
        Ok((target, snap))
    }
    fn prepare_up(&self, req: UpRequest) -> Result<PreparedUp> {
        assert!(
            !self.panic_prepare.load(Ordering::SeqCst),
            "fixture preparation panic"
        );
        self.prepare_calls.fetch_add(1, Ordering::SeqCst);
        self.prepare_started.notify_one();
        let gate = self.prepare_gate.lock().unwrap().clone();
        if let Some(gate) = gate {
            let (released, signal) = &*gate;
            let mut ready = released.lock().unwrap();
            while !*ready {
                ready = signal.wait(ready).unwrap();
            }
        }
        if self.start_deny.load(Ordering::SeqCst) {
            return Err(AppError {
                kind: "invalid".into(),
                message: "fresh Start rejected: fixture policy".into(),
            });
        }
        Ok(PreparedUp {
            config: self.d.cfg.clone(),
            options: control::UpOpts {
                provider: req.provider.unwrap_or_default(),
                model: req.model.unwrap_or_default(),
                ..Default::default()
            },
        })
    }
    fn up(
        &self,
        prepared: PreparedUp,
        previous: Option<RuntimeTarget>,
        c: CancellationToken,
        owner: OwnerSink,
    ) -> Result<control::UpOperation> {
        assert!(
            !self.panic_up.load(Ordering::SeqCst),
            "fixture admission panic"
        );
        self.up_calls.fetch_add(1, Ordering::SeqCst);
        self.calls.lock().unwrap().push("up");
        let persisted = self.owner.clone();
        let owner = Arc::new(move |target: RuntimeTarget| {
            *persisted.lock().unwrap() = Some(target.clone());
            owner(target)
        });
        Ok(control::up_app(
            self.d.clone(),
            prepared.options,
            previous,
            c,
            owner,
        ))
    }
    async fn down(&self, target: RuntimeTarget) -> Result<f64> {
        self.down_calls.fetch_add(1, Ordering::SeqCst);
        self.down_targets.lock().unwrap().push(target.clone());
        self.calls.lock().unwrap().push("down");
        let cost = control::down_app(&self.d, &target).await?;
        let mut owner = self.owner.lock().unwrap();
        if owner.as_ref() == Some(&target) {
            *owner = None;
        }
        Ok(cost)
    }
    async fn telemetry(&self, target: RuntimeTarget) -> Result<Status> {
        Ok(control::sample_app(&self.d, &target).await?)
    }
    async fn api_key(&self) -> Result<String> {
        self.calls.lock().unwrap().push("api_key");
        Ok("fixture".into())
    }
    async fn save(&self, _: BTreeMap<String, String>) -> Result<()> {
        Ok(())
    }
}
#[derive(Default)]
struct Notes(Mutex<Vec<Note>>);
impl Notifier for Notes {
    fn send(&self, n: &Note) {
        self.0.lock().unwrap().push(n.clone());
    }
}
fn fixture(
    delay: u64,
    fail_delete: bool,
    panic_create: bool,
) -> (Arc<Controller>, Arc<FakeBackend>, Arc<SlowProvider>) {
    let p = Arc::new(SlowProvider {
        running: Mutex::new(vec![]),
        rents: AtomicUsize::new(0),
        deletes: AtomicUsize::new(0),
        delay: Duration::from_secs(delay),
        fail_delete: AtomicBool::new(fail_delete),
        panic_create,
    });
    let clock = Arc::new(FixedClock("2026-09-29T12:00:00Z".parse().unwrap()));
    let owner = Arc::new(Mutex::new(None));
    let mut d = testkit::deps(
        Arc::new(FakeRunPod::default()),
        Arc::new(FakeAgent::new(vec![Some(Status {
            stage: Stage::Ready,
            ..Default::default()
        })])),
        clock.clone(),
    );
    d.providers.insert("runpod".into(), p.clone());
    let agent = Arc::new(OwnedAgent {
        owner: owner.clone(),
    });
    d.new_agent = Arc::new(move |_| agent.clone());
    let b = Arc::new(FakeBackend {
        log_root: tempfile::tempdir().unwrap(),
        panic_prepare: AtomicBool::new(false),
        panic_up: AtomicBool::new(false),
        d,
        calls: Mutex::new(vec![]),
        ready: AtomicBool::new(true),
        up_calls: AtomicUsize::new(0),
        down_calls: AtomicUsize::new(0),
        start_deny: AtomicBool::new(false),
        owner,
        prepare_gate: Mutex::new(None),
        prepare_started: tokio::sync::Notify::new(),
        prepare_calls: AtomicUsize::new(0),
        down_targets: Mutex::new(vec![]),
        discovery: Mutex::new(None),
        snapshot_delay_ms: AtomicU64::new(0),
        snapshot_started: tokio::sync::Notify::new(),
    });
    let c = Controller::new(
        b.clone(),
        Arc::new(Notes::default()),
        clock,
        Prefs::default(),
        None,
        tokio::runtime::Handle::current(),
        Arc::new(|_| {}),
    );
    (c, b, p)
}
async fn start(c: &Arc<Controller>, p: &SlowProvider) {
    c.load_config().await;
    c.refresh().await;
    c.submit_start().unwrap().await.unwrap().unwrap();
    for _ in 0..1000 {
        if p.rents.load(Ordering::SeqCst) > 0 {
            return;
        }
        tokio::task::yield_now().await;
    }
    panic!("rent never began");
}
async fn join_stop(c: &Controller) {
    let h = c.active.lock().unwrap().stop.take().unwrap();
    h.await.unwrap();
}
fn setup_controller(b: Arc<crate::opencode::tests::FixtureBackend>) -> Arc<Controller> {
    let c = Controller::new(
        b.clone(),
        Arc::new(Notes::default()),
        Arc::new(FixedClock("2026-09-29T12:00:00Z".parse().unwrap())),
        Prefs::default(),
        None,
        tokio::runtime::Handle::current(),
        Arc::new(|_| {}),
    );
    c.change(|store| store.apply_snap(b.snap.lock().unwrap().clone(), c.clock.now()));
    c
}
fn setup_path(b: &crate::opencode::tests::FixtureBackend) -> String {
    b.destination().to_string_lossy().into_owned()
}

#[tokio::test]
async fn opencode_off_info_keeps_path_and_never_queries_runtime() {
    let b = crate::opencode::tests::FixtureBackend::new("http://127.0.0.1:1234/v1", "runpod", "q6");
    let c = setup_controller(b.clone());
    c.change(|s| {
        s.set_runtime(None);
        s.stop_done();
        vec![]
    });
    let info = c.opencode_info(None).unwrap();
    assert_eq!(info.path, setup_path(&b));
    assert!(!info.can_configure);
    assert!(info.endpoint.is_none() && info.model_alias.is_none());
    assert!(info.reason.is_some());
    assert_eq!(b.snapshots.load(Ordering::SeqCst), 0);
    assert!(c.configure_opencode(setup_path(&b), true).await.is_err());
    assert_eq!(b.preparations.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn opencode_selected_valid_file_remains_usable_when_default_is_invalid() {
    let b = crate::opencode::tests::FixtureBackend::new("http://127.0.0.1:1234/v1", "runpod", "q6");
    let c = setup_controller(b.clone());
    std::fs::write(b.destination(), "{ bad: private-parser-secret }").unwrap();
    let default = c.opencode_info(None).unwrap();
    assert!(!default.can_configure);
    assert!(
        !serde_json::to_string(&default)
            .unwrap()
            .contains("private-parser-secret")
    );
    let selected = b.root.path().join("chosen.jsonc");
    std::fs::write(&selected, "{}").unwrap();
    let info = c
        .opencode_info(Some(selected.to_string_lossy().into_owned()))
        .unwrap();
    assert!(info.can_configure);
    assert_eq!(info.path, selected.to_string_lossy());
    assert_eq!(
        info.model_alias,
        Some(lobo_proto::catalog::get("q6").unwrap().alias.clone())
    );
    assert_eq!(info.context, Some(4096));
    assert!(info.warnings.is_empty());
    assert_eq!(b.snapshots.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn opencode_stop_settings_and_change_back_during_http_invalidate_setup() {
    use wiremock::{Mock, MockServer, ResponseTemplate, matchers::path};
    for mutation in 0..4 {
        let server = MockServer::start().await;
        let b = crate::opencode::tests::FixtureBackend::new(
            &format!("{}/v1", server.uri()),
            "runpod",
            "q6",
        );
        let c = setup_controller(b.clone());
        let original = b.bytes();
        let controller = c.clone();
        Mock::given(path("/v1/models")).respond_with(move |_: &wiremock::Request| {
            match mutation {
                0 => controller.stop(),
                1 => { controller.begin_config_write(); controller.end_config_write(); },
                2 => { controller.set_model("q6".into()); controller.set_model("q8".into()); },
                3 => { controller.set_provider("vast".into()).unwrap(); controller.set_provider("runpod".into()).unwrap(); },
                _ => unreachable!(),
            }
            ResponseTemplate::new(200).set_body_json(serde_json::json!({"data":[{"id":lobo_proto::catalog::get("q6").unwrap().alias}]}))
        }).expect(1).mount(&server).await;
        assert!(c.configure_opencode(setup_path(&b), true).await.is_err());
        assert_eq!(b.bytes(), original);
        assert!(c.active.lock().unwrap().setup.is_none());
        assert_eq!(b.preparations.load(Ordering::SeqCst), 1);
        if mutation == 0 {
            join_stop(&c).await;
        }
    }
}

#[tokio::test]
async fn opencode_duplicate_and_cancelled_verification_release_reservation() {
    let b = crate::opencode::tests::FixtureBackend::new("http://127.0.0.1:1234/v1", "runpod", "q6");
    b.gated.store(true, Ordering::SeqCst);
    let c = setup_controller(b.clone());
    let controller = c.clone();
    let chosen = setup_path(&b);
    let task = tokio::spawn(async move { controller.configure_opencode(chosen, true).await });
    b.started.notified().await;
    assert!(c.configure_opencode(setup_path(&b), true).await.is_err());
    assert_eq!(b.preparations.load(Ordering::SeqCst), 1);
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    assert!(c.active.lock().unwrap().setup.is_none());
    assert_eq!(std::fs::read_dir(b.root.path()).unwrap().count(), 2);
}

#[tokio::test]
async fn opencode_stale_owner_stop_or_selection_before_final_callback_rejects() {
    use wiremock::MockServer;
    for mutation in 0..3 {
        let server = MockServer::start().await;
        crate::opencode::tests::serve_models(&server, "q6", 1).await;
        let b = crate::opencode::tests::FixtureBackend::new(
            &format!("{}/v1", server.uri()),
            "runpod",
            "q6",
        );
        let c = setup_controller(b.clone());
        let original = b.bytes();
        let controller = c.clone();
        *b.before_commit.lock().unwrap() = Some(Arc::new(move || match mutation {
            0 => controller.change(|s| {
                let mut owner = s.runtime().unwrap();
                owner.boot_id = "replacement-boot".into();
                s.set_runtime(Some(owner));
                vec![]
            }),
            1 => {
                controller.stop();
            }
            _ => {
                controller.set_model("q6".into());
                controller.set_model("q8".into());
            }
        }));
        assert!(c.configure_opencode(setup_path(&b), true).await.is_err());
        assert_eq!(b.bytes(), original);
        assert!(c.active.lock().unwrap().setup.is_none());
        if mutation == 1 {
            join_stop(&c).await;
        }
        *b.before_commit.lock().unwrap() = None; // Break the fixture-only Arc cycle.
    }
}

#[tokio::test]
async fn opencode_commit_and_noop_retain_both_gates_until_backend_returns() {
    use wiremock::MockServer;
    let server = MockServer::start().await;
    crate::opencode::tests::serve_models(&server, "q6", 2).await;
    let b = crate::opencode::tests::FixtureBackend::new(
        &format!("{}/v1", server.uri()),
        "runpod",
        "q6",
    );
    let c = setup_controller(b.clone());
    let controller = c.clone();
    let validations = Arc::new(AtomicUsize::new(0));
    let count = validations.clone();
    *b.after_validate.lock().unwrap() = Some(Arc::new(move || {
        assert!(controller.active.try_lock().is_err());
        assert!(controller.store.try_lock().is_err());
        count.fetch_add(1, Ordering::SeqCst);
    }));
    assert!(
        c.configure_opencode(setup_path(&b), true)
            .await
            .unwrap()
            .changed
    );
    assert!(
        !c.configure_opencode(setup_path(&b), true)
            .await
            .unwrap()
            .changed
    );
    assert_eq!(validations.load(Ordering::SeqCst), 4);
    assert!(c.active.lock().unwrap().setup.is_none());
    assert!(c.store.try_lock().is_ok());
    *b.after_validate.lock().unwrap() = None;
}

#[tokio::test]
async fn opencode_panic_after_validation_cleans_files_and_does_not_poison_app_gates() {
    use wiremock::MockServer;
    let server = MockServer::start().await;
    crate::opencode::tests::serve_models(&server, "q6", 2).await;
    let b = crate::opencode::tests::FixtureBackend::new(
        &format!("{}/v1", server.uri()),
        "runpod",
        "q6",
    );
    let c = setup_controller(b.clone());
    let original = b.bytes();
    *b.after_validate.lock().unwrap() =
        Some(Arc::new(|| panic!("task-owned post-validation panic")));
    let error = c
        .configure_opencode(setup_path(&b), true)
        .await
        .unwrap_err();
    assert_eq!(error.message, "OpenCode setup worker failed. Retry setup.");
    assert_eq!(b.bytes(), original);
    assert!(c.active.lock().unwrap().setup.is_none());
    assert!(c.store.try_lock().is_ok());
    *b.after_validate.lock().unwrap() = None;
    assert!(
        c.configure_opencode(setup_path(&b), true)
            .await
            .unwrap()
            .changed
    );
}

#[tokio::test]
async fn opencode_preparation_panic_releases_reservation() {
    let b = crate::opencode::tests::FixtureBackend::new("http://127.0.0.1:1234/v1", "runpod", "q6");
    b.panic_prepare.store(true, Ordering::SeqCst);
    let c = setup_controller(b.clone());
    let controller = c.clone();
    let path = setup_path(&b);
    let task = tokio::spawn(async move { controller.configure_opencode(path, true).await });
    assert!(task.await.unwrap_err().is_panic());
    assert!(c.active.lock().unwrap().setup.is_none());
    assert!(c.store.try_lock().is_ok());
}

#[tokio::test(start_paused = true)]
async fn refresh_polls_cloud_and_requires_cloud_setup() {
    let (c, b, _) = fixture(20, false, false);
    c.load_config().await;
    c.refresh().await;
    assert_eq!(*b.calls.lock().unwrap(), ["config", "snapshot"]);
    b.calls.lock().unwrap().clear();
    c.refresh().await;
    assert_eq!(*b.calls.lock().unwrap(), ["snapshot"]);
    b.calls.lock().unwrap().clear();
    c.refresh().await;
    assert_eq!(*b.calls.lock().unwrap(), ["snapshot"]);
    b.calls.lock().unwrap().clear();
    c.load_config().await;
    assert_eq!(*b.calls.lock().unwrap(), ["config"]);
    b.ready.store(false, Ordering::SeqCst);
    c.load_config().await;
    b.calls.lock().unwrap().clear();
    c.refresh().await;
    assert_eq!(*b.calls.lock().unwrap(), ["config"]);
    assert_eq!(c.state().phase, Phase::NoConfig);
}
#[tokio::test(start_paused = true)]
async fn stop_waits_for_real_core_rent_and_duplicate_start() {
    let (c, b, p) = fixture(20, false, false);
    start(&c, &p).await;
    c.submit_start().unwrap().await.unwrap().unwrap();
    assert_eq!(b.up_calls.load(Ordering::SeqCst), 1);
    c.stop();
    c.stop();
    c.dismiss();
    c.submit_start().unwrap().await.unwrap().unwrap();
    assert_eq!(c.state().phase, Phase::Stopping);
    tokio::time::advance(Duration::from_secs(19)).await;
    assert_eq!(b.down_calls.load(Ordering::SeqCst), 0);
    assert_eq!(p.deletes.load(Ordering::SeqCst), 0);
    join_stop(&c).await;
    assert!(p.running.lock().unwrap().is_empty());
    assert_eq!(p.deletes.load(Ordering::SeqCst), 1);
    assert_eq!(b.down_calls.load(Ordering::SeqCst), 2);
    assert_eq!(c.state().phase, Phase::Off);
}
#[tokio::test(start_paused = true)]
async fn stop_timeout_keeps_ownership_and_warning() {
    let (c, b, p) = fixture(121, false, false);
    start(&c, &p).await;
    c.stop();
    tokio::task::yield_now().await;
    tokio::time::advance(Duration::from_secs(120)).await;
    for _ in 0..10 {
        tokio::task::yield_now().await;
    }
    assert_eq!(c.state().phase, Phase::Stopping);
    assert_eq!(
        c.state().warning.as_deref(),
        Some("stop: cleanup is still running")
    );
    c.refresh().await;
    assert_eq!(
        c.state().warning.as_deref(),
        Some("stop: cleanup is still running")
    );
    c.submit_start().unwrap().await.unwrap().unwrap();
    c.dismiss();
    assert_eq!(b.up_calls.load(Ordering::SeqCst), 1);
    assert_eq!(b.down_calls.load(Ordering::SeqCst), 0);
    join_stop(&c).await;
    assert!(p.running.lock().unwrap().is_empty());
    assert_eq!(c.state().phase, Phase::Off);
}
#[tokio::test(start_paused = true)]
async fn cleanup_failure_stays_visible_after_poll_and_allows_stop_retry() {
    let (c, b, p) = fixture(20, true, false);
    start(&c, &p).await;
    c.stop();
    join_stop(&c).await;
    assert!(matches!(c.state().phase, Phase::Failed { .. }));
    c.refresh().await;
    assert!(matches!(c.state().phase, Phase::Failed { .. }));
    assert_eq!(b.down_calls.load(Ordering::SeqCst), 0);
    p.fail_delete.store(false, Ordering::SeqCst);
    c.stop();
    join_stop(&c).await;
    assert!(p.running.lock().unwrap().is_empty());
    assert_eq!(c.state().phase, Phase::Off);
}
#[tokio::test(start_paused = true)]
async fn quit_awaits_cancelled_create() {
    let (c, _, p) = fixture(121, false, false);
    start(&c, &p).await;
    c.quit().await.unwrap();
    assert!(p.running.lock().unwrap().is_empty());
    assert_eq!(p.deletes.load(Ordering::SeqCst), 1);
}
#[tokio::test(start_paused = true)]
async fn worker_panic_retains_unproven_cloud_ownership_and_remains_failed() {
    let (c, b, p) = fixture(20, false, true);
    start(&c, &p).await;
    let h = c.active.lock().unwrap().up.take().unwrap();
    assert!(h.await.unwrap().is_err());
    // This fake has no saved SSH connection proving the uncertain instance boot.
    assert_eq!(p.running.lock().unwrap().len(), 1);
    assert_eq!(p.deletes.load(Ordering::SeqCst), 0);
    let owner = b.load_owner().unwrap().unwrap();
    assert_eq!(owner.instance_id, None);
    assert!(!owner.boot_id.is_empty());
    let guard =
        b.d.operations
            .acquire(&CancellationToken::new())
            .await
            .unwrap();
    assert_eq!(guard.pending().unwrap().boot_id, owner.boot_id);
    assert!(matches!(c.state().phase, Phase::Failed { .. }));
}
#[tokio::test(start_paused = true)]
async fn quit_after_ready_keeps_runtime() {
    let (c, b, p) = fixture(0, false, false);
    start(&c, &p).await;
    let h = c.active.lock().unwrap().up.take().unwrap();
    h.await.unwrap().unwrap();
    assert_eq!(c.state().phase, Phase::Ready);
    c.quit().await.unwrap();
    assert_eq!(b.down_calls.load(Ordering::SeqCst), 0);
    assert_eq!(p.deletes.load(Ordering::SeqCst), 0);
    assert_eq!(p.running.lock().unwrap().len(), 1);
}

#[tokio::test(start_paused = true)]
async fn failed_quit_keeps_polling_and_retries_cleanup_on_the_next_quit() {
    let (c, b, p) = fixture(20, true, false);
    start(&c, &p).await;
    assert!(c.quit().await.is_err());
    assert!(!c.shutdown.is_cancelled());
    assert!(!c.active.lock().unwrap().quitting);
    assert!(matches!(c.state().phase, Phase::Failed { .. }));
    c.refresh().await;
    assert!(matches!(c.state().phase, Phase::Failed { .. }));
    assert!(c.quit().await.is_err());
    assert_eq!(b.down_calls.load(Ordering::SeqCst), 1);
    p.fail_delete.store(false, Ordering::SeqCst);
    c.quit().await.unwrap();
    assert!(c.shutdown.is_cancelled());
    assert!(p.running.lock().unwrap().is_empty());
}

#[tokio::test(start_paused = true)]
async fn denied_start_and_retry_never_launch_an_operation() {
    let (c, b, p) = fixture(0, false, false);
    c.load_config().await;
    c.refresh().await;
    b.start_deny.store(true, Ordering::SeqCst);
    assert!(
        c.submit_start()
            .unwrap()
            .await
            .unwrap()
            .unwrap_err()
            .message
            .contains("fresh Start rejected")
    );
    assert!(matches!(c.state().phase, Phase::Failed { .. }));
    assert!(c.submit_start().unwrap().await.unwrap().is_err());
    assert_eq!(p.rents.load(Ordering::SeqCst), 0);
    assert_eq!(b.prepare_calls.load(Ordering::SeqCst), 2);
    assert_eq!(b.up_calls.load(Ordering::SeqCst), 0);
    assert!(!c.store.lock().unwrap().needs_cleanup());
}

#[test]
fn queued_cloud_start_preserves_selection_and_rejects_changed_config() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .max_blocking_threads(1)
        .build()
        .unwrap();
    runtime.block_on(async {
        let (c, b, p) = fixture(0, false, false);
        c.load_config().await;
        c.refresh().await;
        let (started, wait_started) = tokio::sync::oneshot::channel();
        let (release, wait_release) = std::sync::mpsc::channel();
        let blocker = tokio::task::spawn_blocking(move || {
            let _ = started.send(());
            wait_release.recv().unwrap();
        });
        wait_started.await.unwrap();
        let reply = c.submit_start().unwrap();
        tokio::task::yield_now().await;
        c.set_model("q6".into());
        c.set_provider("vast".into()).unwrap();
        // Existing UI policy rejects selection while Start is reserved.
        assert_eq!(c.state().model, "q8");
        assert_eq!(c.state().provider, "runpod");
        c.submit_start().unwrap().await.unwrap().unwrap();
        c.begin_config_write();
        c.end_config_write();
        release.send(()).unwrap();
        blocker.await.unwrap();
        let error = reply.await.unwrap().unwrap_err();
        assert_eq!(error.kind, "stale");
        let task = c.active.lock().unwrap().up.take().unwrap();
        assert!(task.await.unwrap().is_err());
        assert_eq!(b.up_calls.load(Ordering::SeqCst), 0);
        assert_eq!(p.rents.load(Ordering::SeqCst), 0);
        assert!(!c.store.lock().unwrap().needs_cleanup());
    });
}

#[tokio::test]
async fn stop_cancels_preparation_without_holding_active_or_launching() {
    let (c, b, p) = fixture(0, false, false);
    c.load_config().await;
    c.refresh().await;
    let gate = Arc::new((Mutex::new(false), std::sync::Condvar::new()));
    *b.prepare_gate.lock().unwrap() = Some(gate.clone());
    let reply = c.submit_start().unwrap();
    b.prepare_started.notified().await;
    c.stop();
    assert_eq!(c.state().phase, Phase::Stopping);
    *gate.0.lock().unwrap() = true;
    gate.1.notify_all();
    assert_eq!(reply.await.unwrap().unwrap_err().kind, "cancelled");
    join_stop(&c).await;
    assert_eq!(b.up_calls.load(Ordering::SeqCst), 0);
    assert_eq!(b.down_calls.load(Ordering::SeqCst), 0);
    assert_eq!(p.rents.load(Ordering::SeqCst), 0);
    assert_eq!(c.state().phase, Phase::Off);
}

#[tokio::test(start_paused = true)]
async fn stop_uses_the_owned_provider_despite_selection_attempts() {
    let (c, b, p) = fixture(20, false, false);
    start(&c, &p).await;
    c.stop();
    c.set_provider("vast".into()).unwrap();
    join_stop(&c).await;
    let targets = b.down_targets.lock().unwrap();
    assert_eq!(targets.len(), 2);
    assert_eq!(targets[0], targets[1]);
    assert_eq!(targets[0].provider, "runpod");
    assert_eq!(targets[0].instance_id.as_deref(), Some("slow"));
    assert!(!targets[0].boot_id.is_empty());
}

#[tokio::test(start_paused = true)]
async fn foreign_pending_operation_is_preserved_and_never_rented_or_deleted() {
    let (c, b, p) = fixture(0, false, false);
    let pending = control::operation_state::PendingCreate {
        provider: "runpod".into(),
        boot_id: "foreign".into(),
        before: vec!["other".into()],
        instance_id: None,
    };
    {
        let mut guard =
            b.d.operations
                .acquire(&CancellationToken::new())
                .await
                .unwrap();
        guard.record(pending.clone()).unwrap();
    }
    c.load_config().await;
    c.refresh().await;
    c.submit_start().unwrap().await.unwrap().unwrap();
    let task = c.active.lock().unwrap().up.take().unwrap();
    assert!(task.await.unwrap().is_err());
    c.stop();
    join_stop(&c).await;
    let guard =
        b.d.operations
            .acquire(&CancellationToken::new())
            .await
            .unwrap();
    assert_eq!(guard.pending(), Some(&pending));
    assert_eq!(p.rents.load(Ordering::SeqCst), 0);
    assert_eq!(p.deletes.load(Ordering::SeqCst), 0);
    assert_eq!(b.down_calls.load(Ordering::SeqCst), 0);
}

fn cloud_owned_fixture() -> (
    Arc<Controller>,
    Arc<FakeBackend>,
    Arc<testkit::RecordingProvider>,
) {
    let (c, mut b, _) = fixture(0, false, false);
    drop(c);
    let runpod = Arc::new(testkit::RecordingProvider::new("runpod"));
    {
        let mut state = runpod.state.lock().unwrap();
        state.boot_id = "cloud-boot".into();
        state.start_id = 100;
        state.instances.push(Instance {
            provider: "runpod".into(),
            id: "4242".into(),
            api_url: testkit::LOCAL_API_URL.into(),
            agent_url: testkit::LOCAL_AGENT_URL.into(),
            ..Default::default()
        });
    }
    let vast = Arc::new(testkit::RecordingProvider::new("vast"));
    vast.state.lock().unwrap().broken = true;
    let backend = Arc::get_mut(&mut b).unwrap();
    backend.d.providers.insert("runpod".into(), runpod);
    *backend.discovery.lock().unwrap() = Some(RuntimeTarget {
        provider: "runpod".into(),
        instance_id: Some("4242".into()),
        boot_id: "cloud-boot".into(),
        agent_url: Some(testkit::LOCAL_AGENT_URL.into()),
        api_url: Some(testkit::LOCAL_API_URL.into()),
        local_pid: None,
        local_start_id: None,
    });
    backend.d.providers.insert("vast".into(), vast.clone());
    let agent = Arc::new(FakeAgent::new(vec![Some(Status {
        stage: Stage::Ready,
        boot_id: "cloud-boot".into(),
        ..Default::default()
    })]));
    backend.d.new_agent = Arc::new(move |_| agent.clone());
    let c = Controller::new(
        b.clone(),
        Arc::new(Notes::default()),
        backend_clock(),
        Prefs::default(),
        None,
        tokio::runtime::Handle::current(),
        Arc::new(|_| {}),
    );
    (c, b, vast)
}
fn backend_clock() -> Arc<FixedClock> {
    Arc::new(FixedClock("2026-09-29T12:00:00Z".parse().unwrap()))
}

#[tokio::test(start_paused = true)]
async fn cloud_ready_and_stop_ignore_a_failing_vast_provider() {
    let (c, b, vast) = cloud_owned_fixture();
    c.load_config().await;
    c.refresh().await;
    assert_eq!(c.state().phase, Phase::Ready);
    assert_eq!(b.load_owner().unwrap().unwrap().provider, "runpod");
    assert!(vast.calls().is_empty());
    c.set_provider("vast".into()).unwrap();
    c.stop();
    join_stop(&c).await;
    assert_eq!(b.down_targets.lock().unwrap().len(), 2);
    assert_eq!(b.down_targets.lock().unwrap()[0].provider, "runpod");
    assert_eq!(c.state().phase, Phase::Off);
    let snap = c.state().snap.unwrap();
    assert!(snap.down);
    assert_eq!(snap.pod, None);
    assert_eq!(snap.status, None);
    assert_ne!(c.state().endpoint.as_deref(), Some(testkit::LOCAL_API_URL));
    assert!(vast.calls().is_empty());
}

#[tokio::test(start_paused = true)]
async fn stale_discovery_does_not_persist_or_adopt_an_old_selection() {
    let (c, b, _) = cloud_owned_fixture();
    c.load_config().await;
    b.snapshot_delay_ms.store(1000, Ordering::SeqCst);
    let worker = c.clone();
    let poll = tokio::spawn(async move { worker.refresh().await });
    b.snapshot_started.notified().await;
    c.set_provider("vast".into()).unwrap();
    b.snapshot_delay_ms.store(0, Ordering::SeqCst);
    poll.await.unwrap();
    assert_eq!(b.load_owner().unwrap(), None);
    assert_eq!(c.store.lock().unwrap().runtime(), None);
}

#[tokio::test(start_paused = true)]
async fn restart_uses_persisted_cloud_owner_when_selection_changes() {
    let (old, b, vast) = cloud_owned_fixture();
    old.load_config().await;
    old.refresh().await;
    let target = b.load_owner().unwrap().unwrap();
    let c = Controller::new(
        b.clone(),
        Arc::new(Notes::default()),
        backend_clock(),
        Prefs::default(),
        None,
        tokio::runtime::Handle::current(),
        Arc::new(|_| {}),
    );
    c.load_config().await;
    c.set_provider("vast".into()).unwrap();
    c.refresh().await;
    assert_eq!(c.state().phase, Phase::Ready);
    assert_eq!(c.runtime_target().unwrap(), target);
    c.stop();
    join_stop(&c).await;
    assert_eq!(b.down_targets.lock().unwrap()[0], target);
    assert!(vast.calls().is_empty());
}

#[tokio::test(start_paused = true)]
async fn legacy_local_owner_and_selection_never_reach_backend_operations() {
    let (old, backend, provider) = fixture(0, false, false);
    drop(old);
    let legacy = RuntimeTarget {
        provider: "local".into(),
        instance_id: Some("4242".into()),
        boot_id: "legacy-local".into(),
        local_pid: Some(4242),
        local_start_id: Some(100),
        agent_url: None,
        api_url: None,
    };
    *backend.owner.lock().unwrap() = Some(legacy.clone());
    backend.ready.store(false, Ordering::SeqCst);
    let controller = Controller::new(
        backend.clone(),
        Arc::new(Notes::default()),
        backend_clock(),
        Prefs::default(),
        None,
        tokio::runtime::Handle::current(),
        Arc::new(|_| {}),
    );
    assert!(controller.store.lock().unwrap().runtime().is_none());
    assert!(controller.copy_api_key().await.is_err());
    assert_eq!(
        controller.set_provider("local".into()).unwrap_err().kind,
        "invalid"
    );
    controller.refresh().await;
    assert_eq!(controller.state().phase, Phase::NoConfig);
    controller.submit_start().unwrap().await.unwrap().unwrap();
    controller.stop();
    join_stop(&controller).await;
    assert_eq!(*backend.owner.lock().unwrap(), Some(legacy));
    assert_eq!(backend.up_calls.load(Ordering::SeqCst), 0);
    assert_eq!(backend.down_calls.load(Ordering::SeqCst), 0);
    assert_eq!(provider.rents.load(Ordering::SeqCst), 0);
    assert!(!backend.calls.lock().unwrap().contains(&"snapshot"));
    assert!(!backend.calls.lock().unwrap().contains(&"api_key"));
}

#[tokio::test(start_paused = true)]
async fn copy_key_requires_the_owned_cloud_ready_runtime() {
    let (controller, backend, _) = cloud_owned_fixture();
    assert!(controller.copy_api_key().await.is_err());
    assert!(!backend.calls.lock().unwrap().contains(&"api_key"));
    controller.load_config().await;
    controller.refresh().await;
    assert_eq!(controller.copy_api_key().await.unwrap(), "fixture");
    backend.calls.lock().unwrap().clear();
    controller.stop();
    assert!(controller.copy_api_key().await.is_err());
    join_stop(&controller).await;
    assert!(controller.copy_api_key().await.is_err());
    assert!(!backend.calls.lock().unwrap().contains(&"api_key"));
}

#[tokio::test]
async fn admission_panics_end_boot_and_leave_gates_usable() {
    for admission in [false, true] {
        let (controller, backend, provider) = fixture(0, false, false);
        controller.load_config().await;
        controller.refresh().await;
        if admission {
            backend.panic_up.store(true, Ordering::SeqCst);
        } else {
            backend.panic_prepare.store(true, Ordering::SeqCst);
        }
        let reply = controller.submit_start().unwrap();
        assert!(reply.await.unwrap().is_err());
        let task = controller.active.lock().unwrap().up.take().unwrap();
        assert!(task.await.unwrap().is_err());
        assert!(matches!(controller.state().phase, Phase::Failed { .. }));
        assert!(controller.state().start_allowed);
        assert!(controller.active.try_lock().is_ok());
        assert!(controller.store.try_lock().is_ok());
        assert_eq!(provider.rents.load(Ordering::SeqCst), 0);
        let logs = std::fs::read_to_string(controller.log_path().unwrap()).unwrap();
        assert!(logs.contains("start_failed"));
        assert!(logs.contains("start_rejected"));
    }
}
#[tokio::test(start_paused = true)]
async fn startup_deadline_retains_slow_create_and_blocks_duplicate_rent() {
    let (controller, backend, provider) = fixture(40 * 60 + 30, false, false);
    start(&controller, &provider).await;
    tokio::time::advance(START_DEADLINE).await;
    for _ in 0..20 {
        tokio::task::yield_now().await;
    }
    assert!(matches!(controller.state().phase, Phase::Failed { .. }));
    assert!(!controller.state().start_allowed);
    assert!(controller.store.lock().unwrap().up_running);
    controller.submit_start().unwrap().await.unwrap().unwrap();
    assert_eq!(backend.up_calls.load(Ordering::SeqCst), 1);
    assert_eq!(provider.deletes.load(Ordering::SeqCst), 0);
    let task = controller.active.lock().unwrap().up.take().unwrap();
    assert_eq!(task.await.unwrap().unwrap_err().kind, "timeout");
    assert!(provider.running.lock().unwrap().is_empty());
    assert_eq!(provider.deletes.load(Ordering::SeqCst), 1);
    assert!(!controller.state().start_allowed);
    controller.stop();
    join_stop(&controller).await;
    assert_eq!(controller.state().phase, Phase::Off);
    assert!(controller.state().start_allowed);
    let logs = std::fs::read_to_string(controller.log_path().unwrap()).unwrap();
    assert!(logs.contains("start_timeout"));
    assert!(logs.contains("stop_finished"));
}
#[tokio::test]
async fn controller_logs_survive_restart_and_disk_failure_is_visible() {
    let (first, backend, _) = fixture(0, false, false);
    first.load_config().await;
    let path = first.log_path().unwrap();
    drop(first);
    let second = Controller::new(
        backend,
        Arc::new(Notes::default()),
        backend_clock(),
        Prefs::default(),
        None,
        tokio::runtime::Handle::current(),
        Arc::new(|_| {}),
    );
    let records: Vec<serde_json::Value> = std::fs::read_to_string(&path)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(records.iter().filter(|r| r["event"] == "launch").count(), 2);
    assert!(records.iter().any(|r| r["event"] == "config_loaded"));
    std::fs::remove_file(&path).unwrap();
    std::fs::create_dir(&path).unwrap();
    second.load_config().await;
    assert!(
        second
            .state()
            .logging_error
            .unwrap()
            .contains("cannot be saved")
    );
    assert_eq!(second.state().log_path.as_deref(), path.to_str());
    second.stop();
    join_stop(&second).await;
    assert_eq!(second.state().phase, Phase::Off);
}

#[tokio::test(start_paused = true)]
async fn restored_runtime_uses_observation_time_and_ready_reconnects_without_false_failure() {
    let (controller, backend, _) = cloud_owned_fixture();
    controller.load_config().await;
    controller.refresh().await;
    let owner = controller.runtime_target().unwrap();
    let mut ready = controller.state().snap.unwrap();
    ready.pod.as_mut().unwrap().started_at =
        lobo_proto::GoTime::from_utc(controller.clock.now() - chrono::TimeDelta::hours(2));
    let mut disconnected = ready.clone();
    disconnected.status = None;
    controller.change(|store| {
        store.apply_snap(
            disconnected.clone(),
            controller.clock.now() - chrono::TimeDelta::minutes(41),
        )
    });
    controller.expire_resumed_start();
    assert!(controller.store.lock().unwrap().runtime_seen_ready());
    assert_eq!(controller.state().phase, Phase::Booting);
    controller.refresh().await;
    assert_eq!(controller.state().phase, Phase::Ready);
    let restarted = Controller::new(
        backend,
        Arc::new(Notes::default()),
        backend_clock(),
        Prefs::default(),
        None,
        tokio::runtime::Handle::current(),
        Arc::new(|_| {}),
    );
    restarted.change(|store| store.apply_snap(disconnected.clone(), restarted.clock.now()));
    restarted.expire_resumed_start();
    assert_eq!(restarted.state().phase, Phase::Booting);
    assert_eq!(
        restarted.state().boot_start_ms,
        Some(restarted.clock.now().timestamp_millis())
    );
    assert!(!restarted.store.lock().unwrap().runtime_seen_ready());
    // Simulate forty-one minutes of observed not-ready state after a restart.
    restarted.change(|store| {
        store.stop_done();
        store.apply_snap(
            disconnected,
            restarted.clock.now() - chrono::TimeDelta::minutes(41),
        )
    });
    restarted.expire_resumed_start();
    let Phase::Failed { message } = restarted.state().phase else {
        panic!("observed boot did not fail")
    };
    assert!(message.contains("40 minutes of observation"));
    restarted.dismiss();
    assert!(!restarted.state().start_allowed);
    assert_eq!(restarted.runtime_target().unwrap(), owner);
    assert!(
        std::fs::read_to_string(restarted.log_path().unwrap())
            .unwrap()
            .contains("resumed_start_timeout")
    );
}
#[tokio::test(start_paused = true)]
async fn bounded_snapshot_reports_unknown_without_deleting_or_losing_owner() {
    let (controller, backend, provider) = cloud_owned_fixture();
    controller.load_config().await;
    controller.refresh().await;
    let owner = controller.runtime_target().unwrap();
    backend.snapshot_delay_ms.store(46_000, Ordering::SeqCst);
    controller.refresh().await;
    assert!(
        controller
            .state()
            .warning
            .unwrap()
            .contains("status is unknown")
    );
    assert_eq!(controller.runtime_target().unwrap(), owner);
    assert!(provider.calls().is_empty());
    assert_eq!(backend.down_calls.load(Ordering::SeqCst), 0);
    assert!(
        std::fs::read_to_string(controller.log_path().unwrap())
            .unwrap()
            .contains("snapshot_failed")
    );
}

#[tokio::test(start_paused = true)]
async fn stop_at_ready_event_cleans_the_successful_runtime() {
    let (old, backend, provider) = fixture(0, false, false);
    drop(old);
    let owner = Arc::new(Mutex::new(std::sync::Weak::<Controller>::new()));
    let captured = owner.clone();
    let stopped = Arc::new(tokio::sync::Notify::new());
    let notify = stopped.clone();
    let once = AtomicBool::new(false);
    let controller = Controller::new(
        backend.clone(),
        Arc::new(Notes::default()),
        backend_clock(),
        Prefs::default(),
        None,
        tokio::runtime::Handle::current(),
        Arc::new(move |state| {
            if state.phase == Phase::Ready
                && state.up_phase.as_deref() == Some("ready")
                && !once.swap(true, Ordering::SeqCst)
            {
                let controller = captured.lock().unwrap().upgrade().unwrap();
                controller.stop();
                notify.notify_one();
            }
        }),
    );
    *owner.lock().unwrap() = Arc::downgrade(&controller);
    start(&controller, &provider).await;
    stopped.notified().await;
    join_stop(&controller).await;
    assert_eq!(controller.state().phase, Phase::Off);
    assert!(provider.running.lock().unwrap().is_empty());
    assert_eq!(backend.down_calls.load(Ordering::SeqCst), 2);
    let logs = std::fs::read_to_string(controller.log_path().unwrap()).unwrap();
    assert!(logs.contains("start_finished"));
    assert!(!logs.contains("ended without a Ready"));
    assert!(logs.contains("stop_finished"));
}

#[tokio::test(start_paused = true)]
async fn outer_start_worker_panic_is_observed_and_cleanup_blocks_another_rental() {
    let (old, backend, provider) = fixture(20, false, false);
    drop(old);
    let panicked = Arc::new(AtomicBool::new(false));
    let once = panicked.clone();
    let controller = Controller::new(
        backend.clone(),
        Arc::new(Notes::default()),
        backend_clock(),
        Prefs::default(),
        None,
        tokio::runtime::Handle::current(),
        Arc::new(move |state| {
            assert!(
                state.up_phase.as_deref() != Some("create") || once.swap(true, Ordering::SeqCst),
                "private request content must not appear in diagnostics"
            );
        }),
    );
    start(&controller, &provider).await;
    let task = controller.active.lock().unwrap().up.take().unwrap();
    let failure = task.await.unwrap().unwrap_err();
    assert_eq!(failure.kind, "worker");
    assert!(panicked.load(Ordering::SeqCst));
    assert!(matches!(controller.state().phase, Phase::Failed { .. }));
    assert!(!controller.state().start_allowed);
    controller.submit_start().unwrap().await.unwrap().unwrap();
    assert_eq!(backend.up_calls.load(Ordering::SeqCst), 1);
    let logs = std::fs::read_to_string(controller.log_path().unwrap()).unwrap();
    assert!(logs.contains("start_failed"));
    assert!(!logs.contains("private request content"));
    controller.stop();
    join_stop(&controller).await;
    assert!(provider.running.lock().unwrap().is_empty());
    assert_eq!(controller.state().phase, Phase::Off);
}

#[tokio::test]
async fn malformed_config_never_logs_an_opaque_credential() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("config.env");
    let credential = "opaque-credential-with-no-known-prefix-891273";
    std::fs::write(&path, format!("VASTAI_API_KEY=\"{credential}\n")).unwrap();
    let backend = Arc::new(crate::backend::CoreBackend::new(path).unwrap());
    assert!(backend.diagnostic_secrets().is_err());
    let controller = Controller::new(
        backend,
        Arc::new(Notes::default()),
        backend_clock(),
        Prefs::default(),
        None,
        tokio::runtime::Handle::current(),
        Arc::new(|_| {}),
    );
    controller.load_config().await;
    let state = controller.state();
    assert_eq!(state.warning.as_deref(), Some(CONFIG_ERROR_MESSAGE));
    assert!(!serde_json::to_string(&state).unwrap().contains(credential));
    let logs = std::fs::read_to_string(controller.log_path().unwrap()).unwrap();
    assert!(logs.contains("config_failed"));
    assert!(!logs.contains(credential));
}
