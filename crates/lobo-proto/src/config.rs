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
