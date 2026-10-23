//! Bounded E2E diagnostics. Never record arguments, raw errors or config values.
use std::{io::Write, sync::{Mutex, atomic::{AtomicU64, Ordering}}};
static SEQUENCE: AtomicU64 = AtomicU64::new(0);
static WRITE_LOCK: Mutex<()> = Mutex::new(());
pub(crate) fn event(event: &str, result: Option<&str>) {
    // Only fixed event/command/kind identifiers may reach this function.
    let safe = |s: &str| s.len() <= 96 && s.bytes().all(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-' | b':' | b'|'));
    if !safe(event) || result.is_some_and(|r| !safe(r)) { return; }
    let Ok(root) = crate::e2e_runtime::metadata_root() else { return; };
    let Ok(_guard) = WRITE_LOCK.lock() else { return; };
    let path = root.join("native-trace.jsonl");
    if std::fs::symlink_metadata(&path).is_ok_and(|m| !m.is_file() || m.len() > 1 << 20) { return; }
    let row = serde_json::json!({"at_ms": chrono::Utc::now().timestamp_millis(), "pid": std::process::id(),
        "seq": SEQUENCE.fetch_add(1, Ordering::Relaxed), "event": event, "result": result});
    use std::os::unix::fs::OpenOptionsExt;
    if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK).open(path) {
        let _ = writeln!(file, "{row}");
    }
}
