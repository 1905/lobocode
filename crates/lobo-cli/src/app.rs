use futures_util::future::BoxFuture;
use lobo_core::{
    clock::{Clock, SystemClock},
    config::{self, Laptop},
    control::{self, Deps, Wiring},
    local::{self, RunConfig, Spawner},
};
use std::{
    io::{IsTerminal, Read, Write},
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio_util::sync::CancellationToken;

#[derive(Clone, Copy, Default)]
pub struct Term {
    pub stdout_tty: bool,
    pub stdin_tty: bool,
    pub stderr_tty: bool,
    pub no_color: bool,
}
pub struct Io {
    pub out: Box<dyn Write + Send>,
    pub err: Box<dyn Write + Send>,
    pub input: Box<dyn Read + Send>,
}
impl Io {
    pub fn real() -> Self {
        Self {
            out: Box::new(std::io::stdout()),
            err: Box::new(std::io::stderr()),
            input: Box::new(std::io::stdin()),
        }
    }
}
pub type DepsFactory = Arc<dyn Fn(Laptop, &Wiring) -> lobo_core::Result<Deps> + Send + Sync>;
pub type Supervisor = Arc<
    dyn Fn(RunConfig, CancellationToken) -> BoxFuture<'static, lobo_core::Result<()>> + Send + Sync,
>;
pub struct App {
    pub deps: DepsFactory,
    pub local_supported: fn() -> lobo_core::Result<()>,
    pub supervise: Supervisor,
    pub prompter: Arc<dyn Fn() -> Box<dyn crate::wizard::Prompter> + Send + Sync>,
    pub clock: Arc<dyn Clock>,
    pub term: Term,
    pub exe: PathBuf,
    /// Main owns signal handling; tests can cancel without changing process-global handlers.
    pub cancel: CancellationToken,
}
impl App {
    pub fn real() -> Self {
        Self {
            deps: Arc::new(control::deps_from_config),
            local_supported: local::supported,
            supervise: Arc::new(|cfg, cancel| Box::pin(local::supervise(cfg, cancel))),
            prompter: Arc::new(|| Box::new(crate::wizard::InquirePrompter)),
            clock: Arc::new(SystemClock),
            exe: std::env::current_exe().unwrap_or_else(|_| PathBuf::from("lobo")),
            cancel: CancellationToken::new(),
            term: Term {
                stdout_tty: std::io::stdout().is_terminal(),
                stdin_tty: std::io::stdin().is_terminal(),
                stderr_tty: std::io::stderr().is_terminal(),
                no_color: std::env::var_os("NO_COLOR").is_some_and(|s| !s.is_empty()),
            },
        }
    }
    pub fn wiring(&self, path: &Path) -> Wiring {
        Wiring {
            config_path: path.into(),
            spawner: Spawner::cli(self.exe.clone()),
            supported: self.local_supported,
        }
    }
    pub fn deps(&self, cfg: Laptop, path: &Path) -> lobo_core::Result<Deps> {
        (self.deps)(cfg, &self.wiring(path))
    }
}
pub fn load_cfg(path: &Path) -> anyhow::Result<Laptop> {
    if let Err(e) = std::fs::metadata(path)
        && e.kind() == std::io::ErrorKind::NotFound
    {
        anyhow::bail!("no config at {}. Run `lobo config` first", path.display());
    }
    if config::loose_mode(path) {
        tracing::warn!(
            "{} holds API keys and other users can read it: chmod 600 {}",
            path.display(),
            path.display()
        );
    }
    Ok(config::load_laptop(path)?)
}
