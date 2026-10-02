//! Authenticated setup only. Keys and raw config revisions stay private to Rust.
use crate::{
    backend::{Backend, Result},
    types::{AppError, OpenCodeResult},
};
use lobo_core::{config, control::RuntimeTarget, opencode::Binding};
use lobo_proto::{Snap, Stage};
use reqwest::{header::HeaderValue, Client, Url};
use serde::Deserialize;
use std::{
    cell::RefCell,
    fs::{self, OpenOptions},
    io::Read,
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::{Component, Path, PathBuf},
    time::Duration,
};

const CONFIG_LIMIT: u64 = 1 << 20;
const MODELS_LIMIT: usize = 64 << 10;
pub(crate) const SAVED: &str =
    "Saved. Restart OpenCode to reload. Project settings can override this file.";

// Neither this record nor its fields may derive Debug or an IPC representation.
pub struct PreparedOpenCode {
    pub(crate) owner: RuntimeTarget,
    pub(crate) binding: Binding,
    revision: ConfigRevision,
}

#[derive(PartialEq, Eq)]
struct Fingerprint {
    dev: u64,
    ino: u64,
    len: u64,
    mtime: (i64, i64),
    ctime: (i64, i64),
}
impl From<&fs::Metadata> for Fingerprint {
    fn from(m: &fs::Metadata) -> Self {
        Self {
            dev: m.dev(),
            ino: m.ino(),
            len: m.len(),
            mtime: (m.mtime(), m.mtime_nsec()),
            ctime: (m.ctime(), m.ctime_nsec()),
        }
    }
}
#[derive(PartialEq, Eq)]
struct ConfigRevision {
    fingerprint: Fingerprint,
    bytes: Vec<u8>,
}

pub(crate) fn error(message: &'static str) -> AppError {
    AppError {
        kind: "opencode".into(),
        message: message.into(),
    }
}
fn read_revision(path: &Path) -> Result<ConfigRevision> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
        .map_err(|_| error("Cannot read the configuration safely."))?;
    let before = file
        .metadata()
        .map_err(|_| error("Cannot inspect the configuration."))?;
    if !before.is_file() || before.len() > CONFIG_LIMIT {
        return Err(error("Configuration must be a regular file under 1 MiB."));
    }
    let mut bytes = Vec::new();
    (&file)
        .take(CONFIG_LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| error("Cannot read the configuration."))?;
    let after = file
        .metadata()
        .map_err(|_| error("Cannot inspect the configuration."))?;
    let current =
        fs::symlink_metadata(path).map_err(|_| error("Configuration changed. Retry setup."))?;
    let fingerprint = Fingerprint::from(&before);
    if bytes.len() > CONFIG_LIMIT as usize
        || !current.is_file()
        || fingerprint != Fingerprint::from(&after)
        || fingerprint != Fingerprint::from(&current)
        || bytes.len() as u64 != before.len()
    {
        return Err(error("Configuration changed. Retry setup."));
    }
    Ok(ConfigRevision { fingerprint, bytes })
}
fn existing_key(path: &Path) -> Result<(ConfigRevision, String)> {
    let revision = read_revision(path)?;
    let source = std::str::from_utf8(&revision.bytes)
        .map_err(|_| error("Cannot read the saved API key."))?;
    let key = config::dotenv::parse(source)
        .map_err(|_| error("Cannot read the saved API key."))?
        .remove("LOBO_API_KEY")
        .filter(|key| !key.trim().is_empty())
        .ok_or_else(|| error("A saved API key is required. Configure Lobocode first."))?;
    if key.len() > 4096 || read_revision(path)? != revision {
        return Err(error("Lobocode configuration changed. Retry setup."));
    }
    Ok((revision, key))
}

pub(crate) fn client() -> Result<Client> {
    Client::builder()
        .user_agent(lobo_core::http::USER_AGENT)
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(3))
        .timeout(Duration::from_secs(8))
        .build()
        .map_err(|_| error("Cannot prepare authenticated model discovery."))
}
fn models_url(endpoint: &str) -> Result<Url> {
    let mut url = Url::parse(endpoint).map_err(|_| error("Runtime endpoint is invalid."))?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || !url.path().trim_end_matches('/').ends_with("/v1")
        || endpoint.len() > 4096
    {
        return Err(error("Runtime endpoint is invalid."));
    }
    url.set_path(&format!("{}/models", url.path().trim_end_matches('/')));
    Ok(url)
}

