use super::{models, platform};
use crate::{Error, Result};
use async_trait::async_trait;
use futures_util::future::BoxFuture;
use lobo_agent::{
    LogSource,
    process::{CleanEnv, LlamaArgs},
    runner::{Download, Exit, GpuCheck, Killer, Llama, Metrics, Tunnel},
};
use lobo_proto::{
    DownloadProgress, Gpu, Host, Llama as LlamaMetrics,
    catalog::{Model, min_free_mib},
};
use std::{
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU32, Ordering},
    },
    time::Duration,
};
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;

pub struct MacConfig {
    pub weights: PathBuf,
    pub llama_server: PathBuf,
    pub api_key: String,
    pub port: u16,
    pub ctx: i64,
    pub model: Model,
    pub base_env: Vec<(String, String)>,
}
type VmStat = Arc<dyn Fn() -> BoxFuture<'static, Result<String>> + Send + Sync>;
pub struct MacDeps {
    cfg: MacConfig,
    logs: LogSource,
    hf_base: String,
    llama_url: String,
    poll: Duration,
    usable_mib: Arc<dyn Fn() -> Result<i64> + Send + Sync>,
    vm_stat: VmStat,
    host: std::result::Result<Gpu, String>,
    pid: AtomicU32,
    llama_started: AtomicBool,
    llama_done: CancellationToken,
    stop: CancellationToken,
    tunnel_sender: Mutex<Option<oneshot::Sender<lobo_agent::Result<()>>>>,
}
impl MacDeps {
    fn new(cfg: MacConfig, logs: LogSource, stop: CancellationToken) -> Self {
        let env = CleanEnv::filter(cfg.base_env.clone());
        let vm_stat: VmStat = Arc::new(move || {
            let env = env.clone();
            Box::pin(async move {
                let output = tokio::process::Command::new("vm_stat")
                    .env_clear()
                    .envs(env)
                    .kill_on_drop(true)
                    .output()
                    .await?;
                if !output.status.success() {
                    return Err(Error::Local(format!("vm_stat: {}", output.status)));
                }
                Ok(String::from_utf8_lossy(&output.stdout).into())
            })
        });
        Self {
            llama_url: format!("http://127.0.0.1:{}", cfg.port),
            cfg,
            logs,
            hf_base: models::HF_BASE.into(),
            poll: Duration::from_secs(2),
            usable_mib: Arc::new(platform::usable_mib),
            vm_stat,
            host: host_gpu(&platform::sysctl_string, &platform::mem_bytes)
                .map_err(|e| e.to_string()),
            pid: AtomicU32::new(0),
            llama_started: AtomicBool::new(false),
            llama_done: CancellationToken::new(),
            stop,
            tunnel_sender: Mutex::new(None),
        }
    }
    pub(crate) async fn check_gpu(&self, cancel: &CancellationToken) -> Result<()> {
        let output = tokio::select! {biased;
            _ = cancel.cancelled() => return Err(Error::Cancelled),
            output = tokio::time::timeout(Duration::from_secs(60),tokio::process::Command::new(&self.cfg.llama_server)
                .arg("--list-devices").env_clear().envs(CleanEnv::filter(self.cfg.base_env.clone())).kill_on_drop(true).output()) => output,
        };
        let output =
            output.map_err(|_| Error::Local("llama-server --list-devices: timeout".into()))??;
        let out = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let line = format!("[gpu-check] {}\n", out.trim());
        self.logs.write(line.as_bytes());
        print!("{line}");
        if !out.contains("MTL0") {
            let detail = if out.trim().is_empty() {
                "no output".into()
            } else {
                lobo_agent::process::last_line(&out)
            };
            return Err(Error::Local(format!(
                "llama-server sees no Metal device: {detail}"
            )));
        }
        let usable = (self.usable_mib)()?;
        let need = min_free_mib(self.cfg.model.size);
        if need > usable {
            return Err(Error::Local(format!(
                "{} needs {:.1} GB, this Mac allows ~{:.0} GB to the GPU",
                self.cfg.model.id,
                need as f64 / 1024.0,
                usable as f64 / 1024.0
            )));
        }
        Ok(())
    }
    pub(crate) async fn download(
        &self,
        cancel: CancellationToken,
        on_progress: &(dyn Fn(DownloadProgress) + Sync),
    ) -> Result<()> {
        if cancel.is_cancelled() {
            return Err(Error::Cancelled);
        }
        let m = &self.cfg.model;
        let w = &self.cfg.weights;
        let dst = w.join(&m.file);
        let marker = models::marker_path(w, &m.file);
        if let Ok(meta) = std::fs::metadata(&dst)
            && meta.len() == m.size as u64
        {
            if models::marker_valid(w, m, Some(&meta)) {
                return Ok(());
            }
            let got =
                lobo_agent::download::hash_file(cancel, &dst, m.size, Some(on_progress)).await?;
            if got != m.sha256 {
                return Err(self.quarantine(
                    &dst,
                    Error::Local(format!("{}: sha256 {got}, want {}", m.file, m.sha256)),
                ));
            }
            return models::write_marker(w, m);
        }
        match std::fs::remove_file(marker) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
        let source = lobo_agent::source::HttpSource {
            url: format!("{}{}", self.hf_base, m.file),
        };
        if let Err(e) =
            lobo_agent::download::download(cancel, &source, &dst, &m.sha256, Some(on_progress))
                .await
        {
            let bad = PathBuf::from(format!("{}.bad", dst.display()));
            return Err(if bad.exists() {
                self.quarantine(&bad, e.into())
            } else {
                e.into()
            });
        }
        models::write_marker(w, m)
    }
    fn quarantine(&self, src: &Path, cause: Error) -> Error {
        let dir = self.cfg.weights.join(".bad");
        let to = dir.join(format!(
            "{}.{}",
            self.cfg.model.file,
            chrono::Utc::now().timestamp()
        ));
        if let Err(e) = std::fs::create_dir_all(&dir).and_then(|()| std::fs::rename(src, &to)) {
            return Error::Local(format!("{cause} (quarantine: {e})"));
        }
        Error::Local(format!("{cause}, moved to {}", to.display()))
    }
    pub(crate) async fn gpu(&self) -> Result<Gpu> {
        let mut gpu = self.host.clone().map_err(Error::Local)?;
        let out = tokio::time::timeout(Duration::from_secs(3), (self.vm_stat)())
            .await
            .map_err(|_| Error::Local("vm_stat: timeout".into()))??;
        gpu.vram_used_mb = parse_vm_stat(&out)?;
        Ok(gpu)
    }
    pub async fn wait_llama(&self, timeout: Duration) -> bool {
        if !self.llama_started.load(Ordering::Acquire) {
            return true;
        }
        tokio::time::timeout(timeout, self.llama_done.cancelled())
            .await
            .is_ok()
    }
}
impl Drop for MacDeps {
    fn drop(&mut self) {
        self.stop.cancel();
    }
}
pub fn new_deps(
    cfg: MacConfig,
    logs: LogSource,
    stop: CancellationToken,
) -> (lobo_agent::Deps, Arc<MacDeps>) {
    let d = Arc::new(MacDeps::new(cfg, logs, stop));
    (
        lobo_agent::Deps {
            tunnel: d.clone(),
            gpu_check: d.clone(),
            download: d.clone(),
            llama: d.clone(),
            metrics: d.clone(),
            killer: d.clone(),
        },
        d,
    )
}
fn agent_error(e: Error) -> lobo_agent::Error {
    match e {
        Error::Agent(e) => e,
        Error::Cancelled => lobo_agent::Error::Cancelled,
        e => lobo_agent::Error::msg(e.to_string()),
    }
}
#[async_trait]
impl Tunnel for MacDeps {
    async fn start(
        &self,
        _boot: CancellationToken,
        _life: CancellationToken,
    ) -> lobo_agent::Result<Exit> {
        let (sender, receiver) = oneshot::channel();
        *self.tunnel_sender.lock().unwrap() = Some(sender);
        Ok(receiver)
    }
}
#[async_trait]
impl GpuCheck for MacDeps {
    async fn check(&self, cancel: CancellationToken) -> lobo_agent::Result<()> {
        self.check_gpu(&cancel).await.map_err(agent_error)
    }
}
#[async_trait]
impl Download for MacDeps {
    async fn run(
        &self,
        cancel: CancellationToken,
        on_progress: &(dyn Fn(DownloadProgress) + Sync),
    ) -> lobo_agent::Result<()> {
        self.download(cancel, on_progress)
            .await
            .map_err(agent_error)
    }
}
#[async_trait]
impl Llama for MacDeps {
    async fn start(&self) -> lobo_agent::Result<Exit> {
        if self.llama_started.swap(true, Ordering::AcqRel) {
            return Err(lobo_agent::Error::msg("llama-server already started"));
        }
        let args = LlamaArgs {
            model_path: self
                .cfg
                .weights
                .join(&self.cfg.model.file)
                .to_string_lossy()
                .into(),
            alias: self.cfg.model.alias.clone(),
            host: "127.0.0.1".into(),
            port: self.cfg.port,
            ctx: self.cfg.ctx,
        }
        .to_args();
        let mut env = CleanEnv::filter(self.cfg.base_env.clone());
        env.push(("LLAMA_API_KEY".into(), self.cfg.api_key.clone()));
        let proc = match lobo_agent::process::start_process(
            &self.cfg.llama_server,
            &args,
            &env,
            self.logs.clone(),
            Some("llama"),
            self.stop.clone(),
        ) {
            Ok(proc) => proc,
            Err(e) => {
                self.llama_done.cancel();
                return Err(e);
            }
        };
        self.pid.store(proc.pid, Ordering::Release);
        let done = self.llama_done.clone();
        let (sender, receiver) = oneshot::channel();
        tokio::spawn(async move {
            let result = proc.exited.await.unwrap_or_else(|_| {
                Err(lobo_agent::Error::msg("llama-server exit monitor closed"))
            });
            done.cancel();
            let _ = sender.send(result);
        });
        Ok(receiver)
    }
    async fn wait_healthy(&self, cancel: CancellationToken) -> lobo_agent::Result<()> {
        lobo_agent::health::wait_healthy(&self.llama_url, self.poll, cancel).await
    }
}
#[async_trait]
impl Metrics for MacDeps {
    async fn llama(&self) -> lobo_agent::Result<LlamaMetrics> {
        lobo_agent::metrics::Collector {
            llama_url: self.llama_url.clone(),
            api_key: self.cfg.api_key.clone(),
            http: lobo_agent::http::client(),
            nvidia_smi: PathBuf::new(),
            proc_dir: PathBuf::new(),
        }
        .llama()
        .await
    }
    async fn gpu(&self) -> lobo_agent::Result<Gpu> {
        self.gpu().await.map_err(agent_error)
    }
    async fn host(&self) -> lobo_agent::Result<Host> {
        Err(lobo_agent::Error::msg(
            "host metrics: not collected on macOS",
        ))
    }
}
#[async_trait]
impl Killer for MacDeps {
    async fn kill_self(&self, _cancel: CancellationToken) -> lobo_agent::Result<()> {
        self.stop.cancel();
        Ok(())
    }
}

