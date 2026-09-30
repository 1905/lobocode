pub mod dotenv;
pub mod envfile;
pub use envfile::{save, set_env_value, values};
mod readiness;
pub use readiness::{readiness, validate_set};

use crate::{Error, Result};
use std::{
    collections::BTreeMap,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
};
#[derive(Debug, Clone, Default, PartialEq)]
pub struct R2Creds {
    pub account_id: String,
    pub access_key: String,
    pub secret_key: String,
    pub endpoint: String,
}
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Laptop {
    pub runpod_api_key: String,
    pub lobo_api_key: String,
    pub cf_tunnel_token: String,
    pub domain: String,
    pub connection: String,
    pub cloud_port: String,
    pub bucket_url: String,
    pub model_source: String,
    pub model_ssh_key_file: String,
    pub model_ssh_host_key: String,
    pub min_mbps: String,
    pub feesh_http_url: String,
    pub vast_api_key: String,
    pub vast_max_dph: String,
    pub pod_image: String,
    pub provider: String,
    pub model: String,
    pub cloud: String,
    pub ctx: String,
    pub idle_min: String,
    pub max_hours: String,
    pub weights_dir: String,
    pub local_port: String,
    pub r2: R2Creds,
}
impl Laptop {
    pub fn from_values(m: &BTreeMap<String, String>) -> Self {
        let get = |k: &str| m.get(k).cloned().unwrap_or_default();
        Self {
            runpod_api_key: get("RUNPOD_API_KEY"),
            lobo_api_key: get("LOBO_API_KEY"),
            cf_tunnel_token: get("CF_TUNNEL_TOKEN"),
            domain: get("LOBO_DOMAIN"),
            connection: get("LOBO_CONNECTION"),
            cloud_port: get("LOBO_CLOUD_PORT"),
            bucket_url: get("LOBO_BUCKET_URL"),
            model_source: get("LOBO_MODEL_SOURCE"),
            model_ssh_key_file: get("LOBO_MODEL_SSH_KEY_FILE"),
            model_ssh_host_key: get("LOBO_MODEL_SSH_HOSTKEY"),
            min_mbps: get("LOBO_MIN_MBPS"),
            feesh_http_url: get("LOBO_FEESH_HTTP_URL"),
            vast_api_key: get("VASTAI_API_KEY"),
            vast_max_dph: get("LOBO_VAST_MAX_DPH"),
            pod_image: get("LOBO_POD_IMAGE"),
            provider: get("LOBO_PROVIDER"),
            model: get("LOBO_MODEL"),
            cloud: get("LOBO_CLOUD"),
            ctx: get("LOBO_CTX"),
            idle_min: get("LOBO_IDLE_MIN"),
            max_hours: get("LOBO_MAX_HOURS"),
            weights_dir: get("LOBO_WEIGHTS_DIR"),
            local_port: get("LOBO_LOCAL_PORT"),
            r2: R2Creds {
                account_id: get("R2_ACCOUNT_ID"),
                access_key: get("R2_ACCESS_KEY"),
                secret_key: get("R2_SECRET_KEY"),
                endpoint: get("R2_ENDPOINT"),
            },
        }
    }
    pub fn require_cloud(&self) -> Result<()> {
        self.connection_mode()?;
        let missing: Vec<_> = [
            (
                "CF_TUNNEL_TOKEN",
                if self.uses_ssh() {
                    "unused"
                } else {
                    &self.cf_tunnel_token
                },
            ),
            (
                "LOBO_DOMAIN",
                if self.uses_ssh() {
                    "unused"
                } else {
                    &self.domain
                },
            ),
            ("LOBO_BUCKET_URL", &self.bucket_url),
        ]
        .into_iter()
        .filter(|(_, v)| v.is_empty())
        .map(|(k, _)| k)
        .collect();
        if !missing.is_empty() {
            return Err(Error::Config(format!(
                "config: cloud needs {}",
                missing.join(", ")
            )));
        }
        self.require_bucket()?;
        self.require_provider_key()
    }
    pub fn connection_mode(&self) -> Result<&str> {
        match self.connection.as_str() {
            "ssh" => Ok("ssh"),
            "cloudflare" => Ok("cloudflare"),
            "" if !self.domain.is_empty() && !self.cf_tunnel_token.is_empty() => Ok("cloudflare"),
            "" => Ok("ssh"),
            _ => Err(Error::Config(
                "LOBO_CONNECTION: want ssh or cloudflare".into(),
            )),
        }
    }
    pub fn uses_ssh(&self) -> bool {
        matches!(self.connection_mode(), Ok("ssh"))
    }
    pub fn cloud_port(&self) -> u16 {
        if self.cloud_port.is_empty() || self.cloud_port == "0" {
            8933
        } else {
            parse_local_port(&self.cloud_port).unwrap_or(8933)
        }
    }
    pub fn cloud_url(&self) -> String {
        if self.uses_ssh() {
            format!("http://127.0.0.1:{}/v1", self.cloud_port())
        } else {
            format!("https://{}/v1", self.domain)
        }
    }
    pub fn require_provider_key(&self) -> Result<()> {
        if self.runpod_api_key.is_empty() && self.vast_api_key.is_empty() {
            Err(Error::Config(
                "config: set RUNPOD_API_KEY or VASTAI_API_KEY".into(),
            ))
        } else {
            Ok(())
        }
    }
    pub fn require_bucket(&self) -> Result<()> {
        if self.bucket_url.is_empty() {
            return Err(Error::Config("config: set LOBO_BUCKET_URL".into()));
        }
        if !valid_url(&self.bucket_url) {
            return Err(Error::Config(format!(
                "config: LOBO_BUCKET_URL: want a URL, got {:?}",
                self.bucket_url
            )));
        }
        Ok(())
    }
    pub fn require_r2(&self) -> Result<()> {
        let mut bad = Vec::new();
        for (k, v) in [
            ("R2_ACCOUNT_ID", &self.r2.account_id),
            ("R2_ACCESS_KEY", &self.r2.access_key),
            ("R2_SECRET_KEY", &self.r2.secret_key),
            ("R2_ENDPOINT", &self.r2.endpoint),
        ] {
            if v.is_empty() {
                bad.push(format!("{k}: required"));
            } else if k == "R2_ENDPOINT" && !valid_url(v) {
                bad.push(format!("{k}: url"));
            }
        }
        if bad.is_empty() {
            Ok(())
        } else {
            Err(Error::Config(format!("config: {}", bad.join("; "))))
        }
    }
    pub fn secret_values(&self) -> BTreeMap<String, String> {
        use base64::Engine;
        let mut m: BTreeMap<_, _> = [
            ("RUNPOD_API_KEY", &self.runpod_api_key),
            ("LOBO_API_KEY", &self.lobo_api_key),
            ("CF_TUNNEL_TOKEN", &self.cf_tunnel_token),
            ("R2_ACCESS_KEY", &self.r2.access_key),
            ("R2_SECRET_KEY", &self.r2.secret_key),
            ("VASTAI_API_KEY", &self.vast_api_key),
        ]
        .into_iter()
        .map(|(k, v)| (k.into(), v.clone()))
        .collect();
        if !self.model_ssh_key_file.is_empty()
            && let Ok(bytes) = std::fs::read(&self.model_ssh_key_file)
        {
            m.insert(
                "LOBO_MODEL_SSH_KEY_FILE".into(),
                String::from_utf8_lossy(&bytes).into_owned(),
            );
            m.insert(
                "LOBO_MODEL_SSH_KEY_FILE (base64)".into(),
                base64::engine::general_purpose::STANDARD.encode(bytes),
            );
        }
        m
    }
    pub fn defaults(&self) -> Result<Defaults> {
        let (d, bad) = defaults_partial(self);
        if bad.is_empty() {
            Ok(d)
        } else {
            Err(Error::Defaults(bad))
        }
    }
    pub fn weights(&self) -> PathBuf {
        self.weights_with_home(
            &std::env::var_os("HOME")
                .map(PathBuf::from)
                .unwrap_or_default(),
        )
    }
    pub fn weights_with_home(&self, home: &Path) -> PathBuf {
        let w = &self.weights_dir;
        if w.is_empty() {
            home.join("Library/Application Support/lobo/weights")
        } else if w == "~" || w.starts_with("~/") {
            home.join(w.trim_start_matches('~').trim_start_matches('/'))
        } else {
            w.into()
        }
    }
    pub fn port(&self) -> u16 {
        parse_local_port(&self.local_port).unwrap_or(DEFAULT_LOCAL_PORT)
    }
    pub fn providers(&self) -> Vec<String> {
        let mut out = Vec::new();
        if !self.runpod_api_key.is_empty() {
            out.push("runpod".into());
        }
        if !self.vast_api_key.is_empty() {
            out.push("vast".into());
        }
        out
    }
    pub fn default_provider(&self) -> String {
        if self.provider == "local" {
            return "local".into();
        }
        let keyed = self.providers();
        if keyed.contains(&self.provider) {
            self.provider.clone()
        } else {
            keyed.first().cloned().unwrap_or_else(|| "runpod".into())
        }
    }
}
fn valid_url(value: &str) -> bool {
    url::Url::parse(value).is_ok()
}
pub fn load_laptop(path: &Path) -> Result<Laptop> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| Error::Config(format!("read {}: {e}", path.display())))?;
    let m =
        dotenv::parse(&text).map_err(|e| Error::Config(format!("read {}: {e}", path.display())))?;
    let l = Laptop::from_values(&m);
    let mut bad = Vec::new();
    if l.lobo_api_key.is_empty() {
        bad.push("LOBO_API_KEY: required");
    }
    if !l.bucket_url.is_empty() && !valid_url(&l.bucket_url) {
        bad.push("LOBO_BUCKET_URL: url");
    }
    if !bad.is_empty() {
        return Err(Error::Config(format!("config: {}", bad.join("; "))));
    }
    if l.model_source.starts_with("ssh://")
        && (l.model_ssh_key_file.is_empty() || l.model_ssh_host_key.is_empty())
    {
        return Err(Error::Config("config: LOBO_MODEL_SOURCE is ssh://: LOBO_MODEL_SSH_KEY_FILE and LOBO_MODEL_SSH_HOSTKEY are required".into()));
    }
    if !l.model_source.is_empty() && l.model_source != "r2" && !l.model_source.starts_with("ssh://")
    {
        return Err(Error::Config(format!(
            "config: LOBO_MODEL_SOURCE: want r2 or ssh://user@host:port, got {:?}",
            l.model_source
        )));
    }
    Ok(l)
}
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Defaults {
    pub provider: String,
    pub model: String,
    pub cloud: String,
    pub ctx: i64,
    pub idle_min: i64,
    pub max_hours: i64,
    pub min_mbps: i64,
    pub vast_max_dph: f64,
}
pub fn defaults_partial(l: &Laptop) -> (Defaults, BTreeMap<String, String>) {
    let mut bad = BTreeMap::new();
    if let Err(e) = l.connection_mode() {
        bad.insert("LOBO_CONNECTION".into(), e.to_string());
    }
    if let Err(e) = parse_local_port(&l.cloud_port) {
        bad.insert("LOBO_CLOUD_PORT".into(), e.to_string());
    }
    if l.uses_ssh() && l.port().abs_diff(l.cloud_port()) < 2 {
        bad.insert(
            "LOBO_CLOUD_PORT".into(),
            "cloud and local port pairs overlap".into(),
        );
    }
    let mut one_of = |key: &str, v: &str, choices: &[&str]| {
        if v.is_empty() || choices.contains(&v) {
            v.to_owned()
        } else {
            bad.insert(
                key.into(),
                format!("want {}, got {v:?}", choices.join(" or ")),
            );
            String::new()
        }
    };
    let mut d = Defaults {
        provider: one_of("LOBO_PROVIDER", &l.provider, &["runpod", "vast", "local"]),
        model: one_of("LOBO_MODEL", &l.model, &["q8", "q6"]),
        cloud: one_of("LOBO_CLOUD", &l.cloud, &["secure", "community"]),
        ..Default::default()
    };
    let mut num = |key: &str, v: &str, min: i64| {
        if v.is_empty() || v == "0" {
            return 0;
        }
        if let Ok(n) = v.parse::<i64>()
            && n >= min
        {
            return n;
        }
        bad.insert(
            key.into(),
            format!("want a whole number ≥ {min} (or empty), got {v:?}"),
        );
        0
    };
    d.ctx = num("LOBO_CTX", &l.ctx, 512);
    d.idle_min = num("LOBO_IDLE_MIN", &l.idle_min, 1);
    d.max_hours = num("LOBO_MAX_HOURS", &l.max_hours, 1);
    d.min_mbps = num("LOBO_MIN_MBPS", &l.min_mbps, 1);
    if !l.vast_max_dph.is_empty() {
        match l.vast_max_dph.parse::<f64>() {
            Ok(n) if n > 0.0 => d.vast_max_dph = n,
            _ => {
                bad.insert(
                    "LOBO_VAST_MAX_DPH".into(),
                    format!("want $/h > 0, got {:?}", l.vast_max_dph),
                );
            }
        }
    }
    if let Err(e) = parse_local_port(&l.local_port) {
        bad.insert("LOBO_LOCAL_PORT".into(), e.to_string());
    }
    (d, bad)
}
pub const DEFAULT_LOCAL_PORT: u16 = 8931;
pub fn parse_local_port(v: &str) -> Result<u16> {
    if v.is_empty() || v == "0" {
        return Ok(DEFAULT_LOCAL_PORT);
    }
    v.parse::<u16>()
        .ok()
        .filter(|n| (1024..=65534).contains(n))
        .ok_or_else(|| Error::Config(format!("want a port 1024-65534 (or empty), got {v:?}")))
}
pub fn default_path() -> PathBuf {
    default_path_from(
        std::env::var("XDG_CONFIG_HOME").ok().as_deref(),
        std::env::var_os("HOME").as_deref().map(Path::new),
    )
}
pub fn default_path_from(xdg_config_home: Option<&str>, home: Option<&Path>) -> PathBuf {
    if let Some(x) = xdg_config_home.filter(|v| !v.is_empty()) {
        Path::new(x).join("lobo/config.env")
    } else {
        home.unwrap_or(Path::new(""))
            .join(".config/lobo/config.env")
    }
}
pub fn loose_mode(path: &Path) -> bool {
    std::fs::metadata(path).is_ok_and(|m| m.permissions().mode() & 0o077 != 0)
}

pub mod show;
#[cfg(test)]
mod tests;

pub use envfile::{HEADER, LAYOUT, LayoutGroup};
pub use show::{PLAIN_KEYS, mask, masked, show};
