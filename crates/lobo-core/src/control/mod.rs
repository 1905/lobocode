use crate::{
    Error, Result,
    clock::Clock,
    config::Laptop,
    provider::{Instance, Provider},
};
use async_trait::async_trait;
use lobo_proto::{Manifest, Status};
use std::{collections::BTreeMap, sync::Arc, time::Duration};
pub mod agent_http;
pub mod operation_state;
pub use operation_state::OperationState;
pub mod status;
pub use agent_http::HttpAgent;
pub use status::{down, snapshot, target};
#[cfg(any(test, feature = "testkit"))]
pub mod testkit;
#[async_trait]
pub trait AgentApi: Send + Sync {
    async fn status(&self) -> Result<Status>;
    async fn version(&self) -> Result<Manifest>;
    async fn logs(&self, n: usize) -> Result<String>;
}
#[derive(Debug, Clone, Default, PartialEq)]
pub struct UpOpts {
    pub model: String,
    pub ctx: i64,
    pub release: String,
    pub idle_min: i64,
    pub max_life: Duration,
    pub timeout: Duration,
    pub source: String,
    pub conns: i64,
    pub cloud: String,
    pub provider: String,
    pub min_mbps: i64,
    pub ssh_key: String,
    pub image: String,
}
pub type AgentFactory = Arc<dyn Fn(&str) -> Arc<dyn AgentApi> + Send + Sync>;
#[derive(Clone)]
pub struct Deps {
    pub connection: Option<Arc<crate::connection::Manager>>,
    pub images: Arc<dyn crate::images::ImageResolver>,
    pub providers: BTreeMap<String, Arc<dyn Provider>>,
    pub operations: Arc<OperationState>,
    pub new_agent: AgentFactory,
    pub cfg: Laptop,
    pub clock: Arc<dyn Clock>,
    pub poll: Duration,
}
pub const MAX_GPU_RETRIES: u32 = 4;
pub const CONTAINER_TIMEOUT: Duration = Duration::from_secs(30 * 60);
pub const STALE_SLACK: Duration = Duration::from_secs(15);
pub const POD_CHECK_EVERY: Duration = Duration::from_secs(30);
pub(crate) async fn list_all(d: &Deps) -> (Vec<Instance>, Option<Error>) {
    let mut instances = vec![];
    let mut errors = vec![];
    for name in ["runpod", "vast", "local"] {
        if let Some(p) = d.providers.get(name) {
            match p.list().await {
                Ok(mut list) => instances.append(&mut list),
                Err(e) => errors.push(Error::Other(format!("{name}: {e}"))),
            }
        }
    }
    (
        instances,
        if errors.is_empty() {
            None
        } else {
            Some(Error::Multi(errors))
        },
    )
}
#[cfg(test)]
mod tests;

pub mod precheck;
pub mod wiring;
pub use wiring::{Wiring, deps_from_config, local_provider_from_config, providers_from_config};

mod cleanup;

mod up;
pub use up::{UpOperation, up};

#[cfg(test)]
mod up_tests;

pub use precheck::{apply_defaults, check_providers, check_release, check_target, resolve_up};
