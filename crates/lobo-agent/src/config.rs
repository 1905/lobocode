use crate::{Error, Result};
use chrono::{DateTime, Utc};
use std::time::Duration;

pub struct AgentConfig {
    pub lobo_api_key: String,
    pub cf_tunnel_token: String,
    pub model: String,
    pub model_url: String,
    pub model_fallback: String,
    pub provider: String,
    pub runpod_pod_id: String,
    pub runpod_api_key: String,
    pub vast_id: String,
    pub vast_api_key: String,
    pub model_ssh_key: String,
    pub model_host_key: String,
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
        let cf_tunnel_token = required("CF_TUNNEL_TOKEN")?;
        let model = required("LOBO_MODEL")?;
        let model_url = required("LOBO_MODEL_URL")?;
        let url = url::Url::parse(&model_url)
            .map_err(|_| Error::msg("config: LOBO_MODEL_URL: invalid URL"))?;
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
        let model_ssh_key = value("LOBO_MODEL_SSH_KEY");
        let model_host_key = value("LOBO_MODEL_SSH_HOSTKEY");
        if url.scheme() == "ssh" && (model_ssh_key.is_empty() || model_host_key.is_empty()) {
            return Err(Error::msg(
                "config: LOBO_MODEL_URL is ssh://: LOBO_MODEL_SSH_KEY and LOBO_MODEL_SSH_HOSTKEY are required",
            ));
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
            lobo_api_key,
            cf_tunnel_token,
            model,
            model_url,
            model_fallback: value("LOBO_MODEL_URL_FALLBACK"),
            provider,
            runpod_pod_id,
            runpod_api_key,
            vast_id,
            vast_api_key,
            model_ssh_key,
            model_host_key,
            boot_id: value("LOBO_BOOT_ID"),
            ctx: positive("LOBO_CTX", 8192)?,
            idle_min: positive("LOBO_IDLE_MIN", 30)?,
            dl_conns: usize::try_from(positive("LOBO_DL_CONNS", 1)?)
                .map_err(|_| Error::msg("config: LOBO_DL_CONNS: too large"))?,
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
            ("LOBO_MODEL_URL", "https://example.com/m"),
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
        assert_eq!((a.ctx, a.idle_min, a.dl_conns), (8192, 30, 1));
        assert_eq!(a.expires_at.hour(), 20);
        assert_eq!(a.boot_timeout, Duration::from_secs(2400));
    }
    #[test]
    fn min_mbps_default_100() {
        assert_eq!(load(&vars()).unwrap().min_mbps, 100);
    }
    #[test]
    fn load_agent_errors() {
        for (key, bad) in [
            ("LOBO_API_KEY", ""),
            ("LOBO_MODEL_URL", "bad"),
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
        v.insert("LOBO_MODEL_URL".into(), "ssh://lobo@host".into());
        assert!(load(&v).is_err());
        v.insert("LOBO_MODEL_SSH_KEY".into(), "ssh".into());
        v.insert("LOBO_MODEL_SSH_HOSTKEY".into(), "pin".into());
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
