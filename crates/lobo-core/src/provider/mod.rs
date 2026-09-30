use async_trait::async_trait;
use chrono::{DateTime, Utc};
pub use lobo_proto::control::Instance;
use tokio_util::sync::CancellationToken;

use crate::Result;

pub mod runpod;
pub mod vast;

pub const POD_NAME: &str = "lobo";

#[derive(Debug, Clone, Default, PartialEq)]
pub struct CreateOpts {
    pub image_model: bool,
    pub connection: String,
    pub connection_public_key: String,
    pub connection_host_key: String,
    pub image: String,
    pub release_url: String,
    pub release_sha256: String,
    pub model_url: String,
    pub model_fallback: String,
    pub lobo_api_key: String,
    pub cf_tunnel_token: String,
    pub model: String,
    pub ctx: i64,
    pub idle_min: i64,
    pub dl_conns: i64,
    pub min_mbps: i64,
    pub expires_at: DateTime<Utc>,
    pub cloud: String,
    pub ssh_pub_key: String,
    pub model_ssh_key: String,
    pub model_host_key: String,
    pub boot_id: String,
}

#[async_trait]
pub trait Provider: Send + Sync {
    fn name(&self) -> &'static str;
    fn replaceable(&self) -> bool;
    /// Do not abandon an in-flight create on cancellation. Return its instance
    /// so the owning operation can delete it before reporting completion.
    async fn rent(
        &self,
        opts: &CreateOpts,
        cancel: CancellationToken,
        note: &(dyn Fn(String) + Sync),
    ) -> Result<Instance>;
    async fn list(&self) -> Result<Vec<Instance>>;
    async fn get(&self, id: &str) -> Result<Instance>;
    async fn delete(&self, id: &str) -> Result<()>;
    /// Local supervisor identity. Cloud providers use the provider's exact ID.
    async fn runtime_identity(&self, _id: &str) -> Result<Option<(String, i32, u64)>> {
        Ok(None)
    }
    /// Destructive app boundary. Local implementations must verify boot/start.
    async fn delete_owned(&self, id: &str, boot_id: &str, _start_id: Option<u64>) -> Result<()> {
        if boot_id.is_empty() || self.name() == "local" {
            return Err(crate::Error::Other(
                "runtime identity cannot be verified".into(),
            ));
        }
        self.delete(id).await
    }
}

pub fn on_domain(mut instance: Instance, domain: &str) -> Instance {
    if domain.is_empty() {
        return instance;
    }
    instance.api_url = format!("https://{domain}/v1");
    instance.agent_url = format!("https://{domain}");
    instance
}

#[cfg(test)]
pub(crate) fn fixture_opts() -> CreateOpts {
    CreateOpts {
        image: "img".into(),
        release_url: "https://b/r.zip".into(),
        release_sha256: "abc".into(),
        model_url: "https://b/m.gguf".into(),
        lobo_api_key: "sk".into(),
        cf_tunnel_token: "tok".into(),
        model: "q8".into(),
        ctx: 8192,
        idle_min: 30,
        expires_at: "2026-09-23T22:00:00Z".parse().unwrap(),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn on_domain_sets_urls() {
        let i = on_domain(Instance::default(), "lobo.example.com");
        assert_eq!(i.api_url, "https://lobo.example.com/v1");
        assert_eq!(i.agent_url, "https://lobo.example.com");
    }
}

pub mod local {
    pub use crate::local::provider::*;
}
