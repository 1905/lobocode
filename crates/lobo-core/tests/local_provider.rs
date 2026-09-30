use async_trait::async_trait;
use lobo_core::{
    Error, Result,
    local::{
        EnsureRuntime, LocalHooks, LocalProvider, Spawner, StateFile,
        provider::{command_of, log_path, wait_group_gone},
        state::alive,
    },
    provider::{CreateOpts, Provider},
};
use lobo_proto::LocalState;
use nix::{
    errno::Errno,
    sys::signal::{Signal, kill},
    unistd::{Pid, getpgid},
};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio_util::sync::CancellationToken;
struct Runtime(Arc<AtomicUsize>);
#[async_trait]
impl EnsureRuntime for Runtime {
    async fn ensure(
        &self,
        _: &Path,
        _: CancellationToken,
        _: &(dyn Fn(String) + Sync),
    ) -> Result<PathBuf> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Ok("/fake/llama-server".into())
    }
}
fn opts() -> CreateOpts {
    CreateOpts {
        model: "q6".into(),
        ctx: 4096,
        idle_min: 7,
        boot_id: "b1".into(),
        ..Default::default()
    }
}
fn free_pair() -> u16 {
    static USED: std::sync::Mutex<Vec<u16>> = std::sync::Mutex::new(Vec::new());
    let mut used = USED.lock().unwrap();
    // Port zero chooses the ephemeral range, where unrelated outbound sockets
    // can take the port after this probe closes. Use distinct low pairs instead.
    for p in (10_000..30_000u16).step_by(2) {
        if !used.contains(&p)
            && !used.contains(&(p + 1))
            && let Ok(_first) = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, p))
            && std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, p + 1)).is_ok()
        {
            used.extend([p, p + 1]);
            return p;
        }
    }
    panic!("no free port pair")
}
struct Fixture {
    _process_guard: tokio::sync::MutexGuard<'static, ()>,
    tmp: tempfile::TempDir,
    p: LocalProvider,
    calls: Arc<AtomicUsize>,
}
impl Fixture {
    async fn new(mode: &str) -> Self {
        // pre_exec forces fork. A concurrently forked child can briefly inherit
        // another test's port probe until exec closes its CLOEXEC descriptors.
        // Keep real-process fixtures separate through cleanup, including probes.
        static PROCESSES: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
        let guard = PROCESSES.lock().await;
        let tmp = tempfile::tempdir().unwrap();
        let state = StateFile::at(tmp.path().join("local.json"));
        let calls = Arc::new(AtomicUsize::new(0));
        let p = LocalProvider {
            spawner: Spawner::cli(env!("CARGO_BIN_EXE_lobo-core-testchild").into()),
            config_path: Some("/cfg/lobo.env".into()),
            weights: tmp.path().join("w"),
            port: free_pair(),
            hooks: LocalHooks {
                supported: || Ok(()),
                memory: Arc::new(|| {
                    Ok(lobo_core::local::memory::MemorySnapshot {
                        total_bytes: 128 << 30,
                        available_bytes: 100 << 30,
                        metal_limit_bytes: 96 << 30,
                    })
                }),
                ensure_runtime: Arc::new(Runtime(calls.clone())),
                free_bytes: |_| Ok(1 << 50),
                state_wait: Duration::from_secs(5),
                stop_wait: Duration::from_millis(150),
                child_env: vec![
                    ("LOBO_LOCAL_HELPER".into(), mode.into()),
                    (
                        "LOBO_TEST_STATE".into(),
                        state.path.to_string_lossy().into(),
                    ),
                ],
                ..Default::default()
            },
            state,
        };
        Self {
            _process_guard: guard,
            tmp,
            p,
            calls,
        }
    }
    fn pid(&self, name: &str) -> i32 {
        fs::read_to_string(self.tmp.path().join(name))
            .unwrap()
            .parse()
            .unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        // Only the group started by this fixture. The helper publishes its pid.
        if let Ok(s) = fs::read_to_string(self.tmp.path().join("pid"))
            && let Ok(pid) = s.parse::<i32>()
        {
            let _ = kill(Pid::from_raw(-pid), Signal::SIGKILL);
        }
    }
}
async fn rent(p: &LocalProvider) -> lobo_proto::Instance {
    p.rent(&opts(), CancellationToken::new(), &|_| {})
        .await
        .unwrap()
}
#[tokio::test]
async fn prechecks_before_runtime() {
    for case in [
        "platform", "model", "running", "weights", "space", "port", "api-port",
    ] {
        let mut f = Fixture::new("ok").await;
        let mut o = opts();
        let mut listener = None;
        let want = match case {
            "platform" => {
                f.p.hooks.supported = || Err(Error::Local("unsupported platform".into()));
                "unsupported"
            }
            "model" => {
                o.model = "q2".into();
                "unknown model"
            }
            "running" => {
                f.p.state
                    .claim(
                        &LocalState {
                            pid: std::process::id().into(),
                            boot_id: "existing".into(),
                            ..Default::default()
                        },
                        &|_, _| true,
                    )
                    .unwrap();
                "already running"
            }
            "weights" => {
                fs::write(&f.p.weights, b"file").unwrap();
                "not writable"
            }
            "space" => {
                f.p.hooks.free_bytes = |_| Ok(0);
                "not enough space"
            }
            "port" => {
                listener = Some(
                    std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, f.p.port)).unwrap(),
                );
                "LOBO_LOCAL_PORT"
            }
            _ => {
                listener = Some(
                    std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, f.p.port + 1))
                        .unwrap(),
                );
                "agent API"
            }
        };
        let e =
            f.p.rent(&o, CancellationToken::new(), &|_| {})
                .await
                .unwrap_err();
        assert!(e.to_string().contains(want), "{case}: {e}");
        assert_eq!(f.calls.load(Ordering::SeqCst), 0);
        assert_eq!(f.p.state.path.exists(), case == "running");
        drop(listener);
    }
}
#[tokio::test]
async fn lifecycle() {
    let f = Fixture::new("ok").await;
    let i = rent(&f.p).await;
    let pid: i32 = i.id.parse().unwrap();
    assert_eq!(
        (i.provider.as_str(), i.status.as_str(), i.detail.as_str()),
        ("local", "running", "this Mac, q6")
    );
    assert_eq!(i.cost_per_hr, 0.0);
    assert_eq!(i.api_url, format!("http://127.0.0.1:{}/v1", f.p.port));
    assert_eq!(f.calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        fs::read_to_string(f.tmp.path().join("args")).unwrap(),
        format!(
            "local run --config /cfg/lobo.env --model q6 --ctx 4096 --idle-min 7 --boot-id b1 --port {} --api-port {}",
            f.p.port,
            f.p.port + 1
        )
    );
    assert!(f.p.weights.is_dir());
    assert!(
        f.p.rent(&opts(), CancellationToken::new(), &|_| {})
            .await
            .unwrap_err()
            .to_string()
            .contains("already running")
    );
    assert_eq!(f.p.list().await.unwrap(), vec![i.clone()]);
    assert_eq!(f.p.get(&i.id).await.unwrap(), i);
    assert!(matches!(f.p.get("1").await, Err(Error::NotFound)));
    f.p.delete(&i.id).await.unwrap();
    assert!(!alive(pid));
    assert!(!f.p.state.path.exists());
    assert!(f.p.list().await.unwrap().is_empty());
    f.p.delete(&i.id).await.unwrap();
}
#[tokio::test]
async fn child_fails_shows_log_tail() {
    let f = Fixture::new("fail").await;
    fs::write(log_path(&f.p.state), b"old secret log line\n").unwrap();
    let e =
        f.p.rent(&opts(), CancellationToken::new(), &|_| {})
            .await
            .unwrap_err()
            .to_string();
    assert!(e.contains("exited early"), "{e}");
    assert!(e.contains("boom: weights gone"));
    assert!(e.contains("line 25"));
    assert!(!e.contains("line 5\n"));
    assert!(!e.contains("old secret"));
    assert!(log_path(&f.p.state).exists());
}
#[tokio::test]
async fn delete_kills_stubborn() {
    let f = Fixture::new("stubborn").await;
    let i = rent(&f.p).await;
    let start = tokio::time::Instant::now();
    f.p.delete(&i.id).await.unwrap();
    assert!(start.elapsed() >= f.p.hooks.stop_wait);
    assert!(!alive(i.id.parse().unwrap()));
    assert!(!f.p.state.path.exists());
}
async fn group_case(mode: &str) {
    let f = Fixture::new(mode).await;
    let i = rent(&f.p).await;
    let pid = i.id.parse::<i32>().unwrap();
    let child = f.pid("child");
    assert_eq!(
        getpgid(Some(Pid::from_raw(child))).unwrap(),
        Pid::from_raw(pid)
    );
    f.p.delete(&i.id).await.unwrap();
    assert_eq!(kill(Pid::from_raw(-pid), None), Err(Errno::ESRCH));
    assert!(!f.p.state.path.exists());
}
#[tokio::test]
async fn delete_kills_group() {
    group_case("stubborn-child").await;
}
#[tokio::test]
async fn delete_dead_leader_live_child_killed() {
    group_case("ok-child").await;
}
#[tokio::test]
async fn delete_reverifies_before_sigkill() {
    let mut f = Fixture::new("stubborn").await;
    let i = rent(&f.p).await;
    let calls = Arc::new(AtomicUsize::new(0));
    let c = calls.clone();
    f.p.hooks.ps = Arc::new(move |pid| {
        if c.fetch_add(1, Ordering::SeqCst) == 0 {
            command_of(pid)
        } else {
            Ok("/usr/bin/vim notes.txt".into())
        }
    });
    f.p.delete(&i.id).await.unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert!(alive(i.id.parse().unwrap()));
    assert!(!f.p.state.path.exists());
}
#[tokio::test]
async fn delete_never_signals_stranger() {
    for cmd in [
        "vim notes.txt",
        "lobo local run --boot-id b1x",
        "lobo local status --boot-id b1",
        "lobo local run --boot-id",
        "/tmp/local run.txt --boot-id b1",
    ] {
        let mut f = Fixture::new("ok").await;
        let i = rent(&f.p).await;
        f.p.hooks.ps = Arc::new(move |_| Ok(cmd.into()));
        f.p.delete(&i.id).await.unwrap();
        assert!(alive(i.id.parse().unwrap()));
        assert!(!f.p.state.path.exists());
    }
}
#[tokio::test]
async fn cancellation_before_state_kills_entire_group() {
    let f = Fixture::new("no-state-child").await;
    let cancel = CancellationToken::new();
    let c = cancel.clone();
    let options = opts();
    let operation = f.p.rent(&options, cancel, &|_| {});
    let cancellation = async {
        tokio::time::timeout(Duration::from_secs(3), async {
            while !f.tmp.path().join("child-ready").exists() {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
        c.cancel();
    };
    let (result, ()) = tokio::join!(operation, cancellation);
    assert!(matches!(result, Err(Error::Cancelled)), "{result:?}");
    assert!(wait_group_gone(f.pid("pid"), Duration::ZERO).await);
    assert!(!f.p.state.path.exists());
}
#[tokio::test]
async fn timeout_before_state_kills_entire_group() {
    let mut f = Fixture::new("no-state-child").await;
    f.p.hooks.state_wait = Duration::from_secs(2);
    let e =
        f.p.rent(&opts(), CancellationToken::new(), &|_| {})
            .await
            .unwrap_err();
    assert!(e.to_string().contains("wrote no state"));
    assert!(wait_group_gone(f.pid("pid"), Duration::ZERO).await);
    assert!(!f.p.state.path.exists());
}
struct CancelRuntime;
#[async_trait]
impl EnsureRuntime for CancelRuntime {
    async fn ensure(
        &self,
        _: &Path,
        c: CancellationToken,
        _: &(dyn Fn(String) + Sync),
    ) -> Result<PathBuf> {
        c.cancel();
        Ok("/fake/runtime".into())
    }
}
#[tokio::test]
async fn cancelled_runtime_cannot_spawn() {
    let mut f = Fixture::new("ok").await;
    f.p.hooks.ensure_runtime = Arc::new(CancelRuntime);
    let result = f.p.rent(&opts(), CancellationToken::new(), &|_| {}).await;
    assert!(matches!(result, Err(Error::Cancelled)), "{result:?}");
    assert!(!f.tmp.path().join("pid").exists());
}

struct UnusedImages;
#[async_trait]
impl lobo_core::images::ImageResolver for UnusedImages {
    async fn latest(&self, _: &str) -> Result<String> {
        panic!("local start must not resolve cloud images")
    }
}
struct ReadyAgent;
#[async_trait]
impl lobo_core::control::AgentApi for ReadyAgent {
    async fn status(&self) -> Result<lobo_proto::Status> {
        Ok(lobo_proto::Status {
            stage: lobo_proto::Stage::Ready,
            ..Default::default()
        })
    }
    async fn version(&self) -> Result<lobo_proto::Manifest> {
        Ok(Default::default())
    }
    async fn logs(&self, _: usize) -> Result<String> {
        Ok(String::new())
    }
}
fn core_deps(f: &Fixture) -> lobo_core::control::Deps {
    lobo_core::control::Deps {
        connection: None,
        providers: [("local".into(), Arc::new(f.p.clone()) as Arc<dyn Provider>)].into(),
        operations: Arc::new(lobo_core::control::OperationState::memory()),
        images: Arc::new(UnusedImages),
        new_agent: Arc::new(|_| Arc::new(ReadyAgent)),
        cfg: Default::default(),
        clock: Arc::new(lobo_core::clock::SystemClock),
        poll: Duration::from_millis(1),
    }
}
#[tokio::test]
async fn core_cancel_after_spawn_waits_for_real_local_group() {
    let f = Fixture::new("no-state-child").await;
    let mut op = lobo_core::control::up(
        core_deps(&f),
        lobo_core::control::UpOpts {
            provider: "local".into(),
            ..Default::default()
        },
        CancellationToken::new(),
    );
    let mut events = op.take_events().unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        while !f.tmp.path().join("child-ready").exists() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    op.cancel();
    assert!(matches!(op.wait().await, Err(Error::Cancelled)));
    assert!(wait_group_gone(f.pid("pid"), Duration::ZERO).await);
    assert!(!f.p.state.path.exists());
    assert_eq!(events.recv().await.unwrap().phase, "cancelled");
}
struct WaitingRuntime(Arc<tokio::sync::Notify>);
#[async_trait]
impl EnsureRuntime for WaitingRuntime {
    async fn ensure(
        &self,
        _: &Path,
        c: CancellationToken,
        _: &(dyn Fn(String) + Sync),
    ) -> Result<PathBuf> {
        self.0.notify_one();
        c.cancelled().await;
        Err(Error::Cancelled)
    }
}
#[tokio::test]
async fn core_cancel_during_runtime_preparation_spawns_nothing() {
    let mut f = Fixture::new("ok").await;
    let entered = Arc::new(tokio::sync::Notify::new());
    f.p.hooks.ensure_runtime = Arc::new(WaitingRuntime(entered.clone()));
    let mut op = lobo_core::control::up(
        core_deps(&f),
        lobo_core::control::UpOpts {
            provider: "local".into(),
            ..Default::default()
        },
        CancellationToken::new(),
    );
    let mut events = op.take_events().unwrap();
    tokio::time::timeout(Duration::from_secs(5), entered.notified())
        .await
        .unwrap();
    op.cancel();
    assert!(matches!(op.wait().await, Err(Error::Cancelled)));
    assert!(!f.tmp.path().join("pid").exists());
    assert_eq!(events.recv().await.unwrap().phase, "cancelled");
}

#[tokio::test]
async fn rejected_memory_has_no_startup_side_effects() {
    for unknown in [false, true] {
        let mut f = Fixture::new("ok").await;
        f.p.hooks.memory = Arc::new(move || {
            if unknown {
                return Err(Error::Local(
                    "fixture native measurement unavailable".into(),
                ));
            }
            Ok(lobo_core::local::memory::MemorySnapshot {
                total_bytes: 64 << 30,
                available_bytes: 10 << 30,
                metal_limit_bytes: 48 << 30,
            })
        });
        let error =
            f.p.rent(&opts(), CancellationToken::new(), &|_| {})
                .await
                .unwrap_err()
                .to_string();
        assert!(
            error.contains(if unknown {
                "measurement unavailable"
            } else {
                "GiB"
            }),
            "{error}"
        );
        assert_eq!(f.calls.load(Ordering::SeqCst), 0);
        assert!(!f.p.weights.exists());
        assert!(!f.p.state.path.exists());
        assert!(!log_path(&f.p.state).exists());
        assert!(!f.tmp.path().join("pid").exists());
    }
}
