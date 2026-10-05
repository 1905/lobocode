//! Small, private lifecycle records. Never serialize config, status or request bodies.
use crate::types::PanelState;
use lobo_core::control::RuntimeTarget;
use regex::Regex;
use serde::Serialize;
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Write},
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt},
    },
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicU64, Ordering},
    },
};

const MAX_FILE: u64 = 2 * 1024 * 1024;
const MAX_TEXT: usize = 2048;
const FILES: [&str; 4] = ["app.jsonl", "app.1.jsonl", "app.2.jsonl", "app.3.jsonl"];
static SESSIONS: AtomicU64 = AtomicU64::new(0);

pub struct Diagnostics {
    path: PathBuf,
    session: String,
    inner: Mutex<Inner>,
}
#[derive(Default)]
struct Inner {
    secrets: Vec<String>,
    error: Option<String>,
}
#[derive(Serialize)]
struct Record {
    at: String,
    version: &'static str,
    session: String,
    event: &'static str,
    provider: Option<String>,
    model: Option<String>,
    phase: Option<String>,
    instance_id: Option<String>,
    boot_id: Option<String>,
    detail: Option<String>,
}
impl Diagnostics {
    pub fn new(config: &Path) -> Arc<Self> {
        Arc::new(Self {
            path: config.with_extension("app-logs").join(FILES[0]),
            session: format!(
                "{}-{}-{}",
                chrono::Utc::now().timestamp_micros(),
                std::process::id(),
                SESSIONS.fetch_add(1, Ordering::Relaxed)
            ),
            inner: Mutex::new(Inner::default()),
        })
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
    pub fn error(&self) -> Option<String> {
        self.inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .error
            .clone()
    }
    pub fn add_secrets(&self, secrets: Vec<String>) {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        for secret in secrets {
            if !secret.is_empty() && !inner.secrets.contains(&secret) {
                inner.secrets.push(secret);
            }
        }
        inner.secrets.sort_by_key(|s| std::cmp::Reverse(s.len()));
    }
    pub fn scrub(&self, text: &str) -> String {
        let inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        redact(text, &inner.secrets)
    }
    pub fn write(
        &self,
        event: &'static str,
        state: Option<&PanelState>,
        owner: Option<&RuntimeTarget>,
        detail: Option<&str>,
    ) {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        self.write_locked(&mut inner, event, state, owner, detail);
    }
    fn write_locked(
        &self,
        inner: &mut Inner,
        event: &'static str,
        state: Option<&PanelState>,
        owner: Option<&RuntimeTarget>,
        detail: Option<&str>,
    ) {
        let clean = |text: &str| redact(text, &inner.secrets);
        let record = Record {
            at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            version: env!("CARGO_PKG_VERSION"),
            session: self.session.clone(),
            event,
            provider: owner
                .map(|o| o.provider.as_str())
                .or_else(|| state.map(|s| s.provider.as_str()))
                .map(clean),
            model: state.map(|s| clean(&s.model)),
            phase: state.map(|s| clean(s.up_phase.as_deref().unwrap_or(s.phase.word()))),
            instance_id: owner.and_then(|o| o.instance_id.as_deref()).map(clean),
            boot_id: owner.map(|o| clean(&o.boot_id)),
            detail: detail.map(clean),
        };
        let result = serde_json::to_vec(&record)
            .map_err(io::Error::other)
            .and_then(|mut line| {
                line.push(b'\n');
                append(&self.path, &line, MAX_FILE)
            });
        inner.error = result
            .err()
            .map(|e| format!("Startup logs cannot be saved: {e}"));
    }
    /// Recheck the fixed path before passing it to the native opener.
    pub fn checked_path(&self) -> io::Result<PathBuf> {
        let dir = private_dir(self.path.parent().unwrap())?;
        let _ = private_file(&dir, FILES[0], false)?;
        Ok(self.path.clone())
    }
    pub fn install_panic_hook(self: &Arc<Self>) {
        let diagnostics = self.clone();
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            // Panic payloads may include arbitrary user data. Record location only.
            let location = info
                .location()
                .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()));
            // A panic during logging must not reacquire its own mutex or block
            // another worker. Payloads are deliberately never persisted.
            if let Ok(mut inner) = diagnostics.inner.try_lock() {
                diagnostics.write_locked(&mut inner, "panic", None, None, location.as_deref());
            }
            previous(info);
        }));
    }
}

