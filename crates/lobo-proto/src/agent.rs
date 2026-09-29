use crate::GoTime;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum Stage {
    #[default]
    Boot,
    Tunnel,
    Gpu,
    Verify,
    Download,
    Load,
    Ready,
    Failed,
    Terminating,
    #[serde(other)]
    Unknown,
}
impl Stage {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Boot => "boot",
            Self::Tunnel => "tunnel",
            Self::Gpu => "gpu",
            Self::Verify => "verify",
            Self::Download => "download",
            Self::Load => "load",
            Self::Ready => "ready",
            Self::Failed => "failed",
            Self::Terminating => "terminating",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct DownloadProgress {
    #[ts(type = "number")]
    pub bytes: i64,
    #[ts(type = "number")]
    pub total: i64,
    pub mbps: f64,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub verifying: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct Gpu {
    pub name: String,
    #[ts(type = "number")]
    pub vram_used_mb: i64,
    #[ts(type = "number")]
    pub vram_total_mb: i64,
    #[ts(type = "number")]
    pub util_pct: i64,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct Host {
    pub load1: f64,
    pub load5: f64,
    pub load15: f64,
    #[ts(type = "number")]
    pub mem_used_mb: i64,
    #[ts(type = "number")]
    pub mem_total_mb: i64,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct Llama {
    #[ts(type = "number")]
    pub requests_processing: i64,
    #[ts(type = "number")]
    pub requests_deferred: i64,
    #[ts(type = "number")]
    pub prompt_tokens_total: i64,
    #[ts(type = "number")]
    pub gen_tokens_total: i64,
    pub prompt_tps: f64,
    pub gen_tps: f64,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct Timings {
    pub container_started_at: GoTime,
    pub bootstrap_apt_s: f64,
    pub bootstrap_zip_s: f64,
    pub tunnel_s: f64,
    pub gpu_check_s: f64,
    pub download_s: f64,
    pub download_mbps: f64,
    pub download_source: String,
    #[ts(type = "number")]
    pub download_conns: i64,
    pub verify_s: f64,
    pub load_s: f64,
    pub ready_at: GoTime,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct Status {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub boot_id: String,
    pub stage: Stage,
    pub stage_detail: String,
    pub download: DownloadProgress,
    #[ts(type = "number")]
    pub uptime_s: i64,
    #[ts(type = "number")]
    pub idle_s: i64,
    #[ts(type = "number")]
    pub kill_in_s: i64,
    pub kill_reason: String,
    pub expires_at: GoTime,
    pub gpu: Option<Gpu>,
    pub host: Option<Host>,
    pub llama: Option<Llama>,
    #[ts(type = "number")]
    pub metrics_failures: i64,
    pub model: String,
    #[ts(type = "number")]
    pub ctx: i64,
    pub timings: Timings,
}
