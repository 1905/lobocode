use crate::{
    Error, LogSource, Result,
    process::{self, LlamaArgs},
    runner::*,
    source::{Source, redact_url},
};
use async_trait::async_trait;
use lobo_proto::{DownloadProgress, GoTime, Timings, catalog};
use std::{
    future::Future,
    path::PathBuf,
    sync::{Arc, LazyLock, Mutex},
    time::Duration,
};
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

pub const LLAMA_BIN: &str = "/app/llama-server";
pub const CUDA_LD_PATH: &str = "/app:/usr/local/cuda/lib64";
pub const MODEL_DIR: &str = "/models";
pub const SLOW_CHECK_AFTER: Duration = Duration::from_secs(20);
pub const SSH_MAX_CONNS: usize = 8;
pub const CLOUDFLARED_URL: &str =
    "https://github.com/cloudflare/cloudflared/releases/download/2026.9.1/cloudflared-linux-amd64";
pub const BIN_DIR: &str = "/lobo/bin";
pub const RELEASE_JSON: &str = "/lobo/release.json";

pub fn free_mib(text: &str) -> Option<i64> {
    static RE: LazyLock<regex_lite::Regex> = LazyLock::new(|| {
        regex_lite::Regex::new(r"CUDA0: .*\((\d+) MiB, (\d+) MiB free\)").unwrap()
    });
    RE.captures(text)?.get(2)?.as_str().parse().ok()
}
pub struct GpuCheckCfg {
    pub llama_bin: PathBuf,
    pub model_id: String,
    pub min_free_mib: i64,
    pub give_up_after: Duration,
    pub retry_every: Duration,
    pub try_timeout: Duration,
    pub env: Vec<(String, String)>,
    pub logs: LogSource,
}
impl GpuCheckCfg {
    pub fn pod(model: &catalog::Model, env: Vec<(String, String)>, logs: LogSource) -> Self {
        Self {
            llama_bin: LLAMA_BIN.into(),
            model_id: model.id.clone(),
            min_free_mib: catalog::min_free_mib(model.size),
            give_up_after: Duration::from_secs(180),
            retry_every: Duration::from_secs(10),
            try_timeout: Duration::from_secs(60),
            env,
            logs,
        }
    }
}
pub struct PodGpuCheck(pub GpuCheckCfg);
#[async_trait]
impl GpuCheck for PodGpuCheck {
    async fn check(&self, cancel: CancellationToken) -> Result<()> {
        let cfg = &self.0;
        let start = Instant::now();
        for attempt in 1.. {
            let mut command = tokio::process::Command::new(&cfg.llama_bin);
            command
                .arg("--list-devices")
                .env_clear()
                .envs(cfg.env.iter().cloned())
                .env("LD_LIBRARY_PATH", CUDA_LD_PATH)
                .kill_on_drop(true);
            let output = tokio::select! {biased;_=cancel.cancelled()=>return Err(Error::Cancelled),out=tokio::time::timeout(cfg.try_timeout,command.output())=>out};
            let text = match output {
                Ok(Ok(o)) => String::from_utf8_lossy(&[o.stdout, o.stderr].concat()).into_owned(),
                Ok(Err(e)) => e.to_string(),
                Err(_) => "CUDA check timed out".into(),
            };
            if text.contains("CUDA0:") {
                for line in text.lines() {
                    cfg.logs.write(format!("[gpu-check] {line}\n").as_bytes());
                    println!("[gpu-check] {line}");
                }
                let free = free_mib(&text).unwrap_or(0);
                if free < cfg.min_free_mib {
                    return Err(Error::msg(format!(
                        "only {free} MiB VRAM free, {} needs {} MiB",
                        cfg.model_id, cfg.min_free_mib
                    )));
                }
                tracing::info!(
                    service = "lobo-agent",
                    attempt,
                    after_s = start.elapsed().as_secs_f64(),
                    free_mib = free,
                    "gpu check ok"
                );
                return Ok(());
            }
            tracing::warn!(service="lobo-agent", attempt,out=%process::last_line(&text),"gpu check failed, retrying");
            if start.elapsed() > cfg.give_up_after {
                return Err(Error::msg(format!(
                    "llama-server sees no CUDA device after {attempt} tries / {:.0}s: {}",
                    start.elapsed().as_secs_f64(),
                    process::last_line(&text)
                )));
            }
            tokio::select! {biased;_=cancel.cancelled()=>return Err(Error::Cancelled),_=tokio::time::sleep(cfg.retry_every)=>{}}
        }
        unreachable!()
    }
}
pub struct SlowGate {
    after: Duration,
    floor: f64,
    source: String,
    checked: bool,
}
impl SlowGate {
    pub fn new(after: Duration, floor_mbps: f64, redacted_src: String) -> Self {
        Self {
            after,
            floor: floor_mbps,
            source: redacted_src,
            checked: false,
        }
    }
    pub fn observe(&mut self, elapsed: Duration, mbps: f64) -> Option<String> {
        if self.checked || elapsed <= self.after {
            return None;
        }
        self.checked = true;
        (mbps < self.floor).then(|| {
            format!(
                "host: download too slow: {mbps:.1} MB/s after {:.0}s from {} (min {:.0})",
                elapsed.as_secs_f64(),
                self.source,
                self.floor
            )
        })
    }
}
pub async fn download_any<F, Fut>(
    cancel: &CancellationToken,
    urls: &[String],
    try_one: F,
    on_switch: impl Fn(&Error, &str),
) -> Result<()>
where
    F: Fn(usize, String) -> Fut,
    Fut: Future<Output = Result<()>>,
{
    for (i, url) in urls.iter().enumerate() {
        if cancel.is_cancelled() {
            return Err(Error::Cancelled);
        }
        match try_one(i, url.clone()).await {
            Ok(()) => return Ok(()),
            Err(e) => {
                if cancel.is_cancelled() {
                    return Err(Error::Cancelled);
                }
                if i + 1 == urls.len() {
                    return Err(e);
                }
                on_switch(&e, &urls[i + 1]);
            }
        }
    }
    Ok(())
}
pub type MakeSource = Arc<dyn Fn(&str) -> Result<Arc<dyn Source>> + Send + Sync>;
pub struct ModelDownload {
    pub urls: Vec<String>,
    pub dst: PathBuf,
    pub size: i64,
    pub sha: String,
    pub chunk_sha: Vec<String>,
    pub conns: usize,
    pub min_mbps: f64,
    pub slow_check_after: Duration,
    pub make_source: MakeSource,
}
impl ModelDownload {
    pub fn conns_for(&self, url: &str) -> usize {
        if url.starts_with("ssh://") {
            self.conns.min(SSH_MAX_CONNS)
        } else {
            self.conns
        }
    }
}
#[async_trait]
impl Download for ModelDownload {
    async fn run(
        &self,
        cancel: CancellationToken,
        on_progress: &(dyn Fn(DownloadProgress) + Sync),
    ) -> Result<()> {
        if cancel.is_cancelled() {
            return Err(Error::Cancelled);
        }
        if let Some(parent) = self.dst.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        download_any(
            &cancel,
            &self.urls,
            |i, url| {
                let cancel = &cancel;
                async move {
                    let source = (self.make_source)(&url)?;
                    let child = cancel.child_token();
                    let _guard = child.clone().drop_guard();
                    let start = Instant::now();
                    let gate = Mutex::new(SlowGate::new(self.slow_check_after, self.min_mbps, redact_url(&url)));
                    let cause = Mutex::new(None);
                    let progress = |mut dp: DownloadProgress| {
                        if i > 0 { dp.source = redact_url(&url); }
                        let mbps = dp.mbps;
                        on_progress(dp);
                        if let Some(error) = gate.lock().unwrap().observe(start.elapsed(), mbps) {
                            *cause.lock().unwrap() = Some(error);
                            child.cancel();
                        }
                    };
                    let result = crate::download::download_parallel(
                        child.clone(), source.as_ref(), &self.dst, self.size, &self.sha,
                        &self.chunk_sha, self.conns_for(&url), Some(&progress),
                    ).await;
                    if cancel.is_cancelled() { return Err(Error::Cancelled); }
                    if let Some(error) = cause.into_inner().unwrap() { return Err(Error::msg(error)); }
                    result
                }
            },
            |error, next| {
                tracing::warn!(service="lobo-agent", error=%error, next=%redact_url(next), "source failed, switching");
            },
        ).await
    }
}
pub fn tunnel_env(clean: &[(String, String)], token: &str) -> Vec<(String, String)> {
    let mut env = clean.to_vec();
    env.push(("TUNNEL_TOKEN".into(), token.into()));
    env
}
pub fn llama_env(clean: &[(String, String)], api_key: &str) -> Vec<(String, String)> {
    let mut env = clean.to_vec();
    env.push(("LD_LIBRARY_PATH".into(), CUDA_LD_PATH.into()));
    env.push(("LLAMA_API_KEY".into(), api_key.into()));
    env
}
pub struct PodTunnel {
    pub bin: PathBuf,
    pub url: String,
    pub token: String,
    pub env: Vec<(String, String)>,
    pub logs: LogSource,
}
#[async_trait]
impl Tunnel for PodTunnel {
    async fn start(&self, boot: CancellationToken, life: CancellationToken) -> Result<Exit> {
        tokio::select! {biased;_=boot.cancelled()=>return Err(Error::Cancelled),result=async {
            if let Some(parent)=self.bin.parent(){tokio::fs::create_dir_all(parent).await?;}
            crate::fetch::fetch_file(&self.url,&self.bin,0o755,-1,"").await
        }=>result?}
        if boot.is_cancelled() || life.is_cancelled() {
            return Err(Error::Cancelled);
        }
        Ok(process::start_process(
            &self.bin,
            &["tunnel".into(), "--no-autoupdate".into(), "run".into()],
            &tunnel_env(&self.env, &self.token),
            self.logs.clone(),
            Some("cloudflared"),
            life,
        )?
        .exited)
    }
}
pub struct PodLlama {
    pub bin: PathBuf,
    pub args: LlamaArgs,
    pub api_key: String,
    pub env: Vec<(String, String)>,
    pub logs: LogSource,
    pub life: CancellationToken,
    pub health_base: String,
    pub poll: Duration,
}
#[async_trait]
impl Llama for PodLlama {
    async fn start(&self) -> Result<Exit> {
        if self.life.is_cancelled() {
            return Err(Error::Cancelled);
        }
        Ok(process::start_process(
            &self.bin,
            &self.args.to_args(),
            &llama_env(&self.env, &self.api_key),
            self.logs.clone(),
            Some("llama"),
            self.life.clone(),
        )?
        .exited)
    }
    async fn wait_healthy(&self, cancel: CancellationToken) -> Result<()> {
        crate::health::wait_healthy(&self.health_base, self.poll, cancel).await
    }
}
pub fn boot_timings(get: &dyn Fn(&str) -> Option<String>) -> Timings {
    let ts = |key| {
        get(key)
            .and_then(|v| v.parse::<f64>().ok())
            .filter(|v| v.is_finite())
            .unwrap_or(0.0)
    };
    let (t0, apt, zip) = (ts("LOBO_T_BOOT0"), ts("LOBO_T_APT"), ts("LOBO_T_ZIP"));
    let mut t = Timings::default();
    if t0 > 0.0 {
        t.container_started_at =
            GoTime::from_utc(chrono::DateTime::from_timestamp_nanos((t0 * 1e9) as i64));
        if apt > t0 {
            t.bootstrap_apt_s = apt - t0;
        }
        if zip > apt && apt > 0.0 {
            t.bootstrap_zip_s = zip - apt;
        }
    }
    t
}