fn redact(text: &str, secrets: &[String]) -> String {
    // Omit oversized input entirely: truncation before scrubbing can leak a token prefix.
    if text.len() > 64 * 1024 {
        return "[oversized diagnostic omitted]".into();
    }
    let mut text = text.to_string();
    for secret in secrets {
        text = text.replace(secret, "[redacted]");
    }
    static PATTERNS: OnceLock<Vec<Regex>> = OnceLock::new();
    let patterns = PATTERNS.get_or_init(|| [
        r"(?s)-----BEGIN[^\r\n]*PRIVATE KEY-----.*?(?:-----END[^\r\n]*PRIVATE KEY-----|$)",
        r"(?i)\b(?:authorization|proxy-authorization)\s*[:=]\s*[^\r\n]+",
        r"(?i)\b(?:bearer|basic)\s+[A-Za-z0-9+/_.=:-]+",
        r#"(?i)\b(?:[A-Z0-9_]*(?:API_KEY|TOKEN|SECRET|PASSWORD|ACCESS_KEY|PRIVATE_KEY)[A-Z0-9_]*|apiKey|api_key|key|token|password|secret)\s*[\"']?\s*[:=]\s*(?:\"[^\"]*\"|'[^']*'|[^\s&,;]+)"#,
        r"(?i)\b(?:sk-|rpa_|rps_|ghp_|github_pat_)[A-Za-z0-9_-]+",
        r"(?i)https?://[^\s/@]+:[^\s/@]+@[^\s]+",
    ].into_iter().map(|p| Regex::new(p).expect("constant redaction pattern")).collect());
    for pattern in patterns {
        text = pattern.replace_all(&text, "[redacted]").into_owned();
    }
    // Diagnostics do not need URL queries, which can carry signed credentials.
    static URL_QUERY: OnceLock<Regex> = OnceLock::new();
    text = URL_QUERY
        .get_or_init(|| Regex::new(r"(https?://[^\s?]+)\?[^\s]+").unwrap())
        .replace_all(&text, "$1?[redacted]")
        .into_owned();
    let end = text
        .char_indices()
        .map(|(i, _)| i)
        .take_while(|i| *i <= MAX_TEXT)
        .last()
        .unwrap_or(0);
    if text.len() > MAX_TEXT {
        text.truncate(end);
        text.push_str("…");
    }
    text
}
fn private_dir(path: &Path) -> io::Result<File> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    match fs::DirBuilder::new().mode(0o700).create(path) {
        Ok(()) => (),
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => (),
        Err(e) => return Err(e),
    }
    let dir = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_DIRECTORY)
        .open(path)?;
    if dir.metadata()?.uid() != unsafe { libc::geteuid() } {
        return Err(io::Error::other("log directory belongs to another user"));
    }
    dir.set_permissions(fs::Permissions::from_mode(0o700))?;
    Ok(dir)
}
fn name(value: &str) -> std::ffi::CString {
    std::ffi::CString::new(value).expect("fixed log filename")
}
fn private_file(dir: &File, filename: &str, create: bool) -> io::Result<File> {
    let flags = libc::O_WRONLY
        | libc::O_APPEND
        | libc::O_NOFOLLOW
        | libc::O_NONBLOCK
        | libc::O_CLOEXEC
        | if create { libc::O_CREAT } else { 0 };
    let fd = unsafe { libc::openat(dir.as_raw_fd(), name(filename).as_ptr(), flags, 0o600) };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    let file = unsafe { File::from_raw_fd(fd) };
    let meta = file.metadata()?;
    if !meta.is_file() || meta.nlink() != 1 || meta.uid() != unsafe { libc::geteuid() } {
        return Err(io::Error::other("log must be a private regular file"));
    }
    file.set_permissions(fs::Permissions::from_mode(0o600))?;
    Ok(file)
}
fn append(path: &Path, line: &[u8], limit: u64) -> io::Result<()> {
    let dir = private_dir(
        path.parent()
            .ok_or_else(|| io::Error::other("missing log directory"))?,
    )?;
    // Reject unsafe archives too. All operations stay anchored to the opened directory.
    for filename in FILES.iter().skip(1) {
        match private_file(&dir, filename, false) {
            Ok(_) => (),
            Err(e) if e.kind() == io::ErrorKind::NotFound => (),
            Err(e) => return Err(e),
        }
    }
    let mut file = private_file(&dir, FILES[0], true)?;
    if file.metadata()?.len() + line.len() as u64 > limit {
        drop(file);
        for index in (1..FILES.len()).rev() {
            let result = unsafe {
                libc::renameat(
                    dir.as_raw_fd(),
                    name(FILES[index - 1]).as_ptr(),
                    dir.as_raw_fd(),
                    name(FILES[index]).as_ptr(),
                )
            };
            if result < 0 {
                let error = io::Error::last_os_error();
                if error.kind() != io::ErrorKind::NotFound {
                    return Err(error);
                }
            }
        }
        file = private_file(&dir, FILES[0], true)?;
    }
    file.write_all(line)?;
    file.sync_all()?;
    dir.sync_all()
}

#[cfg(test)]
mod tests;
