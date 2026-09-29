use super::{
    deps::{MacConfig, MacDeps, new_deps},
    provider::{command_of, is_supervisor},
    runtime::ensure_runtime,
    state::StateFile,
};
use crate::{
    Error, Result,
    config::{self, Laptop},
};
use lobo_agent::{LogRing, LogSource, Runner, RunnerConfig};
use lobo_proto::{GoTime, LocalState, Manifest, catalog};
use std::{future::IntoFuture, io::Write, path::PathBuf, sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;
use tracing::instrument::WithSubscriber;

pub const SUPERVISOR_ARG: &str = "--lobo-local-run";
#[derive(Debug, Clone, PartialEq)]
pub struct RunConfig {
    pub model: String,
    pub ctx: i64,
    pub idle_min: i64,
    pub boot_id: String,
    pub port: u16,
    pub api_port: u16,
    pub config_path: Option<PathBuf>,
    pub version: Manifest,
}
impl RunConfig {
    pub fn from_args(args: &[String], version: Manifest) -> Result<Self> {
        let mut cfg = Self {
            model: "q6".into(),
            ctx: 0,
            idle_min: 0,
            boot_id: String::new(),
            port: 8931,
            api_port: 8932,
            config_path: None,
            version,
        };
        let mut port = 8931i64;
        let mut api_port = 8932i64;
        let mut args = args.iter();
        while let Some(flag) = args.next() {
            if !matches!(
                flag.as_str(),
                "--model"
                    | "--ctx"
                    | "--idle-min"
                    | "--boot-id"
                    | "--port"
                    | "--api-port"
                    | "--config"
            ) {
                return Err(Error::Other(format!("unknown argument {flag:?}")));
            }
            let value = args
                .next()
                .ok_or_else(|| Error::Other(format!("{flag}: missing value")))?;
            let number = || {
                value
                    .parse::<i64>()
                    .map_err(|_| Error::Other(format!("{flag}: want an integer, got {value:?}")))
            };
            match flag.as_str() {
                "--model" => cfg.model = value.clone(),
                "--ctx" => cfg.ctx = number()?,
                "--idle-min" => cfg.idle_min = number()?,
                "--boot-id" => cfg.boot_id = value.clone(),
                "--port" => port = number()?,
                "--api-port" => api_port = number()?,
                "--config" => cfg.config_path = Some(value.into()),
                _ => unreachable!(),
            }
        }
        validate_values(&cfg.model, cfg.ctx, cfg.idle_min, port, api_port)?;
        cfg.port = port as u16;
        cfg.api_port = api_port as u16;
        Ok(cfg)
    }
    pub fn validate(&self) -> Result<()> {
        validate_values(
            &self.model,
            self.ctx,
            self.idle_min,
            self.port.into(),
            self.api_port.into(),
        )
    }
    pub fn to_args(&self) -> Vec<String> {
        let mut args = Vec::new();
        if let Some(path) = &self.config_path {
            args.extend(["--config".into(), path.to_string_lossy().into()]);
        }
        for (flag, value) in [
            ("--model", self.model.clone()),
            ("--ctx", self.ctx.to_string()),
            ("--idle-min", self.idle_min.to_string()),
            ("--boot-id", self.boot_id.clone()),
            ("--port", self.port.to_string()),
            ("--api-port", self.api_port.to_string()),
        ] {
            args.extend([flag.into(), value]);
        }
        args
    }
}
fn validate_values(model: &str, ctx: i64, idle: i64, port: i64, api_port: i64) -> Result<()> {
    catalog::get(model).map_err(|e| Error::Other(e.to_string()))?;
    let error = if ctx <= 0 {
        Some(format!("--ctx: want > 0, got {ctx}"))
    } else if idle <= 0 {
        Some(format!("--idle-min: want > 0, got {idle}"))
    } else if (idle as u64).checked_mul(60).is_none() {
        Some("--idle-min: duration overflow".into())
    } else if !(1..=65535).contains(&port) {
        Some(format!("--port: want 1-65535, got {port}"))
    } else if !(1..=65535).contains(&api_port) || api_port == port {
        Some(format!(
            "--api-port: want 1-65535 and not --port, got {api_port}"
        ))
    } else {
        None
    };
    error.map_or(Ok(()), |e| Err(Error::Other(e)))
}

pub async fn supervise(cfg: RunConfig, cancel: CancellationToken) -> Result<()> {
    cfg.validate()?;
    let laptop =
        config::load_laptop(&cfg.config_path.clone().unwrap_or_else(config::default_path))?;
    let stop = cancel.child_token();
    let _guard = stop.clone().drop_guard();
    let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    let mut interrupt = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())?;
    let signal_stop = stop.clone();
    let signals = tokio::spawn(async move {
        tokio::select! { _=signal_stop.cancelled()=>{}, _=term.recv()=>signal_stop.cancel(), _=interrupt.recv()=>signal_stop.cancel() }
    });
    let logs = Arc::new(LogRing::new(5000));
    let result = async {
        let weights = laptop.weights();
        let llama_server = ensure_runtime(&weights, stop.clone(), &|note| {
            let line = format!("runtime: {note}\n");
            logs.write(line.as_bytes());
            print!("{line}");
        })
        .await?;
        let (deps, mac) = new_deps(
            MacConfig {
                weights,
                llama_server,
                api_key: laptop.lobo_api_key.clone(),
                port: cfg.port,
                ctx: cfg.ctx,
                model: catalog::get(&cfg.model).unwrap().clone(),
                base_env: std::env::vars().collect(),
            },
            logs.clone(),
            stop.clone(),
        );
        supervise_with(
            cfg,
            laptop,
            StateFile::at(StateFile::default_path()),
            deps,
            mac,
            logs,
            stop.clone(),
        )
        .await
    }
    .await;
    stop.cancel();
    signals
        .await
        .map_err(|e| Error::Local(format!("signal worker: {e}")))?;
    match result {
        Err(Error::Cancelled) => Ok(()),
        other => other,
    }
}

