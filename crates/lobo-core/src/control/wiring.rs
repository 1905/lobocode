use super::*;
use crate::{
    local::{self, LocalHooks, LocalProvider, Spawner, StateFile},
    provider::{runpod, vast},
    release::{BucketReleases, Store},
};
use std::path::PathBuf;
pub struct Wiring {
    pub config_path: PathBuf,
    pub spawner: Spawner,
    pub supported: fn() -> Result<()>,
}
impl Wiring {
    pub fn new(config_path: PathBuf, spawner: Spawner) -> Self {
        Self {
            config_path,
            spawner,
            supported: local::supported,
        }
    }
}
pub fn local_provider_from_config(cfg: &Laptop, w: &Wiring) -> Option<LocalProvider> {
    (w.supported)().ok()?;
    let config_path = std::path::absolute(&w.config_path).unwrap_or_else(|_| w.config_path.clone());
    Some(LocalProvider {
        spawner: w.spawner.clone(),
        config_path: Some(config_path),
        weights: cfg.weights(),
        port: cfg.port(),
        state: StateFile::at(StateFile::default_path()),
        hooks: LocalHooks {
            supported: w.supported,
            ..Default::default()
        },
    })
}
pub fn providers_from_config(cfg: &Laptop, w: &Wiring) -> BTreeMap<String, Arc<dyn Provider>> {
    let mut out: BTreeMap<String, Arc<dyn Provider>> = BTreeMap::new();
    if let Some(local) = local_provider_from_config(cfg, w) {
        out.insert("local".into(), Arc::new(local));
    }
    if !cfg.runpod_api_key.is_empty() {
        out.insert(
            "runpod".into(),
            Arc::new(runpod::RunPodProvider {
                api: Arc::new(runpod::Client::new(&cfg.runpod_api_key)),
                domain: cfg.domain.clone(),
            }),
        );
    }
    if !cfg.vast_api_key.is_empty() {
        out.insert(
            "vast".into(),
            Arc::new(vast::VastProvider::new(
                vast::Client::new(&cfg.vast_api_key),
                cfg.vast_max_dph.parse().unwrap_or(0.0),
                &cfg.domain,
            )),
        );
    }
    out
}
pub fn deps_from_config(cfg: Laptop, w: &Wiring) -> Result<Deps> {
    let key = cfg.lobo_api_key.clone();
    let presign = if cfg.require_r2().is_ok() {
        Some(Arc::new(Store::new(&cfg.r2)?) as Arc<dyn Presigner>)
    } else {
        None
    };
    Ok(Deps {
        operations: Arc::new(OperationState::persistent(
            StateFile::default_path().with_file_name("operation.json"),
        )),
        providers: providers_from_config(&cfg, w),
        releases: Arc::new(BucketReleases::new(&cfg.bucket_url)),
        presign,
        new_agent: Arc::new(move |base| Arc::new(HttpAgent::new(base, &key))),
        cfg,
        clock: Arc::new(crate::clock::SystemClock),
        poll: Duration::ZERO,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    fn wiring(supported: bool) -> Wiring {
        let mut w = Wiring::new("override.env".into(), Spawner::app("/app/lobocode".into()));
        w.supported = if supported {
            || Ok(())
        } else {
            || Err(Error::Local("unsupported".into()))
        };
        w
    }
    #[test]
    fn providers_from_config_table() {
        for (runpod, vast, supported, want) in [
            (true, false, true, vec!["local", "runpod"]),
            (false, true, false, vec!["vast"]),
            (true, true, false, vec!["runpod", "vast"]),
            (false, false, true, vec!["local"]),
        ] {
            let cfg = Laptop {
                runpod_api_key: if runpod { "r" } else { "" }.into(),
                vast_api_key: if vast { "v" } else { "" }.into(),
                ..Default::default()
            };
            assert_eq!(
                providers_from_config(&cfg, &wiring(supported))
                    .keys()
                    .map(String::as_str)
                    .collect::<Vec<_>>(),
                want
            );
        }
        let w = Wiring::new("cfg".into(), Spawner::cli("/bin/lobo".into()));
        assert_eq!(
            local_provider_from_config(&Laptop::default(), &w).is_some(),
            local::supported().is_ok()
        );
    }
    #[tokio::test]
    async fn local_instance_urls_from_state() {
        let cfg = Laptop {
            local_port: "9000".into(),
            ..Default::default()
        };
        let w = wiring(true);
        let mut p = local_provider_from_config(&cfg, &w).unwrap();
        assert_eq!(p.port, 9000);
        assert_eq!(
            p.config_path,
            Some(std::path::absolute("override.env").unwrap())
        );
        assert_eq!(
            p.spawner.args_prefix,
            vec![local::SUPERVISOR_ARG, "local", "run"]
        );
        let tmp = tempfile::tempdir().unwrap();
        p.state = StateFile::at(tmp.path().join("local.json"));
        p.state
            .claim(
                &lobo_proto::LocalState {
                    pid: std::process::id().into(),
                    boot_id: "b".into(),
                    port: 8931,
                    api_port: 8932,
                    ..Default::default()
                },
                &|_, _| true,
            )
            .unwrap();
        let i = p.list().await.unwrap().remove(0);
        assert_eq!(i.api_url, "http://127.0.0.1:8931/v1");
        assert_eq!(i.agent_url, "http://127.0.0.1:8932");
    }
    #[test]
    fn deps_from_config_sample() {
        let mut cfg = Laptop {
            runpod_api_key: "r".into(),
            vast_api_key: "v".into(),
            bucket_url: "https://b".into(),
            ..Default::default()
        };
        cfg.r2 = crate::config::R2Creds {
            account_id: "a".into(),
            access_key: "k".into(),
            secret_key: "s".into(),
            endpoint: "https://r2.test".into(),
        };
        let w = wiring(true);
        let d = deps_from_config(cfg.clone(), &w).unwrap();
        assert!(d.presign.is_some());
        assert_eq!(d.providers.len(), 3);
        cfg.r2 = Default::default();
        assert!(deps_from_config(cfg, &w).unwrap().presign.is_none());
    }
}