// The subscriber writes one formatted event at a time to both destinations.
#[derive(Clone)]
struct LogWriter(LogSource);
impl std::io::Write for LogWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        std::io::stdout().lock().write_all(bytes)?;
        self.0.write(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        std::io::stdout().flush()
    }
}
pub fn init_logging(logs: LogSource) {
    let writer = LogWriter(logs);
    let _ = tracing_subscriber::fmt()
        .json()
        .flatten_event(true)
        .with_target(false)
        .with_current_span(false)
        .with_span_list(false)
        .with_writer(move || writer.clone())
        .try_init();
}
pub async fn run(get: &(dyn Fn(&str) -> Option<String> + Sync), logs: LogSource) -> Result<()> {
    use crate::{
        config::AgentConfig,
        metrics::Collector,
        selfkill::{RetryKiller, self_api},
    };
    let cfg = AgentConfig::from_vars(get)?;
    let model = catalog::get(&cfg.model)
        .map_err(|e| Error::msg(e.to_string()))?
        .clone();
    let version = tokio::fs::read(RELEASE_JSON)
        .await
        .unwrap_or_else(|_| br#"{"version":"unknown"}"#.to_vec());
    tracing::info!(service="lobo-agent", model=%model.file,ctx=cfg.ctx,expires_at=%cfg.expires_at,"start");
    let clean = process::clean_env();
    let life = CancellationToken::new();
    let _guard = life.clone().drop_guard();
    let mut urls = vec![cfg.model_url.clone()];
    if !cfg.model_fallback.is_empty() {
        urls.push(cfg.model_fallback.clone());
    }
    let (ssh, host, file, size) = (
        cfg.model_ssh_key.clone(),
        cfg.model_host_key.clone(),
        model.file.clone(),
        model.size,
    );
    let api = self_api(
        &cfg.provider,
        &cfg.runpod_pod_id,
        &cfg.runpod_api_key,
        &cfg.vast_id,
        &cfg.vast_api_key,
    )
    .ok_or_else(|| Error::msg("invalid provider instance credentials"))?;
    let d = Deps {
        tunnel: Arc::new(PodTunnel {
            bin: PathBuf::from(BIN_DIR).join("cloudflared"),
            url: CLOUDFLARED_URL.into(),
            token: cfg.cf_tunnel_token,
            env: clean.clone(),
            logs: logs.clone(),
        }),
        gpu_check: Arc::new(PodGpuCheck(GpuCheckCfg::pod(
            &model,
            clean.clone(),
            logs.clone(),
        ))),
        download: Arc::new(ModelDownload {
            urls,
            dst: PathBuf::from(MODEL_DIR).join(&model.file),
            size: model.size,
            sha: model.sha256.clone(),
            chunk_sha: model.chunk_sha.clone(),
            conns: cfg.dl_conns,
            min_mbps: cfg.min_mbps as f64,
            slow_check_after: SLOW_CHECK_AFTER,
            make_source: Arc::new(move |url| {
                crate::source::model_source(url, &ssh, &host, &file, size)
            }),
        }),
        llama: Arc::new(PodLlama {
            bin: LLAMA_BIN.into(),
            args: LlamaArgs {
                model_path: PathBuf::from(MODEL_DIR)
                    .join(&model.file)
                    .to_string_lossy()
                    .into_owned(),
                alias: model.alias,
                host: "127.0.0.1".into(),
                port: 8080,
                ctx: cfg.ctx,
            },
            api_key: cfg.lobo_api_key.clone(),
            env: clean,
            logs: logs.clone(),
            life: life.clone(),
            health_base: format!("http://{}", crate::LLAMA_ADDR),
            poll: Duration::from_secs(2),
        }),
        metrics: Arc::new(Collector {
            llama_url: format!("http://{}", crate::LLAMA_ADDR),
            api_key: cfg.lobo_api_key.clone(),
            http: crate::http::client().clone(),
            nvidia_smi: PathBuf::new(),
            proc_dir: PathBuf::new(),
        }),
        killer: Arc::new(RetryKiller { api }),
    };
    let mut timings = boot_timings(get);
    timings.download_conns = cfg.dl_conns as i64;
    timings.download_source = redact_url(&cfg.model_url);
    let idle = Duration::from_secs(
        (cfg.idle_min as u64)
            .checked_mul(60)
            .ok_or_else(|| Error::msg("config: LOBO_IDLE_MIN: too large"))?,
    );
    let runner = Runner::new(
        d,
        RunnerConfig {
            boot_id: cfg.boot_id,
            timings,
            model: cfg.model,
            ctx: cfg.ctx,
            idle,
            expires_at: cfg.expires_at,
            boot_timeout: cfg.boot_timeout,
            tick: Duration::from_secs(30),
            fail_grace: Duration::from_secs(120),
        },
    );
    let server = async {
        let listener = tokio::net::TcpListener::bind(crate::AGENT_ADDR).await?;
        axum::serve(
            listener,
            crate::api::router(runner.clone(), cfg.lobo_api_key, logs, version.into()),
        )
        .await
    };
    tokio::pin!(server);
    let run = runner.clone().run(life);
    tokio::pin!(run);
    tokio::select! {
        result=&mut run=>result,
        result=&mut server=>{if let Err(e)=result{tracing::error!(service="lobo-agent", error=%e,"api server");}run.await}
    }
}
pub async fn main_flow<F>(
    run: F,
    api: Option<Arc<dyn crate::selfkill::PodApi>>,
    kill_budget: Duration,
) -> std::process::ExitCode
where
    F: Future<Output = Result<()>> + Send + 'static,
{
    let error = match tokio::spawn(run).await {
        Ok(Ok(())) => return std::process::ExitCode::SUCCESS,
        Ok(Err(e)) => e.to_string(),
        Err(e) if e.is_panic() => {
            let payload = e.into_panic();
            let msg = payload
                .downcast_ref::<String>()
                .map(String::as_str)
                .or_else(|| payload.downcast_ref::<&str>().copied())
                .unwrap_or("unknown panic");
            format!("panic: {msg}")
        }
        Err(e) => e.to_string(),
    };
    eprintln!("lobo-agent fatal: {error}");
    if let Some(api) = api {
        let killer = crate::selfkill::RetryKiller { api };
        let _ = tokio::time::timeout(kill_budget, killer.kill_self(CancellationToken::new())).await;
    } else {
        eprintln!(
            "lobo-agent: no instance id/key for provider {:?}, cannot self-terminate",
            std::env::var("LOBO_PROVIDER").unwrap_or_default()
        );
    }
    std::process::ExitCode::FAILURE
}
pub fn bench_line(
    source: &str,
    conns: usize,
    bytes: i64,
    mbps: f64,
    error: Option<&str>,
) -> String {
    let mut value = serde_json::json!({"source":source.chars().take(40).collect::<String>(),"conns":conns,"bytes":bytes,"mbps":mbps});
    if let Some(error) = error {
        value["err"] = error.into();
    }
    value.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        LogRing,
        source::{BoxRead, HttpSource},
        testutil::*,
    };
    use sha2::{Digest, Sha256};
    use std::{
        os::unix::fs::PermissionsExt,
        sync::atomic::{AtomicUsize, Ordering},
    };
    fn script(dir: &std::path::Path, name: &str, body: &str) -> PathBuf {
        let p = dir.join(name);
        std::fs::write(&p, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
        p
    }
    fn gpu_cfg(bin: PathBuf) -> GpuCheckCfg {
        let mut c = GpuCheckCfg::pod(
            catalog::get("q8").unwrap(),
            vec![("PATH".into(), "/bin:/usr/bin".into())],
            Arc::new(LogRing::new(100)),
        );
        c.llama_bin = bin;
        c.retry_every = Duration::from_millis(10);
        c.give_up_after = Duration::from_secs(3);
        c.try_timeout = Duration::from_secs(2);
        c
    }
    async fn fatal(panic: bool) {
        use wiremock::{
            Mock, MockServer, ResponseTemplate,
            matchers::{body_string_contains, header, method},
        };
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(header("Authorization", "Bearer podkey"))
            .and(header("User-Agent", crate::http::USER_AGENT.as_str()))
            .and(body_string_contains("podTerminate"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({"data":{"podTerminate":null}})),
            )
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(body_string_contains("desiredStatus"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({"data":{"pod":null}})),
            )
            .expect(1)
            .mount(&server)
            .await;
        let mut api = crate::selfkill::RunPodSelf::new("pod", "podkey");
        api.url = server.uri();
        let code = main_flow(
            async move {
                if panic {
                    panic!("test startup panic");
                }
                Err(Error::msg("test startup error"))
            },
            Some(Arc::new(api)),
            Duration::from_secs(2),
        )
        .await;
        assert_eq!(code, std::process::ExitCode::FAILURE);
    }
    #[tokio::test]
    async fn fatal_error_self_terminates() {
        fatal(false).await;
    }
    #[tokio::test]
    async fn panic_in_run_self_terminates() {
        fatal(true).await;
    }
    #[test]
    fn bench_line_shape() {
        let value: serde_json::Value =
            serde_json::from_str(&bench_line(&"x".repeat(60), 4, 100, 12.3, None)).unwrap();
        assert_eq!(value.as_object().unwrap().len(), 4);
        assert_eq!(value["source"].as_str().unwrap().len(), 40);
        assert_eq!(value["conns"], 4);
        assert_eq!(value["bytes"], 100);
        assert_eq!(value["mbps"], 12.3);
        let value: serde_json::Value =
            serde_json::from_str(&bench_line("src", 1, 0, 0.0, Some("failed"))).unwrap();
        assert_eq!(value["err"], "failed");
    }
    #[test]
    fn free_mib_reads_cuda0_free() {
        assert_eq!(
            free_mib("CUDA0: NVIDIA GeForce RTX 5090 (32109 MiB, 31000 MiB free)"),
            Some(31000)
        );
        assert_eq!(free_mib("CUDA1: GPU (32000 MiB, 31000 MiB free)"), None);
        assert_eq!(free_mib("CUDA0: unknown"), None);
    }
    #[test]
    fn gpu_check_defaults_are_3min_10s_1min() {
        let c = GpuCheckCfg::pod(
            catalog::get("q8").unwrap(),
            vec![],
            Arc::new(LogRing::new(1)),
        );
        assert_eq!(
            (c.give_up_after, c.retry_every, c.try_timeout),
            (
                Duration::from_secs(180),
                Duration::from_secs(10),
                Duration::from_secs(60)
            )
        );
        assert_eq!(c.min_free_mib, 29831);
    }
    #[tokio::test]
    async fn gpu_check_retries_until_cuda0() {
        let dir = tempfile::tempdir().unwrap();
        let count = dir.path().join("n");
        let bin = script(
            dir.path(),
            "gpu",
            &format!(
                "n=0; [ ! -f '{0}' ] || n=$(cat '{0}'); n=$((n+1)); echo $n > '{0}'; [ $n -ge 3 ] && echo 'CUDA0: GPU (32109 MiB, 31000 MiB free)'",
                count.display()
            ),
        );
        PodGpuCheck(gpu_cfg(bin))
            .check(CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(std::fs::read_to_string(count).unwrap().trim(), "3");
    }
    #[tokio::test]
    async fn gpu_check_gives_up_after_window() {
        let dir = tempfile::tempdir().unwrap();
        let bin = script(dir.path(), "gpu", "echo 'error no device'");
        let mut cfg = gpu_cfg(bin);
        cfg.give_up_after = Duration::from_millis(50);
        let e = PodGpuCheck(cfg)
            .check(CancellationToken::new())
            .await
            .unwrap_err();
        assert!(e.to_string().contains("sees no CUDA device"));
    }
    #[tokio::test]
    async fn gpu_check_hung_try_is_bounded() {
        let dir = tempfile::tempdir().unwrap();
        let marker = dir.path().join("once");
        let bin = script(
            dir.path(),
            "gpu",
            &format!(
                "if [ ! -f '{0}' ]; then touch '{0}'; exec sleep 30; fi\necho 'CUDA0: GPU (32109 MiB, 31000 MiB free)'",
                marker.display()
            ),
        );
        let mut c = gpu_cfg(bin);
        c.give_up_after = Duration::from_secs(2);
        c.try_timeout = Duration::from_millis(100);
        within(
            Duration::from_secs(5),
            PodGpuCheck(c).check(CancellationToken::new()),
        )
        .await
        .unwrap();
    }
    #[tokio::test]
    async fn gpu_check_short_vram_is_bad_host() {
        let dir = tempfile::tempdir().unwrap();
        let bin = script(
            dir.path(),
            "gpu",
            "echo 'CUDA0: GPU (32109 MiB, 20000 MiB free)'",
        );
        let e = PodGpuCheck(gpu_cfg(bin))
            .check(CancellationToken::new())
            .await
            .unwrap_err();
        assert_eq!(
            e.to_string(),
            "only 20000 MiB VRAM free, q8 needs 29831 MiB"
        );
    }
    #[test]
    fn slow_gate_decides_once_after_20s() {
        let mut s = SlowGate::new(SLOW_CHECK_AFTER, 100.0, "https://h".into());
        assert!(s.observe(Duration::from_secs(19), 10.0).is_none());
        assert_eq!(
            s.observe(Duration::from_secs(21), 99.9).unwrap(),
            "host: download too slow: 99.9 MB/s after 21s from https://h (min 100)"
        );
        assert!(s.observe(Duration::from_secs(22), 1.0).is_none());
        let mut s = SlowGate::new(SLOW_CHECK_AFTER, 100.0, "https://h".into());
        assert!(s.observe(Duration::from_secs(21), 150.0).is_none());
        assert!(s.observe(Duration::from_secs(22), 1.0).is_none());
    }
    #[tokio::test]
    async fn download_any_falls_back_on_any_error() {
        for error in [
            "host: download too slow",
            "HTTP 403",
            "gave up after 8 resumes",
        ] {
            let calls = AtomicUsize::new(0);
            let switches = AtomicUsize::new(0);
            download_any(
                &CancellationToken::new(),
                &["a".into(), "b".into()],
                |i, _| {
                    calls.fetch_add(1, Ordering::SeqCst);
                    async move {
                        if i == 0 {
                            Err(Error::msg(error))
                        } else {
                            Ok(())
                        }
                    }
                },
                |_, _| {
                    switches.fetch_add(1, Ordering::SeqCst);
                },
            )
            .await
            .unwrap();
            assert_eq!(calls.load(Ordering::SeqCst), 2);
            assert_eq!(switches.load(Ordering::SeqCst), 1);
        }
        let cancel = CancellationToken::new();
        let calls = AtomicUsize::new(0);
        let e = download_any(
            &cancel,
            &["a".into(), "b".into()],
            |_, _| {
                calls.fetch_add(1, Ordering::SeqCst);
                cancel.cancel();
                async { Err(Error::Cancelled) }
            },
            |_, _| panic!("switched after cancel"),
        )
        .await
        .unwrap_err();
        assert!(matches!(e, Error::Cancelled));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        let e = download_any(
            &CancellationToken::new(),
            &["a".into(), "b".into()],
            |_, url| async move { Err(Error::msg(url)) },
            |_, _| {},
        )
        .await
        .unwrap_err();
        assert_eq!(e.to_string(), "b");
    }
    struct Trickle;
    impl std::fmt::Display for Trickle {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("https://slow")
        }
    }
    #[async_trait]
    impl Source for Trickle {
        async fn open(&self, _: i64, _: i64) -> Result<(BoxRead, i64)> {
            let stream = futures_util::stream::unfold((), |()| async {
                tokio::time::sleep(Duration::from_millis(50)).await;
                Some((
                    Ok::<_, std::io::Error>(bytes::Bytes::from_static(&[0; 1024])),
                    (),
                ))
            });
            Ok((
                Box::new(tokio_util::io::StreamReader::new(Box::pin(stream))),
                100_000,
            ))
        }
    }
    fn download(dst: PathBuf) -> ModelDownload {
        ModelDownload {
            urls: vec!["https://slow".into()],
            dst,
            size: 100_000,
            sha: String::new(),
            chunk_sha: vec![],
            conns: 1,
            min_mbps: 1000.0,
            slow_check_after: Duration::from_millis(100),
            make_source: Arc::new(|_| Ok(Arc::new(Trickle))),
        }
    }
    #[tokio::test]
    async fn slow_source_fails_with_host_marker() {
        let dir = tempfile::tempdir().unwrap();
        let d = download(dir.path().join("m"));
        let e = within(
            Duration::from_secs(3),
            d.run(CancellationToken::new(), &|_| {}),
        )
        .await
        .unwrap_err();
        assert!(e.to_string().contains("host: download too slow"), "{e}");
    }
    #[tokio::test]
    async fn r2_slow_switches_to_fallback_source() {
        let dir = tempfile::tempdir().unwrap();
        let bytes = data(100_000);
        let server = RangeServer::new(bytes.clone(), Fault::Normal).await;
        let mut d = download(dir.path().join("m"));
        d.urls.push(server.url.clone());
        d.sha = hex::encode(Sha256::digest(&*bytes));
        d.conns = 2;
        d.make_source = Arc::new(|url| {
            if url == "https://slow" {
                Ok(Arc::new(Trickle))
            } else {
                Ok(Arc::new(HttpSource { url: url.into() }))
            }
        });
        let sources = Mutex::new(Vec::new());
        within(
            Duration::from_secs(4),
            d.run(CancellationToken::new(), &|p| {
                sources.lock().unwrap().push(p.source);
            }),
        )
        .await
        .unwrap();
        assert_eq!(std::fs::read(&d.dst).unwrap(), *bytes);
        assert!(sources.lock().unwrap().contains(&redact_url(&server.url)));
    }
    #[test]
    fn ssh_source_conns_capped_at_8() {
        let mut d = download(PathBuf::new());
        d.conns = 32;
        assert_eq!(d.conns_for("ssh://host"), 8);
        assert_eq!(d.conns_for("https://host"), 32);
    }
    #[test]
    fn boot_timings() {
        let get = |key: &str| match key {
            "LOBO_T_BOOT0" => Some("100.5".into()),
            "LOBO_T_APT" => Some("110.5".into()),
            "LOBO_T_ZIP" => Some("112".into()),
            _ => None,
        };
        let t = super::boot_timings(&get);
        assert_eq!(
            serde_json::to_value(t.container_started_at).unwrap(),
            "1970-01-01T00:01:40.5Z"
        );
        assert_eq!((t.bootstrap_apt_s, t.bootstrap_zip_s), (10.0, 1.5));
        assert!(
            super::boot_timings(&|_| None)
                .container_started_at
                .is_zero()
        );
        assert_eq!(
            super::boot_timings(&|k| if k == "LOBO_T_APT" { None } else { get(k) }).bootstrap_zip_s,
            0.0
        );
    }
    #[tokio::test]
    async fn pod_children_env_has_only_their_own_secret() {
        let dir = tempfile::tempdir().unwrap();
        let bin = script(dir.path(), "dump", "env");
        let clean = process::CleanEnv::filter(
            [
                "LOBO_API_KEY",
                "CF_TUNNEL_TOKEN",
                "RUNPOD_API_KEY",
                "CONTAINER_API_KEY",
                "LLAMA_ARG_HOST",
                "PATH",
            ]
            .into_iter()
            .map(|k| {
                (
                    k.into(),
                    if k == "PATH" {
                        "/usr/bin:/bin".into()
                    } else {
                        "secret".into()
                    },
                )
            }),
        );
        for (env, want, other) in [
            (
                tunnel_env(&clean, "tunnel"),
                "TUNNEL_TOKEN=tunnel",
                "LLAMA_API_KEY",
            ),
            (
                llama_env(&clean, "llama"),
                "LLAMA_API_KEY=llama",
                "TUNNEL_TOKEN",
            ),
        ] {
            let logs = Arc::new(LogRing::new(100));
            let p = process::start_process(
                &bin,
                &[],
                &env,
                logs.clone(),
                None,
                CancellationToken::new(),
            )
            .unwrap();
            p.exited.await.unwrap().unwrap();
            let output = logs.tail(100).join("\n");
            assert!(output.contains(want));
            assert!(!output.contains(other));
            for bad in [
                "LOBO_API_KEY",
                "CF_TUNNEL_TOKEN",
                "RUNPOD_API_KEY",
                "CONTAINER_API_KEY",
                "LLAMA_ARG_HOST",
            ] {
                assert!(!output.contains(bad));
            }
            if want.starts_with("LLAMA") {
                assert!(output.contains(&format!("LD_LIBRARY_PATH={CUDA_LD_PATH}")));
            }
        }
    }
    #[tokio::test]
    async fn pod_tunnel_fetches_then_starts() {
        use wiremock::{Mock, MockServer, ResponseTemplate, matchers::any};
        let server = MockServer::start().await;
        Mock::given(any())
            .respond_with(ResponseTemplate::new(200).set_body_string("#!/bin/sh\necho \"$@\"\n"))
            .mount(&server)
            .await;
        let dir = tempfile::tempdir().unwrap();
        let logs = Arc::new(LogRing::new(10));
        let t = PodTunnel {
            bin: dir.path().join("bin/cloudflared"),
            url: server.uri(),
            token: "test".into(),
            env: vec![],
            logs: logs.clone(),
        };
        t.start(CancellationToken::new(), CancellationToken::new())
            .await
            .unwrap()
            .await
            .unwrap()
            .unwrap();
        assert_eq!(logs.tail(1), ["[cloudflared] tunnel --no-autoupdate run"]);
    }
}
