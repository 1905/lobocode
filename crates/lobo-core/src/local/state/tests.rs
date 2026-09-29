use super::*;
use lobo_proto::GoTime;
use std::{
    os::unix::fs::PermissionsExt,
    process::Command,
    sync::{Arc, Barrier, mpsc},
    thread,
    time::{Duration, Instant},
};

fn own_state() -> LocalState {
    LocalState {
        pid: i64::from(std::process::id()),
        port: 8931,
        api_port: 8932,
        model: "q6".into(),
        weights: "/w".into(),
        started_at: GoTime::from_utc("2026-09-29T12:00:00Z".parse().unwrap()),
        boot_id: "b1".into(),
    }
}
fn dead_pid() -> i32 {
    let mut p = Command::new("true").spawn().unwrap();
    let pid = p.id() as i32;
    p.wait().unwrap();
    pid
}
fn file(dir: &Path) -> StateFile {
    StateFile::at(dir.join("local.json"))
}

#[test]
fn state_path_xdg_and_home() {
    assert_eq!(
        StateFile::path_from(Some("/x"), Some(Path::new("/home/u"))),
        Path::new("/x/lobo/local.json")
    );
    assert_eq!(
        StateFile::path_from(Some(""), Some(Path::new("/home/u"))),
        Path::new("/home/u/.local/state/lobo/local.json")
    );
    assert_eq!(
        StateFile::path_from(None, None),
        Path::new("./.local/state/lobo/local.json")
    );
}
#[test]
fn state_round_trip() {
    let tmp = tempfile::tempdir().unwrap();
    let f = file(tmp.path());
    let state = own_state();
    assert_eq!(f.read().unwrap(), None);
    f.claim(&state, &|_, _| false).unwrap();
    assert_eq!(
        fs::metadata(&f.path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert_eq!(f.read().unwrap(), Some(state.clone()));
    f.remove_if(state.pid as i32, &state.boot_id).unwrap();
    f.remove_if(state.pid as i32, &state.boot_id).unwrap();
    assert_eq!(f.read().unwrap(), None);
    assert!(fs::read_dir(tmp.path()).unwrap().all(|f| {
        !f.unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".local-")
    }));
}
#[test]
fn dead_pid_state_removed() {
    let tmp = tempfile::tempdir().unwrap();
    let f = file(tmp.path());
    let state = LocalState {
        pid: i64::from(dead_pid()),
        ..own_state()
    };
    f.claim(&state, &|_, _| false).unwrap();
    assert_eq!(f.read().unwrap(), None);
    assert!(!f.path.exists());
}
#[test]
fn corrupt_state_errors() {
    let tmp = tempfile::tempdir().unwrap();
    let f = file(tmp.path());
    fs::write(&f.path, b"{").unwrap();
    assert!(f.read().unwrap_err().to_string().contains("read "));
    assert!(f.claim(&own_state(), &|_, _| false).is_err());
    assert_eq!(fs::read(&f.path).unwrap(), b"{");
}
#[test]
fn reads_go_written_state() {
    let tmp = tempfile::tempdir().unwrap();
    let f = file(tmp.path());
    let mut v: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../lobo-proto/fixtures/local_state.json"
    ))
    .unwrap();
    v["pid"] = serde_json::json!(std::process::id());
    fs::write(&f.path, serde_json::to_vec(&v).unwrap()).unwrap();
    let state = f.read().unwrap().unwrap();
    assert_eq!(serde_json::to_value(state).unwrap(), v);
}
#[test]
fn written_bytes_match_go_layout() {
    let tmp = tempfile::tempdir().unwrap();
    let f = file(tmp.path());
    let state = own_state();
    f.claim(&state, &|_, _| false).unwrap();
    let expected = format!(
        "{{\n  \"pid\": {},\n  \"port\": 8931,\n  \"api_port\": 8932,\n  \"model\": \"q6\",\n  \"weights\": \"/w\",\n  \"started_at\": \"2026-09-29T12:00:00Z\",\n  \"boot_id\": \"b1\"\n}}\n",
        state.pid
    );
    assert_eq!(fs::read_to_string(&f.path).unwrap(), expected);
}
#[test]
fn claim_state_table() {
    for (pid, old_boot, is_ours, blocked) in [
        (std::process::id() as i32, "b1", true, true),
        (dead_pid(), "b1", true, false),
        (std::process::id() as i32, "b0", true, false),
        (std::process::id() as i32, "b1", false, false),
    ] {
        let tmp = tempfile::tempdir().unwrap();
        let f = file(tmp.path());
        let old = LocalState {
            pid: i64::from(pid),
            boot_id: old_boot.into(),
            ..own_state()
        };
        fs::write(&f.path, serde_json::to_vec(&old).unwrap()).unwrap();
        let mine = LocalState {
            boot_id: "b2".into(),
            ..own_state()
        };
        let result = f.claim(&mine, &|_, boot| is_ours && boot == "b1");
        assert_eq!(result.is_err(), blocked);
        if blocked {
            assert!(matches!(result, Err(Error::AlreadyRunning(_))));
        }
        assert_eq!(f.raw().unwrap(), if blocked { old } else { mine });
    }
}
#[test]
fn claim_state_concurrent_one_winner() {
    let tmp = tempfile::tempdir().unwrap();
    let f = file(tmp.path());
    let start = Arc::new(Barrier::new(16));
    let threads: Vec<_> = (0..16)
        .map(|i| {
            let f = f.clone();
            let start = start.clone();
            thread::spawn(move || {
                start.wait();
                let state = LocalState {
                    port: 9000 + i,
                    ..own_state()
                };
                (state.clone(), f.claim(&state, &|_, _| true))
            })
        })
        .collect();
    let mut winners = Vec::new();
    for t in threads {
        let (s, result) = t.join().unwrap();
        match result {
            Ok(()) => winners.push(s),
            Err(Error::AlreadyRunning(_)) => {}
            Err(e) => panic!("{e}"),
        }
    }
    assert_eq!(winners.len(), 1);
    assert_eq!(f.raw().unwrap(), winners[0]);
}
#[test]
fn remove_state_if_table() {
    for (pid, boot, keep) in [
        (111, "b1", false),
        (112, "b1", true),
        (111, "b2", true),
        (111, "", true),
    ] {
        let tmp = tempfile::tempdir().unwrap();
        let f = file(tmp.path());
        f.claim(
            &LocalState {
                pid: 111,
                ..own_state()
            },
            &|_, _| false,
        )
        .unwrap();
        f.remove_if(pid, boot).unwrap();
        assert_eq!(f.path.exists(), keep);
    }
}
#[test]
fn claim_waits_for_lock_holder() {
    let tmp = tempfile::tempdir().unwrap();
    let f = file(tmp.path());
    let child = f.clone();
    let (tx, rx) = mpsc::channel();
    let holder = thread::spawn(move || {
        let _lock = child.lock().unwrap();
        tx.send(()).unwrap();
        thread::sleep(Duration::from_millis(300));
    });
    rx.recv().unwrap();
    let started = Instant::now();
    f.claim(&own_state(), &|_, _| false).unwrap();
    assert!(started.elapsed() >= Duration::from_millis(250));
    holder.join().unwrap();
}