pub(crate) fn host_gpu(
    sysctl: &dyn Fn(&str) -> Result<String>,
    mem: &dyn Fn() -> Result<u64>,
) -> Result<Gpu> {
    Ok(Gpu {
        name: sysctl("machdep.cpu.brand_string")?.trim().into(),
        vram_total_mb: (mem()? >> 20) as i64,
        ..Default::default()
    })
}
pub(crate) fn parse_vm_stat(out: &str) -> Result<i64> {
    let mut page = 0i64;
    let mut pages = 0i64;
    let mut found = 0;
    for line in out.lines() {
        if let Some((_, tail)) = line.split_once("page size of ") {
            page = tail
                .split_whitespace()
                .next()
                .and_then(|v| v.parse().ok())
                .ok_or_else(|| Error::Local(format!("vm_stat: bad header {line:?}")))?;
            continue;
        }
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        if matches!(
            key.trim(),
            "Pages active" | "Pages wired down" | "Pages occupied by compressor"
        ) {
            let n: i64 = value
                .trim()
                .trim_end_matches('.')
                .parse()
                .map_err(|_| Error::Local(format!("vm_stat: bad {line:?}")))?;
            pages = pages
                .checked_add(n)
                .ok_or_else(|| Error::Local("vm_stat: page count overflow".into()))?;
            found += 1;
        }
    }
    if page <= 0 || pages < 0 || found != 3 {
        return Err(Error::Local("vm_stat: unexpected output".into()));
    }
    Ok(pages
        .checked_mul(page)
        .ok_or_else(|| Error::Local("vm_stat: page count overflow".into()))?
        >> 20)
}

#[cfg(test)]
mod tests;
