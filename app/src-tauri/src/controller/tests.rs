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
use lobo_proto::{
    ConfigShow, Instance, Listing, Manifest, Readiness, Snap, Stage, Status, UpRequest,
};
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
    d: control::Deps,
    calls: Mutex<Vec<&'static str>>,
    ready: AtomicBool,
    up_calls: AtomicUsize,
    down_calls: AtomicUsize,
    memory_mode: AtomicUsize,
    memory_delay_ms: AtomicU64,
    memory_started: tokio::sync::Notify,
    start_deny: AtomicBool,
    owner: Arc<Mutex<Option<RuntimeTarget>>>,
    prepare_gate: Mutex<Option<PreparationGate>>,
    prepare_started: tokio::sync::Notify,
    prepare_calls: AtomicUsize,
    down_targets: Mutex<Vec<RuntimeTarget>>,
    snapshot_delay_ms: AtomicU64,
    snapshot_started: tokio::sync::Notify,
}
#[async_trait]
impl Backend for FakeBackend {
    fn config_path(&self) -> PathBuf {
        PathBuf::from("/fixture")
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
                local_supported: true,
                default_provider: "runpod".into(),
                default_model: "q8".into(),
                local_port: 8931,
                ..Default::default()
            },
        ))
    }
    async fn models(&self) -> Result<Listing> {
        self.calls.lock().unwrap().push("models");
        Ok(Listing::default())
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
        let owner = captured.or(self.load_owner()?);
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
    async fn local_memory(
        &self,
        model: &str,
    ) -> Result<lobo_core::local::memory::MemoryAssessment> {
        let mode = self.memory_mode.load(Ordering::SeqCst);
        let delay = self.memory_delay_ms.load(Ordering::SeqCst);
        self.memory_started.notify_one();
        tokio::time::sleep(Duration::from_millis(delay)).await;
        if mode == 2 {
            return Err(AppError {
                kind: "local".into(),
                message: "probe unavailable".into(),
            });
        }
        Ok(lobo_core::local::memory::assess(
            model,
            8192,
            &lobo_core::local::memory::MemorySnapshot {
                total_bytes: 64 << 30,
                available_bytes: (if mode == 1 { 8 } else { 60 }) << 30,
                metal_limit_bytes: 48 << 30,
            },
        )?)
    }
    fn prepare_up(&self, req: UpRequest) -> Result<PreparedUp> {
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
        if req.provider.as_deref() == Some("local") && self.start_deny.load(Ordering::SeqCst) {
            return Err(AppError {
                kind: "local".into(),
                message: "fresh Start rejected: memory insufficient".into(),
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
        d,
        calls: Mutex::new(vec![]),
        ready: AtomicBool::new(true),
        up_calls: AtomicUsize::new(0),
        down_calls: AtomicUsize::new(0),
        memory_mode: AtomicUsize::new(0),
        memory_delay_ms: AtomicU64::new(0),
        memory_started: tokio::sync::Notify::new(),
        start_deny: AtomicBool::new(false),
        owner,
        prepare_gate: Mutex::new(None),
        prepare_started: tokio::sync::Notify::new(),
        prepare_calls: AtomicUsize::new(0),
        down_targets: Mutex::new(vec![]),
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
    c.load_config(true).await;
    c.refresh(false).await;
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
async fn memory_status(c: &Controller, status: &str) {
    for _ in 0..1000 {
        if c.state()
            .local_memory
            .as_ref()
            .is_some_and(|m| m.status == status)
        {
            return;
        }
        tokio::task::yield_now().await;
    }
    panic!("memory status never became {status}");
}

#[tokio::test(start_paused = true)]
async fn refresh_models_only_when_requested() {
    let (c, b, _) = fixture(20, false, false);
    c.load_config(true).await;
    c.refresh(false).await;
    assert_eq!(*b.calls.lock().unwrap(), ["config", "models", "snapshot"]);
    b.calls.lock().unwrap().clear();
    c.refresh(false).await;
    assert_eq!(*b.calls.lock().unwrap(), ["snapshot"]);
    b.calls.lock().unwrap().clear();
    c.refresh(true).await;
    assert_eq!(*b.calls.lock().unwrap(), ["snapshot", "models"]);
    b.calls.lock().unwrap().clear();
    c.load_config(true).await;
    assert_eq!(*b.calls.lock().unwrap(), ["config", "models"]);
    b.ready.store(false, Ordering::SeqCst);
    c.load_config(false).await;
    b.calls.lock().unwrap().clear();
    c.refresh(false).await;
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
    c.refresh(false).await;
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
    c.refresh(false).await;
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
    c.refresh(false).await;
    assert!(matches!(c.state().phase, Phase::Failed { .. }));
    assert!(c.quit().await.is_err());
    assert_eq!(b.down_calls.load(Ordering::SeqCst), 1);
    p.fail_delete.store(false, Ordering::SeqCst);
    c.quit().await.unwrap();
    assert!(c.shutdown.is_cancelled());
    assert!(p.running.lock().unwrap().is_empty());
}

#[tokio::test(start_paused = true)]
async fn local_memory_denial_unavailable_and_cloud_selection() {
    let (c, b, p) = fixture(0, false, false);
    c.load_config(true).await;
    c.refresh(false).await;
    c.choose(Target::Local);
    b.memory_mode.store(1, Ordering::SeqCst);
    c.refresh_memory().await;
    memory_status(&c, "insufficient").await;
    assert_eq!(c.state().local_memory.unwrap().status, "insufficient");
    b.memory_mode.store(2, Ordering::SeqCst);
    c.refresh_memory().await;
    memory_status(&c, "unavailable").await;
    let unavailable = c.state().local_memory.unwrap();
    assert_eq!(unavailable.status, "unavailable");
    assert_eq!(unavailable.required_bytes, None);
    c.choose(Target::Cloud);
    assert_eq!(c.state().target, Target::Cloud);
    assert_eq!(c.state().local_memory, None);
    assert_eq!(p.rents.load(Ordering::SeqCst), 0);
}

#[tokio::test(start_paused = true)]
async fn stale_memory_response_cannot_replace_new_model_or_target() {
    let (c, b, _) = fixture(0, false, false);
    c.load_config(true).await;
    c.refresh(false).await;
    c.store.lock().unwrap().choose(Target::Local);
    b.memory_delay_ms.store(1000, Ordering::SeqCst);
    let worker = c.clone();
    let old = tokio::spawn(async move { worker.refresh_memory().await });
    b.memory_started.notified().await;
    c.store.lock().unwrap().set_model("q6".into());
    b.memory_delay_ms.store(0, Ordering::SeqCst);
    b.memory_mode.store(1, Ordering::SeqCst);
    c.refresh_memory().await;
    old.await.unwrap();
    assert_eq!(c.state().local_memory.as_ref().unwrap().model, "q6");
    assert_eq!(
        c.state().local_memory.as_ref().unwrap().status,
        "insufficient"
    );
    c.store.lock().unwrap().choose(Target::Cloud);
    c.refresh_memory().await;
    assert_eq!(c.state().local_memory, None);
}

#[tokio::test(start_paused = true)]
async fn displayed_pass_does_not_authorize_a_denied_start_or_retry() {
    let (c, b, p) = fixture(0, false, false);
    c.load_config(true).await;
    c.refresh(false).await;
    c.choose(Target::Local);
    c.refresh_memory().await;
    memory_status(&c, "ready").await;
    assert_eq!(c.state().local_memory.unwrap().status, "ready");
    // The next display sample must reflect the same memory loss as admission.
    // The owned worker now refreshes status after a rejected admission too.
    b.memory_mode.store(1, Ordering::SeqCst);
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
    memory_status(&c, "insufficient").await;
    assert!(c.submit_start().unwrap().await.unwrap().is_err());
    assert_eq!(p.rents.load(Ordering::SeqCst), 0);
    assert_eq!(b.prepare_calls.load(Ordering::SeqCst), 2);
    assert_eq!(b.up_calls.load(Ordering::SeqCst), 0);
    assert!(!c.store.lock().unwrap().needs_cleanup());
}

#[test]
fn queued_local_start_never_redirects_to_cloud_and_rejects_changed_config() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .max_blocking_threads(1)
        .build()
        .unwrap();
    runtime.block_on(async {
        let (c, b, p) = fixture(0, false, false);
        c.load_config(true).await;
        c.refresh(false).await;
        c.choose(Target::Local);
        let (started, wait_started) = tokio::sync::oneshot::channel();
        let (release, wait_release) = std::sync::mpsc::channel();
        let blocker = tokio::task::spawn_blocking(move || {
            let _ = started.send(());
            wait_release.recv().unwrap();
        });
        wait_started.await.unwrap();
        let reply = c.submit_start().unwrap();
        tokio::task::yield_now().await;
        c.choose(Target::Cloud);
        c.set_model("q6".into());
        c.set_provider("vastai".into());
        // Existing UI policy rejects selection while Start is reserved.
        assert_eq!(c.state().target, Target::Local);
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
    c.load_config(true).await;
    c.refresh(false).await;
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
    c.choose(Target::Local);
    c.set_provider("vastai".into());
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
    c.load_config(true).await;
    c.refresh(false).await;
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

fn local_owned_fixture() -> (
    Arc<Controller>,
    Arc<FakeBackend>,
    Arc<testkit::RecordingProvider>,
) {
    let (c, mut b, _) = fixture(0, false, false);
    drop(c);
    let local = Arc::new(testkit::RecordingProvider::new("local"));
    {
        let mut state = local.state.lock().unwrap();
        state.boot_id = "local-boot".into();
        state.start_id = 100;
        state.instances.push(Instance {
            provider: "local".into(),
            id: "4242".into(),
            api_url: testkit::LOCAL_API_URL.into(),
            agent_url: testkit::LOCAL_AGENT_URL.into(),
            ..Default::default()
        });
    }
    let vast = Arc::new(testkit::RecordingProvider::new("vastai"));
    vast.state.lock().unwrap().broken = true;
    let backend = Arc::get_mut(&mut b).unwrap();
    backend.d.providers.insert("local".into(), local);
    backend.d.providers.insert("vastai".into(), vast.clone());
    let agent = Arc::new(FakeAgent::new(vec![Some(Status {
        stage: Stage::Ready,
        boot_id: "local-boot".into(),
        ..Default::default()
    })]));
    backend.d.new_agent = Arc::new(move |_| agent.clone());
    let c = Controller::new(
        b.clone(),
        Arc::new(Notes::default()),
        backend_clock(),
        Prefs {
            target: Some(Target::Local),
        },
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
async fn local_ready_and_stop_ignore_a_failing_vast_provider() {
    let (c, b, vast) = local_owned_fixture();
    c.load_config(true).await;
    c.refresh(false).await;
    assert_eq!(c.state().phase, Phase::Ready);
    assert!(c.state().is_local);
    assert_eq!(b.load_owner().unwrap().unwrap().provider, "local");
    assert!(vast.calls().is_empty());
    c.choose(Target::Cloud);
    c.set_provider("vastai".into());
    c.stop();
    join_stop(&c).await;
    assert_eq!(b.down_targets.lock().unwrap().len(), 1);
    assert_eq!(b.down_targets.lock().unwrap()[0].provider, "local");
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
    let (c, b, _) = local_owned_fixture();
    c.load_config(true).await;
    b.snapshot_delay_ms.store(1000, Ordering::SeqCst);
    let worker = c.clone();
    let poll = tokio::spawn(async move { worker.refresh(false).await });
    b.snapshot_started.notified().await;
    c.choose(Target::Cloud);
    c.set_provider("runpod".into());
    b.snapshot_delay_ms.store(0, Ordering::SeqCst);
    poll.await.unwrap();
    assert_eq!(b.load_owner().unwrap(), None);
    assert_eq!(c.store.lock().unwrap().runtime(), None);
    c.refresh(false).await;
    assert_eq!(c.state().phase, Phase::Off);
    assert_eq!(b.load_owner().unwrap(), None);
}

#[tokio::test(start_paused = true)]
async fn restart_uses_persisted_local_owner_when_the_ui_selects_cloud() {
    let (old, b, vast) = local_owned_fixture();
    old.load_config(true).await;
    old.refresh(false).await;
    let target = b.load_owner().unwrap().unwrap();
    let c = Controller::new(
        b.clone(),
        Arc::new(Notes::default()),
        backend_clock(),
        Prefs {
            target: Some(Target::Cloud),
        },
        None,
        tokio::runtime::Handle::current(),
        Arc::new(|_| {}),
    );
    c.load_config(true).await;
    c.set_provider("vastai".into());
    c.refresh(false).await;
    assert_eq!(c.state().phase, Phase::Ready);
    assert_eq!(c.runtime_target().unwrap(), target);
    c.stop();
    join_stop(&c).await;
    assert_eq!(b.down_targets.lock().unwrap()[0], target);
    assert!(vast.calls().is_empty());
}
