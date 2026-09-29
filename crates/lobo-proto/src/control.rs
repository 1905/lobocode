use crate::GoTime;
use crate::{DownloadProgress, Manifest, Status, Timings};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct Instance {
    pub provider: String,
    pub id: String,
    pub status: String,
    pub detail: String,
    pub cost_per_hr: f64,
    pub started_at: GoTime,
    #[ts(type = "number")]
    pub host_download_mbps: i64,
    #[serde(skip)]
    #[ts(skip)]
    pub api_url: String,
    #[serde(skip)]
    #[ts(skip)]
    pub agent_url: String,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct ReadyInfo {
    pub pod_id: String,
    pub provider: String,
    pub detail: String,
    #[ts(type = "number")]
    pub attempts: i64,
    #[serde(rename = "rent_to_container_s")]
    pub rent_s: f64,
    #[ts(type = "number")]
    pub host_download_mbps: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timings: Option<Timings>,
    pub url: String,
    pub version: String,
    pub git_sha: String,
    #[serde(rename = "usd_per_h")]
    pub cost_per_hr: f64,
    #[ts(type = "number")]
    pub elapsed_ns: i64,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct Snap {
    pub pod: Option<Instance>,
    pub version: Option<Manifest>,
    pub status: Option<Status>,
    pub down: bool,
    pub at: GoTime,
}

/// One line of `lobo up --json`. An error string marks a failed operation.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct UpEvent {
    pub phase: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub detail: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub download: Option<DownloadProgress>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ready: Option<ReadyInfo>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub done: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub err: Option<String>,
}
