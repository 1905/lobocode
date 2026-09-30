//! Deterministic providers and agent scripts shared with CLI tests.
use super::*;
use crate::provider::{
    CreateOpts,
    runpod::{Pod, RunPodApi, RunPodProvider},
};
use lobo_proto::{DownloadProgress, Stage};
use std::{collections::BTreeSet, sync::Mutex};
use tokio_util::sync::CancellationToken;
#[derive(Debug, Clone)]
pub struct Created {
    pub opts: CreateOpts,
    pub cloud_type: String,
    pub min_download_mbps: f64,
}
#[derive(Default)]
pub struct RunPodState {
    pub pods: Vec<Pod>,
    pub no_cap: BTreeSet<String>,
    pub max_mbps: f64,
    pub created: Vec<Created>,
    pub deleted: Vec<String>,
    pub create_err: Option<String>,
}
#[derive(Default)]
pub struct FakeRunPod {
    pub state: Mutex<RunPodState>,
}
#[async_trait]
impl RunPodApi for FakeRunPod {
    async fn create(&self, o: &CreateOpts, cloud: &str, mbps: f64) -> Result<Pod> {
        let mut s = self.state.lock().unwrap();
        s.created.push(Created {
            opts: o.clone(),
            cloud_type: cloud.into(),
            min_download_mbps: mbps,
        });
        if s.no_cap.contains(cloud) || (s.max_mbps > 0.0 && mbps > s.max_mbps) {
            return Err(Error::NoCapacity(String::new()));
        }
        if let Some(e) = &s.create_err {
            return Err(Error::Other(e.clone()));
        }
        let pod = Pod {
            id: "pod1".into(),
            name: "lobo".into(),
            cost_per_hr: 0.69,
            desired_status: "RUNNING".into(),
            ..Default::default()
        };
        s.pods.push(pod.clone());
        Ok(pod)
    }
    async fn list(&self) -> Result<Vec<Pod>> {
        Ok(self.state.lock().unwrap().pods.clone())
    }
    async fn get(&self, id: &str) -> Result<Pod> {
        self.state
            .lock()
            .unwrap()
            .pods
            .iter()
            .find(|p| p.id == id)
            .cloned()
            .ok_or(Error::NotFound)
    }
    async fn delete(&self, id: &str) -> Result<()> {
        let mut s = self.state.lock().unwrap();
        s.deleted.push(id.into());
        s.pods.retain(|p| p.id != id);
        Ok(())
    }
}
#[derive(Default)]
pub struct AgentState {
    pub script: Vec<Option<Status>>,
    pub index: usize,
    pub calls: usize,
}
#[derive(Default)]
pub struct FakeAgent {
    pub state: Mutex<AgentState>,
}
impl FakeAgent {
    pub fn new(script: Vec<Option<Status>>) -> Self {
        Self {
            state: Mutex::new(AgentState {
                script,
                ..Default::default()
            }),
        }
    }
    pub fn calls(&self) -> usize {
        self.state.lock().unwrap().calls
    }
}
#[async_trait]
impl AgentApi for FakeAgent {
    async fn status(&self) -> Result<Status> {
        let mut s = self.state.lock().unwrap();
        s.calls += 1;
        let value = s.script.get(s.index).cloned().flatten();
        if s.index + 1 < s.script.len() {
            s.index += 1;
        }
        value.ok_or_else(|| Error::Other("530".into()))
    }
    async fn version(&self) -> Result<Manifest> {
        self.state.lock().unwrap().calls += 1;
        Ok(Manifest {
            version: "2026.09.23-1".into(),
            git_sha: "abc1234".into(),
            ..Default::default()
        })
    }
    async fn logs(&self, _: usize) -> Result<String> {
        self.state.lock().unwrap().calls += 1;
        Ok("last log line".into())
    }
}
pub const LOCAL_API_URL: &str = "http://127.0.0.1:8931/v1";
pub const LOCAL_AGENT_URL: &str = "http://127.0.0.1:8932";
#[derive(Default)]
pub struct LocalState {
    pub running: Vec<Instance>,
    pub created: Vec<CreateOpts>,
    pub deleted: Vec<String>,
}
#[derive(Default)]
pub struct FakeLocal {
    pub state: Mutex<LocalState>,
}
fn local_urls(mut i: Instance) -> Instance {
    i.api_url = LOCAL_API_URL.into();
    i.agent_url = LOCAL_AGENT_URL.into();
    i
}
#[async_trait]
impl Provider for FakeLocal {
    fn name(&self) -> &'static str {
        "local"
    }
    fn replaceable(&self) -> bool {
        false
    }
    async fn rent(
        &self,
        o: &CreateOpts,
        c: CancellationToken,
        _: &(dyn Fn(String) + Sync),
    ) -> Result<Instance> {
        if c.is_cancelled() {
            return Err(Error::Cancelled);
        }
        let mut s = self.state.lock().unwrap();
        s.created.push(o.clone());
        let i = local_urls(Instance {
            provider: "local".into(),
            id: "4242".into(),
            status: "running".into(),
            detail: format!("this Mac, {}", o.model),
            ..Default::default()
        });
        s.running.push(i.clone());
        Ok(i)
    }
    async fn list(&self) -> Result<Vec<Instance>> {
        Ok(self
            .state
            .lock()
            .unwrap()
            .running
            .iter()
            .cloned()
            .map(local_urls)
            .collect())
    }
    async fn get(&self, id: &str) -> Result<Instance> {
        self.state
            .lock()
            .unwrap()
            .running
            .iter()
            .find(|i| i.id == id)
            .cloned()
            .map(local_urls)
            .ok_or(Error::NotFound)
    }
    async fn delete(&self, id: &str) -> Result<()> {
        let mut s = self.state.lock().unwrap();
        s.deleted.push(id.into());
        s.running.retain(|i| i.id != id);
        Ok(())
    }
}
pub fn deps(rp: Arc<FakeRunPod>, ag: Arc<FakeAgent>, clock: Arc<dyn Clock>) -> Deps {
    Deps {
        connection: None,
        operations: Arc::new(OperationState::memory()),
        providers: [(
            "runpod".into(),
            Arc::new(RunPodProvider {
                api: rp,
                domain: "lobo.example.com".into(),
            }) as Arc<dyn Provider>,
        )]
        .into(),
        images: Arc::new(FakeImages),
        new_agent: Arc::new(move |_| ag.clone()),
        clock,
        poll: Duration::from_millis(1),
        cfg: Laptop {
            domain: "lobo.example.com".into(),
            bucket_url: "https://pub-x.r2.dev".into(),
            lobo_api_key: "sk".into(),
            cf_tunnel_token: "tok".into(),
            ..Default::default()
        },
    }
}
pub fn boot_script() -> Vec<Option<Status>> {
    vec![
        None,
        None,
        Some(Status {
            stage: Stage::Tunnel,
            ..Default::default()
        }),
        Some(Status {
            stage: Stage::Download,
            download: DownloadProgress {
                bytes: 12357400000,
                total: 28595762272,
                mbps: 51.3,
                ..Default::default()
            },
            ..Default::default()
        }),
        Some(Status {
            stage: Stage::Load,
            ..Default::default()
        }),
        Some(Status {
            stage: Stage::Ready,
            ..Default::default()
        }),
    ]
}
pub fn local_boot_script() -> Vec<Option<Status>> {
    vec![
        Some(Status {
            stage: Stage::Gpu,
            ..Default::default()
        }),
        Some(Status {
            stage: Stage::Download,
            download: DownloadProgress {
                bytes: 1 << 30,
                total: 22082528352,
                mbps: 12.0,
                ..Default::default()
            },
            ..Default::default()
        }),
        Some(Status {
            stage: Stage::Load,
            ..Default::default()
        }),
        Some(Status {
            stage: Stage::Ready,
            ..Default::default()
        }),
    ]
}
pub async fn events(script: Vec<Option<Status>>, no_cap: &[&str]) -> Vec<lobo_proto::UpEvent> {
    let rp = Arc::new(FakeRunPod::default());
    rp.state.lock().unwrap().no_cap = no_cap.iter().map(|s| (*s).to_owned()).collect();
    let clock = Arc::new(crate::clock::StepClock::new(
        "2026-09-23T10:00:00Z".parse().unwrap(),
        Duration::from_secs(1),
    ));
    let mut operation = super::up(
        deps(rp, Arc::new(FakeAgent::new(script)), clock),
        UpOpts::default(),
        CancellationToken::new(),
    );
    let mut events = operation.take_events().unwrap();
    let mut out = Vec::new();
    while let Some(event) = events.recv().await {
        out.push(event);
    }
    let _ = operation.wait().await;
    out
}

