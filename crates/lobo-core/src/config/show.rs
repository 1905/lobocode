use crate::Result;
use std::path::Path;
pub const PLAIN_KEYS: &[&str] = &[
    "LOBO_CONNECTION",
    "LOBO_CLOUD_PORT",
    "LOBO_DOMAIN",
    "LOBO_BUCKET_URL",
    "LOBO_PROVIDER",
    "LOBO_MODEL",
    "LOBO_CTX",
    "LOBO_IDLE_MIN",
    "LOBO_MAX_HOURS",
    "LOBO_MIN_MBPS",
    "LOBO_CLOUD",
    "LOBO_VAST_MAX_DPH",
    "LOBO_POD_IMAGE",
    "LOBO_IMAGE_REPO",
    "LOBO_MODEL_BASE_URL",
    "LOBO_MODEL_SOURCE",
    "LOBO_MODEL_SSH_KEY_FILE",
    "LOBO_MODEL_SSH_HOSTKEY",
    "R2_ACCOUNT_ID",
    "R2_ENDPOINT",
];
pub fn mask(s: &str) -> String {
    if s.is_empty() {
        return "(not set)".into();
    }
    if s.len() < 12 {
        return "••••".into();
    }
    if s.is_char_boundary(4) && s.is_char_boundary(s.len() - 4) {
        format!("{}…{}", &s[..4], &s[s.len() - 4..])
    } else {
        let first: String = s.chars().take(4).collect();
        let last: String = s
            .chars()
            .rev()
            .take(4)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        format!("{first}…{last}")
    }
}
pub fn masked(k: &str, v: &str) -> String {
    if PLAIN_KEYS.contains(&k) {
        v.into()
    } else {
        mask(v)
    }
}
pub fn show(path: &Path) -> Result<lobo_proto::ConfigShow> {
    let mut values = super::values(path)?;
    values.remove("LOBO_WEIGHTS_DIR");
    values.remove("LOBO_LOCAL_PORT");
    let cfg = super::Laptop::from_values(&values);
    if values.contains_key("LOBO_MODEL") {
        values.insert("LOBO_MODEL".into(), cfg.model);
    }
    if values.contains_key("LOBO_PROVIDER") {
        values.insert("LOBO_PROVIDER".into(), cfg.provider);
    }
    Ok(lobo_proto::ConfigShow {
        path: path.to_string_lossy().into_owned(),
        exists: path.exists(),
        set: values
            .iter()
            .map(|(k, v)| (k.clone(), !v.is_empty()))
            .collect(),
        values: values
            .into_iter()
            .map(|(k, v)| {
                let value = masked(&k, &v);
                (k, value)
            })
            .collect(),
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn masked_unknown_keys() {
        assert_eq!(masked("CUSTOM_TOKEN", "abcdefghijklmnop"), "abcd…mnop");
        assert_eq!(masked("LOBO_DOMAIN", "example.com"), "example.com");
        assert_eq!(mask("short"), "••••");
        assert_eq!(mask(""), "(not set)");
    }
    #[test]
    fn plain_keys_show_image_repo() {
        assert_eq!(masked("LOBO_IMAGE_REPO", "1905/lobocode"), "1905/lobocode");
    }
    #[test]
    fn plain_keys_show_model_base_url() {
        assert!(PLAIN_KEYS.contains(&"LOBO_MODEL_BASE_URL"));
    }
    #[test]
    fn show_masks_secrets() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("config");
        std::fs::write(
            &p,
            "RUNPOD_API_KEY=rpa_SECRETSECRETSECRET\nLOBO_DOMAIN=lobo.x.cc\nLOBO_CTX=\n",
        )
        .unwrap();
        let out = serde_json::to_value(show(&p).unwrap()).unwrap();
        assert!(!out.to_string().contains("SECRETSECRET"));
        assert_eq!(out["values"]["LOBO_DOMAIN"], "lobo.x.cc");
        assert_eq!(out["set"]["LOBO_CTX"], false);
        assert!(!show(&d.path().join("missing")).unwrap().exists);
    }
    #[test]
    fn mask_non_ascii_no_panic() {
        assert_eq!(mask("🦀🦀🦀🦀🦀🦀"), "🦀…🦀");
        assert_eq!(mask("日本語日本語"), "日本語日…語日本語");
    }
    #[test]
    fn show_migrates_retired_selections_and_hides_local_settings() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("config");
        let original = "VASTAI_API_KEY=fixture\nLOBO_PROVIDER=local\nLOBO_MODEL=q8\nLOBO_WEIGHTS_DIR=/existing\nLOBO_LOCAL_PORT=invalid\n";
        std::fs::write(&p, original).unwrap();
        let shown = show(&p).unwrap();
        assert_eq!(shown.values["LOBO_PROVIDER"], "vast");
        assert_eq!(shown.values["LOBO_MODEL"], lobo_proto::DEFAULT_MODEL);
        for key in ["LOBO_WEIGHTS_DIR", "LOBO_LOCAL_PORT"] {
            assert!(!shown.values.contains_key(key));
            assert!(!shown.set.contains_key(key));
        }
        assert_eq!(std::fs::read_to_string(&p).unwrap(), original);
    }
}
