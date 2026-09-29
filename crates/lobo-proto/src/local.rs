use crate::GoTime;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct ModelState {
    pub id: String,
    pub file: String,
    #[ts(type = "number")]
    pub size: i64,
    #[ts(type = "number")]
    pub on_disk: i64,
    pub verified: bool,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct LocalState {
    #[ts(type = "number")]
    pub pid: i64,
    #[ts(type = "number")]
    pub port: i64,
    #[ts(type = "number")]
    pub api_port: i64,
    pub model: String,
    pub weights: String,
    pub started_at: GoTime,
    pub boot_id: String,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct RuntimeInfo {
    pub version: String,
    pub present: bool,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct Listing {
    pub weights: String,
    #[ts(type = "number")]
    pub free_bytes: u64,
    #[serde(default, deserialize_with = "crate::null_as_empty")]
    pub models: Vec<ModelState>,
    pub runtime: RuntimeInfo,
}