pub(crate) fn binding_from_snap(
    owner: &RuntimeTarget,
    snap: &Snap,
    api_key: String,
) -> Result<Binding> {
    crate::backend::require_cloud_provider(&owner.provider)?;
    let pod = snap
        .pod
        .as_ref()
        .filter(|pod| {
            !snap.down
                && pod.provider == owner.provider
                && Some(pod.id.as_str()) == owner.instance_id.as_deref()
                && !pod.id.is_empty()
        })
        .ok_or_else(|| error("The owned runtime is unavailable. Refresh and retry."))?;
    let status = snap
        .status
        .as_ref()
        .filter(|status| {
            status.stage == Stage::Ready
                && !owner.boot_id.is_empty()
                && status.boot_id == owner.boot_id
                && status.ctx > 0
        })
        .ok_or_else(|| error("The owned runtime is not Ready. Refresh and retry."))?;
    let model = lobo_proto::catalog::get(&status.model)
        .map_err(|_| error("The running model is not in the Lobocode catalog."))?;
    if pod.api_url.is_empty()
        || owner
            .api_url
            .as_ref()
            .is_some_and(|url| *url != pod.api_url)
    {
        return Err(error(
            "The owned runtime endpoint changed. Refresh and retry.",
        ));
    }
    models_url(&pod.api_url)?;
    Ok(Binding {
        provider: "lobo".into(),
        model_alias: model.alias.clone(),
        context: status.ctx as u64,
        endpoint: pod.api_url.clone(),
        api_key,
    })
}
pub(crate) fn same_runtime(a: &Binding, b: &Binding) -> bool {
    a.provider == b.provider
        && a.model_alias == b.model_alias
        && a.context == b.context
        && a.endpoint == b.endpoint
}

#[derive(Deserialize)]
struct ModelList {
    data: Vec<ModelId>,
}
#[derive(Deserialize)]
struct ModelId {
    id: String,
}
async fn verify_models(client: &Client, binding: &Binding) -> Result<()> {
    let url = models_url(&binding.endpoint)?;
    if binding.api_key.trim().is_empty() {
        return Err(error(
            "A saved API key is required. Configure Lobocode first.",
        ));
    }
    let mut auth = HeaderValue::from_str(&format!("Bearer {}", binding.api_key))
        .map_err(|_| error("The saved API key is invalid."))?;
    auth.set_sensitive(true);
    let mut response = client
        .get(url)
        .header(reqwest::header::AUTHORIZATION, auth)
        .send()
        .await
        .map_err(|_| error("Authenticated model discovery failed. Check the runtime and retry."))?;
    if matches!(response.status().as_u16(), 401 | 403) {
        return Err(error(
            "The runtime rejected the saved API key. Repair Lobocode access first.",
        ));
    }
    if !response.status().is_success() {
        return Err(error(
            "Authenticated model discovery failed. Check the runtime and retry.",
        ));
    }
    if response
        .content_length()
        .is_some_and(|n| n > MODELS_LIMIT as u64)
    {
        return Err(error("The model discovery response is too large."));
    }
    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| error("Cannot read the model discovery response."))?
    {
        if body.len().saturating_add(chunk.len()) > MODELS_LIMIT {
            return Err(error("The model discovery response is too large."));
        }
        body.extend_from_slice(&chunk);
    }
    let list: ModelList = serde_json::from_slice(&body)
        .map_err(|_| error("The model discovery response is invalid."))?;
    if !list
        .data
        .iter()
        .any(|model| model.id == binding.model_alias)
    {
        return Err(error(
            "The authenticated endpoint does not list the running model.",
        ));
    }
    Ok(())
}

pub(crate) async fn prepare<B: Backend + ?Sized>(
    backend: &B,
    owner: RuntimeTarget,
    client: &Client,
) -> Result<PreparedOpenCode> {
    crate::backend::require_cloud_provider(&owner.provider)?;
    let path = backend.config_path();
    let (revision, key) = existing_key(&path)?;
    let fresh = |(actual, snap): (Option<RuntimeTarget>, Snap)| -> Result<Binding> {
        if actual.as_ref() != Some(&owner) {
            return Err(error("Runtime ownership changed. Refresh and retry."));
        }
        binding_from_snap(&owner, &snap, String::new())
    };
    let mut binding = fresh(
        backend
            .snapshot_owned(&owner.provider, Some(owner.clone()))
            .await
            .map_err(|_| error("Cannot verify the owned runtime. Refresh and retry."))?,
    )?;
    binding.api_key = key;
    verify_models(client, &binding).await?;
    if read_revision(&path)? != revision {
        return Err(error("Lobocode configuration changed. Retry setup."));
    }
    let latest = fresh(
        backend
            .snapshot_owned(&owner.provider, Some(owner.clone()))
            .await
            .map_err(|_| error("Cannot recheck the owned runtime. Refresh and retry."))?,
    )?;
    if !same_runtime(&binding, &latest) {
        return Err(error(
            "The running model or endpoint changed. Refresh and retry.",
        ));
    }
    Ok(PreparedOpenCode {
        owner,
        binding,
        revision,
    })
}

