use std::process::Command;
#[test]
fn bin_fatal_without_creds_exits_1() {
    let out = Command::new(env!("CARGO_BIN_EXE_lobo-agent"))
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("lobo-agent fatal: config:"), "{err}");
    assert!(err.contains("cannot self-terminate"), "{err}");
}
#[test]
fn bin_version() {
    let out = Command::new(env!("CARGO_BIN_EXE_lobo-agent"))
        .env_clear()
        .arg("version")
        .output()
        .unwrap();
    assert!(out.status.success());
    assert_eq!(
        String::from_utf8(out.stdout).unwrap(),
        format!("{}\n", lobo_agent::VERSION)
    );
}
