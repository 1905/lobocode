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
pub fn local_port(v: &str) -> Result<(), String> {
    lobo_core::config::parse_local_port(v.trim())
        .map(|_| ())
        .map_err(|_| "a port 1024-65534, or empty for 8931".into())
}
pub fn provider_options(local: bool) -> Vec<(&'static str, &'static str)> {
    let mut out = vec![];
    if local {
        out.push(("local (this Mac)", "local"));
    }
    out.extend([
        ("RunPod", "runpod"),
        ("Vast.ai (cheapest verified host)", "vast"),
    ]);
    out
}
impl WizardState {
    pub fn provider_keys(&self, typed: &str) -> Result<(), String> {
        if !self.is_local()
            && self.runpod_key().is_empty()
            && resolve_secret(self.current("VASTAI_API_KEY"), typed).is_empty()
        {
            Err("set at least one provider key".into())
        } else {
            Ok(())
        }
    }
    pub fn tunnel_token(&self, typed: &str) -> Result<(), String> {
        if !self.is_local() && resolve_secret(self.current("CF_TUNNEL_TOKEN"), typed).is_empty() {
            Err("required: the pod serves the API through this tunnel".into())
        } else {
            Ok(())
        }
    }
    pub fn cloud_only<'a>(
        &'a self,
        check: &'a dyn Fn(&str) -> Result<(), String>,
    ) -> impl Fn(&str) -> Result<(), String> + 'a {
        move |v| {
            if self.is_local() && v.trim().is_empty() {
                Ok(())
            } else {
                check(v)
            }
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
        for s in ["", "0", "9000"] {
            assert!(local_port(s).is_ok());
        }
        for s in ["80", "65535", "x"] {
            assert!(local_port(s).is_err());
        }
    }
    #[test]
    fn local_validation_and_options() {
        for (provider, local, bad) in [
            ("local", true, false),
            ("runpod", true, true),
            ("local", false, true),
        ] {
            let mut s = WizardState::new(Default::default(), local);
            s.provider = provider.into();
            for result in [
                s.provider_keys(""),
                s.tunnel_token(""),
                s.cloud_only(&hostname)(""),
                s.cloud_only(&https_url)(""),
            ] {
                assert_eq!(result.is_err(), bad);
            }
            assert!(s.cloud_only(&hostname)("https://x.cc").is_err());
            assert!(s.cloud_only(&https_url)("pub.r2.dev").is_err());
        }
        assert_eq!(provider_options(true).len(), 3);
        assert!(provider_options(false).iter().all(|(_, v)| *v != "local"));
    }
}
