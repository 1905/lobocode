use super::state::StateFile;
use crate::{Error, Result};
use lobo_proto::{Instance, LocalState};
use std::{path::PathBuf, process::Command};

#[derive(Debug, Clone)]
pub struct Spawner {
    pub exe: PathBuf,
    pub args_prefix: Vec<String>,
}
impl Spawner {
    pub fn cli(exe: PathBuf) -> Self {
        Self {
            exe,
            args_prefix: vec!["local".into(), "run".into()],
        }
    }
    pub fn app(exe: PathBuf) -> Self {
        Self {
            exe,
            args_prefix: vec![
                super::supervise::SUPERVISOR_ARG.into(),
                "local".into(),
                "run".into(),
            ],
        }
    }
}

pub fn command_of(pid: i32) -> Result<String> {
    let out = Command::new("ps")
        .args(["-ww", "-o", "command=", "-p", &pid.to_string()])
        .output()?;
    if !out.status.success() {
        return Err(Error::Local(format!("ps {pid}: {}", out.status)));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
}
/// Opaque process-start identity. macOS uses kernel start microseconds; Linux
/// uses boot-relative clock ticks. Compare only IDs collected on this host.
/// macOS API: https://github.com/apple-oss-distributions/xnu/blob/main/bsd/sys/proc_info.h
pub fn process_start_id(pid: i32) -> Result<u64> {
    if pid <= 0 {
        return Err(Error::Local("invalid process pid".into()));
    }
    #[cfg(target_os = "macos")]
    {
        // SAFETY: proc_pidinfo writes at most the supplied struct size.
        let mut info: libc::proc_bsdinfo = unsafe { std::mem::zeroed() };
        let size = std::mem::size_of_val(&info) as i32;
        let n = unsafe {
            libc::proc_pidinfo(
                pid,
                libc::PROC_PIDTBSDINFO,
                0,
                (&mut info as *mut libc::proc_bsdinfo).cast(),
                size,
            )
        };
        if n != size {
            let e = std::io::Error::last_os_error();
            return if e.raw_os_error() == Some(libc::ESRCH) {
                Err(Error::NotFound)
            } else {
                Err(e.into())
            };
        }
        info.pbi_start_tvsec
            .checked_mul(1_000_000)
            .and_then(|s| s.checked_add(info.pbi_start_tvusec))
            .ok_or_else(|| Error::Local("process start identity overflow".into()))
    }
    #[cfg(target_os = "linux")]
    {
        let text = std::fs::read_to_string(format!("/proc/{pid}/stat")).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                Error::NotFound
            } else {
                e.into()
            }
        })?;
        // comm may contain spaces or parentheses. Field 22 starts after its final ).
        text.rsplit_once(')')
            .and_then(|(_, fields)| fields.split_whitespace().nth(19))
            .and_then(|field| field.parse::<u64>().ok())
            .ok_or_else(|| Error::Local("invalid process start identity".into()))
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    Err(Error::Local(
        "process identity is unsupported on this host".into(),
    ))
}
pub fn is_supervisor(pid: i32, boot_id: &str, ps: &dyn Fn(i32) -> Result<String>) -> bool {
    if pid <= 0 || boot_id.is_empty() {
        return false;
    }
    let Ok(command) = ps(pid) else { return false };
    let fields: Vec<_> = command.split_whitespace().collect();
    fields.windows(2).any(|f| f == ["local", "run"])
        && fields.windows(2).any(|f| f == ["--boot-id", boot_id])
}
pub fn instance(s: &LocalState) -> Instance {
    Instance {
        provider: "local".into(),
        id: s.pid.to_string(),
        status: "running".into(),
        started_at: s.started_at.clone(),
        detail: format!("this Mac, {}", s.model),
        api_url: format!("http://127.0.0.1:{}/v1", s.port),
        agent_url: format!("http://127.0.0.1:{}", s.api_port),
        ..Default::default()
    }
}
pub fn log_path(state: &StateFile) -> PathBuf {
    state.path.with_file_name("local.log")
}

