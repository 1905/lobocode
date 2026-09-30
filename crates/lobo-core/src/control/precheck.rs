use super::{Error, Laptop, Result, UpOpts};
use std::{path::Path, time::Duration};
pub fn resolve_up(
    cfg: &Laptop,
    cfg_path: &Path,
    req: &lobo_proto::UpRequest,
    supported: fn() -> Result<()>,
) -> Result<UpOpts> {
    let mut o = UpOpts {
        provider: req.provider.clone().unwrap_or_default(),
        model: req.model.clone().unwrap_or_default(),
        ctx: req.ctx.unwrap_or_default(),
        source: req.source.clone().unwrap_or_default(),
        cloud: req.cloud.clone().unwrap_or_else(|| "community".into()),
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
            "q6" => req.model.is_some(),
            "ctx" => req.ctx.is_some(),
            "cloud" => req.cloud.is_some(),
            _ => false,
        },
        cfg_path,
    )?;
    check_target(cfg, &o.provider, supported)?;
    Ok(o)
}
pub fn check_target(cfg: &Laptop, provider: &str, supported: fn() -> Result<()>) -> Result<()> {
    if provider == "local" {
        supported()
    } else {
        cfg.require_cloud()
    }
}
pub fn check_providers(cfg: &Laptop, supported: fn() -> Result<()>) -> Result<()> {
    if supported().is_ok() {
        Ok(())
    } else {
        cfg.require_provider_key()
    }
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
            "LOBO_MODEL" => "q6",
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
    if !set("q6") && !d.model.is_empty() {
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
    if o.provider == "local" || cfg.providers().contains(&o.provider) {
        return Ok(());
    }
    let key = match o.provider.as_str() {
        "runpod" => "RUNPOD_API_KEY",
        "vast" => "VASTAI_API_KEY",
        _ => {
            return Err(Error::Config(format!(
                "--provider: want runpod, vast or local, got {:?}",
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
    #[test]
    fn resolve_up_composes_cli_defaults_and_precheck() {
        use lobo_proto::UpRequest;
        let cfg = Laptop {
            model: "q6".into(),
            ctx: "8192".into(),
            idle_min: "10".into(),
            max_hours: "2".into(),
            min_mbps: "150".into(),
            ..cloud()
        };
        let o = resolve_up(&cfg, Path::new("/config"), &UpRequest::default(), || Ok(())).unwrap();
        assert_eq!(
            (o.provider.as_str(), o.model.as_str(), o.cloud.as_str()),
            ("runpod", "q6", "community")
        );
        assert_eq!((o.ctx, o.idle_min, o.min_mbps), (8192, 10, 150));
        assert_eq!(o.max_life, Duration::from_secs(7200));
        let r = UpRequest {
            model: Some("q8".into()),
            ctx: Some(4096),
            ..Default::default()
        };
        let bad = Laptop {
            ctx: "bad".into(),
            ..cfg.clone()
        };
        let o = resolve_up(&bad, Path::new("/config"), &r, || Ok(())).unwrap();
        assert_eq!(o.model, "q8");
        assert_eq!(o.ctx, 4096);
        for (provider, cfg, expected) in [
            ("local", cfg.clone(), "unsupported"),
            (
                "vast",
                cfg.clone(),
                "no VASTAI_API_KEY in /config. Run `lobo config` to add it",
            ),
            (
                "x",
                cfg,
                "--provider: want runpod, vast or local, got \"x\"",
            ),
            (
                "runpod",
                Laptop {
                    runpod_api_key: "r".into(),
                    ..Default::default()
                },
                "config: cloud needs CF_TUNNEL_TOKEN, LOBO_DOMAIN, LOBO_BUCKET_URL",
            ),
        ] {
            let req = UpRequest {
                provider: Some(provider.into()),
                ..Default::default()
            };
            assert_eq!(
                resolve_up(&cfg, Path::new("/config"), &req, unsupported)
                    .unwrap_err()
                    .to_string(),
                expected
            );
        }
    }
    fn unsupported() -> Result<()> {
        Err(Error::Local("unsupported".into()))
    }
    fn cloud() -> Laptop {
        Laptop {
            runpod_api_key: "r".into(),
            cf_tunnel_token: "t".into(),
            domain: "d".into(),
            bucket_url: "https://b".into(),
            ..Default::default()
        }
    }
    #[test]
    fn check_target_table() {
        for (cfg, provider, supported, want) in [
            (Laptop::default(), "local", true, ""),
            (Laptop::default(), "local", false, "unsupported"),
            (Laptop::default(), "runpod", true, "cloud needs"),
            (cloud(), "runpod", false, ""),
            (cloud(), "vast", false, ""),
        ] {
            let r = check_target(
                &cfg,
                provider,
                if supported { || Ok(()) } else { unsupported },
            );
            if want.is_empty() {
                r.unwrap();
            } else {
                assert!(r.unwrap_err().to_string().contains(want));
            }
        }
    }
    #[test]
    fn check_providers_table() {
        for (runpod, vast, supported, want) in [
            (false, false, true, true),
            (false, false, false, false),
            (true, false, false, true),
            (false, true, false, true),
            (true, true, false, true),
        ] {
            let cfg = Laptop {
                runpod_api_key: if runpod { "r" } else { "" }.into(),
                vast_api_key: if vast { "v" } else { "" }.into(),
                ..Default::default()
            };
            assert_eq!(
                check_providers(&cfg, if supported { || Ok(()) } else { unsupported }).is_ok(),
                want
            );
        }
    }
    #[test]
    fn check_release_table() {
        for (r2, bucket, want) in [
            (false, "https://b", "R2_"),
            (true, "", "LOBO_BUCKET_URL"),
            (true, "https://b", ""),
        ] {
            let mut c = Laptop {
                bucket_url: bucket.into(),
                ..Default::default()
            };
            if r2 {
                c.r2 = crate::config::R2Creds {
                    account_id: "a".into(),
                    access_key: "k".into(),
                    secret_key: "s".into(),
                    endpoint: "https://r2.test".into(),
                };
            }
            let r = check_release(&c);
            if want.is_empty() {
                r.unwrap();
            } else {
                assert!(r.unwrap_err().to_string().contains(want));
            }
        }
    }
    #[test]
    fn apply_defaults_table() {
        let base = UpOpts {
            provider: "runpod".into(),
            cloud: "community".into(),
            ..Default::default()
        };
        for case in 0..13 {
            let mut cfg = Laptop {
                runpod_api_key: "r".into(),
                ..Default::default()
            };
            let mut o = UpOpts {
                cloud: "community".into(),
                ..Default::default()
            };
            let mut want = base.clone();
            let mut flags = vec![];
            let mut error = "";
            match case {
                0 => {}
                1 => {
                    cfg.runpod_api_key.clear();
                    cfg.vast_api_key = "v".into();
                    want.provider = "vast".into();
                }
                2 => {
                    cfg.vast_api_key = "v".into();
                    cfg.provider = "vast".into();
                    want.provider = "vast".into();
                }
                3 => {
                    cfg.vast_api_key = "v".into();
                    cfg.provider = "vast".into();
                    flags.push("provider");
                    o.provider = "runpod".into();
                }
                4 => {
                    cfg.model = "q6".into();
                    cfg.ctx = "32768".into();
                    cfg.idle_min = "10".into();
                    cfg.max_hours = "2".into();
                    cfg.min_mbps = "200".into();
                    cfg.cloud = "community".into();
                    want.model = "q6".into();
                    want.ctx = 32768;
                    want.idle_min = 10;
                    want.max_life = Duration::from_secs(7200);
                    want.min_mbps = 200;
                }
                5 => {
                    cfg.ctx = "32768".into();
                    cfg.min_mbps = "200".into();
                    cfg.cloud = "community".into();
                    flags.extend(["ctx", "min-mbps", "cloud"]);
                    o.ctx = 8192;
                    o.min_mbps = 50;
                    o.cloud = "secure".into();
                    want.ctx = 8192;
                    want.min_mbps = 50;
                    want.cloud = "secure".into();
                }
                6 => {
                    flags.push("provider");
                    o.provider = "vast".into();
                    error = "no VASTAI_API_KEY in /cfg";
                }
                7 => {
                    cfg.vast_api_key = "v".into();
                    flags.push("provider");
                    o.provider = "aws".into();
                    error = "want runpod, vast or local";
                }
                8 => {
                    cfg.runpod_api_key.clear();
                    cfg.lobo_api_key = "sk".into();
                    flags.push("provider");
                    o.provider = "local".into();
                    want.provider = "local".into();
                }
                9 => {
                    cfg.vast_api_key = "v".into();
                    cfg.provider = "local".into();
                    want.provider = "local".into();
                }
                10 => {
                    cfg.provider = "local".into();
                    flags.push("provider");
                    o.provider = "runpod".into();
                }
                11 => {
                    cfg.ctx = "100".into();
                    error = "LOBO_CTX";
                }
                12 => {
                    cfg.ctx = "100".into();
                    flags.push("ctx");
                    o.ctx = 8192;
                    want.ctx = 8192;
                }
                _ => unreachable!(),
            }
            let r = apply_defaults(&mut o, &cfg, &|f| flags.contains(&f), Path::new("/cfg"));
            if error.is_empty() {
                r.unwrap();
                assert_eq!(o, want, "case {case}");
            } else {
                assert!(r.unwrap_err().to_string().contains(error), "case {case}");
            }
        }
    }
}
