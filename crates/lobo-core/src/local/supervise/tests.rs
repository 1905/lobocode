use super::*;
use async_trait::async_trait;
use lobo_agent::runner::{Download, Exit, GpuCheck, Killer, Llama, Metrics, Tunnel};
use lobo_proto::{DownloadProgress, Gpu, Host, Llama as LlamaMetrics, Status};
use std::sync::Mutex;
use tokio::sync::oneshot;

fn args(s: &str) -> Vec<String> {
    s.split_whitespace().map(str::to_owned).collect()
}
fn cfg() -> RunConfig {
    RunConfig::from_args(
        &args("--ctx 4096 --idle-min 7 --boot-id b1"),
        Manifest {
            version: "dev".into(),
            git_sha: "abc".into(),
            ..Default::default()
        },
    )
    .unwrap()
}
#[test]
fn run_config_from_args_table() {
    for (s, want) in [
        (
            "--model q8 --ctx 65536 --idle-min 20 --boot-id b1 --port 9000 --api-port 9001",
            "",
        ),
        ("--ctx 4096 --idle-min 5", ""),
        ("--model q2 --ctx 1 --idle-min 1", "q2"),
        ("--idle-min 1", "--ctx"),
        ("--ctx 1", "--idle-min"),
        (
            "--ctx 1 --idle-min 1 --port 9000 --api-port 9000",
            "--api-port",
        ),
        ("--ctx 1 --idle-min 1 x", "unknown argument \"x\""),
        (
            "--ctx 1 --idle-min 1 --port 65536",
            "--port: want 1-65535, got 65536",
        ),
        (
            "--ctx 1 --idle-min 1 --port -1",
            "--port: want 1-65535, got -1",
        ),
        ("--ctx 1 --idle-min 1 --api-port 0", "--api-port"),
        ("--ctx abc", "want an integer"),
        ("--ctx", "missing value"),
        (
            "--ctx 1 --idle-min 9223372036854775807",
            "duration overflow",
        ),
    ] {
        let r = RunConfig::from_args(&args(s), Manifest::default());
        if want.is_empty() {
            let c = r.unwrap();
            if s.contains("q8") {
                assert_eq!(
                    (c.model.as_str(), c.ctx, c.idle_min, c.port, c.api_port),
                    ("q8", 65536, 20, 9000, 9001)
                );
            } else {
                assert_eq!(
                    (c.model.as_str(), c.ctx, c.idle_min, c.port, c.api_port),
                    ("q6", 4096, 5, 8931, 8932)
                );
            }
        } else {
            assert!(r.unwrap_err().to_string().contains(want), "{s}");
        }
    }
}
#[test]
fn to_args_round_trips() {
    for path in [None, Some(PathBuf::from("/a space/lobo.env"))] {
        let mut c = cfg();
        c.config_path = path;
        assert_eq!(
            RunConfig::from_args(&c.to_args(), c.version.clone()).unwrap(),
            c
        );
    }
}
#[test]
fn app_argv_passes_identity() {
    for spawner in [
        super::super::Spawner::app("/Applications/lobocode.app/Contents/MacOS/lobocode".into()),
        super::super::Spawner::cli("/bin/lobo".into()),
    ] {
        let argv = spawner
            .args_prefix
            .into_iter()
            .chain(cfg().to_args())
            .collect::<Vec<_>>()
            .join(" ");
        assert!(is_supervisor(123, "b1", &|_| Ok(argv.clone())));
    }
}
#[derive(Default)]
struct Fake {
    senders: Mutex<Vec<oneshot::Sender<lobo_agent::Result<()>>>>,
}
impl Fake {
    fn exit(&self) -> Exit {
        let (tx, rx) = oneshot::channel();
        self.senders.lock().unwrap().push(tx);
        rx
    }
}
#[async_trait]
impl Tunnel for Fake {
    async fn start(&self, _: CancellationToken, _: CancellationToken) -> lobo_agent::Result<Exit> {
        Ok(self.exit())
    }
}
#[async_trait]
impl GpuCheck for Fake {
    async fn check(&self, _: CancellationToken) -> lobo_agent::Result<()> {
        Ok(())
    }
}
#[async_trait]
impl Download for Fake {
    async fn run(
        &self,
        _: CancellationToken,
        _: &(dyn Fn(DownloadProgress) + Sync),
    ) -> lobo_agent::Result<()> {
        Ok(())
    }
}
#[async_trait]
impl Llama for Fake {
    async fn start(&self) -> lobo_agent::Result<Exit> {
        Ok(self.exit())
    }
    async fn wait_healthy(&self, _: CancellationToken) -> lobo_agent::Result<()> {
        Ok(())
    }
}
#[async_trait]
impl Metrics for Fake {
    async fn llama(&self) -> lobo_agent::Result<LlamaMetrics> {
        Ok(LlamaMetrics::default())
    }
    async fn gpu(&self) -> lobo_agent::Result<Gpu> {
        Ok(Gpu::default())
    }
    async fn host(&self) -> lobo_agent::Result<Host> {
        Ok(Host::default())
    }
}
#[async_trait]
impl Killer for Fake {
    async fn kill_self(&self, _: CancellationToken) -> lobo_agent::Result<()> {
        Ok(())
    }
}
struct Setup {
    _tmp: tempfile::TempDir,
    cfg: RunConfig,
    laptop: Laptop,
    state: StateFile,
    stop: CancellationToken,
}
impl Setup {
    fn new() -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let mut cfg = cfg();
        cfg.api_port = listener.local_addr().unwrap().port();
        let laptop = Laptop {
            weights_dir: tmp.path().join("weights").to_string_lossy().into(),
            lobo_api_key: "sk-x".into(),
            ..Default::default()
        };
        let state = StateFile::at(tmp.path().join("local.json"));
        Self {
            _tmp: tmp,
            cfg,
            laptop,
            state,
            stop: CancellationToken::new(),
        }
    }
    fn run(&self) -> tokio::task::JoinHandle<Result<()>> {
        let logs = Arc::new(LogRing::new(5000));
        let (_, mac) = new_deps(
            MacConfig {
                weights: self.laptop.weights(),
                llama_server: PathBuf::new(),
                api_key: "sk-x".into(),
                port: self.cfg.port,
                ctx: self.cfg.ctx,
                model: catalog::get("q6").unwrap().clone(),
                base_env: vec![],
            },
            logs.clone(),
            self.stop.clone(),
        );
        let f = Arc::new(Fake::default());
        let deps = lobo_agent::Deps {
            tunnel: f.clone(),
            gpu_check: f.clone(),
            download: f.clone(),
            llama: f.clone(),
            metrics: f.clone(),
            killer: f,
        };
        tokio::spawn(supervise_with(
            self.cfg.clone(),
            self.laptop.clone(),
            self.state.clone(),
            deps,
            mac,
            logs,
            self.stop.clone(),
        ))
    }
    async fn wait_state(&self) {
        tokio::time::timeout(Duration::from_secs(3), async {
            while !self.state.path.exists() {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
    }
}
impl Drop for Setup {
    fn drop(&mut self) {
        self.stop.cancel();
    }
}
#[tokio::test]
async fn supervise_claims_state_and_serves_api() {
    let s = Setup::new();
    let task = s.run();
    s.wait_state().await;
    let client = crate::http::client(Duration::from_secs(3));
    let url = format!("http://127.0.0.1:{}", s.cfg.api_port);
    let status: Status = client
        .get(format!("{url}/api/status"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(status.boot_id, "b1");
    let state = s.state.read().unwrap().unwrap();
    assert_eq!(state.pid, i64::from(std::process::id()));
    assert_eq!(state.api_port, i64::from(s.cfg.api_port));
    let version = client
        .get(format!("{url}/api/version"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert_eq!(version, r#"{"git_sha":"abc","version":"dev"}"#);
    let logs = client
        .get(format!("{url}/api/logs"))
        .bearer_auth("sk-x")
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(logs.contains("start"));
    assert!(logs.contains("stage"));
    s.stop.cancel();
    task.await.unwrap().unwrap();
    assert!(!s.state.path.exists());
}
#[tokio::test]
async fn supervise_busy_api_port_fails_before_state() {
    let mut s = Setup::new();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    s.cfg.api_port = listener.local_addr().unwrap().port();
    assert!(
        s.run()
            .await
            .unwrap()
            .unwrap_err()
            .to_string()
            .contains("agent API")
    );
    assert!(!s.state.path.exists());
}
#[tokio::test]
async fn supervise_cancel_removes_state_ok() {
    let s = Setup::new();
    let task = s.run();
    s.wait_state().await;
    s.stop.cancel();
    task.await.unwrap().unwrap();
    assert!(!s.state.path.exists());
}
#[tokio::test]
async fn supervise_second_instance_refused() {
    let s = Setup::new();
    // This process has the actual identity tokens. No global ps hook is changed.
    let mut child = tokio::process::Command::new("python3")
        .args([
            "-c",
            "import time; time.sleep(60)",
            "local",
            "run",
            "--boot-id",
            "old",
        ])
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let pid = child.id().unwrap() as i32;
    s.state
        .claim(
            &LocalState {
                pid: pid.into(),
                boot_id: "old".into(),
                ..Default::default()
            },
            &|_, _| true,
        )
        .unwrap();
    let result = s.run().await.unwrap();
    child.kill().await.unwrap();
    child.wait().await.unwrap();
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("local already running")
    );
    let state: LocalState = serde_json::from_slice(&std::fs::read(&s.state.path).unwrap()).unwrap();
    assert_eq!(state.boot_id, "old");
}
