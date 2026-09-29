use super::testkit::*;
use super::*;
use crate::{
    clock::{FixedClock, StepClock},
    provider::{CreateOpts, runpod::Pod},
};
use lobo_proto::{Stage, UpEvent};
use std::sync::{
    Mutex,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};
use tokio_util::sync::CancellationToken;
fn test_deps(script: Vec<Option<Status>>) -> (Deps, Arc<FakeRunPod>) {
    let rp = Arc::new(FakeRunPod::default());
    let clock = Arc::new(StepClock::new(
        "2026-09-23T10:00:00Z".parse().unwrap(),
        Duration::from_secs(1),
    ));
    (
        deps(rp.clone(), Arc::new(FakeAgent::new(script)), clock),
        rp,
    )
}
async fn collect(d: Deps, o: UpOpts) -> (Vec<UpEvent>, Result<()>) {
    let mut op = up(d, o, CancellationToken::new());
    let mut receiver = op.take_events().unwrap();
    let mut events = vec![];
    while let Some(event) = receiver.recv().await {
        events.push(event);
    }
    (events, op.wait().await)
}
fn ready() -> Vec<Option<Status>> {
    vec![Some(Status {
        stage: Stage::Ready,
        ..Default::default()
    })]
}
#[tokio::test]
async fn up_happy_and_events_helper() {
    let (d, rp) = test_deps(boot_script());
    let (events, result) = collect(d, UpOpts::default()).await;
    result.unwrap();
    assert_eq!(
        events.iter().map(|e| e.phase.as_str()).collect::<Vec<_>>(),
        vec!["create", "image", "tunnel", "download", "load", "ready"]
    );
    let r = events.last().unwrap().ready.as_ref().unwrap();
    assert_eq!(r.url, "https://lobo.example.com/v1");
    assert_eq!(r.git_sha, "abc1234");
    assert_eq!(r.attempts, 1);
    assert!(r.elapsed_ns > 0);
    assert_eq!(events[3].download.as_ref().unwrap().bytes, 12357400000);
    {
        let s = rp.state.lock().unwrap();
        assert_eq!(s.created.len(), 1);
        assert_eq!(s.created[0].cloud_type, "COMMUNITY");
        assert_eq!(s.created[0].opts.boot_id.len(), 16);
        assert!(s.deleted.is_empty());
    }
    assert_eq!(
        testkit::events(boot_script(), &[])
            .await
            .last()
            .unwrap()
            .phase,
        "ready"
    );
}
#[tokio::test]
async fn up_preflight_rejects_before_renting() {
    for case in [
        "running",
        "unconfigured",
        "image-release",
        "ctx",
        "idle",
        "life",
        "model",
    ] {
        let (mut d, rp) = test_deps(ready());
        let mut o = UpOpts::default();
        let want = match case {
            "running" => {
                rp.state.lock().unwrap().pods.push(Pod {
                    id: "existing".into(),
                    name: "lobo".into(),
                    ..Default::default()
                });
                "already running"
            }
            "unconfigured" => {
                o.provider = "missing".into();
                "not configured"
            }
            "image-release" => {
                o.image = "image".into();
                o.release = "release".into();
                "use one"
            }
            "ctx" => {
                o.ctx = 2;
                "bad options"
            }
            "idle" => {
                o.idle_min = -1;
                "bad options"
            }
            "life" => {
                o.max_life = Duration::from_secs(1);
                "bad options"
            }
            _ => {
                o.model = "q2".into();
                d.cfg.pod_image = "image".into();
                "unknown model"
            }
        };
        let (events, result) = collect(d, o).await;
        assert!(result.unwrap_err().to_string().contains(want), "{case}");
        assert!(events.last().unwrap().done);
        assert!(rp.state.lock().unwrap().created.is_empty());
    }
}
struct PanicRelease;
#[async_trait]
impl ReleaseResolver for PanicRelease {
    async fn resolve(&self, _: &str) -> Result<Resolved> {
        panic!("must not resolve")
    }
}
#[tokio::test]
async fn up_baked_image_uses_builtin_defaults() {
    for from_config in [false, true] {
        let (mut d, rp) = test_deps(ready());
        d.releases = Arc::new(PanicRelease);
        let mut o = UpOpts::default();
        if from_config {
            d.cfg.pod_image = "baked".into();
        } else {
            o.image = "baked".into();
        }
        collect(d, o).await.1.unwrap();
        let s = rp.state.lock().unwrap();
        let o = &s.created[0].opts;
        assert_eq!((o.model.as_str(), o.ctx, o.idle_min), ("q8", 65536, 30));
        assert_eq!(
            o.expires_at,
            "2026-09-23T22:00:01Z"
                .parse::<chrono::DateTime<chrono::Utc>>()
                .unwrap()
        );
        assert_eq!(o.image, "baked");
        assert!(o.release_url.is_empty());
        assert!(o.release_sha256.is_empty());
    }
}
#[tokio::test]
async fn up_overrides() {
    let (d, rp) = test_deps(ready());
    collect(
        d,
        UpOpts {
            model: "q6".into(),
            ctx: 4096,
            idle_min: 7,
            max_life: Duration::from_secs(3600),
            cloud: "secure".into(),
            ..Default::default()
        },
    )
    .await
    .1
    .unwrap();
    let s = rp.state.lock().unwrap();
    let o = &s.created[0].opts;
    assert_eq!(
        (o.model.as_str(), o.ctx, o.idle_min, o.cloud.as_str()),
        ("q6", 4096, 7, "secure")
    );
}
#[tokio::test]
async fn up_agent_failed_and_expiry() {
    for (stage, reason, detail, want) in [
        (Stage::Failed, "", "download: sha256 mismatch", "sha256"),
        (Stage::Terminating, "expired", "", "watchdog: expired"),
    ] {
        let (d, _) = test_deps(vec![Some(Status {
            stage,
            kill_reason: reason.into(),
            stage_detail: detail.into(),
            ..Default::default()
        })]);
        let (events, result) = collect(d, UpOpts::default()).await;
        let error = result.unwrap_err().to_string();
        assert!(error.contains(want));
        assert!(error.contains("last log line"));
        assert_eq!(events.last().unwrap().phase, "failed");
    }
}
#[tokio::test]
async fn up_timeout_and_failed_polls_do_not_reset_stall() {
    let (d, rp) = test_deps(vec![
        Some(Status {
            stage: Stage::Download,
            ..Default::default()
        }),
        None,
    ]);
    let (events, result) = collect(
        d,
        UpOpts {
            timeout: Duration::from_secs(5),
            ..Default::default()
        },
    )
    .await;
    assert!(result.unwrap_err().to_string().contains("no progress"));
    assert_eq!(events.last().unwrap().phase, "terminated");
    assert_eq!(rp.state.lock().unwrap().deleted, vec!["pod1"]);
}
#[tokio::test]
async fn up_capacity_and_network_tiers() {
    for (cloud, secure_missing, max_mbps, no_capacity, want_calls) in [
        ("", false, 5000.0, false, 2),
        ("secure", true, 5000.0, false, 7),
        ("", true, 0.0, true, 5),
    ] {
        let (d, rp) = test_deps(ready());
        {
            let mut s = rp.state.lock().unwrap();
            s.max_mbps = max_mbps;
            if secure_missing {
                s.no_cap.insert("SECURE".into());
            }
            if no_capacity {
                s.no_cap.insert("COMMUNITY".into());
            }
        }
        let result = collect(
            d,
            UpOpts {
                cloud: cloud.into(),
                ..Default::default()
            },
        )
        .await
        .1;
        if no_capacity {
            assert!(result.unwrap_err().to_string().contains("no gpu capacity"));
        } else {
            result.unwrap();
        }
        let s = rp.state.lock().unwrap();
        assert_eq!(s.created.len(), want_calls);
        if cloud.is_empty() {
            assert!(s.created.iter().all(|c| c.cloud_type == "COMMUNITY"));
        }
        if cloud == "secure" {
            assert!(s.created[..5].iter().all(|c| c.cloud_type == "SECURE"));
        }
    }
}
#[tokio::test]
async fn up_replaces_bad_gpu_and_ignores_stale_status() {
    let bad = Status {
        stage: Stage::Failed,
        stage_detail: "gpu: no CUDA".into(),
        ..Default::default()
    };
    let stale = Status {
        uptime_s: 9999,
        ..bad.clone()
    };
    let (d, rp) = test_deps(vec![
        Some(bad),
        Some(stale),
        Some(Status {
            stage: Stage::Ready,
            ..Default::default()
        }),
    ]);
    let (events, result) = collect(d, UpOpts::default()).await;
    result.unwrap();
    assert_eq!(events.last().unwrap().ready.as_ref().unwrap().attempts, 2);
    let s = rp.state.lock().unwrap();
    assert_eq!(s.created.len(), 2);
    assert_eq!(s.deleted.len(), 1);
    assert_ne!(s.created[0].opts.boot_id, s.created[1].opts.boot_id);
}
#[tokio::test]
async fn up_gives_up_after_four_bad_hosts() {
    let (d, rp) = test_deps(vec![Some(Status {
        stage: Stage::Failed,
        stage_detail: "download: host: too slow".into(),
        ..Default::default()
    })]);
    assert!(
        collect(d, UpOpts::default())
            .await
            .1
            .unwrap_err()
            .to_string()
            .contains("gave up: 4 pods")
    );
    let s = rp.state.lock().unwrap();
    assert_eq!(s.created.len(), 4);
    assert_eq!(s.deleted.len(), 4);
}
#[tokio::test]
async fn up_local_and_local_failure_stops_without_rerent() {
    for fail in [false, true] {
        let (mut d, rp) = test_deps(ready());
        let local = Arc::new(FakeLocal::default());
        d.providers.insert("local".into(), local.clone());
        d.releases = Arc::new(PanicRelease);
        let script = if fail {
            vec![Some(Status {
                stage: Stage::Failed,
                stage_detail: "gpu: no Metal".into(),
                ..Default::default()
            })]
        } else {
            local_boot_script()
        };
        let agent = Arc::new(FakeAgent::new(script));
        d.new_agent = Arc::new(move |url| {
            assert_eq!(url, LOCAL_AGENT_URL);
            agent.clone()
        });
        let (events, result) = collect(
            d,
            UpOpts {
                provider: "local".into(),
                ..Default::default()
            },
        )
        .await;
        if fail {
            assert!(
                result
                    .unwrap_err()
                    .to_string()
                    .contains("local run stopped")
            );
        } else {
            result.unwrap();
            assert_eq!(
                events.last().unwrap().ready.as_ref().unwrap().url,
                LOCAL_API_URL
            );
        }
        let s = local.state.lock().unwrap();
        assert_eq!(s.created.len(), 1);
        assert_eq!(s.deleted.len(), usize::from(fail));
        assert!(s.created[0].image.is_empty());
        assert!(s.created[0].model_url.is_empty());
        assert!(s.created[0].cf_tunnel_token.is_empty());
        assert!(rp.state.lock().unwrap().created.is_empty());
    }
}
struct SlowProvider {
    running: Mutex<Vec<Instance>>,
    rents: AtomicUsize,
    deletes: AtomicUsize,
    delay: Duration,
    delete_fails: AtomicBool,
    panic_after_create: bool,
    notes: usize,
    cancel_on_delete: Option<CancellationToken>,
}
impl SlowProvider {
    fn new(delay: Duration) -> Self {
        Self {
            running: Mutex::new(vec![]),
            rents: AtomicUsize::new(0),
            deletes: AtomicUsize::new(0),
            delay,
            delete_fails: AtomicBool::new(false),
            panic_after_create: false,
            notes: 0,
            cancel_on_delete: None,
        }
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
    ) -> Result<Instance> {
        self.rents.fetch_add(1, Ordering::SeqCst);
        for n in 0..self.notes {
            note(format!("note {n}"));
        }
        tokio::time::sleep(self.delay).await;
        let i = Instance {
            provider: "runpod".into(),
            id: "slow".into(),
            ..Default::default()
        };
        self.running.lock().unwrap().push(i.clone());
        assert!(!self.panic_after_create, "worker exploded");
        Ok(i)
    }
    async fn list(&self) -> Result<Vec<Instance>> {
        Ok(self.running.lock().unwrap().clone())
    }
    async fn get(&self, id: &str) -> Result<Instance> {
        self.running
            .lock()
            .unwrap()
            .iter()
            .find(|i| i.id == id)
            .cloned()
            .ok_or(Error::NotFound)
    }
    async fn delete(&self, _: &str) -> Result<()> {
        self.deletes.fetch_add(1, Ordering::SeqCst);
        if let Some(cancel) = &self.cancel_on_delete {
            cancel.cancel();
        }
        if self.delete_fails.load(Ordering::SeqCst) {
            return Err(Error::Other("delete denied".into()));
        }
        self.running.lock().unwrap().clear();
        Ok(())
    }
}
fn slow_deps(p: Arc<SlowProvider>) -> Deps {
    let (mut d, _) = test_deps(ready());
    d.providers.insert("runpod".into(), p);
    d.clock = Arc::new(FixedClock("2026-09-23T10:00:00Z".parse().unwrap()));
    d
}
async fn started(p: &SlowProvider) {
    while p.rents.load(Ordering::SeqCst) == 0 {
        tokio::task::yield_now().await;
    }
}
#[tokio::test(start_paused = true)]
async fn up_cancel_during_121_second_rent_awaits_and_deletes_result() {
    let p = Arc::new(SlowProvider::new(Duration::from_secs(121)));
    let d = slow_deps(p.clone());
    let mut operation = up(d, UpOpts::default(), CancellationToken::new());
    let mut events = operation.take_events().unwrap();
    started(&p).await;
    tokio::time::advance(Duration::from_secs(1)).await;
    operation.cancel();
    assert!(
        tokio::time::timeout(Duration::from_secs(119), operation.wait())
            .await
            .is_err()
    );
    assert_eq!(p.deletes.load(Ordering::SeqCst), 0);
    assert!(matches!(
        events.try_recv(),
        Err(tokio::sync::mpsc::error::TryRecvError::Empty)
    ));
    assert!(matches!(operation.wait().await, Err(Error::Cancelled)));
    assert_eq!(p.deletes.load(Ordering::SeqCst), 1);
    assert_eq!(p.rents.load(Ordering::SeqCst), 1);
    assert_eq!(events.recv().await.unwrap().phase, "cancelled");
    assert!(events.recv().await.is_none());
}
#[tokio::test(start_paused = true)]
async fn up_full_and_closed_receivers_cannot_block_cleanup() {
    for closed in [false, true] {
        let mut provider = SlowProvider::new(Duration::from_secs(20));
        provider.notes = 1000;
        let p = Arc::new(provider);
        let mut op = up(
            slow_deps(p.clone()),
            UpOpts::default(),
            CancellationToken::new(),
        );
        let mut events = op.take_events();
        started(&p).await;
        if closed {
            drop(events.take());
        } else {
            op.cancel();
        }
        assert!(matches!(op.wait().await, Err(Error::Cancelled)));
        assert_eq!(p.deletes.load(Ordering::SeqCst), 1);
        if let Some(mut events) = events {
            let mut last = None;
            while let Some(e) = events.recv().await {
                last = Some(e);
            }
            assert_eq!(last.unwrap().phase, "cancelled");
        }
    }
}
#[tokio::test(start_paused = true)]
async fn up_failed_cleanup_stays_failed_and_recorded() {
    let p = Arc::new(SlowProvider::new(Duration::from_secs(20)));
    p.delete_fails.store(true, Ordering::SeqCst);
    let d = slow_deps(p.clone());
    let state = d.operations.clone();
    let mut op = up(d, UpOpts::default(), CancellationToken::new());
    let mut events = op.take_events().unwrap();
    started(&p).await;
    op.cancel();
    assert_eq!(op.wait().await.unwrap_err().kind(), "unresolved_create");
    assert_eq!(events.recv().await.unwrap().phase, "failed");
    assert!(
        state
            .acquire(&CancellationToken::new())
            .await
            .unwrap()
            .pending()
            .is_some()
    );
}
#[tokio::test(start_paused = true)]
async fn up_worker_panic_is_reported_and_created_instance_is_cleaned() {
    let mut p = SlowProvider::new(Duration::ZERO);
    p.panic_after_create = true;
    let p = Arc::new(p);
    let (events, result) = collect(slow_deps(p.clone()), UpOpts::default()).await;
    assert!(result.unwrap_err().to_string().contains("worker exploded"));
    assert_eq!(events.last().unwrap().phase, "failed");
    assert_eq!(p.deletes.load(Ordering::SeqCst), 1);
    assert!(p.running.lock().unwrap().is_empty());
}
#[tokio::test(start_paused = true)]
async fn unresolved_record_blocks_another_up_until_reconciled() {
    let p = Arc::new(SlowProvider::new(Duration::ZERO));
    let d = slow_deps(p.clone());
    {
        let mut guard = d
            .operations
            .acquire(&CancellationToken::new())
            .await
            .unwrap();
        guard
            .record(operation_state::PendingCreate {
                provider: "runpod".into(),
                boot_id: "old-boot".into(),
                before: vec![],
                instance_id: None,
            })
            .unwrap();
    }
    assert_eq!(
        collect(d.clone(), UpOpts::default())
            .await
            .1
            .unwrap_err()
            .kind(),
        "unresolved_create"
    );
    assert_eq!(p.rents.load(Ordering::SeqCst), 0);
    p.running.lock().unwrap().push(Instance {
        provider: "runpod".into(),
        id: "late".into(),
        ..Default::default()
    });
    collect(d, UpOpts::default()).await.1.unwrap();
    assert_eq!(p.deletes.load(Ordering::SeqCst), 1);
    assert_eq!(p.rents.load(Ordering::SeqCst), 1);
}

