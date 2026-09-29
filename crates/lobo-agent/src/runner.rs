use crate::{Error, Result, watchdog};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use lobo_proto::{
    DownloadProgress, GoTime, Gpu, Host, Llama as LlamaMetrics, Stage, Status, Timings,
};
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tokio::{sync::oneshot, time::Instant};
use tokio_util::sync::CancellationToken;

pub type Exit = oneshot::Receiver<Result<()>>;
#[async_trait]
pub trait Tunnel: Send + Sync {
    async fn start(&self, boot: CancellationToken, life: CancellationToken) -> Result<Exit>;
}
#[async_trait]
pub trait GpuCheck: Send + Sync {
    async fn check(&self, cancel: CancellationToken) -> Result<()>;
}
#[async_trait]
pub trait Download: Send + Sync {
    async fn run(
        &self,
        cancel: CancellationToken,
        on_progress: &(dyn Fn(DownloadProgress) + Sync),
    ) -> Result<()>;
}
#[async_trait]
pub trait Llama: Send + Sync {
    async fn start(&self) -> Result<Exit>;
    async fn wait_healthy(&self, cancel: CancellationToken) -> Result<()>;
}
#[async_trait]
pub trait Metrics: Send + Sync {
    async fn llama(&self) -> Result<LlamaMetrics>;
    async fn gpu(&self) -> Result<Gpu>;
    async fn host(&self) -> Result<Host>;
}
#[async_trait]
pub trait Killer: Send + Sync {
    async fn kill_self(&self, cancel: CancellationToken) -> Result<()>;
}
pub struct Deps {
    pub tunnel: Arc<dyn Tunnel>,
    pub gpu_check: Arc<dyn GpuCheck>,
    pub download: Arc<dyn Download>,
    pub llama: Arc<dyn Llama>,
    pub metrics: Arc<dyn Metrics>,
    pub killer: Arc<dyn Killer>,
}
pub struct RunnerConfig {
    pub boot_id: String,
    pub timings: Timings,
    pub model: String,
    pub ctx: i64,
    pub idle: Duration,
    pub expires_at: DateTime<Utc>,
    pub boot_timeout: Duration,
    pub tick: Duration,
    pub fail_grace: Duration,
}
struct State {
    status: Status,
    watchdog: watchdog::State,
    stage_at: Option<Instant>,
}
pub struct Runner {
    d: Deps,
    cfg: RunnerConfig,
    state: Mutex<State>,
    start: Instant,
    gpu_ok: AtomicBool,
}
impl Runner {
    pub fn new(d: Deps, cfg: RunnerConfig) -> Arc<Self> {
        let status = Status {
            boot_id: cfg.boot_id.clone(),
            stage: Stage::Boot,
            model: cfg.model.clone(),
            ctx: cfg.ctx,
            expires_at: GoTime::from_utc(cfg.expires_at),
            timings: cfg.timings.clone(),
            ..Default::default()
        };
        Arc::new(Self {
            d,
            cfg,
            state: Mutex::new(State {
                status,
                watchdog: watchdog::State::new(Utc::now()),
                stage_at: None,
            }),
            start: Instant::now(),
            gpu_ok: AtomicBool::new(false),
        })
    }
    pub fn status(&self) -> Status {
        let mut s = self.state.lock().unwrap().status.clone();
        s.uptime_s = self.start.elapsed().as_secs() as i64;
        s
    }
    fn set_stage(&self, stage: Stage, detail: String) {
        let mut s = self.state.lock().unwrap();
        Self::change_stage(&mut s, stage, detail);
    }
    fn change_stage(s: &mut State, stage: Stage, detail: String) {
        let now = Instant::now();
        if let Some(at) = s.stage_at {
            let seconds = (now - at).as_secs_f64();
            match s.status.stage {
                Stage::Tunnel => s.status.timings.tunnel_s = seconds,
                Stage::Gpu => s.status.timings.gpu_check_s = seconds,
                Stage::Download => s.status.timings.download_s = seconds,
                Stage::Verify => s.status.timings.verify_s = seconds,
                Stage::Load => s.status.timings.load_s = seconds,
                _ => {}
            }
        }
        if stage == Stage::Ready {
            s.status.timings.ready_at = GoTime::from_utc(Utc::now());
        }
        s.stage_at = Some(now);
        s.status.stage = stage;
        s.status.stage_detail = detail;
        tracing::info!(service="lobo-agent", stage=stage.as_str(),detail=%s.status.stage_detail,"stage");
    }
    // Boot, watchdog and grace are owned futures. Returning drops them together;
    // there are no detached stage workers that can keep starting processes.
    pub async fn run(self: Arc<Self>, cancel: CancellationToken) -> Result<()> {
        let life = cancel.child_token();
        let _life_guard = life.clone().drop_guard();
        let boot = self.boot(life);
        let watch = self.watch();
        let grace = tokio::time::sleep(self.cfg.fail_grace);
        tokio::pin!(boot, watch, grace);
        let mut failed = false;
        let reason = loop {
            tokio::select! {
                biased;
                _=cancel.cancelled()=>return Err(Error::Cancelled),
                reason=&mut watch=>break reason,
                error=&mut boot, if !failed=> {
                    failed=true; self.set_stage(Stage::Failed,error.to_string());
                    tracing::error!(service="lobo-agent", error=%error,grace_s=self.cfg.fail_grace.as_secs_f64(),"boot failed, terminating after grace");
                    grace.as_mut().reset(Instant::now()+self.cfg.fail_grace);
                },
                _=&mut grace,if failed=>break "failed",
            }
        };
        {
            let mut s = self.state.lock().unwrap();
            s.status.kill_reason = reason.into();
            let detail = s.status.stage_detail.clone();
            Self::change_stage(&mut s, Stage::Terminating, detail);
        }
        // The boot deadline must never cancel deletion.
        let killed = self.d.killer.kill_self(CancellationToken::new());
        tokio::select! { biased; _=cancel.cancelled()=>Err(Error::Cancelled), result=killed=>{ if let Err(e)=result {tracing::error!(service="lobo-agent", error=%e,"kill self");} Ok(()) } }
    }
    async fn boot(&self, life: CancellationToken) -> Error {
        let boot = life.child_token();
        let _guard = boot.clone().drop_guard();
        let deadline = Instant::now() + self.cfg.boot_timeout;
        self.set_stage(Stage::Tunnel, String::new());
        let mut tunnel = match tokio::time::timeout_at(
            deadline,
            self.d.tunnel.start(boot.clone(), life),
        )
        .await
        {
            Ok(Ok(exit)) => exit,
            Ok(Err(e)) => return Error::msg(format!("tunnel: {e}")),
            Err(_) => return self.boot_timeout(),
        };
        tokio::select! { biased;
            result=&mut tunnel=>Error::msg(format!("cloudflared exited: {}",exit_text(result))),
            error=async {
                let mut llama=match tokio::time::timeout_at(deadline,self.prepare(boot.clone())).await {
                    Ok(Ok(exit))=>exit,Ok(Err(e))=>return e,Err(_)=>return self.boot_timeout(),
                };
                boot.cancel();
                let result=(&mut llama).await;
                Error::msg(format!("llama-server exited: {}",exit_text(result)))
            }=>error,
        }
        // Tunnel failure is selected before dropping/cancelling the stage future.
    }
    fn boot_timeout(&self) -> Error {
        Error::msg(format!(
            "{}: boot timeout {:?} during {}: context deadline exceeded",
            self.status().stage.as_str(),
            self.cfg.boot_timeout,
            self.status().stage.as_str()
        ))
    }
    async fn prepare(&self, boot: CancellationToken) -> Result<Exit> {
        self.set_stage(Stage::Gpu, String::new());
        self.d
            .gpu_check
            .check(boot.clone())
            .await
            .map_err(|e| Error::msg(format!("gpu: {e}")))?;
        self.gpu_ok.store(true, Ordering::Release);
        self.set_stage(Stage::Download, String::new());
        self.d
            .download
            .run(boot.clone(), &|p| {
                let mut s = self.state.lock().unwrap();
                if p.verifying && s.status.stage == Stage::Download {
                    Self::change_stage(&mut s, Stage::Verify, String::new());
                }
                if !p.source.is_empty() {
                    s.status.timings.download_source = p.source.clone();
                }
                if p.total > 0 && p.bytes == p.total && p.mbps > 0.0 {
                    s.status.timings.download_mbps = p.mbps;
                }
                s.status.download = p;
            })
            .await
            .map_err(|e| Error::msg(format!("download: {e}")))?;
        self.set_stage(Stage::Load, String::new());
        let mut exited = self
            .d
            .llama
            .start()
            .await
            .map_err(|e| Error::msg(format!("load: {e}")))?;
        tokio::select! { biased;
            result=&mut exited=>return Err(Error::msg(format!("load: llama-server exited while loading: {}",exit_text(result)))),
            healthy=self.d.llama.wait_healthy(boot)=>healthy.map_err(|e|Error::msg(format!("load: {e}")))?,
        }
        {
            let mut s = self.state.lock().unwrap();
            s.watchdog.set_ready(Utc::now());
            Self::change_stage(&mut s, Stage::Ready, String::new());
        }
        Ok(exited)
    }
    async fn watch(&self) -> &'static str {
        let mut tick = tokio::time::interval(self.cfg.tick.max(Duration::from_millis(1)));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tick.tick().await;
            if let Some(reason) = self.tick().await {
                return reason;
            }
        }
    }
    async fn tick(&self) -> Option<&'static str> {
        let now = Utc::now();
        let ready = self.status().stage == Stage::Ready;
        let llama = if ready {
            self.d.metrics.llama().await.ok()
        } else {
            None
        };
        let gpu = if self.gpu_ok.load(Ordering::Acquire) {
            self.d.metrics.gpu().await.ok()
        } else {
            None
        };
        let host = self.d.metrics.host().await.ok();
        let sample = if let Some(l) = &llama {
            watchdog::Sample {
                at: now,
                ok: true,
                processing: l.requests_processing,
                deferred: l.requests_deferred,
                prompt_tokens: l.prompt_tokens_total,
                gen_tokens: l.gen_tokens_total,
            }
        } else {
            watchdog::Sample {
                at: now,
                ..Default::default()
            }
        };
        let mut s = self.state.lock().unwrap();
        if ready {
            s.watchdog.observe(sample);
        }
        let decision = s.watchdog.decide(
            now,
            &watchdog::Config {
                idle: self.cfg.idle,
                expires_at: self.cfg.expires_at,
            },
        );
        s.status.llama = llama;
        s.status.gpu = gpu;
        s.status.host = host;
        s.status.metrics_failures = s.watchdog.failed_samples();
        s.status.idle_s = s.watchdog.idle_for(now).as_secs() as i64;
        s.status.kill_in_s = decision.kill_in().as_secs() as i64;
        if s.status.stage != Stage::Terminating {
            s.status.kill_reason = decision.reason().as_str().into();
        }
        decision.is_kill().then_some(decision.reason().as_str())
    }
}
fn exit_text(result: std::result::Result<Result<()>, oneshot::error::RecvError>) -> String {
    match result {
        Ok(Ok(())) => "<nil>".into(),
        Ok(Err(e)) => e.to_string(),
        Err(_) => "exit monitor closed".into(),
    }
}
#[async_trait]
impl Metrics for crate::metrics::Collector {
    async fn llama(&self) -> Result<LlamaMetrics> {
        Self::llama(self).await
    }
    async fn gpu(&self) -> Result<Gpu> {
        Self::gpu(self).await
    }
    async fn host(&self) -> Result<Host> {
        Self::host(self).await
    }
}
#[async_trait]
impl Killer for crate::selfkill::RetryKiller {
    async fn kill_self(&self, cancel: CancellationToken) -> Result<()> {
        Self::kill_self(self, cancel).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::*;
    fn start(
        d: Deps,
        c: RunnerConfig,
    ) -> (
        Arc<Runner>,
        CancellationToken,
        tokio::task::JoinHandle<Result<()>>,
    ) {
        let r = Runner::new(d, c);
        let cancel = CancellationToken::new();
        let task = tokio::spawn(r.clone().run(cancel.clone()));
        (r, cancel, task)
    }
    async fn failed(d: Deps, c: RunnerConfig, needle: &str) {
        let (r, _, task) = start(d, c);
        wait_done(task).await.unwrap();
        let s = r.status();
        assert_eq!(s.stage, Stage::Terminating);
        assert!(
            s.stage_detail.contains(needle),
            "{} missing {needle}",
            s.stage_detail
        );
        assert_eq!(s.kill_reason, "failed");
    }
    #[tokio::test]
    async fn happy_path() {
        let k = Arc::new(FakeKiller::default());
        let (r, c, task) = start(fake_deps(k.clone()), cfg());
        wait_stage(&r, Stage::Ready).await;
        tokio::time::sleep(Duration::from_millis(15)).await;
        let s = r.status();
        assert_eq!(s.model, "q8");
        assert_eq!(s.download.bytes, 5);
        assert!(s.llama.is_some());
        assert!(s.gpu.is_some());
        assert!(s.host.is_none());
        assert_eq!(k.calls.load(Ordering::SeqCst), 0);
        c.cancel();
        assert!(matches!(wait_done(task).await, Err(Error::Cancelled)));
    }
    #[tokio::test]
    async fn status_timings_record_stage_seconds() {
        let (r, c, task) = start(fake_deps(Arc::new(FakeKiller::default())), cfg());
        let s = wait_stage(&r, Stage::Ready).await;
        assert!(!s.timings.ready_at.is_zero());
        assert!(s.timings.gpu_check_s >= 0.0);
        c.cancel();
        let _ = wait_done(task).await;
    }
    #[tokio::test]
    async fn download_fails() {
        let k = Arc::new(FakeKiller::default());
        let mut d = fake_deps(k.clone());
        d.download = Arc::new(FakeDownload {
            run: Box::new(|_, _| Box::pin(async { Err(Error::msg("sha mismatch")) })),
        });
        failed(d, cfg(), "sha mismatch").await;
        assert_eq!(k.calls.load(Ordering::SeqCst), 1);
    }
    #[tokio::test]
    async fn download_hangs_hits_boot_timeout() {
        let mut d = fake_deps(Arc::new(FakeKiller::default()));
        d.download = Arc::new(FakeDownload {
            run: Box::new(|_, _| Box::pin(std::future::pending())),
        });
        let mut c = cfg();
        c.boot_timeout = Duration::from_millis(30);
        failed(d, c, "boot timeout").await;
    }
    #[tokio::test]
    async fn never_healthy() {
        let k = Arc::new(FakeKiller::default());
        let mut d = fake_deps(k.clone());
        d.llama = Arc::new(FakeLlama {
            start: Box::new(|| Box::pin(async { Ok(timed_exit(Duration::from_secs(30), "late")) })),
            healthy: Box::new(|_| Box::pin(std::future::pending())),
        });
        let mut c = cfg();
        c.boot_timeout = Duration::from_millis(30);
        failed(d, c, "boot timeout").await;
        assert_eq!(k.calls.load(Ordering::SeqCst), 1);
    }
    #[tokio::test]
    async fn llama_exits_after_ready() {
        let mut d = fake_deps(Arc::new(FakeKiller::default()));
        d.llama = Arc::new(FakeLlama {
            start: Box::new(|| {
                Box::pin(async { Ok(timed_exit(Duration::from_millis(25), "bad exit")) })
            }),
            healthy: Box::new(|_| Box::pin(async { Ok(()) })),
        });
        failed(d, cfg(), "llama-server exited: bad exit").await;
    }
    #[tokio::test]
    async fn no_gpu_fails_before_download() {
        let mut d = fake_deps(Arc::new(FakeKiller::default()));
        d.gpu_check = Arc::new(FakeGpu {
            check: Box::new(|_| Box::pin(async { Err(Error::msg("no CUDA")) })),
        });
        d.download = Arc::new(FakeDownload {
            run: Box::new(|_, _| Box::pin(async { panic!("download before GPU") })),
        });
        failed(d, cfg(), "gpu: no CUDA").await;
    }
    #[tokio::test]
    async fn llama_exits_while_loading() {
        let mut d = fake_deps(Arc::new(FakeKiller::default()));
        d.llama = Arc::new(FakeLlama {
            start: Box::new(|| {
                Box::pin(async { Ok(timed_exit(Duration::from_millis(10), "OOM")) })
            }),
            healthy: Box::new(|_| Box::pin(std::future::pending())),
        });
        failed(d, cfg(), "exited while loading: OOM").await;
    }
    async fn tunnel_failure() {
        let mut d = fake_deps(Arc::new(FakeKiller::default()));
        d.tunnel = Arc::new(FakeTunnel {
            start: Box::new(|_, _| {
                Box::pin(async { Ok(timed_exit(Duration::from_millis(2), "tunnel died")) })
            }),
        });
        d.download = Arc::new(FakeDownload {
            run: Box::new(|c, _| {
                Box::pin(async move {
                    c.cancelled().await;
                    Err(Error::Cancelled)
                })
            }),
        });
        failed(d, cfg(), "cloudflared exited: tunnel died").await;
    }
    #[tokio::test]
    async fn tunnel_exit_during_download() {
        tunnel_failure().await;
    }
    #[tokio::test]
    async fn tunnel_exit_is_the_reported_cause() {
        for _ in 0..30 {
            tunnel_failure().await;
        }
    }
    #[tokio::test]
    async fn stage_detail_bad_host_markers() {
        let mut d = fake_deps(Arc::new(FakeKiller::default()));
        d.download = Arc::new(FakeDownload {
            run: Box::new(|_, _| {
                Box::pin(async { Err(Error::msg("host: download too slow: 2 MB/s")) })
            }),
        });
        failed(d, cfg(), "download: host: download too slow:").await;
    }
    #[tokio::test]
    async fn expires_during_download() {
        let k = Arc::new(FakeKiller::default());
        let mut d = fake_deps(k.clone());
        d.download = Arc::new(FakeDownload {
            run: Box::new(|_, _| Box::pin(std::future::pending())),
        });
        let mut c = cfg();
        c.expires_at = Utc::now() + chrono::TimeDelta::milliseconds(30);
        let (r, _, task) = start(d, c);
        wait_done(task).await.unwrap();
        assert_eq!(r.status().kill_reason, "expired");
        assert_eq!(k.calls.load(Ordering::SeqCst), 1);
    }
    #[tokio::test]
    async fn expired_at_start() {
        let k = Arc::new(FakeKiller::default());
        let mut c = cfg();
        c.expires_at = Utc::now() - chrono::TimeDelta::seconds(1);
        let (r, _, task) = start(fake_deps(k.clone()), c);
        wait_done(task).await.unwrap();
        assert_eq!(r.status().kill_reason, "expired");
        assert_eq!(k.calls.load(Ordering::SeqCst), 1);
    }
    #[tokio::test]
    async fn idle_kill_once() {
        let k = Arc::new(FakeKiller::default());
        let mut c = cfg();
        c.idle = Duration::from_millis(40);
        let (r, _, task) = start(fake_deps(k.clone()), c);
        wait_done(task).await.unwrap();
        assert_eq!(r.status().kill_reason, "idle");
        assert_eq!(k.calls.load(Ordering::SeqCst), 1);
    }
    #[tokio::test]
    async fn busy_not_killed() {
        let k = Arc::new(FakeKiller::default());
        let mut d = fake_deps(k.clone());
        let n = std::sync::atomic::AtomicI64::new(0);
        d.metrics = Arc::new(FakeMetrics {
            llama: Box::new(move || {
                Ok(LlamaMetrics {
                    prompt_tokens_total: n.fetch_add(1, Ordering::SeqCst),
                    ..Default::default()
                })
            }),
        });
        let mut cfg = cfg();
        cfg.idle = Duration::from_millis(40);
        let (r, c, task) = start(d, cfg);
        wait_stage(&r, Stage::Ready).await;
        tokio::time::sleep(Duration::from_millis(150)).await;
        assert_eq!(k.calls.load(Ordering::SeqCst), 0);
        c.cancel();
        let _ = wait_done(task).await;
    }
}
