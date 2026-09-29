use crate::GoTime;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct ModelRef {
    pub id: String,
    pub file: String,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct Defaults {
    #[ts(type = "number")]
    pub ctx: i64,
    #[ts(type = "number")]
    pub idle_min: i64,
    #[ts(type = "number")]
    pub max_hours: i64,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct Manifest {
    pub version: String,
    pub git_sha: String,
    pub git_dirty: bool,
    pub built_at: GoTime,
    pub built_by: String,
    pub llama_image: String,
    pub model: ModelRef,
    pub defaults: Defaults,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct Resolved {
    pub manifest: Manifest,
    pub zip_key: String,
    pub zip_sha256: String,
}

pub const DEFAULT_LLAMA_IMAGE: &str = "ghcr.io/ggml-org/llama.cpp:server-cuda-b11118";
pub const DEFAULT_MODEL: &str = "q8";
pub const DEFAULT_DEFAULTS: Defaults = Defaults {
    ctx: 65536,
    idle_min: 30,
    max_hours: 12,
};
