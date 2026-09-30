use lobo_core::config::mask;
use std::collections::BTreeMap;

pub const SECRETS: [&str; 4] = [
    "RUNPOD_API_KEY",
    "VASTAI_API_KEY",
    "LOBO_API_KEY",
    "CF_TUNNEL_TOKEN",
];
pub const SUMMARY_ROWS: [(&str, &str); 15] = [
    ("RUNPOD_API_KEY", "RunPod key"),
    ("VASTAI_API_KEY", "Vast key"),
    ("LOBO_CONNECTION", "Connection"),
    ("LOBO_API_KEY", "LOBO API key"),
    ("LOBO_BUCKET_URL", "Bucket URL"),
    ("LOBO_PROVIDER", "Default provider"),
    ("LOBO_MIN_MBPS", "Min MB/s"),
    ("LOBO_MODEL", "Model"),
    ("LOBO_CTX", "Context"),
    ("LOBO_IDLE_MIN", "Idle minutes"),
    ("LOBO_MAX_HOURS", "Max hours"),
    ("LOBO_CLOUD", "RunPod cloud"),
    ("LOBO_VAST_MAX_DPH", "Vast max $/h"),
    ("LOBO_WEIGHTS_DIR", "Weights folder"),
    ("LOBO_LOCAL_PORT", "Local port"),
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
    pub bucket: String,
    pub provider: String,
    pub model: String,
    pub cloud: String,
    pub min_mbps: String,
    pub ctx: String,
    pub idle: String,
    pub max_h: String,
    pub vast_dph: String,
    pub weights: String,
    pub port: String,
    pub local_ok: bool,
    pub save: bool,
}
impl WizardState {
    pub fn new(cur: BTreeMap<String, String>, local_ok: bool) -> Self {
        let get = |k: &str| cur.get(k).cloned().unwrap_or_default();
        let mut s = Self {
            api_key: if get("LOBO_API_KEY").is_empty() {
                "new"
            } else {
                "keep"
            }
            .into(),
            domain: get("LOBO_DOMAIN"),
            bucket: get("LOBO_BUCKET_URL"),
            provider: get("LOBO_PROVIDER"),
            model: get("LOBO_MODEL"),
            cloud: get("LOBO_CLOUD"),
            min_mbps: get("LOBO_MIN_MBPS"),
            ctx: get("LOBO_CTX"),
            idle: get("LOBO_IDLE_MIN"),
            max_h: get("LOBO_MAX_HOURS"),
            vast_dph: get("LOBO_VAST_MAX_DPH"),
            weights: get("LOBO_WEIGHTS_DIR"),
            port: get("LOBO_LOCAL_PORT"),
            cur,
            local_ok,
            save: true,
            ..Default::default()
        };
        if s.provider == "local" && !local_ok {
            s.provider.clear();
        }
        if s.provider.is_empty() && local_ok && s.runpod_key().is_empty() && s.vast_key().is_empty()
        {
            s.provider = "local".into();
        }
        if s.provider.is_empty() {
            s.provider = "runpod".into();
        }
        if s.model.is_empty() {
            s.model = "q8".into();
        }
        if s.cloud.is_empty() {
            s.cloud = "community".into();
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
    pub fn is_local(&self) -> bool {
        self.local_ok && self.provider == "local"
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
            ("LOBO_BUCKET_URL", self.bucket.trim().into()),
            ("LOBO_MODEL", self.model.clone()),
            ("LOBO_MIN_MBPS", self.min_mbps.trim().into()),
            ("LOBO_CTX", self.ctx.trim().into()),
            ("LOBO_IDLE_MIN", self.idle.trim().into()),
            ("LOBO_MAX_HOURS", self.max_h.trim().into()),
            ("LOBO_PROVIDER", String::new()),
            ("LOBO_CLOUD", String::new()),
            ("LOBO_VAST_MAX_DPH", String::new()),
            ("LOBO_WEIGHTS_DIR", self.weights.trim().into()),
            ("LOBO_LOCAL_PORT", self.port.trim().into()),
        ]
        .into_iter()
        .map(|(k, v)| (k.into(), v))
        .collect();
        out.insert(
            "LOBO_CONNECTION".into(),
            lobo_core::config::Laptop::from_values(&self.cur)
                .connection_mode()
                .unwrap_or("ssh")
                .into(),
        );
        if self.is_local() || self.both_keys() && self.provider != "local" {
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
            ("LOBO_MODEL", "q8"),
            ("LOBO_CLOUD", "community"),
            ("LOBO_PROVIDER", "runpod"),
            ("LOBO_VAST_MAX_DPH", "1.20"),
            ("LOBO_LOCAL_PORT", "8931"),
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
        let mut s = WizardState::new(old.clone(), false);
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
        s.cloud = "secure".into();
        let r = s.result("");
        assert_eq!(r["RUNPOD_API_KEY"], "rp-new");
        assert_eq!(r["LOBO_CLOUD"], "secure");
    }
    #[test]
    fn new_state_fresh_file_and_local() {
        let s = WizardState::new(BTreeMap::new(), false);
        assert_eq!(
            (&*s.api_key, &*s.provider, &*s.model, &*s.cloud),
            ("new", "runpod", "q8", "community")
        );
        let key = new_api_key();
        assert!(key.starts_with("sk-"));
        assert_eq!(key.len(), 51);
        for (old, local, want) in [
            (vec![], true, "local"),
            (vec![], false, "runpod"),
            (vec![("RUNPOD_API_KEY", "rp")], true, "runpod"),
            (
                vec![("RUNPOD_API_KEY", "rp"), ("LOBO_PROVIDER", "local")],
                true,
                "local",
            ),
            (vec![("LOBO_PROVIDER", "local")], false, "runpod"),
        ] {
            assert_eq!(WizardState::new(cur(&old), local).provider, want);
        }
    }
    #[test]
    fn result_local() {
        let mut s = WizardState::new(cur(&[("LOBO_API_KEY", "sk")]), true);
        s.port = "8931".into();
        let r = s.result("");
        assert_eq!(r["LOBO_PROVIDER"], "local");
        assert_eq!(r["LOBO_LOCAL_PORT"], "");
        s.weights = " /Volumes/Extreme/_lobocode ".into();
        s.port = "9000".into();
        let r = s.result("");
        assert_eq!(r["LOBO_WEIGHTS_DIR"], "/Volumes/Extreme/_lobocode");
        assert_eq!(r["LOBO_LOCAL_PORT"], "9000");
        s.runpod = "rp".into();
        s.vast = "v".into();
        assert_eq!(s.result("")["LOBO_PROVIDER"], "local");
        s.provider = "runpod".into();
        s.vast = "-".into();
        s.port = "0".into();
        let r = s.result("");
        assert_eq!(r["LOBO_PROVIDER"], "");
        assert_eq!(r["LOBO_LOCAL_PORT"], "");
    }
    #[test]
    fn summary_masks_secrets_and_local_rows() {
        let s = WizardState::new(
            cur(&[
                ("RUNPOD_API_KEY", "rpa_SECRETSECRETSECRET"),
                ("CF_TUNNEL_TOKEN", "eyJTOKENTOKENTOKEN"),
                ("LOBO_WEIGHTS_DIR", "/Volumes/Extreme/_lobocode"),
                ("LOBO_LOCAL_PORT", "9000"),
                ("LOBO_PROVIDER", "local"),
            ]),
            true,
        );
        let sum = s.summary(false);
        assert!(!sum.contains("SECRETSECRET"));
        assert!(!sum.contains("TOKENTOKEN"));
        assert!(sum.contains("rpa_…CRET"));
        for want in [
            "Weights folder",
            "/Volumes/Extreme/_lobocode",
            "Local port",
            "9000",
            "local",
        ] {
            assert!(sum.contains(want));
        }
        assert_eq!(sum.lines().count(), 15);
        for (line, (_, label)) in sum.lines().zip(SUMMARY_ROWS) {
            assert!(line.starts_with(label));
        }
    }
}
