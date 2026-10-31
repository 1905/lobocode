use super::{Error, Laptop, Result, UpOpts};
use std::{path::Path, time::Duration};
pub fn resolve_up(cfg: &Laptop, cfg_path: &Path, req: &lobo_proto::UpRequest) -> Result<UpOpts> {
    let mut o = UpOpts {
        provider: req.provider.clone().unwrap_or_default(),
        model: req.model.clone().unwrap_or_default(),
        ctx: req.ctx.unwrap_or_default(),
        source: req.source.clone().unwrap_or_default(),
        cloud: req.cloud.clone().unwrap_or_else(|| "secure".into()),
        ..Default::default()
    };
    if !matches!(o.cloud.as_str(), "secure" | "community") {
        return Err(Error::Config(format!(
            "--cloud: want secure or community, got {:?}",
            o.cloud
        )));
    }
    apply_defaults(
        &mut o,
        cfg,
        &|flag| match flag {
            "provider" => req.provider.is_some(),
            "model" => req.model.is_some(),
            "ctx" => req.ctx.is_some(),
            "cloud" => req.cloud.is_some(),
            _ => false,
        },
        cfg_path,
    )?;
    check_target(cfg, &o.provider)?;
    Ok(o)
}
pub fn check_target(cfg: &Laptop, provider: &str) -> Result<()> {
    if !matches!(provider, "runpod" | "vast") {
        return Err(Error::Config(format!(
            "unsupported provider {provider:?}; use runpod or vast"
        )));
    }
    cfg.require_cloud()
}
pub fn check_providers(cfg: &Laptop) -> Result<()> {
    cfg.require_provider_key()
}
pub fn check_release(cfg: &Laptop) -> Result<()> {
    cfg.require_r2()?;
    cfg.require_bucket()
}
pub fn apply_defaults(
    o: &mut UpOpts,
    cfg: &Laptop,
    set: &dyn Fn(&str) -> bool,
    cfg_path: &Path,
) -> Result<()> {
    let (d, bad) = crate::config::defaults_partial(cfg);
    for key in bad.keys() {
        let flag = match key.as_str() {
            "LOBO_PROVIDER" => "provider",
            "LOBO_MODEL" => "model",
            "LOBO_CLOUD" => "cloud",
            "LOBO_CTX" => "ctx",
            "LOBO_IDLE_MIN" => "idle-min",
            "LOBO_MAX_HOURS" => "max-life",
            "LOBO_MIN_MBPS" => "min-mbps",
            _ => "",
        };
        if flag.is_empty() || !set(flag) {
            return Err(Error::Config(format!(
                "{} (in {}; fix it with `lobo config` or by hand)",
                Error::Defaults(bad),
                cfg_path.display()
            )));
        }
    }
    if !set("provider") {
        o.provider = cfg.default_provider();
    }
    if !set("model") && !d.model.is_empty() {
        o.model = d.model;
    }
    if !set("ctx") && d.ctx > 0 {
        o.ctx = d.ctx;
    }
    if !set("idle-min") && d.idle_min > 0 {
        o.idle_min = d.idle_min;
    }
    if !set("max-life") && d.max_hours > 0 {
        o.max_life = Duration::from_secs(
            (d.max_hours as u64)
                .checked_mul(3600)
                .ok_or_else(|| Error::Config("LOBO_MAX_HOURS: duration overflow".into()))?,
        );
    }
    if !set("min-mbps") && d.min_mbps > 0 {
        o.min_mbps = d.min_mbps;
    }
    if !set("cloud") && !d.cloud.is_empty() {
        o.cloud = d.cloud;
    }
    if !o.model.is_empty() {
        lobo_proto::catalog::get(&o.model).map_err(|e| Error::Config(e.to_string()))?;
    }
    if cfg.providers().contains(&o.provider) {
        return Ok(());
    }
    let key = match o.provider.as_str() {
        "runpod" => "RUNPOD_API_KEY",
        "vast" => "VASTAI_API_KEY",
        _ => {
            return Err(Error::Config(format!(
                "--provider: want runpod or vast, got {:?}",
                o.provider
            )));
        }
    };
    Err(Error::Config(format!(
        "no {key} in {}. Run `lobo config` to add it",
        cfg_path.display()
    )))
}
#[cfg(test)]
mod tests {
    use super::*;
    fn cloud() -> Laptop {
        Laptop {
            runpod_api_key: "key".into(),
            ..Default::default()
        }
    }
    #[test]
    fn request_defaults_and_explicit_values() {
        let cfg = Laptop {
            model: "qwen38-q6".into(),
            ctx: "8192".into(),
            idle_min: "10".into(),
            max_hours: "2".into(),
            min_mbps: "150".into(),
            ..cloud()
        };
        let defaults = resolve_up(&cfg, Path::new("/config"), &Default::default()).unwrap();
        assert_eq!(
            (
                defaults.provider.as_str(),
                defaults.model.as_str(),
                defaults.cloud.as_str()
            ),
            ("runpod", "qwen38-q6", "community")
        );
        assert_eq!(
            (defaults.ctx, defaults.idle_min, defaults.min_mbps),
            (8192, 10, 150)
        );
        assert_eq!(defaults.max_life, Duration::from_secs(7200));
        let cfg = Laptop {
            ctx: "invalid".into(),
            ..cfg
        };
        let request = lobo_proto::UpRequest {
            model: Some("qwen38-q6".into()),
            ctx: Some(4096),
            ..Default::default()
        };
        assert_eq!(
            resolve_up(&cfg, Path::new("/config"), &request)
                .unwrap()
                .ctx,
            4096
        );
        assert!(
            resolve_up(&cfg, Path::new("/config"), &Default::default())
                .unwrap_err()
                .to_string()
                .contains("LOBO_CTX")
        );
    }
    #[test]
    fn explicit_retired_models_and_providers_fail_before_start() {
        for model in ["q6", "q8", "missing"] {
            let request = lobo_proto::UpRequest {
                model: Some(model.into()),
                ..Default::default()
            };
            assert!(resolve_up(&cloud(), Path::new("/config"), &request).is_err());
        }
        for provider in ["local", "missing"] {
            let request = lobo_proto::UpRequest {
                provider: Some(provider.into()),
                ..Default::default()
            };
            assert!(resolve_up(&cloud(), Path::new("/config"), &request).is_err());
            assert!(check_target(&cloud(), provider).is_err());
        }
        let request = lobo_proto::UpRequest {
            provider: Some("vast".into()),
            ..Default::default()
        };
        assert!(
            resolve_up(&cloud(), Path::new("/config"), &request)
                .unwrap_err()
                .to_string()
                .contains("VASTAI_API_KEY")
        );
    }
    #[test]
    fn configured_cloud_keys_are_required() {
        for (runpod, vast, want) in [
            (false, false, false),
            (true, false, true),
            (false, true, true),
            (true, true, true),
        ] {
            let cfg = Laptop {
                runpod_api_key: if runpod { "r" } else { "" }.into(),
                vast_api_key: if vast { "v" } else { "" }.into(),
                ..Default::default()
            };
            assert_eq!(check_providers(&cfg).is_ok(), want);
        }
        assert!(check_target(&Laptop::default(), "runpod").is_err());
        assert!(check_target(&cloud(), "runpod").is_ok());
    }
    #[test]
    fn cloud_mode_needs_tunnel_and_domain() {
        let cfg = Laptop {
            connection: "cloudflare".into(),
            ..cloud()
        };
        assert!(
            check_target(&cfg, "runpod")
                .unwrap_err()
                .to_string()
                .contains("CF_TUNNEL_TOKEN")
        );
    }
    #[test]
    fn release_still_requires_bucket_credentials() {
        for (r2, bucket, want) in [
            (false, "https://b", "R2_"),
            (true, "", "LOBO_BUCKET_URL"),
            (true, "https://b", ""),
        ] {
            let mut cfg = Laptop {
                bucket_url: bucket.into(),
                ..Default::default()
            };
            if r2 {
                cfg.r2 = crate::config::R2Creds {
                    account_id: "a".into(),
                    access_key: "k".into(),
                    secret_key: "s".into(),
                    endpoint: "https://r2.test".into(),
                };
            }
            let result = check_release(&cfg);
            if want.is_empty() {
                result.unwrap();
            } else {
                assert!(result.unwrap_err().to_string().contains(want));
            }
        }
    }
    #[test]
    fn explicit_flags_override_only_their_config_defaults() {
        let cfg = Laptop {
            provider: "vast".into(),
            vast_api_key: "v".into(),
            ctx: "32768".into(),
            min_mbps: "200".into(),
            cloud: "community".into(),
            ..cloud()
        };
        let mut opts = UpOpts {
            provider: "runpod".into(),
            ctx: 8192,
            min_mbps: 50,
            cloud: "secure".into(),
            ..Default::default()
        };
        apply_defaults(
            &mut opts,
            &cfg,
            &|flag| ["provider", "ctx", "min-mbps", "cloud"].contains(&flag),
            Path::new("/config"),
        )
        .unwrap();
        assert_eq!(
            (
                opts.provider.as_str(),
                opts.ctx,
                opts.min_mbps,
                opts.cloud.as_str()
            ),
            ("runpod", 8192, 50, "secure")
        );
    }
}
