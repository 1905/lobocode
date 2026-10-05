use super::*;
use std::os::unix::fs::symlink;

fn read(log: &Diagnostics) -> Vec<serde_json::Value> {
    fs::read_to_string(log.path())
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}
#[test]
fn records_survive_restart_and_are_private_allowlisted_and_bounded() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("config.env");
    let first = Diagnostics::new(&config);
    first.write("launch", None, None, Some("one"));
    let second = Diagnostics::new(&config);
    second.write("launch", None, None, Some(&"é".repeat(3000)));
    let records = read(&second);
    assert_eq!(records.len(), 2);
    assert_ne!(records[0]["session"], records[1]["session"]);
    assert_eq!(records[0]["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(records[0]["detail"], "one");
    assert!(records[1]["detail"].as_str().unwrap().len() <= MAX_TEXT + 3);
    assert_eq!(records[0].as_object().unwrap().len(), 10);
    assert_eq!(
        fs::metadata(first.path()).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert_eq!(
        fs::metadata(first.path().parent().unwrap())
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
    assert_eq!(second.checked_path().unwrap(), second.path());
    assert!(second.error().is_none());
}
#[test]
fn rotates_by_size_and_keeps_only_three_archives() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.app-logs/app.jsonl");
    for i in 0..8 {
        append(&path, format!("{i}\n").as_bytes(), 2).unwrap();
    }
    let names: Vec<_> = fs::read_dir(path.parent().unwrap())
        .unwrap()
        .map(|p| p.unwrap().file_name())
        .collect();
    assert_eq!(names.len(), 4);
    assert_eq!(fs::read_to_string(&path).unwrap(), "7\n");
    for (index, filename) in FILES.iter().enumerate().skip(1) {
        let archive = path.parent().unwrap().join(filename);
        assert_eq!(
            fs::read_to_string(&archive).unwrap(),
            format!("{}\n", 7 - index)
        );
        assert_eq!(
            fs::metadata(archive).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}
#[test]
fn redacts_configured_credentials_tokens_assignments_pem_and_signed_urls() {
    let dir = tempfile::tempdir().unwrap();
    let log = Diagnostics::new(&dir.path().join("config.env"));
    log.add_secrets(vec!["opaque-credential".into(), "short".into()]);
    let message = concat!(
        "opaque-credential short sk-testtoken rpa_testtoken ",
        "RUNPOD_API_KEY=anothersecret apiKey: \\\"quotedsecret\\\" ",
        "https://host.example/path?credential=secretquery ",
        "https://name:password@host.example/path\n",
        "Authorization: Bearer authcredential\n",
        "-----BEGIN OPENSSH PRIVATE KEY-----\npemsecret\n-----END OPENSSH PRIVATE KEY-----"
    );
    log.write("progress", None, None, Some(message));
    let bytes = fs::read_to_string(log.path()).unwrap();
    for secret in [
        "opaque-credential",
        "short",
        "sk-testtoken",
        "rpa_testtoken",
        "anothersecret",
        "quotedsecret",
        "secretquery",
        "name:password",
        "authcredential",
        "pemsecret",
    ] {
        assert!(!bytes.contains(secret), "leaked {secret}: {bytes}");
    }
    assert!(bytes.contains("redacted"));
    assert_eq!(
        log.scrub(&"secret-prefix".repeat(10_000)),
        "[oversized diagnostic omitted]"
    );
}
#[test]
fn rejects_directory_file_archive_symlinks_and_hard_links() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("target");
    fs::create_dir(&target).unwrap();
    let config = dir.path().join("config.env");
    let log_dir = config.with_extension("app-logs");
    symlink(&target, &log_dir).unwrap();
    let log = Diagnostics::new(&config);
    log.write("launch", None, None, None);
    assert!(log.error().is_some());
    assert!(!target.join("app.jsonl").exists());
    fs::remove_file(&log_dir).unwrap();
    fs::create_dir(&log_dir).unwrap();
    let external = target.join("private");
    fs::write(&external, "untouched").unwrap();
    symlink(&external, log.path()).unwrap();
    log.write("launch", None, None, None);
    assert!(log.error().is_some());
    assert!(log.checked_path().is_err());
    assert_eq!(fs::read_to_string(&external).unwrap(), "untouched");
    fs::remove_file(log.path()).unwrap();
    fs::hard_link(&external, log.path()).unwrap();
    log.write("launch", None, None, None);
    assert!(log.error().is_some());
    assert_eq!(fs::read_to_string(&external).unwrap(), "untouched");
    fs::remove_file(log.path()).unwrap();
    symlink(&external, log_dir.join(FILES[1])).unwrap();
    log.write("launch", None, None, None);
    assert!(log.error().is_some());
    assert_eq!(fs::read_to_string(&external).unwrap(), "untouched");
}
#[test]
fn failure_is_reported_and_recovers_after_path_is_repaired() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("config.env");
    fs::write(config.with_extension("app-logs"), "obstruction").unwrap();
    let log = Diagnostics::new(&config);
    log.write("launch", None, None, None);
    assert!(log.error().unwrap().contains("cannot be saved"));
    fs::remove_file(config.with_extension("app-logs")).unwrap();
    log.write("launch", None, None, None);
    assert!(log.error().is_none());
    assert_eq!(read(&log).len(), 1);
}