struct Signer;
#[async_trait]
impl Presigner for Signer {
    async fn presign_get(&self, key: &str, ttl: Duration) -> Result<String> {
        assert!(key.starts_with("models/"));
        assert_eq!(ttl, Duration::from_secs(12 * 3600));
        Ok(format!("https://signed/{key}?signature=secret"))
    }
}
#[tokio::test]
async fn up_model_sources_and_secret_boundaries() {
    for source in ["r2", "feesh", "public", "ssh"] {
        let tmp = tempfile::tempdir().unwrap();
        let key = tmp.path().join("key");
        std::fs::write(&key, b"ssh-private-key").unwrap();
        let (mut d, rp) = test_deps(ready());
        d.presign = Some(Arc::new(Signer));
        d.cfg.feesh_http_url = "https://feesh/".into();
        d.cfg.model_source = "ssh://lobo@host:22/".into();
        d.cfg.model_ssh_key_file = key.to_string_lossy().into();
        d.cfg.model_ssh_host_key = "pinned".into();
        d.cfg.runpod_api_key = "secret-runpod".into();
        d.cfg.vast_api_key = "secret-vast".into();
        d.cfg.r2.access_key = "secret-r2-access".into();
        d.cfg.r2.secret_key = "secret-r2-private".into();
        collect(
            d,
            UpOpts {
                source: source.into(),
                conns: if source == "ssh" { 16 } else { 0 },
                ..Default::default()
            },
        )
        .await
        .1
        .unwrap();
        let s = rp.state.lock().unwrap();
        let o = &s.created[0].opts;
        assert_eq!(
            o.dl_conns,
            if matches!(source, "r2" | "feesh") {
                32
            } else {
                8
            }
        );
        match source {
            "r2" => {
                assert!(o.model_url.starts_with("https://signed/models/"));
                assert!(o.model_fallback.starts_with("https://feesh/"));
            }
            "feesh" => {
                assert!(o.model_url.starts_with("https://feesh/"));
                assert!(o.model_fallback.is_empty());
            }
            "ssh" => {
                use base64::Engine;
                assert_eq!(o.model_url, "ssh://lobo@host:22");
                assert_eq!(
                    base64::engine::general_purpose::STANDARD
                        .decode(&o.model_ssh_key)
                        .unwrap(),
                    b"ssh-private-key"
                );
                assert_eq!(o.model_host_key, "pinned");
            }
            _ => assert!(o.model_url.starts_with("https://pub-x.r2.dev/models/")),
        }
        let payloads = [
            crate::provider::runpod::build_create_payload(o, "COMMUNITY", 0.0).to_string(),
            crate::provider::vast::create_body(o).to_string(),
        ];
        for payload in payloads {
            for secret in [
                "secret-runpod",
                "secret-vast",
                "secret-r2-access",
                "secret-r2-private",
            ] {
                assert!(!payload.contains(secret));
            }
        }
    }
}
#[tokio::test]
async fn up_min_mbps() {
    for (flag, config, want) in [
        (0, "", 100),
        (0, "200", 200),
        (50, "200", 50),
        (0, "bad", 100),
    ] {
        let (mut d, rp) = test_deps(ready());
        d.cfg.min_mbps = config.into();
        collect(
            d,
            UpOpts {
                min_mbps: flag,
                ..Default::default()
            },
        )
        .await
        .1
        .unwrap();
        assert_eq!(rp.state.lock().unwrap().created[0].opts.min_mbps, want);
    }
}
#[tokio::test]
async fn up_wrong_boot_status_is_ignored() {
    let (d, rp) = test_deps(vec![
        Some(Status {
            stage: Stage::Failed,
            boot_id: "other".into(),
            stage_detail: "gpu: broken".into(),
            ..Default::default()
        }),
        Some(Status {
            stage: Stage::Ready,
            ..Default::default()
        }),
    ]);
    let (events, result) = collect(d, UpOpts::default()).await;
    result.unwrap();
    assert_eq!(events[1].phase, "image");
    assert!(rp.state.lock().unwrap().deleted.is_empty());
}
#[tokio::test]
async fn up_replaces_container_that_never_starts() {
    let (mut d, rp) = test_deps(vec![
        None,
        None,
        None,
        Some(Status {
            stage: Stage::Ready,
            ..Default::default()
        }),
    ]);
    d.clock = Arc::new(StepClock::new(
        "2026-09-23T10:00:00Z".parse().unwrap(),
        Duration::from_secs(100),
    ));
    let (events, result) = collect(d, UpOpts::default()).await;
    result.unwrap();
    assert!(
        events
            .iter()
            .any(|e| e.detail.contains("container not started"))
    );
    assert!(!rp.state.lock().unwrap().deleted.is_empty());
}
struct AmbiguousProvider {
    inner: SlowProvider,
    visible_after: usize,
    lists: AtomicUsize,
    reported: AtomicBool,
}
#[async_trait]
impl Provider for AmbiguousProvider {
    fn name(&self) -> &'static str {
        "runpod"
    }
    fn replaceable(&self) -> bool {
        true
    }
    async fn rent(
        &self,
        o: &CreateOpts,
        c: CancellationToken,
        note: &(dyn Fn(String) + Sync),
    ) -> Result<Instance> {
        let _ = self.inner.rent(o, c, note).await?;
        self.reported.store(true, Ordering::SeqCst);
        Err(Error::UnresolvedCreate {
            provider: "runpod".into(),
            boot_id: o.boot_id.clone(),
            detail: "response lost".into(),
        })
    }
    async fn list(&self) -> Result<Vec<Instance>> {
        if self.reported.load(Ordering::SeqCst)
            && self.lists.fetch_add(1, Ordering::SeqCst) < self.visible_after
        {
            return Ok(vec![]);
        }
        self.inner.list().await
    }
    async fn get(&self, id: &str) -> Result<Instance> {
        self.inner.get(id).await
    }
    async fn delete(&self, id: &str) -> Result<()> {
        self.inner.delete(id).await
    }
}
#[tokio::test(start_paused = true)]
async fn uncertain_create_appears_during_reconciliation_without_second_rent() {
    let p = Arc::new(AmbiguousProvider {
        inner: SlowProvider::new(Duration::ZERO),
        visible_after: 2,
        lists: AtomicUsize::new(0),
        reported: AtomicBool::new(false),
    });
    let (mut d, _) = test_deps(ready());
    d.providers.insert("runpod".into(), p.clone());
    collect(d, UpOpts::default()).await.1.unwrap();
    assert_eq!(p.inner.rents.load(Ordering::SeqCst), 1);
    assert_eq!(p.inner.deletes.load(Ordering::SeqCst), 0);
}
#[tokio::test(start_paused = true)]
async fn cancelled_uncertain_create_is_adopted_then_deleted() {
    let p = Arc::new(AmbiguousProvider {
        inner: SlowProvider::new(Duration::from_secs(20)),
        visible_after: 2,
        lists: AtomicUsize::new(0),
        reported: AtomicBool::new(false),
    });
    let (mut d, _) = test_deps(ready());
    d.providers.insert("runpod".into(), p.clone());
    let mut op = up(d, UpOpts::default(), CancellationToken::new());
    let mut events = op.take_events().unwrap();
    started(&p.inner).await;
    op.cancel();
    assert!(matches!(op.wait().await, Err(Error::Cancelled)));
    assert_eq!(p.inner.rents.load(Ordering::SeqCst), 1);
    assert_eq!(p.inner.deletes.load(Ordering::SeqCst), 1);
    assert_eq!(events.recv().await.unwrap().phase, "cancelled");
}
#[tokio::test]
async fn ready_handoff_survives_operation_drop() {
    let p = Arc::new(SlowProvider::new(Duration::ZERO));
    let (events, result) = collect(slow_deps(p.clone()), UpOpts::default()).await;
    result.unwrap();
    assert_eq!(events.last().unwrap().phase, "ready");
    assert_eq!(p.deletes.load(Ordering::SeqCst), 0);
    assert_eq!(p.running.lock().unwrap().len(), 1);
}

