//! Private, persistent cloud forwarding. No inference runs in this process.
use crate::{
    Error, Result, config,
    provider::{CreateOpts, Instance},
};
use base64::Engine;
use nix::{
    errno::Errno,
    fcntl::{Flock, FlockArg},
};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    net::{Ipv4Addr, TcpListener},
    os::unix::fs::{DirBuilderExt, OpenOptionsExt},
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};
use tokio_util::sync::CancellationToken;

pub const HELPER_ARG: &str = "--lobo-cloud-connect";
pub const REMOTE_PORT: u16 = 2222;

#[async_trait::async_trait]
pub trait AddressSource: Send + Sync {
    async fn get(&self, provider: &str, id: &str) -> Result<Instance>;
}
struct ConfigSource(PathBuf);
#[async_trait::async_trait]
impl AddressSource for ConfigSource {
    async fn get(&self, provider: &str, id: &str) -> Result<Instance> {
        let cfg = config::load_laptop(&self.0)?;
        let mut wiring = crate::control::Wiring::new(
            self.0.clone(),
            crate::local::Spawner::cli(std::env::current_exe()?),
        );
        wiring.supported = || Err(Error::Other("cloud helper".into()));
        let providers = crate::control::providers_from_config(&cfg, &wiring);
        let p = providers
            .get(provider)
            .ok_or_else(|| Error::Config("cloud provider key is missing".into()))?;
        p.get(id).await
    }
}

#[derive(Clone)]
pub struct Manager {
    pub root: PathBuf,
    pub config_path: PathBuf,
    pub exe: PathBuf,
    pub port: u16,
}
#[derive(Clone, Serialize, Deserialize, PartialEq)]
struct Desired {
    provider: String,
    id: String,
    boot_id: String,
    config_path: PathBuf,
    port: u16,
    expires_at: chrono::DateTime<chrono::Utc>,
}
#[derive(Default, Serialize, Deserialize)]
struct Health {
    boot_id: String,
    error: String,
    fatal: bool,
}

