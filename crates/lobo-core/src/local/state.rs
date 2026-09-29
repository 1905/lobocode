use super::provider::{command_of, is_supervisor};
use crate::{Error, Result};
use lobo_proto::LocalState;
use nix::{
    errno::Errno,
    fcntl::{Flock, FlockArg},
    sys::signal::kill,
    unistd::Pid,
};
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    os::unix::fs::{DirBuilderExt, OpenOptionsExt},
    path::{Path, PathBuf},
};

#[derive(Debug, Clone)]
pub struct StateFile {
    pub path: PathBuf,
}
impl StateFile {
    pub fn default_path() -> PathBuf {
        state_path()
    }
    pub fn path_from(xdg_state_home: Option<&str>, home: Option<&Path>) -> PathBuf {
        let base = match xdg_state_home.filter(|v| !v.is_empty()) {
            Some(path) => PathBuf::from(path),
            None => home.unwrap_or_else(|| Path::new(".")).join(".local/state"),
        };
        base.join("lobo/local.json")
    }
    pub fn at(path: PathBuf) -> Self {
        Self { path }
    }
    fn parent(&self) -> &Path {
        self.path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."))
    }
    fn raw(&self) -> Result<LocalState> {
        let bytes = fs::read(&self.path)?;
        serde_json::from_slice(&bytes)
            .map_err(|e| Error::Local(format!("read {}: {e}", self.path.display())))
    }
    pub fn read(&self) -> Result<Option<LocalState>> {
        let state = match self.raw() {
            Ok(s) => s,
            Err(Error::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e),
        };
        let pid = state_pid(&state)?;
        if !alive(pid) {
            self.remove_if(pid, &state.boot_id)?;
            return Ok(None);
        }
        Ok(Some(state))
    }
    pub fn claim(
        &self,
        state: &LocalState,
        is_supervisor: &dyn Fn(i32, &str) -> bool,
    ) -> Result<()> {
        state_pid(state)?;
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(self.parent())?;
        let mut temp = tempfile::Builder::new()
            .prefix(".local-")
            .suffix(".json")
            .tempfile_in(self.parent())?;
        temp.write_all(&serde_json::to_vec_pretty(state)?)?;
        temp.write_all(b"\n")?;
        temp.flush()?;
        let _lock = self.lock()?;
        match fs::hard_link(temp.path(), &self.path) {
            Ok(()) => return Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e.into()),
        }
        let old = self.raw()?;
        let pid = state_pid(&old)?;
        if alive(pid) && is_supervisor(pid, &old.boot_id) {
            return Err(Error::AlreadyRunning(format!(
                "local already running (pid {pid})"
            )));
        }
        remove_missing_ok(&self.path)?;
        fs::hard_link(temp.path(), &self.path)?;
        Ok(())
    }
    pub fn remove_if(&self, pid: i32, boot_id: &str) -> Result<()> {
        let _lock = self.lock()?;
        let state = match self.raw() {
            Ok(s) => s,
            Err(Error::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(e),
        };
        if state.pid == i64::from(pid) && state.boot_id == boot_id {
            remove_missing_ok(&self.path)?;
        }
        Ok(())
    }
    pub(crate) fn lock(&self) -> Result<Flock<File>> {
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(self.parent())?;
        let mut lock_path = self.path.as_os_str().to_owned();
        lock_path.push(".lock");
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .open(&lock_path)?;
        loop {
            match Flock::lock(file, FlockArg::LockExclusive) {
                Ok(lock) => return Ok(lock),
                Err((f, Errno::EINTR)) => file = f,
                Err((_, e)) => {
                    return Err(Error::Local(format!(
                        "lock {}: {e}",
                        Path::new(&lock_path).display()
                    )));
                }
            }
        }
    }
}
fn state_pid(s: &LocalState) -> Result<i32> {
    i32::try_from(s.pid).map_err(|_| Error::Local(format!("invalid local pid {}", s.pid)))
}
fn remove_missing_ok(path: &Path) -> Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}
pub fn alive(pid: i32) -> bool {
    pid > 0 && matches!(kill(Pid::from_raw(pid), None), Ok(()) | Err(Errno::EPERM))
}
pub fn state_path() -> PathBuf {
    let xdg = std::env::var("XDG_STATE_HOME").ok();
    let home = std::env::var_os("HOME").map(PathBuf::from);
    StateFile::path_from(xdg.as_deref(), home.as_deref())
}
pub fn read_state() -> Result<Option<LocalState>> {
    StateFile::at(state_path()).read()
}
pub fn claim_state(s: &LocalState) -> Result<()> {
    StateFile::at(state_path()).claim(s, &|pid, boot| is_supervisor(pid, boot, &command_of))
}
pub fn remove_state_if(pid: i32, boot_id: &str) -> Result<()> {
    StateFile::at(state_path()).remove_if(pid, boot_id)
}

#[cfg(test)]
mod tests;