pub struct FakeImages;
#[async_trait]
impl crate::images::ImageResolver for FakeImages {
    async fn latest(&self, model: &str) -> Result<String> {
        Ok(format!("public-image-{model}@sha256:fixture"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn fake_agent_script_sticks_on_last() {
        let f = FakeAgent::new(boot_script());
        for _ in 0..2 {
            assert!(f.status().await.is_err());
        }
        assert_eq!(f.status().await.unwrap().stage, Stage::Tunnel);
        for _ in 0..10 {
            let _ = f.status().await;
        }
        assert_eq!(f.status().await.unwrap().stage, Stage::Ready);
        assert_eq!(f.calls(), 14);
    }
    #[tokio::test]
    async fn fake_runpod_no_cap() {
        let f = FakeRunPod::default();
        f.state.lock().unwrap().no_cap.insert("SECURE".into());
        assert!(matches!(
            f.create(&CreateOpts::default(), "SECURE", 10000.0).await,
            Err(Error::NoCapacity(_))
        ));
        let pod = f
            .create(&CreateOpts::default(), "COMMUNITY", 10000.0)
            .await
            .unwrap();
        assert_eq!(f.list().await.unwrap().len(), 1);
        f.delete(&pod.id).await.unwrap();
        assert!(f.list().await.unwrap().is_empty());
        assert_eq!(f.state.lock().unwrap().created.len(), 2);
    }
}

#[derive(Default)]
pub struct RecordingState {
    pub instances: Vec<Instance>,
    pub calls: Vec<String>,
    pub broken: bool,
    pub boot_id: String,
    pub start_id: u64,
    pub created: Vec<CreateOpts>,
    pub delay: Duration,
    pub panic_after_create: bool,
    pub uncertain_create: bool,
    pub cancel_on_rent: bool,
    pub swap_on_delete: bool,
    pub rents: usize,
}
pub struct RecordingProvider {
    pub name: &'static str,
    pub state: Mutex<RecordingState>,
}
impl RecordingProvider {
    pub fn new(name: &'static str) -> Self {
        Self {
            name,
            state: Mutex::new(RecordingState::default()),
        }
    }
    pub fn calls(&self) -> Vec<String> {
        self.state.lock().unwrap().calls.clone()
    }
}
#[async_trait]
impl Provider for RecordingProvider {
    fn name(&self) -> &'static str {
        self.name
    }
    fn replaceable(&self) -> bool {
        self.name != "local"
    }
    async fn list(&self) -> Result<Vec<Instance>> {
        let mut s = self.state.lock().unwrap();
        s.calls.push("list".into());
        if s.broken {
            return Err(Error::Other("broken cloud key".into()));
        }
        Ok(s.instances.clone())
    }
    async fn get(&self, id: &str) -> Result<Instance> {
        let mut s = self.state.lock().unwrap();
        s.calls.push(format!("get:{id}"));
        s.instances
            .iter()
            .find(|i| i.id == id)
            .cloned()
            .ok_or(Error::NotFound)
    }
    async fn delete(&self, id: &str) -> Result<()> {
        let mut s = self.state.lock().unwrap();
        s.calls.push(format!("delete:{id}"));
        s.instances.retain(|i| i.id != id);
        Ok(())
    }
    async fn runtime_identity(&self, id: &str) -> Result<Option<(String, i32, u64)>> {
        let mut s = self.state.lock().unwrap();
        s.calls.push(format!("identity:{id}"));
        Ok(s.instances
            .iter()
            .find(|i| i.id == id)
            .map(|_| (s.boot_id.clone(), 4242, s.start_id)))
    }
    async fn delete_owned(&self, id: &str, boot_id: &str, start_id: Option<u64>) -> Result<()> {
        if self.name == "local" {
            let mut s = self.state.lock().unwrap();
            s.calls.push(format!("owned-delete:{id}:{boot_id}"));
            if s.swap_on_delete {
                s.boot_id = "replacement".into();
                s.start_id += 1;
            }
            if s.boot_id != boot_id || start_id.is_some_and(|start| s.start_id != start) {
                return Ok(());
            }
        }
        self.delete(id).await
    }
    async fn rent(
        &self,
        opts: &CreateOpts,
        cancel: CancellationToken,
        _: &(dyn Fn(String) + Sync),
    ) -> Result<Instance> {
        let (delay, panic, uncertain, cancel_on_rent) = {
            let mut s = self.state.lock().unwrap();
            s.calls.push("rent".into());
            s.rents += 1;
            s.created.push(opts.clone());
            (
                s.delay,
                s.panic_after_create,
                s.uncertain_create,
                s.cancel_on_rent,
            )
        };
        if cancel_on_rent {
            cancel.cancel();
        }
        tokio::time::sleep(delay).await;
        let i = Instance {
            provider: self.name.into(),
            id: "4242".into(),
            agent_url: LOCAL_AGENT_URL.into(),
            api_url: LOCAL_API_URL.into(),
            ..Default::default()
        };
        {
            let mut s = self.state.lock().unwrap();
            s.boot_id = opts.boot_id.clone();
            s.start_id = 100;
            s.instances.push(i.clone());
        }
        assert!(!panic, "scoped worker exploded");
        if uncertain {
            return Err(Error::UnresolvedCreate {
                provider: self.name.into(),
                boot_id: opts.boot_id.clone(),
                detail: "response lost".into(),
            });
        }
        Ok(i)
    }
}
