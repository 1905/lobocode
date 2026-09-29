use lobo_core::local::StateFile;
use lobo_proto::LocalState;
use nix::fcntl::{Flock, FlockArg};
use std::{
    fs::{self, OpenOptions},
    io::{BufRead, BufReader},
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

fn enabled() -> bool {
    std::env::var("LOBO_GO_INTEROP").as_deref() == Ok("1")
}
fn go() -> Command {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut c = Command::new("go");
    c.current_dir(root).args(["run", "./tools/corefixtures"]);
    c
}
fn own_state() -> LocalState {
    LocalState {
        pid: i64::from(std::process::id()),
        boot_id: "rust-interop".into(),
        ..Default::default()
    }
}
fn lock_path(path: &Path) -> PathBuf {
    let mut p = path.as_os_str().to_owned();
    p.push(".lock");
    p.into()
}

#[test]
fn go_lock_blocks_rust_claim() {
    if !enabled() {
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let f = StateFile::at(tmp.path().join("local.json"));
    let mut child = go()
        .arg("holdlock")
        .arg(lock_path(&f.path))
        .arg("500")
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut output = BufReader::new(child.stdout.take().unwrap());
    let mut line = String::new();
    output.read_line(&mut line).unwrap();
    assert_eq!(line.trim(), "locked");
    let start = Instant::now();
    f.claim(&own_state(), &|_, _| false).unwrap();
    assert!(start.elapsed() >= Duration::from_millis(400));
    assert!(child.wait().unwrap().success());
}
#[test]
fn rust_lock_seen_by_go() {
    if !enabled() {
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("local.json.lock");
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(&path)
        .unwrap();
    let _lock = Flock::lock(file, FlockArg::LockExclusive).unwrap();
    let output = go().arg("trylock").arg(&path).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8(output.stdout).unwrap().trim(), "busy");
}
#[test]
fn go_reads_rust_state() {
    if !enabled() {
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let path = StateFile::path_from(Some(tmp.path().to_str().unwrap()), None);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    StateFile::at(path)
        .claim(&own_state(), &|_, _| false)
        .unwrap();
    let output = go().arg("readstate").arg(tmp.path()).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["ok"], true);
    assert_eq!(result["pid"], std::process::id());
    assert_eq!(result["boot_id"], "rust-interop");
}