// This scope owns the Runner and server futures. No detached worker can start a
// child after shutdown finishes. Its subscriber is local to future polling.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn supervise_with(
    cfg: RunConfig,
    laptop: Laptop,
    state: StateFile,
    deps: lobo_agent::Deps,
    mac: Arc<MacDeps>,
    logs: LogSource,
    stop: CancellationToken,
) -> Result<()> {
    cfg.validate()?;
    if stop.is_cancelled() {
        return Ok(());
    }
    let writer_logs = logs.clone();
    let subscriber = tracing_subscriber::fmt()
        .json()
        .with_ansi(false)
        .with_writer(move || LocalWriter(writer_logs.clone()))
        .finish();
    async move {
        let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST,cfg.api_port)).await
            .map_err(|e| Error::Local(format!("agent API: {e}")))?;
        if stop.is_cancelled() { return Ok(()); }
        let now = chrono::Utc::now();
        let pid = std::process::id() as i32;
        let local = LocalState { pid: pid.into(), port: cfg.port.into(), api_port: cfg.api_port.into(),
            model: cfg.model.clone(), weights: laptop.weights().to_string_lossy().into(), started_at: GoTime::from_utc(now), boot_id: cfg.boot_id.clone() };
        // Prepare fallible data before claiming ownership of the state file.
        let version = serde_json::to_vec(&serde_json::json!({"git_sha": cfg.version.git_sha, "version": cfg.version.version}))?;
        state.claim(&local, &|pid,boot| is_supervisor(pid,boot,&command_of))?;
        let runner = Runner::new(deps,RunnerConfig { boot_id: cfg.boot_id.clone(), model: cfg.model.clone(), ctx: cfg.ctx,
            idle: Duration::from_secs(cfg.idle_min as u64 * 60), expires_at: now + chrono::TimeDelta::days(100*365),
            boot_timeout: Duration::from_secs(8*3600), tick: Duration::from_secs(30), fail_grace: Duration::from_secs(120), timings: Default::default() });
        tracing::info!(service="lobo-local",model=%cfg.model,ctx=cfg.ctx,port=cfg.port,api_port=cfg.api_port,weights=%local.weights,"start");
        let app = lobo_agent::api::router(runner.clone(),laptop.lobo_api_key,logs,version.into());
        let server = axum::serve(listener,app).with_graceful_shutdown(stop.clone().cancelled_owned()).into_future();
        tokio::pin!(server);
        let (result, server_finished) = tokio::select! {
            result = runner.run(stop.clone()) => (match result { Ok(()) | Err(lobo_agent::Error::Cancelled) => Ok(()), Err(e)=>Err(e.into()) },false),
            result = &mut server => (result.map_err(Error::from),true),
        };
        stop.cancel();
        let mut errors = Vec::new();
        if let Err(e) = result { errors.push(e); }
        if !mac.wait_llama(Duration::from_secs(10)).await {
            errors.push(Error::Local("llama-server did not exit in time".into()));
        }
        if !server_finished {
            match tokio::time::timeout(Duration::from_secs(5), &mut server).await {
                Ok(Ok(()))=>{}, Ok(Err(e))=>errors.push(e.into()),
                Err(_)=>errors.push(Error::Local("agent API did not stop in time".into())),
            }
        }
        if let Err(e) = state.remove_if(pid,&cfg.boot_id) { errors.push(e); }
        tracing::info!(service="lobo-local",errors=errors.len(),"stopped");
        if errors.is_empty() { Ok(()) } else { Err(Error::Multi(errors)) }
    }.with_subscriber(subscriber).await
}
struct LocalWriter(LogSource);
impl Write for LocalWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.write(bytes);
        std::io::stdout().write_all(bytes)?;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        std::io::stdout().flush()
    }
}
#[cfg(test)]
mod tests;
