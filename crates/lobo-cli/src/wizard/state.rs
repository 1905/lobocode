use lobo_core::config::mask;
use std::collections::BTreeMap;

pub const SECRETS: [&str; 4] = [
    "RUNPOD_API_KEY",
    "VASTAI_API_KEY",
    "LOBO_API_KEY",
    "CF_TUNNEL_TOKEN",
];
pub const SUMMARY_ROWS: [(&str, &str); 11] = [
    ("RUNPOD_API_KEY", "RunPod key"),
    ("VASTAI_API_KEY", "Vast key"),
    ("LOBO_CONNECTION", "Connection"),
    ("LOBO_API_KEY", "LOBO API key"),
    ("LOBO_PROVIDER", "Default provider"),
    ("LOBO_MODEL", "Model"),
    ("LOBO_CTX", "Context"),
    ("LOBO_IDLE_MIN", "Idle minutes"),
    ("LOBO_MAX_HOURS", "Max hours"),
    ("LOBO_CLOUD", "RunPod cloud"),
    ("LOBO_VAST_MAX_DPH", "Vast max $/h"),
];
pub fn resolve_secret(old: &str, typed: &str) -> String {
    match typed.trim() {
        "" => old,
        "-" => "",
        other => other,
    }
    .into()
}
pub fn new_api_key() -> String {
    lobo_core::genkey::new_api_key()
}
#[derive(Default)]
pub struct WizardState {
    pub cur: BTreeMap<String, String>,
    pub runpod: String,
    pub vast: String,
    pub tunnel: String,
    pub api_key: String,
    pub domain: String,
    pub provider: String,
    pub model: String,
    pub cloud: String,
    pub min_mbps: String,
    pub ctx: String,
    pub idle: String,
    pub max_h: String,
    pub vast_dph: String,
    pub save: bool,
}
impl WizardState {
    pub fn new(cur: BTreeMap<String, String>) -> Self {
        let get = |k: &str| cur.get(k).cloned().unwrap_or_default();
        let effective = lobo_core::config::Laptop::from_values(&cur);
        let mut s = Self {
            api_key: if get("LOBO_API_KEY").is_empty() {
                "new"
            } else {
                "keep"
            }
            .into(),
            domain: get("LOBO_DOMAIN"),
            provider: effective.default_provider(),
            model: lobo_proto::DEFAULT_CLOUD_MODEL.into(),
            cloud: get("LOBO_CLOUD"),
            min_mbps: get("LOBO_MIN_MBPS"),
            ctx: get("LOBO_CTX"),
            idle: get("LOBO_IDLE_MIN"),
            max_h: get("LOBO_MAX_HOURS"),
            vast_dph: get("LOBO_VAST_MAX_DPH"),
            cur,
            save: true,
            ..Default::default()
        };
        if s.cloud.is_empty() {
            s.cloud = "secure".into();
        }
        s
    }
    pub fn current(&self, k: &str) -> &str {
        self.cur.get(k).map(String::as_str).unwrap_or("")
    }
    pub fn runpod_key(&self) -> String {
        resolve_secret(self.current("RUNPOD_API_KEY"), &self.runpod)
    }
    pub fn vast_key(&self) -> String {
        resolve_secret(self.current("VASTAI_API_KEY"), &self.vast)
    }
    pub fn both_keys(&self) -> bool {
        !self.runpod_key().is_empty() && !self.vast_key().is_empty()
    }
    pub fn result(&self, new_key: &str) -> BTreeMap<String, String> {
        let mut out: BTreeMap<String, String> = [
            ("RUNPOD_API_KEY", self.runpod_key()),
            ("VASTAI_API_KEY", self.vast_key()),
            (
                "CF_TUNNEL_TOKEN",
                resolve_secret(self.current("CF_TUNNEL_TOKEN"), &self.tunnel),
            ),
            (
                "LOBO_API_KEY",
                if self.api_key == "new" {
                    new_key
                } else {
                    self.current("LOBO_API_KEY")
                }
                .into(),
            ),
            ("LOBO_DOMAIN", self.domain.trim().into()),
            ("LOBO_MODEL", self.model.clone()),
            ("LOBO_MIN_MBPS", self.min_mbps.trim().into()),
            ("LOBO_CTX", self.ctx.trim().into()),
            ("LOBO_IDLE_MIN", self.idle.trim().into()),
            ("LOBO_MAX_HOURS", self.max_h.trim().into()),
            ("LOBO_PROVIDER", String::new()),
            ("LOBO_CLOUD", String::new()),
            ("LOBO_VAST_MAX_DPH", String::new()),
        ]
        .into_iter()
        .map(|(k, v)| (k.into(), v))
        .collect();
        out.insert("LOBO_CONNECTION".into(), "ssh".into());
        if self.both_keys() {
            out.insert("LOBO_PROVIDER".into(), self.provider.clone());
        }
        if !out["RUNPOD_API_KEY"].is_empty() {
            out.insert("LOBO_CLOUD".into(), self.cloud.clone());
        }
        if !out["VASTAI_API_KEY"].is_empty() {
            out.insert("LOBO_VAST_MAX_DPH".into(), self.vast_dph.trim().into());
        }
        for (k, default) in [
            ("LOBO_MIN_MBPS", "100"),
            ("LOBO_CTX", "0"),
            ("LOBO_IDLE_MIN", "0"),
            ("LOBO_MAX_HOURS", "0"),
            ("LOBO_MODEL", lobo_proto::DEFAULT_CLOUD_MODEL),
            ("LOBO_CLOUD", "secure"),
            ("LOBO_PROVIDER", "runpod"),
            ("LOBO_VAST_MAX_DPH", "1.20"),
        ] {
            if out[k] == default || out[k] == "0" {
                out.insert(k.into(), String::new());
            }
        }
        out
    }
    pub fn summary(&self, color: bool) -> String {
        let r = self.result("(new key)");
        SUMMARY_ROWS
            .iter()
            .map(|(k, label)| {
                let v = if *k == "LOBO_API_KEY" && self.api_key == "new" {
                    "(a new key is generated on save)".into()
                } else if SECRETS.contains(k) {
                    mask(&r[*k])
                } else if r[*k].is_empty() {
                    if color {
                        "\x1b[38;2;139;148;158mdefault\x1b[0m"
                    } else {
                        "default"
                    }
                    .into()
                } else {
                    r[*k].clone()
                };
                format!("{label:<20} {v}")
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn cur(values: &[(&str, &str)]) -> BTreeMap<String, String> {
        values
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }
    #[test]
    fn result_keep_clear_and_defaults() {
        let old = cur(&[
            ("RUNPOD_API_KEY", "rp-old-key-123456"),
            ("VASTAI_API_KEY", "vast-old-key-1234"),
            ("LOBO_API_KEY", "sk-old"),
            ("CF_TUNNEL_TOKEN", "tok"),
            ("LOBO_DOMAIN", "lobo.x.cc"),
            ("LOBO_BUCKET_URL", "https://b"),
            ("LOBO_PROVIDER", "vast"),
            ("LOBO_MIN_MBPS", "150"),
        ]);
        let mut s = WizardState::new(old.clone());
        let r = s.result("sk-new");
        for k in [
            "RUNPOD_API_KEY",
            "LOBO_API_KEY",
            "LOBO_PROVIDER",
            "LOBO_MIN_MBPS",
        ] {
            assert_eq!(r[k], old[k]);
        }
        assert_eq!(r["LOBO_MODEL"], "");
        assert_eq!(r["LOBO_CLOUD"], "");
        s.vast = "-".into();
        s.api_key = "new".into();
        s.min_mbps = "100".into();
        s.ctx = "0".into();
        let r = s.result("sk-new");
        for k in [
            "VASTAI_API_KEY",
            "LOBO_PROVIDER",
            "LOBO_MIN_MBPS",
            "LOBO_CTX",
        ] {
            assert_eq!(r[k], "");
        }
        assert_eq!(r["LOBO_API_KEY"], "sk-new");
        s.runpod = " rp-new ".into();
        s.cloud = "community".into();
        let r = s.result("");
        assert_eq!(r["RUNPOD_API_KEY"], "rp-new");
        assert_eq!(r["LOBO_CLOUD"], "community");
    }
    #[test]
    fn new_state_migrates_saved_model_and_provider_without_local_controls() {
        let s = WizardState::new(BTreeMap::new());
        assert_eq!(
            (&*s.api_key, &*s.provider, &*s.model, &*s.cloud),
            ("new", "runpod", "qwen38-q6", "secure")
        );
        let key = new_api_key();
        assert!(key.starts_with("sk-"));
        assert_eq!(key.len(), 51);
        for model in ["q6", "q8", "qwen38-q6"] {
            let old = cur(&[
                ("LOBO_PROVIDER", "local"),
                ("VASTAI_API_KEY", "key"),
                ("LOBO_MODEL", model),
                ("LOBO_LOCAL_PORT", "9000"),
                ("LOBO_WEIGHTS_DIR", "/untouched"),
            ]);
            let s = WizardState::new(old);
            assert_eq!(s.provider, "vast");
            assert_eq!(s.model, "qwen38-q6");
            assert!(!s.result("").contains_key("LOBO_LOCAL_PORT"));
            assert!(!s.result("").contains_key("LOBO_WEIGHTS_DIR"));
        }
    }
    #[test]
    fn summary_masks_secrets_and_omits_retired_settings() {
        let s = WizardState::new(cur(&[
            ("RUNPOD_API_KEY", "rpa_SECRETSECRETSECRET"),
            ("CF_TUNNEL_TOKEN", "eyJTOKENTOKENTOKEN"),
            ("LOBO_WEIGHTS_DIR", "/untouched"),
            ("LOBO_LOCAL_PORT", "9000"),
        ]));
        let sum = s.summary(false);
        assert!(!sum.contains("SECRETSECRET"));
        assert!(!sum.contains("TOKENTOKEN"));
        assert!(sum.contains("rpa_…CRET"));
        assert!(!sum.contains("Weights folder"));
        assert!(!sum.contains("Local port"));
        assert_eq!(sum.lines().count(), SUMMARY_ROWS.len());
        for (line, (_, label)) in sum.lines().zip(SUMMARY_ROWS) {
            assert!(line.starts_with(label));
        }
    }
}