#[async_trait::async_trait]
pub trait EnsureRuntime: Send + Sync {
    async fn ensure(
        &self,
        weights: &Path,
        cancel: CancellationToken,
        note: &(dyn Fn(String) + Sync),
    ) -> Result<PathBuf>;
}
struct PinnedRuntime;
#[async_trait::async_trait]
impl EnsureRuntime for PinnedRuntime {
    async fn ensure(
        &self,
        weights: &Path,
        cancel: CancellationToken,
        note: &(dyn Fn(String) + Sync),
    ) -> Result<PathBuf> {
        super::runtime::ensure_runtime(weights, cancel, note).await
    }
}
#[derive(Clone)]
pub struct LocalHooks {
    pub supported: fn() -> Result<()>,
    pub memory: super::memory::MemoryProbe,
    pub ensure_runtime: Arc<dyn EnsureRuntime>,
    pub free_bytes: fn(&Path) -> Result<u64>,
    pub ps: Arc<dyn Fn(i32) -> Result<String> + Send + Sync>,
    pub start_id: Arc<dyn Fn(i32) -> Result<u64> + Send + Sync>,
    pub signal: Arc<dyn Fn(i32, Signal) -> Result<()> + Send + Sync>,
    pub state_wait: Duration,
    pub stop_wait: Duration,
    // Explicit child-only environment overrides keep tests off global env state.
    pub child_env: Vec<(String, String)>,
}
impl Default for LocalHooks {
    fn default() -> Self {
        Self {
            supported: super::platform::supported,
            memory: Arc::new(super::memory::snapshot),
            ensure_runtime: Arc::new(PinnedRuntime),
            free_bytes: super::models::free_space,
            ps: Arc::new(command_of),
            start_id: Arc::new(process_start_id),
            signal: Arc::new(signal),
            state_wait: Duration::from_secs(15),
            stop_wait: Duration::from_secs(10),
            child_env: vec![],
        }
    }
}
#[derive(Clone)]
pub struct LocalProvider {
    pub spawner: Spawner,
    pub config_path: Option<PathBuf>,
    pub weights: PathBuf,
    pub port: u16,
    pub state: StateFile,
    pub hooks: LocalHooks,
}
use super::state::alive;
use super::supervise::RunConfig;
use crate::provider::{CreateOpts, Provider};
use nix::{
    errno::Errno,
    sys::signal::{Signal, kill},
    unistd::Pid,
};
use std::{
    fs::{self, OpenOptions},
    io::{BufRead, Seek, SeekFrom},
    os::unix::fs::{DirBuilderExt, OpenOptionsExt},
    path::Path,
    process::Stdio,
    sync::Arc,
    time::Duration,
};
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

