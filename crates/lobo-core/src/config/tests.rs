use super::*;
fn load(text: &str) -> Result<Laptop> {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("config");
    std::fs::write(&p, text).unwrap();
    load_laptop(&p)
}
const FULL: &str = "RUNPOD_API_KEY=rp\nVASTAI_API_KEY=vk\nLOBO_API_KEY=sk\nCF_TUNNEL_TOKEN=tok\nLOBO_DOMAIN=lobo.x.cc\nLOBO_BUCKET_URL=https://b\nR2_ACCOUNT_ID=account\nR2_ACCESS_KEY=access\nR2_SECRET_KEY=secret\nR2_ENDPOINT=https://r2\n";
#[test]
fn cloud_without_domain_and_legacy_selection() {
    for key in ["RUNPOD_API_KEY", "VASTAI_API_KEY"] {
        let cfg = load(&format!("LOBO_API_KEY=sk\n{key}=provider\n")).unwrap();
        cfg.require_cloud().unwrap();
        assert!(cfg.uses_ssh());
        assert_eq!(cfg.cloud_url(), "http://127.0.0.1:8933/v1");
        let generated = crate::genkey::opencode_for_laptop(&cfg, "sk").unwrap();
        assert!(generated.contains("http://127.0.0.1:8933/v1"));
        assert!(!generated.contains("https:///"));
    }
    let legacy = load(FULL).unwrap();
    assert!(!legacy.uses_ssh());
    assert_eq!(legacy.cloud_url(), "https://lobo.x.cc/v1");
    let migrated = load(&format!(
        "{FULL}LOBO_CONNECTION=ssh\nLOBO_CLOUD_PORT=9010\n"
    ))
    .unwrap();
    assert!(migrated.uses_ssh());
    assert_eq!(migrated.cloud_url(), "http://127.0.0.1:9010/v1");
    let mut invalid = migrated;
    invalid.cloud_port = "8932".into();
    assert!(
        invalid
            .defaults()
            .unwrap_err()
            .to_string()
            .contains("overlap")
    );
    invalid.connection = "public-http".into();
    assert!(invalid.require_cloud().is_err());
}
#[test]
fn load_laptop_full() {
    let l = load(FULL).unwrap();
    assert_eq!(l.runpod_api_key, "rp");
    assert_eq!(l.domain, "lobo.x.cc");
    l.require_r2().unwrap();
    l.require_cloud().unwrap();
    assert_eq!(l.secret_values().len(), 6);
}
#[test]
fn load_laptop_vast_only() {
    let l = load(&FULL.replace("RUNPOD_API_KEY=rp\n", "")).unwrap();
    assert!(l.runpod_api_key.is_empty());
    assert_eq!(l.vast_api_key, "vk");
    l.require_cloud().unwrap();
}
#[test]
fn load_laptop_no_r2_then_require_r2_fails() {
    let l = load("LOBO_API_KEY=sk\n").unwrap();
    assert!(
        l.require_r2()
            .unwrap_err()
            .to_string()
            .contains("R2_ACCOUNT_ID")
    );
}
#[test]
fn load_laptop_ignores_bad_defaults() {
    let l = load(&format!("{FULL}LOBO_CTX=100\nLOBO_MODEL=q4\n")).unwrap();
    assert!(l.defaults().is_err());
}
#[test]
fn load_laptop_local_only() {
    let l = load("LOBO_API_KEY=sk-x\nLOBO_PROVIDER=local\n").unwrap();
    let e = l.require_cloud().unwrap_err().to_string();
    assert!(e.contains("RUNPOD_API_KEY or VASTAI_API_KEY"));
    assert!(load("LOBO_API_KEY=sk-x\nLOBO_WEIGHTS_DIR=/w\n").is_ok());
}
#[test]
fn load_laptop_errors() {
    for (text, needle) in [("", "LOBO_API_KEY"), ("LOBO_API_KEY='unclosed", "read")] {
        assert!(load(text).unwrap_err().to_string().contains(needle));
    }
}
#[test]
fn require_cloud() {
    let mut l = Laptop::default();
    assert_eq!(
        l.require_cloud().unwrap_err().to_string(),
        "config: set RUNPOD_API_KEY or VASTAI_API_KEY"
    );
    l.cf_tunnel_token = "t".into();
    l.domain = "d".into();
    l.bucket_url = "https://b".into();
    assert!(
        l.require_cloud()
            .unwrap_err()
            .to_string()
            .contains("RUNPOD_API_KEY")
    );
    l.runpod_api_key = "r".into();
    l.require_cloud().unwrap();
}
#[test]
fn require_parts_table() {
    let good = load(FULL).unwrap();
    good.require_bucket().unwrap();
    good.require_provider_key().unwrap();
    good.require_r2().unwrap();
    let mut no_key = good.clone();
    no_key.runpod_api_key.clear();
    no_key.vast_api_key.clear();
    assert!(no_key.require_provider_key().is_err());
    let mut no_bucket = good.clone();
    no_bucket.bucket_url.clear();
    assert_eq!(
        no_bucket.require_bucket().unwrap_err().to_string(),
        "config: set LOBO_BUCKET_URL"
    );
    no_bucket.bucket_url = "bad".into();
    assert!(no_bucket.require_bucket().is_err());
    for i in 0..5 {
        let mut l = good.clone();
        match i {
            0 => l.r2.account_id.clear(),
            1 => l.r2.access_key.clear(),
            2 => l.r2.secret_key.clear(),
            3 => l.r2.endpoint.clear(),
            _ => l.r2.endpoint = "bad".into(),
        };
        assert!(l.require_r2().is_err());
    }
}
#[test]
fn secret_values_includes_ssh_key_file() {
    use base64::Engine;
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("key");
    let bytes = b"test-private-key\n";
    std::fs::write(&p, bytes).unwrap();
    let l = Laptop {
        model_ssh_key_file: p.to_string_lossy().into_owned(),
        ..Default::default()
    };
    let v = l.secret_values();
    assert_eq!(v["LOBO_MODEL_SSH_KEY_FILE"], String::from_utf8_lossy(bytes));
    assert_eq!(
        v["LOBO_MODEL_SSH_KEY_FILE (base64)"],
        base64::engine::general_purpose::STANDARD.encode(bytes)
    );
}
#[test]
fn defaults_parse_and_reject() {
    let l = Laptop {
        provider: "vast".into(),
        model: "q6".into(),
        cloud: "community".into(),
        ctx: "32768".into(),
        min_mbps: "200".into(),
        vast_max_dph: "0.9".into(),
        ..Default::default()
    };
    assert_eq!(
        l.defaults().unwrap(),
        Defaults {
            provider: "vast".into(),
            model: "q6".into(),
            cloud: "community".into(),
            ctx: 32768,
            min_mbps: 200,
            vast_max_dph: 0.9,
            ..Default::default()
        }
    );
    for (key, v) in [
        ("LOBO_PROVIDER", "aws"),
        ("LOBO_MODEL", "q4"),
        ("LOBO_CLOUD", "x"),
        ("LOBO_CTX", "100"),
        ("LOBO_IDLE_MIN", "-1"),
        ("LOBO_VAST_MAX_DPH", "0"),
    ] {
        let l = Laptop::from_values(&[(key.into(), v.into())].into());
        assert!(l.defaults().is_err());
    }
}
#[test]
fn defaults_local_port() {
    for v in ["bad", "1023", "65535"] {
        let l = Laptop {
            local_port: v.into(),
            ..Default::default()
        };
        assert!(
            l.defaults()
                .unwrap_err()
                .to_string()
                .contains("LOBO_LOCAL_PORT")
        );
        assert_eq!(l.port(), 8931);
    }
    for v in ["", "0", "1024", "65534"] {
        assert!(parse_local_port(v).is_ok());
    }
}
#[test]
fn defaults_partial_keeps_good_fields() {
    let (d, bad) = defaults_partial(&Laptop {
        model: "q6".into(),
        ctx: "100".into(),
        ..Default::default()
    });
    assert_eq!(d.model, "q6");
    assert_eq!(d.ctx, 0);
    assert_eq!(bad.keys().collect::<Vec<_>>(), ["LOBO_CTX"]);
}
#[test]
fn default_path_xdg_and_home() {
    assert_eq!(
        default_path_from(Some("/x"), Some(Path::new("/h"))),
        Path::new("/x/lobo/config.env")
    );
    assert_eq!(
        default_path_from(None, Some(Path::new("/h"))),
        Path::new("/h/.config/lobo/config.env")
    );
    assert_eq!(
        default_path_from(Some(""), None),
        Path::new(".config/lobo/config.env")
    );
}
#[test]
fn weights_and_port() {
    let home = Path::new("/h");
    for (input, want) in [
        ("", "/h/Library/Application Support/lobo/weights"),
        ("~/models", "/h/models"),
        ("~", "/h"),
        ("/w", "/w"),
    ] {
        let l = Laptop {
            weights_dir: input.into(),
            local_port: "9000".into(),
            ..Default::default()
        };
        assert_eq!(l.weights_with_home(home), Path::new(want));
        assert_eq!(l.port(), 9000);
    }
}
#[test]
fn default_provider_table() {
    for (r, v, p, want) in [
        ("r", "", "", "runpod"),
        ("", "v", "", "vast"),
        ("r", "v", "", "runpod"),
        ("r", "v", "vast", "vast"),
        ("r", "", "vast", "runpod"),
        ("r", "v", "local", "local"),
        ("", "", "local", "local"),
    ] {
        assert_eq!(
            Laptop {
                runpod_api_key: r.into(),
                vast_api_key: v.into(),
                provider: p.into(),
                ..Default::default()
            }
            .default_provider(),
            want
        );
    }
}
#[test]
fn loose_mode_bits() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("config");
    std::fs::write(&p, "").unwrap();
    for (mode, want) in [(0o600, false), (0o644, true)] {
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(mode)).unwrap();
        assert_eq!(loose_mode(&p), want);
    }
}
