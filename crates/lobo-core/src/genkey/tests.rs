use super::*;
use serde_json::Value;

#[test]
fn write_opencode_table() {
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("config.json");
    for (domain, port) in [("lobo.example.com", 8931), ("", 9000)] {
        write_opencode(&p, domain, "key", port).unwrap();
        let cfg: Value = serde_json::from_slice(&fs::read(&p).unwrap()).unwrap();
        assert_eq!(
            cfg["provider"]["lobo-local"]["options"]["baseURL"],
            format!("http://127.0.0.1:{port}/v1")
        );
        assert_eq!(cfg["provider"]["lobo-local"]["options"]["apiKey"], "key");
        assert_eq!(cfg["provider"].get("lobo").is_some(), !domain.is_empty());
        assert_eq!(
            fs::metadata(&p).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}

#[test]
fn opencode_matches_go_bytes() {
    assert_eq!(
        opencode_config("lobo.example.com", "sk-fixed", 8931).unwrap(),
        include_str!("../../fixtures/opencode/cloud.json")
    );
    assert_eq!(
        opencode_config("", "sk-fixed", 9000).unwrap(),
        include_str!("../../fixtures/opencode/local_only.json")
    );
}

#[test]
fn new_api_key_shape() {
    let a = new_api_key();
    let b = new_api_key();
    assert_ne!(a, b);
    assert_eq!(a.len(), 51);
    assert!(a.starts_with("sk-"));
    assert!(
        a[3..]
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    );
}

#[test]
fn ensure_keeps_existing() {
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("config.env");
    fs::write(&p, "# hand edits\nLOBO_API_KEY=sk-existing\n").unwrap();
    assert_eq!(
        ensure_api_key(&p, false).unwrap(),
        ("sk-existing".into(), false)
    );
    assert_eq!(
        fs::read_to_string(&p).unwrap(),
        "# hand edits\nLOBO_API_KEY=sk-existing\n"
    );
}
#[test]
fn ensure_creates_or_rotates() {
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("config.env");
    fs::write(&p, "# hand edits\n").unwrap();
    let (first, written) = ensure_api_key(&p, false).unwrap();
    assert!(written);
    assert_eq!(config::values(&p).unwrap()["LOBO_API_KEY"], first);
    let (second, written) = ensure_api_key(&p, true).unwrap();
    assert!(written);
    assert_ne!(first, second);
    assert!(
        fs::read_to_string(&p)
            .unwrap()
            .starts_with("# hand edits\n")
    );
}
#[test]
fn ensure_missing_config_is_error() {
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("missing.env");
    assert!(
        ensure_api_key(&p, false)
            .unwrap_err()
            .to_string()
            .starts_with("read ")
    );
    assert!(!p.exists());
}