fn cancelled(cancel: &CancellationToken) -> Result<()> {
    if cancel.is_cancelled() {
        Err(Error::Cancelled)
    } else {
        Ok(())
    }
}
fn signal(pid: i32, signal: Signal) -> Result<()> {
    match kill(Pid::from_raw(pid), signal) {
        Ok(()) | Err(Errno::ESRCH) => Ok(()),
        Err(e) => Err(Error::Local(format!("signal {pid}: {e}"))),
    }
}
pub async fn wait_group_gone(pid: i32, timeout: Duration) -> bool {
    let end = Instant::now() + timeout;
    loop {
        if kill(Pid::from_raw(-pid), None) == Err(Errno::ESRCH) {
            return true;
        }
        if Instant::now() >= end {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}
struct StartingGroup(i32);
impl Drop for StartingGroup {
    fn drop(&mut self) {
        if self.0 > 0 {
            let _ = signal(-self.0, Signal::SIGKILL);
        }
    }
}
impl LocalProvider {
    fn state_for(&self, id: &str) -> Result<LocalState> {
        self.state
            .read()?
            .filter(|s| s.pid.to_string() == id)
            .ok_or(Error::NotFound)
    }
    fn owned_state(
        &self,
        id: &str,
        boot_id: &str,
        start_id: Option<u64>,
    ) -> Result<Option<LocalState>> {
        if boot_id.is_empty() {
            return Err(Error::Local("local boot identity is missing".into()));
        }
        let s = match self.state_for(id) {
            Ok(s) => s,
            Err(Error::NotFound) => return Ok(None),
            Err(e) => return Err(e),
        };
        if s.boot_id != boot_id {
            return Ok(None);
        }
        let pid = i32::try_from(s.pid).map_err(|_| Error::Local("invalid local pid".into()))?;
        if !is_supervisor(pid, boot_id, &*self.hooks.ps) {
            return Ok(None);
        }
        let current = match (self.hooks.start_id)(pid) {
            Ok(value) => value,
            Err(Error::NotFound) => return Ok(None),
            Err(e) => return Err(e),
        };
        if start_id.is_some_and(|start| start != current) {
            return Ok(None);
        }
        Ok(Some(s))
    }
    async fn spawn(&self, opts: &CreateOpts, cancel: CancellationToken) -> Result<Instance> {
        let log_path = log_path(&self.state);
        let parent = log_path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(parent)?;
        let mut log = OpenOptions::new()
            .append(true)
            .create(true)
            .mode(0o600)
            .open(&log_path)?;
        let offset = log.seek(SeekFrom::End(0))?;
        let cfg = RunConfig {
            model: opts.model.clone(),
            ctx: opts.ctx,
            idle_min: opts.idle_min,
            boot_id: opts.boot_id.clone(),
            port: self.port,
            api_port: self
                .port
                .checked_add(1)
                .ok_or_else(|| Error::Local("local port has no adjacent API port".into()))?,
            config_path: self.config_path.clone(),
            version: Default::default(),
        };
        cfg.validate()?;
        let mut cmd = tokio::process::Command::new(&self.spawner.exe);
        cmd.args(&self.spawner.args_prefix)
            .args(cfg.to_args())
            .envs(self.hooks.child_env.iter().cloned())
            .stdin(Stdio::null())
            .stdout(log.try_clone()?)
            .stderr(log);
        // SAFETY: setsid is async-signal-safe; this closure does no allocation.
        unsafe {
            cmd.pre_exec(|| {
                if libc::setsid() == -1 {
                    Err(std::io::Error::last_os_error())
                } else {
                    Ok(())
                }
            });
        }
        cancelled(&cancel)?;
        let mut child = cmd.spawn()?;
        let pid = child
            .id()
            .ok_or_else(|| Error::Local("supervisor has no pid".into()))? as i32;
        let mut owned = StartingGroup(pid);
        let end = Instant::now() + self.hooks.state_wait;
        let cause = loop {
            if cancel.is_cancelled() {
                break Error::Cancelled;
            }
            match self.state.read() {
                Ok(Some(s)) if s.pid == i64::from(pid) && s.boot_id == opts.boot_id => {
                    owned.0 = 0;
                    // The detached supervisor survives the caller's runtime. Reap it
                    // while the runtime remains alive, without kill-on-drop.
                    tokio::spawn(async move {
                        let _ = child.wait().await;
                    });
                    return Ok(instance(&s));
                }
                Err(e) => break e,
                _ => {}
            }
            tokio::select! { biased;
                _=cancel.cancelled()=>break Error::Cancelled,
                status=child.wait()=>break Error::Local(format!("lobo local run exited early ({}); {}:\n{}",match status {Ok(s)=>s.to_string(),Err(e)=>e.to_string()},log_path.display(),log_tail(&log_path,offset,20))),
                _=tokio::time::sleep_until(end)=>break Error::Local(format!("lobo local run wrote no state in {:?}; {}:\n{}",self.hooks.state_wait,log_path.display(),log_tail(&log_path,offset,20))),
                _=tokio::time::sleep(Duration::from_millis(25))=>{},
            }
        };
        // This group is ours even if no state file has been published yet.
        // Cancellation must await both the leader and the remaining children.
        let mut errors = vec![cause];
        if let Err(e) = signal(-pid, Signal::SIGKILL) {
            errors.push(e);
        }
        if let Err(e) = child.wait().await {
            errors.push(e.into());
        }
        if wait_group_gone(pid, Duration::from_secs(2)).await {
            owned.0 = 0;
            if let Err(e) = self.state.remove_if(pid, &opts.boot_id) {
                errors.push(e);
            }
        } else {
            errors.push(Error::Local(format!("local group {pid} survived SIGKILL")));
        }
        if errors.len() == 1 {
            Err(errors.pop().unwrap())
        } else {
            Err(Error::Multi(errors))
        }
    }
}
#[async_trait::async_trait]
impl Provider for LocalProvider {
    fn name(&self) -> &'static str {
        "local"
    }
    fn replaceable(&self) -> bool {
        false
    }
    async fn rent(
        &self,
        opts: &CreateOpts,
        cancel: CancellationToken,
        note: &(dyn Fn(String) + Sync),
    ) -> Result<Instance> {
        cancelled(&cancel)?;
        (self.hooks.supported)()?;
        let model =
            lobo_proto::catalog::get(&opts.model).map_err(|e| Error::Local(e.to_string()))?;
        if let Some(s) = self.state.read()? {
            return Err(Error::AlreadyRunning(format!(
                "lobo already running: local pid {}",
                s.pid
            )));
        }
        super::memory::inspect_with(&opts.model, opts.ctx, &self.hooks.memory)?.ensure_fit()?;
        fs::create_dir_all(&self.weights)
            .and_then(|()| {
                tempfile::Builder::new()
                    .prefix(".lobo-write-")
                    .tempfile_in(&self.weights)
                    .map(drop)
            })
            .map_err(|e| {
                Error::Local(format!(
                    "weights folder {} is not writable: {e}",
                    self.weights.display()
                ))
            })?;
        let free = (self.hooks.free_bytes)(&self.weights)?;
        let on_disk = fs::metadata(self.weights.join(&model.file))
            .map(|m| m.len())
            .unwrap_or(0);
        let need = (model.size as u64).saturating_sub(on_disk);
        if need > free {
            return Err(Error::Local(format!(
                "not enough space in {} for {}: need {need} bytes, {free} free",
                self.weights.display(),
                model.id
            )));
        }
        let api_port = self
            .port
            .checked_add(1)
            .filter(|_| self.port > 0)
            .ok_or_else(|| Error::Local("invalid local port".into()))?;
        for (port, why) in [
            (self.port, "LOBO_LOCAL_PORT"),
            (api_port, "agent API = LOBO_LOCAL_PORT+1"),
        ] {
            std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port))
                .map_err(|e| Error::Local(format!("port {port} in use ({why}): {e}")))?;
        }
        cancelled(&cancel)?;
        self.hooks
            .ensure_runtime
            .ensure(&self.weights, cancel.clone(), note)
            .await?;
        cancelled(&cancel)?;
        self.spawn(opts, cancel).await
    }
    async fn list(&self) -> Result<Vec<Instance>> {
        Ok(self
            .state
            .read()?
            .as_ref()
            .map(instance)
            .into_iter()
            .collect())
    }
    async fn get(&self, id: &str) -> Result<Instance> {
        Ok(instance(&self.state_for(id)?))
    }
    async fn runtime_identity(&self, id: &str) -> Result<Option<(String, i32, u64)>> {
        let s = match self.state_for(id) {
            Ok(s) => s,
            Err(Error::NotFound) => return Ok(None),
            Err(e) => return Err(e),
        };
        let pid = i32::try_from(s.pid).map_err(|_| Error::Local("invalid local pid".into()))?;
        if !is_supervisor(pid, &s.boot_id, &*self.hooks.ps) {
            return Ok(None);
        }
        let start = (self.hooks.start_id)(pid)?;
        Ok(Some((s.boot_id, pid, start)))
    }
    async fn delete_owned(&self, id: &str, boot_id: &str, start_id: Option<u64>) -> Result<()> {
        let Some(s) = self.owned_state(id, boot_id, start_id)? else {
            return Ok(());
        };
        let pid = s.pid as i32;
        // Capture an identity for pending-create recovery too. Recheck at each
        // signal boundary; never kill a replacement supervisor at the same PID.
        let start = match start_id {
            Some(start) => start,
            None => (self.hooks.start_id)(pid)?,
        };
        if self.owned_state(id, boot_id, Some(start))?.is_none() {
            return Ok(());
        }
        (self.hooks.signal)(pid, Signal::SIGTERM)?;
        if !wait_group_gone(pid, self.hooks.stop_wait).await {
            match (self.hooks.start_id)(pid) {
                Ok(current) if current == start => {
                    if self.owned_state(id, boot_id, Some(start))?.is_none() {
                        return Ok(());
                    }
                }
                Ok(_) => return Ok(()),
                // TERM can exit the verified leader while its child ignores it.
                // Its existing group remains ours. A live replacement leader
                // must never be signalled, including one seen after this read.
                Err(Error::NotFound) if !alive(pid) => {}
                Err(Error::NotFound) => return Ok(()),
                Err(e) => return Err(e),
            }
            (self.hooks.signal)(-pid, Signal::SIGKILL)?;
            if !wait_group_gone(pid, Duration::from_secs(2)).await {
                return Err(Error::Local(format!("local group {pid} survived SIGKILL")));
            }
        }
        self.state.remove_if(pid, boot_id)
    }
    async fn delete(&self, id: &str) -> Result<()> {
        let state = match self.state_for(id) {
            Ok(s) => s,
            Err(Error::NotFound) => return Ok(()),
            Err(e) => return Err(e),
        };
        let pid = i32::try_from(state.pid).map_err(|_| Error::Local("invalid local pid".into()))?;
        if !is_supervisor(pid, &state.boot_id, &*self.hooks.ps) {
            return self.state.remove_if(pid, &state.boot_id);
        }
        signal(pid, Signal::SIGTERM)?;
        if !wait_group_gone(pid, self.hooks.stop_wait).await {
            if alive(pid) && !is_supervisor(pid, &state.boot_id, &*self.hooks.ps) {
                return self.state.remove_if(pid, &state.boot_id);
            }
            signal(-pid, Signal::SIGKILL)?;
            if !wait_group_gone(pid, Duration::from_secs(2)).await {
                return Err(Error::Local(format!("local group {pid} survived SIGKILL")));
            }
        }
        self.state.remove_if(pid, &state.boot_id)
    }
}
fn log_tail(path: &Path, offset: u64, n: usize) -> String {
    let read = || -> std::io::Result<String> {
        let mut file = fs::File::open(path)?;
        file.seek(SeekFrom::Start(offset))?;
        let mut lines = std::collections::VecDeque::new();
        for line in std::io::BufReader::new(file).lines() {
            lines.push_back(line?);
            if lines.len() > n {
                lines.pop_front();
            }
        }
        Ok(lines.into_iter().collect::<Vec<_>>().join("\n"))
    };
    read().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn is_supervisor_table() {
        for (command, boot, want) in [
            ("lobo local run --boot-id b1", "b1", true),
            (
                "/Applications/lobocode.app/Contents/MacOS/lobocode --lobo-supervisor local run --boot-id b1",
                "b1",
                true,
            ),
            ("lobo local run --boot-id b11", "b1", false),
            ("lobo local run --boot-id b1x", "b1", false),
            ("lobo local run --boot-id", "b1", false),
            ("lobo local run --boot-id=b1", "b1", false),
            ("lobo local status --boot-id b1", "b1", false),
            ("vim notes.txt", "b1", false),
            ("lobo local run --boot-id b1", "", false),
        ] {
            assert_eq!(
                is_supervisor(1, boot, &|_| Ok(command.into())),
                want,
                "{command}"
            );
        }
        assert!(!is_supervisor(0, "b1", &|_| panic!(
            "invalid pid must not run ps"
        )));
        assert!(!is_supervisor(1, "b1", &|_| Err(Error::NotFound)));
    }
    #[test]
    fn instance_urls_from_state_ports() {
        let i = instance(&LocalState {
            pid: 123,
            port: 8931,
            api_port: 8932,
            model: "q6".into(),
            ..Default::default()
        });
        assert_eq!(i.api_url, "http://127.0.0.1:8931/v1");
        assert_eq!(i.agent_url, "http://127.0.0.1:8932");
        assert_eq!(i.id, "123");
        assert_eq!(i.detail, "this Mac, q6");
    }
}
