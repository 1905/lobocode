use super::testkit::*;
use super::*;
use crate::{clock::FixedClock, provider::CreateOpts};
use lobo_proto::{GoTime, Stage};
use std::sync::{
    Mutex,
    atomic::{AtomicUsize, Ordering},
};
use tokio_util::sync::CancellationToken;
fn setup() -> Deps {
    deps(
        Arc::new(FakeRunPod::default()),
        Arc::new(FakeAgent::new(vec![Some(Status {
            stage: Stage::Ready,
            ..Default::default()
        })])),
        Arc::new(FixedClock("2026-09-23T12:00:00Z".parse().unwrap())),
    )
}
struct ProviderStub {
    name: &'static str,
    items: Mutex<Vec<Instance>>,
    lists: AtomicUsize,
    deletes: AtomicUsize,
    list_error: bool,
    delete_error: bool,
    lag: usize,
}
impl ProviderStub {
    fn new(name: &'static str) -> Self {
        Self {
            name,
            items: Mutex::new(vec![Instance {
                provider: name.into(),
                id: name.into(),
                api_url: format!("http://{name}/v1"),
                agent_url: format!("http://{name}"),
                cost_per_hr: 0.5,
                started_at: GoTime::from_utc("2026-09-23T10:00:00Z".parse().unwrap()),
                ..Default::default()
            }]),
            lists: AtomicUsize::new(0),
            deletes: AtomicUsize::new(0),
            list_error: false,
            delete_error: false,
            lag: 0,
        }
    }
}
#[async_trait]
impl Provider for ProviderStub {
    fn name(&self) -> &'static str {
        self.name
    }
    fn replaceable(&self) -> bool {
        true
    }
    async fn rent(
        &self,
        _: &CreateOpts,
        _: CancellationToken,
        _: &(dyn Fn(String) + Sync),
    ) -> Result<Instance> {
        unreachable!()
    }
    async fn list(&self) -> Result<Vec<Instance>> {
        self.lists.fetch_add(1, Ordering::SeqCst);
        if self.list_error {
            return Err(Error::Other("list broke".into()));
        }
        if self.deletes.load(Ordering::SeqCst) > 0
            && !self.delete_error
            && self.lists.load(Ordering::SeqCst) > self.lag + 1
        {
            return Ok(vec![]);
        }
        Ok(self.items.lock().unwrap().clone())
    }
    async fn get(&self, id: &str) -> Result<Instance> {
        self.items
            .lock()
            .unwrap()
            .iter()
            .find(|i| i.id == id)
            .cloned()
            .ok_or(Error::NotFound)
    }
    async fn delete(&self, _: &str) -> Result<()> {
        self.deletes.fetch_add(1, Ordering::SeqCst);
        if self.delete_error {
            Err(Error::Other("delete broke".into()))
        } else {
            Ok(())
        }
    }
}
#[tokio::test]
async fn list_all_keeps_going_on_error() {
    let mut d = setup();
    let mut broken = ProviderStub::new("runpod");
    broken.list_error = true;
    d.providers.insert("runpod".into(), Arc::new(broken));
    d.providers
        .insert("local".into(), Arc::new(ProviderStub::new("local")));
    let (items, error) = list_all(&d).await;
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].provider, "local");
    assert_eq!(error.unwrap().to_string(), "runpod: list broke");
}
#[test]
fn lifecycle_limits() {
    assert_eq!(MAX_GPU_RETRIES, 4);
    // Complete images include 22–29 GB of weights; the old image deadline was 6m.
    assert_eq!(CONTAINER_TIMEOUT, Duration::from_secs(30 * 60));
    assert_eq!(STALE_SLACK, Duration::from_secs(15));
    assert_eq!(POD_CHECK_EVERY, Duration::from_secs(30));
}
#[tokio::test]
async fn down_and_snapshot() {
    let mut d = setup();
    let p = Arc::new(ProviderStub::new("runpod"));
    d.providers.insert("runpod".into(), p.clone());
    let snap = snapshot(&d).await.unwrap();
    assert!(!snap.down);
    assert_eq!(snap.pod.unwrap().id, "runpod");
    assert_eq!(snap.status.unwrap().stage, Stage::Ready);
    assert_eq!(snap.version.unwrap().git_sha, "abc1234");
    assert_eq!(down(&d).await.unwrap(), 1.0);
    assert_eq!(p.deletes.load(Ordering::SeqCst), 1);
    assert!(snapshot(&d).await.unwrap().down);
}
#[tokio::test]
async fn down_waits_for_list_to_catch_up() {
    let mut d = setup();
    let mut p = ProviderStub::new("runpod");
    p.lag = 2;
    let p = Arc::new(p);
    d.providers.insert("runpod".into(), p.clone());
    let start = tokio::time::Instant::now();
    down(&d).await.unwrap();
    assert_eq!(p.lists.load(Ordering::SeqCst), 4);
    assert!(start.elapsed() >= Duration::from_millis(2));
}
#[tokio::test]
async fn down_keeps_going_when_one_provider_fails() {
    for list_error in [true, false] {
        let mut d = setup();
        let mut bad = ProviderStub::new("runpod");
        bad.list_error = list_error;
        bad.delete_error = !list_error;
        let good = Arc::new(ProviderStub::new("vast"));
        d.providers.insert("runpod".into(), Arc::new(bad));
        d.providers.insert("vast".into(), good.clone());
        let e = down(&d).await.unwrap_err().to_string();
        assert_eq!(good.deletes.load(Ordering::SeqCst), 1);
        assert!(
            e.contains(if list_error {
                "list: runpod: list broke"
            } else {
                "delete runpod runpod: delete broke"
            }),
            "{e}"
        );
    }
}
#[tokio::test]
async fn snapshot_no_domain_and_local() {
    for name in ["runpod", "vast", "local"] {
        let mut d = setup();
        d.cfg.domain.clear();
        d.providers.clear();
        d.providers
            .insert(name.into(), Arc::new(ProviderStub::new(name)));
        let ag = Arc::new(FakeAgent::default());
        let a = ag.clone();
        d.new_agent = Arc::new(move |_| a.clone());
        let s = snapshot(&d).await.unwrap();
        assert_eq!(s.pod.unwrap().provider, name);
        assert_eq!(
            ag.calls(),
            2,
            "an explicit agent URL works without a domain"
        );
    }
}
#[tokio::test]
async fn target_table() {
    for (running, broken, domain, want) in [
        ("local", true, "", "http://local/v1"),
        ("local", false, "cloud", "http://local/v1"),
        ("runpod", false, "cloud", "http://runpod/v1"),
        ("vast", false, "cloud", "http://vast/v1"),
        ("", true, "cloud", "list broke"),
        ("", false, "", "nothing running"),
        ("", false, "cloud", "https://cloud/v1"),
    ] {
        let mut d = setup();
        d.providers.clear();
        d.cfg.domain = domain.into();
        if !running.is_empty() {
            d.providers
                .insert(running.into(), Arc::new(ProviderStub::new(running)));
        }
        if broken {
            let mut bad = ProviderStub::new("runpod");
            bad.list_error = true;
            d.providers.insert("runpod".into(), Arc::new(bad));
        }
        let a = Arc::new(FakeAgent::default());
        let ag = a.clone();
        d.new_agent = Arc::new(move |_| ag.clone());
        match target(&d).await {
            Ok((agent, url)) => {
                assert_eq!(url, want);
                assert_eq!(
                    Arc::as_ptr(&agent) as *const (),
                    Arc::as_ptr(&a) as *const ()
                );
            }
            Err(e) => assert!(e.to_string().contains(want), "{e}"),
        }
    }
}