pub(crate) fn discover() -> Result<PathBuf> {
    let home = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|home| !home.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
        .filter(|home| home.is_absolute())
        .ok_or_else(|| error("Cannot resolve the global OpenCode configuration directory."))?;
    lobo_core::opencode::discover_config(&home)
        .map(|choice| choice.path)
        .map_err(|_| error("Cannot discover a safe OpenCode configuration path."))
}
pub(crate) fn config_path(path: &Path, must_exist: bool) -> Result<()> {
    if !path.is_absolute()
        || path.as_os_str().len() > 4096
        || path
            .components()
            .any(|c| matches!(c, Component::ParentDir | Component::CurDir))
        || !matches!(
            path.extension().and_then(|s| s.to_str()),
            Some("json" | "jsonc")
        )
        || path.to_str().is_none()
    {
        return Err(error(
            "Choose an absolute JSON or JSONC configuration path.",
        ));
    }
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_file() => Ok(()),
        Err(e) if !must_exist && e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        _ => Err(error(
            "Choose an existing regular JSON or JSONC file. Symlinks are unsupported.",
        )),
    }
}
pub(crate) fn warnings(path: &Path, provider: &str) -> Result<Vec<String>> {
    let source = match read_revision(path) {
        Ok(revision) => String::from_utf8(revision.bytes)
            .map_err(|_| error("OpenCode configuration must be valid UTF-8 JSONC."))?,
        Err(_)
            if !path
                .try_exists()
                .map_err(|_| error("Cannot inspect OpenCode configuration."))? =>
        {
            "{}".into()
        }
        Err(e) => return Err(e),
    };
    let restrictions = lobo_core::opencode::inspect_restrictions(&source, provider)
        .map_err(|_| error("OpenCode configuration or provider restrictions are invalid."))?;
    let mut warnings = Vec::new();
    if restrictions.provider_disabled {
        warnings.push("This file disables the Lobocode provider.".into());
    }
    if restrictions.provider_not_enabled {
        warnings.push("This file's enabled providers exclude Lobocode.".into());
    }
    Ok(warnings)
}
pub(crate) fn configure<B: Backend + ?Sized>(
    backend: &B,
    path: &Path,
    prepared: &PreparedOpenCode,
    make_default: bool,
    validate: &dyn Fn() -> Result<()>,
) -> Result<OpenCodeResult> {
    config_path(path, false)?;
    let config = backend.config_path();
    let key_dir = config
        .parent()
        .ok_or_else(|| error("Lobocode configuration directory is missing."))?
        .join("opencode-keys");
    let inspected = RefCell::new(Vec::new());
    let check = || -> lobo_core::Result<()> {
        let local = || -> Result<()> {
            // The caller retains both gates until configure returns, including no-ops.
            validate()?;
            if backend
                .load_owner()
                .map_err(|_| error("Cannot recheck runtime ownership."))?
                .as_ref()
                != Some(&prepared.owner)
                || read_revision(&config)? != prepared.revision
            {
                return Err(error(
                    "Runtime or Lobocode configuration changed. Retry setup.",
                ));
            }
            *inspected.borrow_mut() = warnings(path, &prepared.binding.provider)?;
            Ok(())
        };
        local().map_err(|_| lobo_core::Error::Other("OpenCode setup validation failed.".into()))
    };
    let outcome =
        lobo_core::opencode::configure(path, &key_dir, &prepared.binding, make_default, &check)
            .map_err(|_| {
                error(
            "OpenCode setup failed or changed during verification. No configuration was replaced.",
        )
            })?;
    Ok(OpenCodeResult {
        path: outcome.path.to_string_lossy().into_owned(),
        provider: outcome.provider,
        model_alias: outcome.model_alias,
        changed: outcome.changed,
        message: SAVED.into(),
        warnings: inspected.into_inner(),
    })
}

#[cfg(test)]
pub(crate) mod tests;
