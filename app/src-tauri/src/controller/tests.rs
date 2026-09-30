use super::*;
use async_trait::async_trait;
use lobo_core::{
    clock::FixedClock,
    control::{
        self,
        testkit::{self, FakeAgent, FakeRunPod},
    },
    provider::{CreateOpts, Provider},
};
use lobo_proto::{ConfigShow, Instance, Listing, Readiness, Snap, Stage, Status, UpRequest};
use std::{
    collections::BTreeMap,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
};

struct SlowProvider {
    running: Mutex<Vec<Instance>>,
    rents: AtomicUsize,
    deletes: AtomicUsize,
    delay: Duration,
    fail_delete: AtomicBool,
    panic_create: bool,
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
struct FakeBackend {
    d: control::Deps,
    calls: Mutex<Vec<&'static str>>,
    ready: AtomicBool,
    up_calls: AtomicUsize,
    down_calls: AtomicUsize,
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
    async fn snapshot(&self) -> Result<Snap> {
        self.calls.lock().unwrap().push("snapshot");
        Ok(control::snapshot(&self.d).await?)
    }
    fn up(&self, req: UpRequest, c: CancellationToken) -> Result<control::UpOperation> {
        self.up_calls.fetch_add(1, Ordering::SeqCst);
        self.calls.lock().unwrap().push("up");
        Ok(control::up(
            self.d.clone(),
            control::UpOpts {
                provider: req.provider.unwrap_or_default(),
                model: req.model.unwrap_or_default(),
                ..Default::default()
            },
            c,
        ))
    }
    async fn down(&self) -> Result<f64> {
        self.down_calls.fetch_add(1, Ordering::SeqCst);
        self.calls.lock().unwrap().push("down");
        Ok(control::down(&self.d).await?)
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
    let mut d = testkit::deps(
        Arc::new(FakeRunPod::default()),
        Arc::new(FakeAgent::new(vec![Some(Status {
            stage: Stage::Ready,
            ..Default::default()
        })])),
        clock.clone(),
    );
    d.providers.insert("runpod".into(), p.clone());
    let b = Arc::new(FakeBackend {
        d,
        calls: Mutex::new(vec![]),
        ready: AtomicBool::new(true),
        up_calls: AtomicUsize::new(0),
        down_calls: AtomicUsize::new(0),
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
    c.start();
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
    c.start();
    assert_eq!(b.up_calls.load(Ordering::SeqCst), 1);
    c.stop();
    c.stop();
    c.dismiss();
    c.start();
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
    c.start();
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
async fn worker_panic_cleans_up_and_remains_failed() {
    let (c, _, p) = fixture(20, false, true);
    start(&c, &p).await;
    let h = c.active.lock().unwrap().up.take().unwrap();
    assert!(h.await.unwrap().is_err());
    assert!(p.running.lock().unwrap().is_empty());
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
