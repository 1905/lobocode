use super::*;
use crate::provider::{runpod, vast};
use std::path::PathBuf;
pub struct Wiring {
    pub config_path: PathBuf,
    pub exe: PathBuf,
}
impl Wiring {
    pub fn new(config_path: PathBuf, exe: PathBuf) -> Self {
        Self { config_path, exe }
    }
}
pub fn providers_from_config(cfg: &Laptop, _w: &Wiring) -> BTreeMap<String, Arc<dyn Provider>> {
    let mut out: BTreeMap<String, Arc<dyn Provider>> = BTreeMap::new();
    if !cfg.runpod_api_key.is_empty() {
        out.insert(
            "runpod".into(),
            Arc::new(runpod::RunPodProvider {
                api: Arc::new(runpod::Client::new(&cfg.runpod_api_key)),
                domain: String::new(),
            }),
        );
    }
    if !cfg.vast_api_key.is_empty() {
        out.insert(
            "vast".into(),
            Arc::new(vast::VastProvider::new(
                vast::Client::new(&cfg.vast_api_key),
                cfg.vast_max_dph.parse().unwrap_or(0.0),
                "",
            )),
        );
    }
    out
}
pub fn deps_from_config(cfg: Laptop, w: &Wiring) -> Result<Deps> {
    let key = cfg.lobo_api_key.clone();
    Ok(Deps {
        images: Arc::new(crate::images::PublicImages::new(&cfg.image_repo())),
        connection: Some(Arc::new(crate::connection::Manager::new(
            w.config_path.clone(),
            w.exe.clone(),
            cfg.cloud_port(),
        )?)),
        operations: Arc::new(OperationState::persistent(
            crate::state::state_dir().join("operation.json"),
        )),
        providers: providers_from_config(&cfg, w),
        new_agent: Arc::new(move |base| Arc::new(HttpAgent::new(base, &key))),
        cfg,
        clock: Arc::new(crate::clock::SystemClock),
        poll: Duration::ZERO,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn providers_from_config_are_cloud_only() {
        let w = Wiring::new("override.env".into(), "/app/lobocode".into());
        for (runpod, vast, want) in [
            (true, false, vec!["runpod"]),
            (false, true, vec!["vast"]),
            (true, true, vec!["runpod", "vast"]),
            (false, false, vec![]),
        ] {
            let cfg = Laptop {
                runpod_api_key: if runpod { "r" } else { "" }.into(),
                vast_api_key: if vast { "v" } else { "" }.into(),
                ..Default::default()
            };
            assert_eq!(
                providers_from_config(&cfg, &w)
                    .keys()
                    .map(String::as_str)
                    .collect::<Vec<_>>(),
                want
            );
            assert_eq!(
                deps_from_config(cfg, &w).unwrap().providers.len(),
                want.len()
            );
        }
    }
}
