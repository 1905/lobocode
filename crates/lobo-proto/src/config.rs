use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use ts_rs::TS;

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct ConfigShow {
    pub path: String,
    pub exists: bool,
    pub values: BTreeMap<String, String>,
    pub set: BTreeMap<String, bool>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct Readiness {
    pub exists: bool,
    pub cloud_ready: bool,
    pub ready: bool,
    pub local_supported: bool,
    pub providers: Vec<String>,
    pub default_provider: String,
    pub default_model: String,
    pub local_port: u16,
    pub error: Option<String>,
}