fn private_dir(path: &Path) -> Result<()> {
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(path)?;
    Ok(())
}
fn atomic(path: &Path, value: &impl Serialize) -> Result<()> {
    let mut file = tempfile::NamedTempFile::new_in(path.parent().unwrap())?;
    file.write_all(&serde_json::to_vec(value)?)?;
    file.as_file().sync_all()?;
    file.persist(path).map_err(|e| Error::Io(e.error))?;
    Ok(())
}
fn load<T: serde::de::DeserializeOwned>(path: &Path) -> Result<Option<T>> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(serde_json::from_slice(&bytes)?)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}
fn lock(path: &Path) -> Result<Option<Flock<File>>> {
    let f = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(path)?;
    match Flock::lock(f, FlockArg::LockExclusiveNonblock) {
        Ok(lock) => Ok(Some(lock)),
        Err((_, Errno::EWOULDBLOCK)) => Ok(None),
        Err((_, e)) => Err(Error::Other(format!("cloud connection lock: {e}"))),
    }
}
async fn mutation_lock(root: &Path) -> Result<Flock<File>> {
    private_dir(root)?;
    for _ in 0..200 {
        if let Some(l) = lock(&root.join("change.lock"))? {
            return Ok(l);
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    Err(Error::Other("cloud connection is busy; try again".into()))
}
fn valid_boot(boot: &str) -> Result<()> {
    if boot.len() != 16 || !boot.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(Error::Other("invalid cloud connection boot ID".into()));
    }
    Ok(())
}
impl Manager {
    pub fn new(config_path: PathBuf, exe: PathBuf, port: u16) -> Result<Self> {
        let config_path = std::path::absolute(config_path)?;
        let root = config_path.with_extension("cloud");
        Ok(Self {
            root,
            config_path,
            exe,
            port,
        })
    }
    pub fn preflight(&self) -> Result<()> {
        if self.root.join("ssh.sock").as_os_str().len() >= 100 {
            return Err(Error::Config(
                "cloud configuration path is too long for an SSH control socket".into(),
            ));
        }
        for binary in ["ssh", "ssh-keygen"] {
            if std::process::Command::new(binary)
                .arg(if binary == "ssh" { "-V" } else { "-?" })
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .is_err()
            {
                return Err(Error::Config(format!(
                    "cloud connection needs OpenSSH ({binary})"
                )));
            }
        }
        for port in [
            self.port,
            self.port
                .checked_add(1)
                .ok_or_else(|| Error::Config("invalid cloud port".into()))?,
        ] {
            TcpListener::bind((Ipv4Addr::LOCALHOST, port)).map_err(|e| {
                Error::Config(format!(
                    "cloud port {port} is occupied; change LOBO_CLOUD_PORT: {e}"
                ))
            })?;
        }
        Ok(())
    }
    pub async fn prepare(&self, o: &mut CreateOpts) -> Result<()> {
        valid_boot(&o.boot_id)?;
        let dir = self.root.join(&o.boot_id);
        private_dir(&dir)?;
        for name in ["client", "host"] {
            let output = tokio::process::Command::new("ssh-keygen")
                .args(["-q", "-t", "ed25519", "-N", "", "-C", "lobo-cloud", "-f"])
                .arg(dir.join(name))
                .stdin(Stdio::null())
                .kill_on_drop(true)
                .output()
                .await?;
            if !output.status.success() {
                return Err(Error::Other("could not create cloud SSH keys".into()));
            }
        }
        o.connection = "ssh".into();
        o.connection_public_key = fs::read_to_string(dir.join("client.pub"))?.trim().into();
        o.connection_host_key =
            base64::engine::general_purpose::STANDARD.encode(fs::read(dir.join("host"))?);
        let known = format!(
            "lobo-{} {}",
            o.boot_id,
            fs::read_to_string(dir.join("host.pub"))?
        );
        let mut f = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(dir.join("known_hosts"))?;
        f.write_all(known.as_bytes())?;
        // The provider receives the host private key. The laptop only needs its pin.
        fs::remove_file(dir.join("host"))?;
        Ok(())
    }
    pub async fn start(&self, i: &mut Instance, o: &CreateOpts) -> Result<()> {
        let _change = mutation_lock(&self.root).await?;
        let desired = Desired {
            provider: i.provider.clone(),
            id: i.id.clone(),
            boot_id: o.boot_id.clone(),
            config_path: self.config_path.clone(),
            port: self.port,
            expires_at: o.expires_at,
        };
        atomic(&self.root.join("desired.json"), &desired)?;
        self.spawn_if_needed(&desired)?;
        endpoint(i, self.port);
        Ok(())
    }
    pub async fn attach(&self, i: &mut Instance) -> Result<()> {
        if i.provider == "local" {
            return Ok(());
        }
        let _change = mutation_lock(&self.root).await?;
        let Some(s) = load::<Desired>(&self.root.join("desired.json"))? else {
            // An already running legacy instance can still use its saved domain.
            if !i.agent_url.is_empty() {
                return Ok(());
            }
            return Err(Error::Other(
                "cloud connection keys are missing; stop this instance and start it again".into(),
            ));
        };
        if s.provider != i.provider || s.id != i.id {
            return Err(Error::Other(
                "cloud connection belongs to another instance; run `lobo down` first".into(),
            ));
        }
        if let Some(h) = load::<Health>(&self.root.join("health.json"))?
            && h.boot_id == s.boot_id
            && h.fatal
        {
            return Err(Error::Other(h.error));
        }
        self.spawn_if_needed(&s)?;
        endpoint(i, s.port);
        Ok(())
    }
    fn spawn_if_needed(&self, s: &Desired) -> Result<()> {
        let Some(guard) = lock(&self.root.join("helper.lock"))? else {
            return Ok(());
        };
        drop(guard);
        // Duplicate starters are harmless: the helper takes the same lock before
        // touching any SSH process. It reads desired state only after that lock.
        let mut command = std::process::Command::new(&self.exe);
        command
            .arg(HELPER_ARG)
            .arg(&self.root)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        use std::os::unix::process::CommandExt;
        // SAFETY: setsid is async-signal-safe and performs no allocation.
        unsafe {
            command.pre_exec(|| {
                if libc::setsid() < 0 {
                    Err(std::io::Error::last_os_error())
                } else {
                    Ok(())
                }
            });
        }
        atomic(
            &self.root.join("health.json"),
            &Health {
                boot_id: s.boot_id.clone(),
                ..Default::default()
            },
        )?;
        let mut child = command.spawn()?;
        std::thread::spawn(move || {
            let _ = child.wait();
        });
        Ok(())
    }
    pub async fn stop(&self, provider: &str, id: &str) -> Result<()> {
        let _change = mutation_lock(&self.root).await?;
        let Some(s) = load::<Desired>(&self.root.join("desired.json"))? else {
            return Ok(());
        };
        if s.provider != provider || s.id != id {
            return Ok(());
        }
        fs::remove_file(self.root.join("desired.json"))?;
        // A private control socket also handles an SSH child orphaned by SIGKILL.
        close_master(&self.root).await;
        for _ in 0..100 {
            if lock(&self.root.join("helper.lock"))?.is_some() {
                valid_boot(&s.boot_id)?;
                let dir = self.root.join(&s.boot_id);
                if dir.exists() {
                    fs::remove_dir_all(dir)?;
                }
                return Ok(());
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        Err(Error::Other(
            "cloud connection did not stop within 5 seconds".into(),
        ))
    }
    pub async fn stop_saved(&self) -> Result<()> {
        if let Some(s) = load::<Desired>(&self.root.join("desired.json"))? {
            self.stop(&s.provider, &s.id).await?;
        }
        Ok(())
    }
    pub fn detail(&self) -> Option<String> {
        load::<Health>(&self.root.join("health.json"))
            .ok()
            .flatten()
            .map(|h| h.error)
            .filter(|s| !s.is_empty())
    }
    pub fn discard_keys(&self, boot: &str) -> Result<()> {
        valid_boot(boot)?;
        let dir = self.root.join(boot);
        if dir.exists() {
            fs::remove_dir_all(dir)?;
        }
        Ok(())
    }
}
fn endpoint(i: &mut Instance, port: u16) {
    i.api_url = format!("http://127.0.0.1:{port}/v1");
    i.agent_url = format!("http://127.0.0.1:{}", port + 1);
}
async fn close_master(root: &Path) {
    let _ = tokio::time::timeout(
        Duration::from_secs(2),
        tokio::process::Command::new("ssh")
            .args(["-F", "/dev/null", "-S"])
            .arg(root.join("ssh.sock"))
            .args(["-O", "exit", "lobo-owned"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .status(),
    )
    .await;
}
fn ssh_command(root: &Path, s: &Desired, i: &Instance) -> Result<tokio::process::Command> {
    // A provider response is data, never SSH options or shell text.
    if i.ssh_port == 0
        || i.ssh_host.is_empty()
        || !i
            .ssh_host
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b".-:".contains(&b))
        || i.ssh_host.starts_with('-')
    {
        return Err(Error::Other(
            "waiting for the provider's SSH address and TCP port".into(),
        ));
    }
    let mut c = tokio::process::Command::new("ssh");
    c.args(["-F", "/dev/null", "-N", "-T", "-M", "-S"])
        .arg(root.join("ssh.sock"))
        .args([
            "-o",
            "BatchMode=yes",
            "-o",
            "IdentitiesOnly=yes",
            "-o",
            "IdentityAgent=none",
            "-o",
            "StrictHostKeyChecking=yes",
            "-o",
            "GlobalKnownHostsFile=/dev/null",
            "-o",
            "ExitOnForwardFailure=yes",
            "-o",
            "ConnectTimeout=10",
            "-o",
            "ConnectionAttempts=1",
            "-o",
            "ServerAliveInterval=10",
            "-o",
            "ServerAliveCountMax=3",
            "-o",
            "ControlPersist=no",
        ])
        .arg("-o")
        .arg(format!("HostKeyAlias=lobo-{}", s.boot_id))
        .arg("-o")
        .arg(format!(
            "UserKnownHostsFile=\"{}\"",
            root.join(&s.boot_id)
                .join("known_hosts")
                .to_string_lossy()
                .replace('\\', "\\\\")
                .replace('"', "\\\"")
        ))
        .arg("-i")
        .arg(root.join(&s.boot_id).join("client"))
        .arg("-L")
        .arg(format!("127.0.0.1:{}:127.0.0.1:8080", s.port))
        .arg("-L")
        .arg(format!("127.0.0.1:{}:127.0.0.1:8081", s.port + 1))
        .arg("-p")
        .arg(i.ssh_port.to_string())
        .arg("-l")
        .arg("root")
        .arg(&i.ssh_host)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .kill_on_drop(true);
    Ok(c)
}
fn wanted(root: &Path, s: &Desired) -> bool {
    chrono::Utc::now() < s.expires_at
        && load::<Desired>(&root.join("desired.json"))
            .ok()
            .flatten()
            .as_ref()
            == Some(s)
}
fn health(root: &Path, s: &Desired, error: impl Into<String>, fatal: bool) -> Result<()> {
    atomic(
        &root.join("health.json"),
        &Health {
            boot_id: s.boot_id.clone(),
            error: error.into(),
            fatal,
        },
    )
}
pub async fn run(root: PathBuf) -> Result<()> {
    let Some(s) = load::<Desired>(&root.join("desired.json"))? else {
        return Ok(());
    };
    run_with_source(root, &ConfigSource(s.config_path)).await
}
/// The address source is injectable for tests with real SSH and no rented GPU.
pub async fn run_with_source(root: PathBuf, source: &dyn AddressSource) -> Result<()> {
    let Some(_owned) = lock(&root.join("helper.lock"))? else {
        return Ok(());
    };
    let Some(s) = load::<Desired>(&root.join("desired.json"))? else {
        return Ok(());
    };
    valid_boot(&s.boot_id)?;
    atomic(
        &root.join("helper.json"),
        &serde_json::json!({"pid":std::process::id(), "boot_id":s.boot_id}),
    )?;
    if !(1024..=65534).contains(&s.port) {
        return Err(Error::Other("invalid cloud port".into()));
    }
    close_master(&root).await;
    let cancel = CancellationToken::new();
    let watch = cancel.clone();
    let watch_root = root.clone();
    let watch_state = s.clone();
    let monitor = tokio::spawn(async move {
        while wanted(&watch_root, &watch_state) {
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
        watch.cancel();
    });
    let result = tokio::select! {biased;
        _ = cancel.cancelled() => Ok(()),
        r = reconnect(&root, &s, &cancel, source) => r,
    };
    close_master(&root).await;
    monitor.abort();
    let _ = monitor.await;
    result
}
async fn reconnect(
    root: &Path,
    s: &Desired,
    cancel: &CancellationToken,
    source: &dyn AddressSource,
) -> Result<()> {
    loop {
        if cancel.is_cancelled() {
            return Ok(());
        }
        let result = connection_attempt(root, s, cancel, source).await;
        match result {
            Err(Error::NotFound) => {
                health(root, s, "cloud instance is gone", true)?;
                return Ok(());
            }
            Err(e) => {
                let msg = e.to_string();
                let fatal = [
                    "REMOTE HOST IDENTIFICATION",
                    "Host key verification failed",
                    "Permission denied (publickey)",
                    "Address already in use",
                    "cannot listen to port",
                    "Could not request local forwarding",
                ]
                .iter()
                .any(|v| msg.contains(v));
                health(root, s, msg, fatal)?;
                if fatal {
                    // Retain the error until Stop. Do not silently retry a changed key.
                    cancel.cancelled().await;
                    return Ok(());
                }
            }
            Ok(()) => {}
        }
        tokio::time::sleep(Duration::from_secs(3)).await;
    }
}
async fn connection_attempt(
    root: &Path,
    s: &Desired,
    cancel: &CancellationToken,
    source: &dyn AddressSource,
) -> Result<()> {
    let i = source.get(&s.provider, &s.id).await?;
    let log_path = root.join("ssh-error.log");
    let log = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&log_path)?;
    let mut child = ssh_command(root, s, &i)?.stderr(log).spawn()?;
    health(root, s, "connecting to cloud", false)?;
    let result = async {
        loop {
            tokio::select! {biased;
                _ = cancel.cancelled() => return Ok(()),
                status = child.wait() => {
                    let mut detail = String::new();
                    File::open(&log_path)?.take(4096).read_to_string(&mut detail)?;
                    return Err(Error::Other(format!("cloud SSH exited ({}): {}", status?, detail.trim())));
                }
                _ = tokio::time::sleep(Duration::from_secs(30)) => {
                    let current = source.get(&s.provider, &s.id).await?;
                    if current.ssh_host != i.ssh_host || current.ssh_port != i.ssh_port { return Ok(()); }
                }
            }
        }
    }.await;
    let _ = child.kill().await;
    let _ = child.wait().await;
    result
}

/// Shared early entry point; the app must call this before opening any windows.
pub async fn entry(args: &[String]) -> i32 {
    let result = if args.len() == 1 {
        run(PathBuf::from(&args[0])).await
    } else {
        Err(Error::Other(
            "cloud helper expects its state directory".into(),
        ))
    };
    match result {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("cloud connection: {e}");
            1
        }
    }
}
