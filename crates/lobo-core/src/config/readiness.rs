use super::*;
use lobo_proto::{DEFAULT_MODEL, Readiness};

pub fn readiness(path: &Path) -> Readiness {
    let mut r = Readiness {
        exists: path.exists(),
        local_supported: crate::local::supported().is_ok(),
        default_provider: "runpod".into(),
        default_model: DEFAULT_MODEL.into(),
        local_port: DEFAULT_LOCAL_PORT,
        ..Default::default()
    };
    match load_laptop(path) {
        Ok(cfg) => {
            r.providers = cfg.providers();
            r.default_provider = cfg.default_provider();
            r.default_model = if cfg.model.is_empty() {
                DEFAULT_MODEL.into()
            } else {
                cfg.model.clone()
            };
            r.local_port = cfg.port();
            r.cloud_ready = r.exists && cfg.require_cloud().is_ok();
            r.ready = r.cloud_ready || (r.local_supported && r.exists);
        }
        Err(e) => r.error = Some(e.to_string()),
    }
    r
}

/// Settings validation. Empty values clear a setting; zero retains built-in limits.
pub fn validate_set(set: &BTreeMap<String, String>) -> std::result::Result<(), String> {
    if let Some(v) = set.get("LOBO_CONNECTION")
        && !matches!(v.as_str(), "" | "ssh" | "cloudflare")
    {
        return Err("LOBO_CONNECTION: want ssh or cloudflare".into());
    }
    if let Some(v) = set.get("LOBO_CLOUD_PORT")
        && !v.is_empty()
        && (v == "0" || parse_local_port(v).is_err())
    {
        return Err("LOBO_CLOUD_PORT: whole number 1024-65534, or empty".into());
    }
    for (key, min) in [
        ("LOBO_MIN_MBPS", 1),
        ("LOBO_CTX", 512),
        ("LOBO_IDLE_MIN", 1),
        ("LOBO_MAX_HOURS", 1),
    ] {
        if let Some(v) = set.get(key)
            && !v.is_empty()
            && v != "0"
            && v.parse::<i64>().map_or(true, |n| n < min)
        {
            return Err(format!("{key}: whole number ≥ {min}, or empty"));
        }
    }
    if let Some(v) = set.get("LOBO_VAST_MAX_DPH")
        && !v.is_empty()
        && v.parse::<f64>()
            .map_or(true, |n| !n.is_finite() || n <= 0.0)
    {
        return Err("LOBO_VAST_MAX_DPH: a price like 1.20".into());
    }
    if set
        .get("LOBO_DOMAIN")
        .is_some_and(|v| v.contains('/') || v.contains(' '))
    {
        return Err("LOBO_DOMAIN: bare hostname, no https://".into());
    }
    if let Some(v) = set.get("LOBO_LOCAL_PORT")
        && !v.is_empty()
        && (v == "0" || parse_local_port(v).is_err())
    {
        return Err("LOBO_LOCAL_PORT: whole number 1024-65534, or empty".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn readiness_uses_cli_loading_rules() {
        let d = tempfile::tempdir().unwrap();
        let path = d.path().join("config.env");
        let r = readiness(&path);
        assert!(!r.exists && !r.ready);
        assert_eq!(r.local_port, 8931);
        let cloud = "RUNPOD_API_KEY=r\nCF_TUNNEL_TOKEN=t\nLOBO_DOMAIN=lobo.test\nLOBO_BUCKET_URL=https://bucket.test\n";
        std::fs::write(&path, cloud).unwrap();
        assert!(!readiness(&path).ready);
        std::fs::write(&path,format!("{cloud}LOBO_API_KEY=fixture\nLOBO_PROVIDER=vast\nLOBO_MODEL=q6\nLOBO_LOCAL_PORT=80\n")).unwrap();
        let r = readiness(&path);
        assert!(r.ready && r.cloud_ready);
        assert_eq!(r.providers, ["runpod"]);
        assert_eq!(r.default_provider, "runpod");
        assert_eq!(r.default_model, "q6");
        assert_eq!(r.local_port, 8931);
        std::fs::write(
            &path,
            "LOBO_API_KEY=fixture\nLOBO_PROVIDER=local\nLOBO_LOCAL_PORT=9000\n",
        )
        .unwrap();
        let r = readiness(&path);
        assert_eq!(r.default_provider, "local");
        assert!(!r.cloud_ready);
        assert_eq!(r.ready, r.local_supported);
        assert_eq!(r.local_port, 9000);
        for bad in ["LOBO_API_KEY=\n", "LOBO_API_KEY='unclosed"] {
            std::fs::write(&path, bad).unwrap();
            let r = readiness(&path);
            assert!(!r.ready && !r.cloud_ready);
            assert!(r.error.is_some());
        }
    }
    #[test]
    fn public_cloud_ignores_obsolete_private_storage() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.env");
        std::fs::write(&path, "LOBO_API_KEY=fixture\nRUNPOD_API_KEY=provider\nLOBO_BUCKET_URL=bad\nLOBO_MODEL_SOURCE=ssh://old-host\nLOBO_MODEL_SSH_KEY_FILE=/missing\nLOBO_POD_IMAGE=old:cached\n").unwrap();
        let r = readiness(&path);
        assert!(r.cloud_ready && r.ready);
        assert!(r.error.is_none());
        assert!(load_laptop(&path).unwrap().require_bucket().is_err());
    }
    #[test]
    fn validate_set_preserves_settings_rules() {
        let check = |k: &str, v: &str| validate_set(&BTreeMap::from([(k.into(), v.into())]));
        for value in ["", "1024", "8931", "65534"] {
            check("LOBO_LOCAL_PORT", value).unwrap();
        }
        for value in ["80", "1023", "65535", "0", "-1", "89.31", "port", " 8931"] {
            assert_eq!(
                check("LOBO_LOCAL_PORT", value).unwrap_err(),
                "LOBO_LOCAL_PORT: whole number 1024-65534, or empty"
            );
        }
        assert!(check("LOBO_CTX", "100").is_err());
        check("LOBO_CTX", "0").unwrap();
        assert_eq!(
            check("LOBO_DOMAIN", "https://x").unwrap_err(),
            "LOBO_DOMAIN: bare hostname, no https://"
        );
        for v in ["0", "-1", "NaN", "inf"] {
            assert!(check("LOBO_VAST_MAX_DPH", v).is_err());
        }
        check("LOBO_VAST_MAX_DPH", "1.20").unwrap();
    }
}
