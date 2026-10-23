//! Owned dependencies for the native UI fixture. No real providers or Start.
use async_trait::async_trait;
use lobo_core::{
    Error, Result, config::Laptop, control::{self, AgentApi, OperationState},
    images::ImageResolver, provider::{CreateOpts, Instance, Provider},
};
use lobo_proto::{Manifest, Status};
use reqwest::{Client, Url};
use serde::{Deserialize, de::DeserializeOwned};
use std::{collections::BTreeMap, path::{Path, PathBuf}, sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;

pub(crate) const START_DENIED: &str = "Runtime UI fixture cannot start a runtime.";
fn invalid() -> Error { Error::Other("Invalid owned native runtime fixture.".into()) }
pub(crate) fn enabled() -> bool {
    std::env::var_os("LOBO_E2E_RUNTIME_UI_ONLY").is_some_and(|v| v == "1")
}

/// Metadata-only validation also supports setup before config.env exists.
/// Dependency construction below additionally requires the canonical config.
pub(crate) fn metadata_root() -> Result<PathBuf> {
    let supplied = PathBuf::from(std::env::var_os("LOBO_E2E_ROOT").ok_or_else(invalid)?);
    let root = supplied.canonicalize().map_err(|_| invalid())?;
    if !root.file_name().and_then(|s| s.to_str()).is_some_and(|s| s.starts_with("lobo-native-e2e-")) {
        return Err(invalid());
    }
    let file = root.join("fixture.json");
    let meta = std::fs::symlink_metadata(&file).map_err(|_| invalid())?;
    if !meta.is_file() || meta.len() > 16 << 10 { return Err(invalid()); }
    let value: serde_json::Value = serde_json::from_slice(&std::fs::read(file).map_err(|_| invalid())?)
        .map_err(|_| invalid())?;
    if value["root"].as_str() != root.to_str()
        || value["config"].as_str() != root.join("config.env").to_str()
        || !matches!(value["mode"].as_str(), Some("setup" | "memory-only" | "runtime-ui-only")) {
        return Err(invalid());
    }
    Ok(root)
}

#[derive(Deserialize)]
struct Fixture { mode: String, runtime_origin: String, agent_url: String, api_url: String, control_token: String }
fn origin_url(value: &str) -> Result<Url> {
    let url = Url::parse(value).map_err(|_| invalid())?;
    if url.scheme() != "http" || url.host_str() != Some("127.0.0.1") || url.port().is_none()
        || !url.username().is_empty() || url.password().is_some() || url.query().is_some()
        || url.fragment().is_some() || url.path() != "/" { return Err(invalid()); }
    Ok(url)
}
pub(crate) fn deny_start(config: &Path) -> Result<()> {
    if enabled() {
        // Validate first; a broken fixture must fail closed too.
        validated(config)?;
        return Err(Error::Other(START_DENIED.into()));
    }
    Ok(())
}
fn validated(config: &Path) -> Result<(PathBuf, Fixture)> {
    let root = metadata_root()?;
    if config.canonicalize().map_err(|_| invalid())? != root.join("config.env") { return Err(invalid()); }
    let fixture: Fixture = serde_json::from_slice(&std::fs::read(root.join("fixture.json")).map_err(|_| invalid())?)
        .map_err(|_| invalid())?;
    let url = origin_url(&fixture.runtime_origin)?;
    let origin = url.origin().ascii_serialization();
    if fixture.mode != "runtime-ui-only" || fixture.runtime_origin != origin
        || fixture.agent_url != origin || fixture.api_url != format!("{origin}/v1")
        || fixture.control_token.len() < 32 || fixture.control_token.len() > 128
        || !fixture.control_token.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_') {
        return Err(invalid());
    }
    Ok((root, fixture))
}

