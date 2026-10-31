use super::state::{WizardState, resolve_secret};
pub type Check = Box<dyn Fn(&str) -> Result<(), String> + Send + Sync>;
pub fn whole_number(min: i64) -> Check {
    Box::new(move |v| {
        let v = v.trim();
        if v.is_empty() || v == "0" || v.parse::<i64>().is_ok_and(|n| n >= min) {
            Ok(())
        } else {
            Err(format!("a whole number ≥ {min}, or empty for the default"))
        }
    })
}
pub fn positive_float(v: &str) -> Result<(), String> {
    let v = v.trim();
    if v.is_empty() || v.parse::<f64>().is_ok_and(|f| f > 0.0 && f.is_finite()) {
        Ok(())
    } else {
        Err("a price in $/h, e.g. 1.20".into())
    }
}
pub fn https_url(v: &str) -> Result<(), String> {
    let v = v.trim();
    if v.is_empty() {
        return Err("required".into());
    }
    if url::Url::parse(v)
        .is_ok_and(|u| ["http", "https"].contains(&u.scheme()) && u.host_str().is_some())
    {
        Ok(())
    } else {
        Err("a URL like https://pub-….r2.dev".into())
    }
}
pub fn hostname(v: &str) -> Result<(), String> {
    let v = v.trim();
    if v.is_empty() {
        Err("required".into())
    } else if v.contains('/') || v.contains(' ') || !v.contains('.') {
        Err("a bare hostname like lobo.example.com (no https://)".into())
    } else {
        Ok(())
    }
}
pub fn provider_options() -> Vec<(&'static str, &'static str)> {
    vec![
        ("RunPod", "runpod"),
        ("Vast.ai (cheapest verified host)", "vast"),
    ]
}
impl WizardState {
    pub fn provider_keys(&self, typed: &str) -> Result<(), String> {
        if self.runpod_key().is_empty()
            && resolve_secret(self.current("VASTAI_API_KEY"), typed).is_empty()
        {
            Err("set at least one provider key".into())
        } else {
            Ok(())
        }
    }
    pub fn tunnel_token(&self, typed: &str) -> Result<(), String> {
        if resolve_secret(self.current("CF_TUNNEL_TOKEN"), typed).is_empty() {
            Err("required: the pod serves the API through this tunnel".into())
        } else {
            Ok(())
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validators() {
        assert!(whole_number(512)("100").is_err());
        for s in ["", "0", "8192"] {
            assert!(whole_number(512)(s).is_ok());
        }
        assert!(whole_number(1)("abc").is_err());
        assert!(positive_float("0").is_err());
        assert!(positive_float("1.2").is_ok());
        assert!(positive_float("NaN").is_err());
        assert!(hostname("https://x.cc").is_err());
        assert!(hostname("lobo.x.cc").is_ok());
        assert!(https_url("pub.r2.dev").is_err());
        assert!(https_url("https://pub.r2.dev").is_ok());
    }
    #[test]
    fn cloud_keys_and_provider_options() {
        let s = WizardState::new(Default::default());
        assert!(s.provider_keys("").is_err());
        assert!(s.provider_keys("key").is_ok());
        assert!(s.tunnel_token("").is_err());
        assert!(s.tunnel_token("token").is_ok());
        assert_eq!(
            provider_options()
                .iter()
                .map(|(_, id)| *id)
                .collect::<Vec<_>>(),
            ["runpod", "vast"]
        );
    }
}
