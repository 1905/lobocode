use super::*;
use serde_json::Value;

#[test]
fn generated_client_has_only_supported_cloud_model_and_endpoint() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("config.json");
    for (connection, expected) in [
        ("ssh", "http://127.0.0.1:9000/v1"),
        // A legacy Cloudflare setting still exports the local SSH endpoint.
        ("cloudflare", "http://127.0.0.1:9000/v1"),
    ] {
        let laptop = config::Laptop {
            connection: connection.into(),
            cloud_port: "9000".into(),
            domain: "lobo.example.com".into(),
            ..Default::default()
        };
        write_opencode_for_laptop(&path, &laptop, "key").unwrap();
        let cfg: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        let model = lobo_proto::catalog::get(lobo_proto::DEFAULT_CLOUD_MODEL).unwrap();
        assert_eq!(cfg["provider"].as_object().unwrap().len(), 1);
        assert_eq!(cfg["provider"]["lobo"]["options"]["baseURL"], expected);
        assert_eq!(cfg["provider"]["lobo"]["options"]["apiKey"], "key");
        assert_eq!(
            cfg["agent"]["lobo"]["model"],
            format!("lobo/{}", model.alias)
        );
        assert!(
            cfg["provider"]["lobo"]["models"]
                .get(&model.alias)
                .is_some()
        );
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
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
