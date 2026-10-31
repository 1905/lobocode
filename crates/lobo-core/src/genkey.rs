use crate::{Error, Result, config};
use serde_json::json;
use std::{
    fs::{self, OpenOptions},
    io::Write,
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::Path,
};

pub const OPENCODE_OUT: &str = "opencode.lobo.json";
pub fn new_api_key() -> String {
    let bytes: [u8; 24] = rand::random();
    format!("sk-{}", hex::encode(bytes))
}
pub fn opencode_for_laptop(cfg: &config::Laptop, key: &str) -> Result<String> {
    cfg.connection_mode()?;
    let m = lobo_proto::catalog::get(lobo_proto::DEFAULT_CLOUD_MODEL)
        .expect("supported model is in the pinned model catalog");
    let models = json!({&m.alias: {"name": format!("{} {}", m.name, m.quant), "limit": {"context": 65536, "output": 8192}}});
    let provider = json!({"lobo": {"npm": "@ai-sdk/openai-compatible", "name": "Lobo", "options": {"baseURL": cfg.cloud_url(), "apiKey": key}, "models": models}});
    let config = json!({"$schema": "https://opencode.ai/config.json", "provider": provider,
        "agent": {"lobo": {"description": "Lean agent for the Lobo cloud pod: core coding tools only", "mode": "primary", "model": format!("lobo/{}", m.alias),
        "tools": {"blender_*":false,"pencil_*":false,"skill":false,"webfetch":false,"todowrite":false,"todoread":false,"task":false}}}});
    // Go's JSON encoder escapes HTML characters, even in strings.
    let text = serde_json::to_string_pretty(&config)?
        .replace('&', "\\u0026")
        .replace('<', "\\u003c")
        .replace('>', "\\u003e")
        .replace('\u{2028}', "\\u2028")
        .replace('\u{2029}', "\\u2029");
    Ok(text + "\n")
}
pub fn write_opencode_for_laptop(path: &Path, cfg: &config::Laptop, key: &str) -> Result<()> {
    write_config(path, &opencode_for_laptop(cfg, key)?)
}
fn write_config(path: &Path, text: &str) -> Result<()> {
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)?;
    file.set_permissions(fs::Permissions::from_mode(0o600))?;
    file.write_all(text.as_bytes())?;
    Ok(())
}
pub fn ensure_api_key(config_path: &Path, rotate: bool) -> Result<(String, bool)> {
    let src = fs::read_to_string(config_path)
        .map_err(|e| Error::Config(format!("read {}: {e}", config_path.display())))?;
    let values = config::dotenv::parse(&src)?;
    let key = values.get("LOBO_API_KEY").cloned().unwrap_or_default();
    if !key.is_empty() && !rotate {
        return Ok((key, false));
    }
    let key = new_api_key();
    config::set_env_value(config_path, "LOBO_API_KEY", &key)?;
    Ok((key, true))
}

#[cfg(test)]
mod tests;