#[tokio::test(start_paused = true)]
async fn up_cancel_stops_replace_loop() {
    let cancel = CancellationToken::new();
    let mut p = SlowProvider::new(Duration::ZERO);
    p.cancel_on_delete = Some(cancel.clone());
    let p = Arc::new(p);
    let mut d = slow_deps(p.clone());
    let agent = Arc::new(FakeAgent::new(vec![Some(Status {
        stage: Stage::Failed,
        stage_detail: "gpu: broken".into(),
        ..Default::default()
    })]));
    d.new_agent = Arc::new(move |_| agent.clone());
    let mut op = up(d, UpOpts::default(), cancel);
    let mut events = op.take_events().unwrap();
    assert!(matches!(op.wait().await, Err(Error::Cancelled)));
    let mut last = None;
    while let Some(e) = events.recv().await {
        last = Some(e);
    }
    assert_eq!(last.unwrap().phase, "cancelled");
    assert_eq!(p.rents.load(Ordering::SeqCst), 1);
    assert_eq!(p.deletes.load(Ordering::SeqCst), 1);
}
struct GoneAgent {
    inner: FakeAgent,
    provider: Arc<SlowProvider>,
}
#[async_trait]
impl AgentApi for GoneAgent {
    async fn status(&self) -> Result<Status> {
        let result = self.inner.status().await;
        if result.is_err() {
            self.provider.running.lock().unwrap().clear();
        }
        result
    }
    async fn version(&self) -> Result<Manifest> {
        self.inner.version().await
    }
    async fn logs(&self, n: usize) -> Result<String> {
        self.inner.logs(n).await
    }
}
#[tokio::test]
async fn up_detects_pod_gone_while_agent_silent() {
    let p = Arc::new(SlowProvider::new(Duration::ZERO));
    let mut d = slow_deps(p.clone());
    d.clock = Arc::new(StepClock::new(
        "2026-09-23T10:00:00Z".parse().unwrap(),
        Duration::from_secs(20),
    ));
    let agent = Arc::new(GoneAgent {
        inner: FakeAgent::new(vec![
            Some(Status {
                stage: Stage::Download,
                ..Default::default()
            }),
            None,
        ]),
        provider: p,
    });
    d.new_agent = Arc::new(move |_| agent.clone());
    let (events, result) = collect(d, UpOpts::default()).await;
    assert!(result.unwrap_err().to_string().contains("pod slow is gone"));
    assert_eq!(events.last().unwrap().phase, "failed");
}
