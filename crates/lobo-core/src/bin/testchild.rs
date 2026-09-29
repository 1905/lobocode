// Process fixture for local-provider integration tests. It never invokes llama.
use lobo_core::local::{
    RunConfig, StateFile,
    provider::{command_of, is_supervisor},
};
use lobo_proto::{GoTime, LocalState, Manifest};
use std::{io::Write, os::unix::fs::OpenOptionsExt, path::PathBuf, time::Duration};
fn write(path: PathBuf, body: impl AsRef<[u8]>) {
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .mode(0o600)
        .open(path)
        .unwrap();
    f.write_all(body.as_ref()).unwrap();
}
fn main() {
    let mode = std::env::var("LOBO_LOCAL_HELPER").expect("test helper requires LOBO_LOCAL_HELPER");
    let args: Vec<_> = std::env::args().skip(1).collect();
    if mode == "fail" {
        for n in 1..=25 {
            println!("line {n}");
        }
        eprintln!("boom: weights gone");
        std::process::exit(1);
    }
    let state = StateFile::at(
        std::env::var_os("LOBO_TEST_STATE")
            .expect("test state path")
            .into(),
    );
    let dir = state.path.parent().unwrap();
    std::fs::create_dir_all(dir).unwrap();
    if args.first().is_some_and(|a| a == "--helper-child") {
        // SAFETY: SIG_IGN is a valid disposition; this fixture is single-threaded.
        unsafe {
            libc::signal(libc::SIGTERM, libc::SIG_IGN);
        }
        write(dir.join("child-ready"), b"ready");
        std::thread::sleep(Duration::from_secs(60));
        return;
    }
    write(dir.join("args"), args.join(" "));
    write(dir.join("pid"), std::process::id().to_string());
    if mode.starts_with("stubborn") {
        // SAFETY: same as above, before creating the reaper thread.
        unsafe {
            libc::signal(libc::SIGTERM, libc::SIG_IGN);
        }
    }
    if mode.ends_with("-child") {
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .arg("--helper-child")
            .spawn()
            .unwrap();
        write(dir.join("child"), child.id().to_string());
        while !dir.join("child-ready").exists() {
            std::thread::sleep(Duration::from_millis(2));
        }
        std::thread::spawn(move || {
            let _ = child.wait();
        });
    }
    if !mode.starts_with("no-state") {
        let prefix = if args
            .first()
            .is_some_and(|a| a == lobo_core::local::SUPERVISOR_ARG)
        {
            3
        } else {
            2
        };
        let cfg = RunConfig::from_args(&args[prefix..], Manifest::default()).unwrap();
        let s = LocalState {
            pid: std::process::id().into(),
            port: cfg.port.into(),
            api_port: cfg.api_port.into(),
            model: cfg.model,
            boot_id: cfg.boot_id,
            started_at: GoTime::from_utc(chrono::Utc::now()),
            ..Default::default()
        };
        if let Err(e) = state.claim(&s, &|pid, boot| is_supervisor(pid, boot, &command_of)) {
            eprintln!("{e}");
            std::process::exit(2);
        }
    }
    std::thread::sleep(Duration::from_secs(60));
    std::process::exit(3);
}