pub(crate) fn deps(config: &Path, cfg: Laptop) -> Result<Option<control::Deps>> {
    if !enabled() { return Ok(None); }
    let (root, fixture) = validated(config)?;
    let client = Client::builder().redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(2)).timeout(Duration::from_secs(3)).build()?;
    let mut providers: BTreeMap<String, Arc<dyn Provider>> = BTreeMap::new();
    for name in ["local", "runpod", "vast"] {
        providers.insert(name.into(), Arc::new(FixtureProvider {
            name, origin: fixture.runtime_origin.clone(), token: fixture.control_token.clone(), client: client.clone(),
        }));
    }
    let base = fixture.agent_url;
    let key = cfg.lobo_api_key.clone();
    Ok(Some(control::Deps {
        connection: None,
        images: Arc::new(DenyImages), providers,
        operations: Arc::new(OperationState::persistent(root.join("state/lobo/operation.json"))),
        new_agent: Arc::new(move |actual| {
            if actual == base { Arc::new(control::HttpAgent::new(actual, &key)) }
            else { Arc::new(DenyAgent) }
        }),
        cfg, clock: Arc::new(lobo_core::clock::SystemClock), poll: Duration::from_millis(20),
    }))
}
struct DenyImages;
#[async_trait]
impl ImageResolver for DenyImages {
    async fn latest(&self, _: &str) -> Result<String> { Err(Error::Other(START_DENIED.into())) }
}
struct DenyAgent;
#[async_trait]
impl AgentApi for DenyAgent {
    async fn status(&self) -> Result<Status> { Err(invalid()) }
    async fn version(&self) -> Result<Manifest> { Err(invalid()) }
    async fn logs(&self, _: usize) -> Result<String> { Err(invalid()) }
}
struct FixtureProvider { name: &'static str, origin: String, token: String, client: Client }
impl FixtureProvider {
    async fn request<T: DeserializeOwned>(&self, method: &str, body: Option<serde_json::Value>) -> Result<T> {
        let url = format!("{}/fixture/provider/{}/{method}", self.origin, self.name);
        let request = match body { Some(body) => self.client.post(url).json(&body), None => self.client.get(url) };
        let mut response = request.header("X-Lobo-Fixture-Control", &self.token).send().await?;
        if response.status() == 404 { return Err(Error::NotFound); }
        if !response.status().is_success() { return Err(Error::Other("Owned fixture operation rejected.".into())); }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await? {
            if bytes.len().saturating_add(chunk.len()) > 64 << 10 { return Err(invalid()); }
            bytes.extend_from_slice(&chunk);
        }
        Ok(serde_json::from_slice(&bytes)?)
    }
    fn instance(&self, mut instance: Instance) -> Result<Instance> {
        if self.name != "local" || instance.provider != "local" || instance.id != "fixture-local" { return Err(invalid()); }
        // Instance deliberately skips transport URLs in its serialized protocol.
        instance.api_url = format!("{}/v1", self.origin);
        instance.agent_url = self.origin.clone();
        Ok(instance)
    }
    async fn forbidden(&self, method: &str) -> Result<()> {
        // Record only the fixed method. No rent arguments or foreign ID leave Rust.
        let _ = self.request::<serde_json::Value>(method, Some(serde_json::json!({}))).await;
        Err(Error::Other(START_DENIED.into()))
    }
}
#[async_trait]
impl Provider for FixtureProvider {
    fn name(&self) -> &'static str { self.name }
    fn replaceable(&self) -> bool { false }
    async fn rent(&self, _: &CreateOpts, _: CancellationToken, _: &(dyn Fn(String) + Sync)) -> Result<Instance> {
        self.forbidden("rent").await?; Err(invalid())
    }
    async fn list(&self) -> Result<Vec<Instance>> {
        let instances: Vec<Instance> = self.request("list", None).await?;
        instances.into_iter().map(|i| self.instance(i)).collect()
    }
    async fn get(&self, id: &str) -> Result<Instance> {
        if self.name != "local" { self.forbidden("get").await?; return Err(invalid()); }
        if id != "fixture-local" { return Err(Error::NotFound); }
        self.instance(self.request("get/fixture-local", None).await?)
    }
    async fn delete(&self, _: &str) -> Result<()> { self.forbidden("delete").await }
    async fn runtime_identity(&self, id: &str) -> Result<Option<(String, i32, u64)>> {
        if self.name != "local" { self.forbidden("identity").await?; return Err(invalid()); }
        if id != "fixture-local" { return Err(Error::NotFound); }
        self.request("identity/fixture-local", None).await
    }
    async fn delete_owned(&self, id: &str, boot_id: &str, start_id: Option<u64>) -> Result<()> {
        if self.name != "local" { return self.forbidden("delete-owned").await; }
        if id != "fixture-local" || !boot_id.starts_with("fixture-boot-") || start_id != Some(100) {
            return Err(invalid());
        }
        self.request::<serde_json::Value>("delete-owned", Some(serde_json::json!({"id": id, "boot_id": boot_id, "start_id": start_id}))).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fixture_origin_rejects_external_redirect_and_credential_targets() {
        for candidate in [
            "https://127.0.0.1:1234", "http://localhost:1234", "http://192.0.2.1:1234",
            "http://user@127.0.0.1:1234", "http://127.0.0.1:1234/api",
            "http://127.0.0.1:1234?other=1", "http://127.0.0.1:1234#other", "http://127.0.0.1",
        ] { assert!(origin_url(candidate).is_err(), "accepted {candidate}"); }
        assert!(origin_url("http://127.0.0.1:1234").is_ok());
    }
}
