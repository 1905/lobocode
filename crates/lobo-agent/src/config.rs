use crate::{Error, Result};
use chrono::{DateTime, Utc};
use std::time::Duration;

pub struct AgentConfig {
    pub image_model: bool,
    pub connection: String,
    pub lobo_api_key: String,
    pub cf_tunnel_token: String,
    pub model: String,
    pub model_base_url: String,
    pub provider: String,
    pub runpod_pod_id: String,
    pub runpod_api_key: String,
    pub vast_id: String,
    pub vast_api_key: String,
    pub boot_id: String,
    pub ctx: i64,
    pub idle_min: i64,
    pub dl_conns: usize,
    pub min_mbps: i64,
    pub expires_at: DateTime<Utc>,
    pub boot_timeout: Duration,
}
impl AgentConfig {
    pub fn from_vars(get: &dyn Fn(&str) -> Option<String>) -> Result<Self> {
        let value = |key: &str| get(key).unwrap_or_default();
        let required = |key: &str| {
            let v = value(key);
            if v.is_empty() {
                Err(Error::msg(format!("config: {key}: required")))
            } else {
                Ok(v)
            }
        };
        let positive = |key: &str, default: i64| -> Result<i64> {
            let v = value(key);
            if v.is_empty() {
                return Ok(default);
            }
            v.parse::<i64>()
                .ok()
                .filter(|n| *n > 0)
                .ok_or_else(|| Error::msg(format!("config: {key}: want positive int, got {v:?}")))
        };
        let lobo_api_key = required("LOBO_API_KEY")?;
        let connection = value("LOBO_CONNECTION");
        if !matches!(connection.as_str(), "" | "ssh" | "cloudflare") {
            return Err(Error::msg(
                "config: LOBO_CONNECTION: want ssh or cloudflare",
            ));
        }
        let cf_tunnel_token = if connection == "ssh" {
            String::new()
        } else {
            required("CF_TUNNEL_TOKEN")?
        };
        let model = required("LOBO_MODEL")?;
        let image_model = value("LOBO_IMAGE_MODEL") == "1";
        let model_base_url = if image_model {
            String::new()
        } else {
            required("LOBO_MODEL_BASE_URL")?
        };
        if !image_model {
            url::Url::parse(&model_base_url)
                .map_err(|_| Error::msg("config: LOBO_MODEL_BASE_URL: invalid URL"))?;
        }
        let dl_conns = usize::try_from(positive("LOBO_DL_CONNS", 1)?)
            .map_err(|_| Error::msg("config: LOBO_DL_CONNS: too large"))?;
        // Chunk objects need the multi-stream path; one stream reads a single URL to EOF.
        if !image_model && dl_conns < 2 {
            return Err(Error::msg(format!(
                "config: LOBO_DL_CONNS: want at least 2, got {dl_conns}"
            )));
        }
        let mut provider = value("LOBO_PROVIDER");
        if provider.is_empty() {
            provider = "runpod".into();
        }
        let runpod_pod_id = value("RUNPOD_POD_ID");
        let runpod_api_key = value("RUNPOD_API_KEY");
        let vast_id = value("CONTAINER_ID");
        let vast_api_key = value("CONTAINER_API_KEY");
        match provider.as_str() {
            "runpod" if runpod_pod_id.is_empty() || runpod_api_key.is_empty() => {
                return Err(Error::msg(
                    "config: LOBO_PROVIDER=runpod needs RUNPOD_POD_ID and RUNPOD_API_KEY (RunPod injects them)",
                ));
            }
            "vast" if vast_id.is_empty() || vast_api_key.is_empty() => {
                return Err(Error::msg(
                    "config: LOBO_PROVIDER=vast needs CONTAINER_ID and CONTAINER_API_KEY (Vast injects them)",
                ));
            }
            "runpod" | "vast" => {}
            _ => {
                return Err(Error::msg(format!(
                    "config: LOBO_PROVIDER: want runpod or vast, got {provider:?}"
                )));
            }
        }
        let exp = value("LOBO_EXPIRES_AT");
        let expires_at = DateTime::parse_from_rfc3339(&exp)
            .map_err(|_| {
                Error::msg(format!(
                    "config: LOBO_EXPIRES_AT: want RFC3339, got {exp:?}"
                ))
            })?
            .with_timezone(&Utc);
        let timeout = value("LOBO_BOOT_TIMEOUT");
        let boot_timeout = if timeout.is_empty() {
            Duration::from_secs(2400)
        } else {
            go_duration(&timeout).ok_or_else(|| {
                Error::msg(format!(
                    "config: LOBO_BOOT_TIMEOUT: invalid duration {timeout:?}"
                ))
            })?
        };
        Ok(Self {
            image_model,
            connection,
            lobo_api_key,
            cf_tunnel_token,
            model,
            model_base_url,
            provider,
            runpod_pod_id,
            runpod_api_key,
            vast_id,
            vast_api_key,
            boot_id: value("LOBO_BOOT_ID"),
            ctx: positive("LOBO_CTX", 8192)?,
            idle_min: positive("LOBO_IDLE_MIN", 30)?,
            dl_conns,
            min_mbps: positive("LOBO_MIN_MBPS", 100)?,
            expires_at,
            boot_timeout,
        })
    }
}
// Go accepts compound durations and fractions. Nonpositive deadlines expire immediately.
fn go_duration(text: &str) -> Option<Duration> {
    let negative = text.starts_with('-');
    let mut rest = text.strip_prefix(['+', '-']).unwrap_or(text);
    if rest == "0" {
        return Some(Duration::ZERO);
    }
    let mut seconds = 0.0;
    let mut terms = 0;
    while !rest.is_empty() {
        let n = rest
            .find(|c: char| !c.is_ascii_digit() && c != '.')
            .unwrap_or(rest.len());
        let amount = rest[..n].parse::<f64>().ok()?;
        rest = &rest[n..];
        let (unit, scale) = [
            ("ms", 0.001),
            ("us", 0.000001),
            ("µs", 0.000001),
            ("μs", 0.000001),
            ("ns", 0.000000001),
            ("h", 3600.0),
            ("m", 60.0),
            ("s", 1.0),
        ]
        .into_iter()
        .find(|(u, _)| rest.starts_with(u))?;
        rest = &rest[unit.len()..];
        seconds += amount * scale;
        terms += 1;
    }
    if terms == 0 || seconds > i64::MAX as f64 / 1e9 {
        return None;
    }
    Duration::try_from_secs_f64(if negative { 0.0 } else { seconds }).ok()
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    fn vars() -> BTreeMap<String, String> {
        [
            ("LOBO_API_KEY", "api"),
            ("CF_TUNNEL_TOKEN", "tunnel"),
            ("LOBO_MODEL", "q8"),
            ("LOBO_MODEL_BASE_URL", "https://models.example"),
            ("LOBO_DL_CONNS", "32"),
            ("RUNPOD_POD_ID", "pod"),
            ("RUNPOD_API_KEY", "key"),
            ("LOBO_EXPIRES_AT", "2026-09-29T20:00:00Z"),
        ]
        .into_iter()
        .map(|(k, v)| (k.into(), v.into()))
        .collect()
    }
    fn load(v: &BTreeMap<String, String>) -> Result<AgentConfig> {
        AgentConfig::from_vars(&|k| v.get(k).cloned())
    }
    #[test]
    fn load_agent() {
        use chrono::Timelike;
        let a = load(&vars()).unwrap();
        assert_eq!((a.ctx, a.idle_min, a.dl_conns), (8192, 30, 32));
        assert_eq!(a.expires_at.hour(), 20);
        assert_eq!(a.boot_timeout, Duration::from_secs(2400));
    }
    #[test]
    fn min_mbps_default_100() {
        assert_eq!(load(&vars()).unwrap().min_mbps, 100);
    }
    #[test]
    fn ssh_needs_no_cloudflare_account() {
        let mut v = vars();
        v.remove("CF_TUNNEL_TOKEN");
        assert!(load(&v).is_err());
        v.insert("LOBO_CONNECTION".into(), "ssh".into());
        assert!(load(&v).unwrap().cf_tunnel_token.is_empty());
        v.insert("LOBO_CONNECTION".into(), "plain-http".into());
        assert!(load(&v).is_err());
    }
    #[test]
    fn image_mode_needs_no_source_and_ignores_obsolete_settings() {
        for connection in ["ssh", "cloudflare"] {
            let mut v = vars();
            v.insert("LOBO_IMAGE_MODEL".into(), "1".into());
            v.insert("LOBO_CONNECTION".into(), connection.into());
            v.insert("LOBO_DL_CONNS".into(), "1".into());
            v.remove("LOBO_MODEL_BASE_URL");
            if connection == "ssh" {
                v.remove("CF_TUNNEL_TOKEN");
            }
            let cfg = load(&v).unwrap();
            assert!(cfg.image_model);
            assert!(cfg.model_base_url.is_empty());
            v.insert("LOBO_MODEL_BASE_URL".into(), "invalid obsolete URL".into());
            assert!(load(&v).unwrap().model_base_url.is_empty());
        }
    }
    #[test]
    fn download_mode_needs_base_url_and_two_streams() {
        let err = |v: &BTreeMap<String, String>| load(v).err().map(|e| e.to_string());
        let mut v = vars();
        v.remove("LOBO_MODEL_BASE_URL");
        assert!(err(&v).unwrap().contains("LOBO_MODEL_BASE_URL: required"));
        v.insert("LOBO_MODEL_BASE_URL".into(), "not a url".into());
        assert!(
            err(&v)
                .unwrap()
                .contains("LOBO_MODEL_BASE_URL: invalid URL")
        );
        let mut v = vars();
        v.insert("LOBO_DL_CONNS".into(), "1".into());
        assert!(err(&v).unwrap().contains("LOBO_DL_CONNS: want at least 2"));
        v.remove("LOBO_DL_CONNS");
        assert!(err(&v).unwrap().contains("LOBO_DL_CONNS: want at least 2"));
        v.insert("LOBO_DL_CONNS".into(), "32".into());
        assert_eq!(load(&v).unwrap().model_base_url, "https://models.example");
    }
    #[test]
    fn load_agent_errors() {
        for (key, bad) in [
            ("LOBO_API_KEY", ""),
            ("LOBO_MODEL_BASE_URL", "bad"),
            ("LOBO_CTX", "0"),
            ("LOBO_IDLE_MIN", "-1"),
            ("LOBO_DL_CONNS", "bad"),
            ("LOBO_MIN_MBPS", "0"),
            ("LOBO_EXPIRES_AT", "bad"),
            ("LOBO_BOOT_TIMEOUT", "bad"),
        ] {
            let mut v = vars();
            v.insert(key.into(), bad.into());
            let Err(e) = load(&v) else {
                panic!("accepted {key}")
            };
            assert!(e.to_string().contains(key), "{e}");
        }
    }
    #[test]
    fn load_agent_per_provider() {
        let mut v = vars();
        v.insert("LOBO_PROVIDER".into(), "vast".into());
        assert!(load(&v).is_err());
        v.insert("CONTAINER_ID".into(), "1".into());
        v.insert("CONTAINER_API_KEY".into(), "vkey".into());
        v.remove("RUNPOD_API_KEY");
        assert!(load(&v).is_ok());
        v.insert("LOBO_PROVIDER".into(), "nope".into());
        assert!(load(&v).is_err());
        v.insert("LOBO_PROVIDER".into(), "runpod".into());
        assert!(load(&v).is_err());
        v.insert("RUNPOD_API_KEY".into(), "key".into());
        assert!(load(&v).is_ok());
    }
    #[test]
    fn boot_timeout_go_syntax() {
        for (v, ms) in [
            ("40m", 2400000),
            ("1h30m", 5400000),
            ("90s", 90000),
            ("1.5s250ms", 1750),
            ("-1s", 0),
            ("0", 0),
        ] {
            assert_eq!(go_duration(v), Some(Duration::from_millis(ms)));
        }
        for v in ["bad", "1", "1d", "", "++1s", "1e3s", "99999999999999h"] {
            assert_eq!(go_duration(v), None, "{v}");
        }
    }
}
