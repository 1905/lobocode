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
pub fn opencode_config(domain: &str, key: &str, port: u16) -> Result<String> {
    let m = lobo_proto::catalog::get("q8").expect("q8 is in the pinned model catalog");
    let models =
        json!({&m.alias:{"name":"Qwen3.5-27B Q8","limit":{"context":65536,"output":8192}}});
    let provider = |name: &str, base: String| json!({"npm":"@ai-sdk/openai-compatible","name":name,"options":{"baseURL":base,"apiKey":key},"models":models});
    let mut providers = serde_json::Map::new();
    providers.insert(
        "lobo-local".into(),
        provider("Lobo (this Mac)", format!("http://127.0.0.1:{port}/v1")),
    );
    let mut agent_model = format!("lobo-local/{}", m.alias);
    if !domain.is_empty() {
        providers.insert(
            "lobo".into(),
            provider("Lobo", format!("https://{domain}/v1")),
        );
        agent_model = format!("lobo/{}", m.alias);
    }
    let config = json!({"$schema":"https://opencode.ai/config.json","provider":providers,
        "agent":{"lobo":{"description":"Lean agent for the lobo pod (Qwen3.5-27B): no MCP, no skills, core coding tools only",
            "mode":"primary","model":agent_model,"tools":{"blender_*":false,"pencil_*":false,"skill":false,"webfetch":false,"todowrite":false,"todoread":false,"task":false}}}});
    // Go's JSON encoder escapes HTML characters, even in strings.
    let text = serde_json::to_string_pretty(&config)?
        .replace('&', "\\u0026")
        .replace('<', "\\u003c")
        .replace('>', "\\u003e")
        .replace('\u{2028}', "\\u2028")
        .replace('\u{2029}', "\\u2029");
    Ok(text + "\n")
}
pub fn write_opencode(path: &Path, domain: &str, key: &str, port: u16) -> Result<()> {
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)?;
    file.set_permissions(fs::Permissions::from_mode(0o600))?;
    file.write_all(opencode_config(domain, key, port)?.as_bytes())?;
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
