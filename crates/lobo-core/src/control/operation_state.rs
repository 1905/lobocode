//! Cross-process ownership of an unfinished create or cleanup.
use crate::{Error, Result};
use nix::{
    errno::Errno,
    fcntl::{Flock, FlockArg},
};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    os::unix::fs::{DirBuilderExt, OpenOptionsExt},
    path::{Path, PathBuf},
    time::Duration,
};
use tokio::sync::{Mutex, MutexGuard};
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PendingCreate {
    pub provider: String,
    pub boot_id: String,
    pub before: Vec<String>,
    pub instance_id: Option<String>,
}
impl PendingCreate {
    pub fn unresolved(&self, detail: impl Into<String>) -> Error {
        Error::UnresolvedCreate {
            provider: self.provider.clone(),
            boot_id: self.boot_id.clone(),
            detail: detail.into(),
        }
    }
}
pub struct OperationState {
    path: Option<PathBuf>,
    pending: Mutex<Option<PendingCreate>>,
}
impl OperationState {
    pub fn memory() -> Self {
        Self {
            path: None,
            pending: Mutex::new(None),
        }
    }
    pub fn persistent(path: PathBuf) -> Self {
        Self {
            path: Some(path),
            pending: Mutex::new(None),
        }
    }
    pub async fn acquire(&self, cancel: &CancellationToken) -> Result<OperationGuard<'_>> {
        let mut pending = tokio::select! {biased; _=cancel.cancelled()=>return Err(Error::Cancelled),guard=self.pending.lock()=>guard};
        let lock = if let Some(path) = &self.path {
            fs::DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(parent(path))?;
            let mut lock_path = path.as_os_str().to_owned();
            lock_path.push(".lock");
            let mut file = OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .mode(0o600)
                .open(lock_path)?;
            let locked = loop {
                if cancel.is_cancelled() {
                    return Err(Error::Cancelled);
                }
                match Flock::lock(file, FlockArg::LockExclusiveNonblock) {
                    Ok(lock) => break lock,
                    Err((f, Errno::EWOULDBLOCK | Errno::EINTR)) => file = f,
                    Err((_, e)) => return Err(Error::Local(format!("operation lock: {e}"))),
                }
                tokio::select! {biased; _=cancel.cancelled()=>return Err(Error::Cancelled),_=tokio::time::sleep(Duration::from_millis(20))=>{}}
            };
            *pending = match fs::read(path) {
                Ok(bytes) => Some(serde_json::from_slice(&bytes).map_err(|e| {
                    Error::Local(format!("read operation {}: {e}", path.display()))
                })?),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
                Err(e) => return Err(e.into()),
            };
            Some(locked)
        } else {
            None
        };
        Ok(OperationGuard {
            path: self.path.as_deref(),
            pending,
            _lock: lock,
        })
    }
}
fn parent(path: &Path) -> &Path {
    path.parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."))
}
pub struct OperationGuard<'a> {
    path: Option<&'a Path>,
    pending: MutexGuard<'a, Option<PendingCreate>>,
    _lock: Option<Flock<File>>,
}
impl OperationGuard<'_> {
    pub fn pending(&self) -> Option<&PendingCreate> {
        self.pending.as_ref()
    }
    pub fn record(&mut self, pending: PendingCreate) -> Result<()> {
        if let Some(path) = self.path {
            let mut file = tempfile::Builder::new()
                .prefix(".operation-")
                .tempfile_in(parent(path))?;
            file.write_all(&serde_json::to_vec(&pending)?)?;
            file.write_all(b"\n")?;
            file.as_file().sync_all()?;
            file.persist(path).map_err(|e| Error::Io(e.error))?;
            File::open(parent(path))?.sync_all()?;
        }
        *self.pending = Some(pending);
        Ok(())
    }
    pub fn clear(&mut self) -> Result<()> {
        if let Some(path) = self.path {
            match fs::remove_file(path) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e.into()),
            }
            File::open(parent(path))?.sync_all()?;
        }
        *self.pending = None;
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    fn pending() -> PendingCreate {
        PendingCreate {
            provider: "runpod".into(),
            boot_id: "b1".into(),
            before: vec!["old".into()],
            instance_id: None,
        }
    }
    #[tokio::test]
    async fn record_survives_reopen_and_clear_is_durable() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("state/operation.json");
        let state = OperationState::persistent(path.clone());
        let c = CancellationToken::new();
        {
            let mut g = state.acquire(&c).await.unwrap();
            g.record(pending()).unwrap();
        }
        assert_eq!(path.metadata().unwrap().permissions().mode() & 0o777, 0o600);
        let reopened = OperationState::persistent(path.clone());
        {
            let mut g = reopened.acquire(&c).await.unwrap();
            assert_eq!(g.pending(), Some(&pending()));
            g.clear().unwrap();
        }
        assert!(!path.exists());
        assert!(state.acquire(&c).await.unwrap().pending().is_none());
    }
    #[tokio::test]
    async fn independent_stores_cannot_overlap_and_wait_can_cancel() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("operation.json");
        let a = OperationState::persistent(path.clone());
        let b = OperationState::persistent(path);
        let c = CancellationToken::new();
        let mut guard = a.acquire(&c).await.unwrap();
        guard.record(pending()).unwrap();
        let timeout = tokio::time::timeout(Duration::from_millis(50), b.acquire(&c)).await;
        assert!(timeout.is_err());
        c.cancel();
        assert!(matches!(b.acquire(&c).await, Err(Error::Cancelled)));
        drop(guard);
        assert!(
            b.acquire(&CancellationToken::new())
                .await
                .unwrap()
                .pending()
                .is_some()
        );
    }
    #[tokio::test]
    async fn corrupt_record_is_not_overwritten() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("operation.json");
        fs::write(&path, b"broken").unwrap();
        let state = OperationState::persistent(path.clone());
        assert!(state.acquire(&CancellationToken::new()).await.is_err());
        assert_eq!(fs::read(path).unwrap(), b"broken");
    }
}
