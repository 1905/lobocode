//! File-backed display fixtures compiled only into the isolated E2E app.
use lobo_core::{
    Error,
    local::memory::{self, MemoryProbe, MemorySnapshot},
};
use serde::Deserialize;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

#[derive(Deserialize)]
struct Fixture {
    mode: String,
    #[serde(default)]
    delay_ms: u64,
    #[serde(default)]
    token: String,
    #[serde(default)]
    message: String,
}

fn fixture_root(config: &Path) -> Option<PathBuf> {
    let root = crate::e2e_runtime::metadata_root().ok()?;
    if config.canonicalize().ok()? != root.join("config.env") {
        return None;
    }
    Some(root)
}

pub fn probe(config: &Path, display: bool) -> Option<MemoryProbe> {
    let root = fixture_root(config)?;
    let path = root.join("memory.json");
    if !path.exists() {
        return None;
    }
    Some(Arc::new(move || {
        let fixture: Fixture = serde_json::from_slice(
            &std::fs::read(&path).map_err(|e| Error::Local(format!("E2E memory fixture: {e}")))?,
        )
        .map_err(|e| Error::Local(format!("E2E memory fixture: {e}")))?;
        // Capture before the bounded delay so selection changes can reject this result.
        if display {
            std::fs::write(
                root.join("memory-read.json"),
                serde_json::json!({"token": fixture.token, "mode": fixture.mode}).to_string(),
            )
            .map_err(|e| Error::Local(format!("E2E memory marker: {e}")))?;
            std::thread::sleep(Duration::from_millis(fixture.delay_ms.min(3000)));
        }
        if !display {
            return Ok(MemorySnapshot {
                total_bytes: 64 << 30,
                available_bytes: 8 << 30,
                metal_limit_bytes: 48 << 30,
            });
        }
        if fixture.mode == "real" {
            return memory::snapshot();
        }
        if fixture.mode == "unavailable" {
            return Err(Error::Local(if fixture.message.is_empty() {
                "native probe unavailable (E2E fixture)".into()
            } else {
                fixture.message
            }));
        }
        let available = match fixture.mode.as_str() {
            // Every Start check denies. A passing fixture is display-only.
            "ready" => 60,
            "q6_only" => 31,
            "insufficient" => 8,
            _ => return Err(Error::Local("invalid E2E memory mode".into())),
        };
        Ok(MemorySnapshot {
            total_bytes: 64 << 30,
            available_bytes: available << 30,
            metal_limit_bytes: 48 << 30,
        })
    }))
}
