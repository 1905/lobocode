use lobo_proto::{ConfigShow, DownloadProgress, Listing, Readiness, Snap};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

// This submission is private to Rust. It freezes the click before queueing.
pub(crate) struct StartSubmission {
    pub request: lobo_proto::UpRequest,
    pub config_generation: u64,
    pub selection_generation: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export, export_to = "../gen/")]
pub enum Phase {
    #[default]
    Loading,
    NoConfig,
    Off,
    Booting,
    Ready,
    Stopping,
    Failed {
        message: String,
    },
}
impl Phase {
    pub fn word(&self) -> &'static str {
        match self {
            Self::Loading => "SCAN",
            Self::NoConfig => "SETUP",
            Self::Off => "OFF",
            Self::Booting => "BOOT",
            Self::Ready => "RUN",
            Self::Stopping => "STOP",
            Self::Failed { .. } => "FAIL",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export, export_to = "../gen/")]
pub enum Target {
    Local,
    #[default]
    Cloud,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export, export_to = "../gen/")]
pub enum Step {
    Rent,
    Container,
    Tunnel,
    Gpu,
    Download,
    Load,
    Ready,
}
impl Step {
    pub fn steps(local: bool) -> &'static [Self] {
        if local {
            &[
                Self::Rent,
                Self::Gpu,
                Self::Download,
                Self::Load,
                Self::Ready,
            ]
        } else {
            &[
                Self::Rent,
                Self::Container,
                Self::Tunnel,
                Self::Gpu,
                Self::Download,
                Self::Load,
                Self::Ready,
            ]
        }
    }
    pub fn label(self, local: bool) -> &'static str {
        match (self, local) {
            (Self::Rent, true) => "start",
            (Self::Gpu, true) => "metal",
            (Self::Download, true) => "model",
            (Self::Rent, _) => "rent",
            (Self::Container, _) => "container",
            (Self::Tunnel, _) => "tunnel",
            (Self::Gpu, _) => "gpu",
            (Self::Download, _) => "download",
            (Self::Load, _) => "load",
            (Self::Ready, _) => "ready",
        }
    }
    pub fn from_up_phase(s: &str) -> Option<Self> {
        Some(match s {
            "create" => Self::Rent,
            "image" | "boot" => Self::Container,
            "tunnel" => Self::Tunnel,
            "gpu" => Self::Gpu,
            "download" | "verify" => Self::Download,
            "load" => Self::Load,
            "ready" => Self::Ready,
            _ => return None,
        })
    }
    pub fn index(self) -> usize {
        Self::steps(false).iter().position(|s| *s == self).unwrap()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../gen/")]
pub struct StepMark {
    pub step: Step,
    pub at_s: f64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../gen/")]
pub struct PanelState {
    pub phase: Phase,
    pub target: Target,
    pub provider: String,
    pub model: String,
    pub local_memory: Option<LocalMemory>,
    pub snap: Option<Snap>,
    pub config: Option<ConfigShow>,
    pub readiness: Option<Readiness>,
    pub models: Option<Listing>,
    pub catalog_ids: Vec<String>,
    pub download: Option<DownloadProgress>,
    pub steps: Vec<StepMark>,
    #[ts(type = "number | null")]
    pub boot_start_ms: Option<i64>,
    pub up_phase: Option<String>,
    pub last_detail: String,
    pub warning: Option<String>,
    pub log_tail: Vec<String>,
    pub ready_url: Option<String>,
    pub is_local: bool,
    pub boot_steps: Vec<Step>,
    pub current_step: Option<Step>,
    pub endpoint: Option<String>,
    pub menu_text: String,
    pub boot_progress: f64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../gen/")]
pub struct LocalMemory {
    pub model: String,
    #[ts(type = "number")]
    pub ctx: i64,
    #[ts(type = "'ready' | 'insufficient' | 'unavailable'")]
    pub status: String,
    pub message: String,
    #[ts(type = "number | null")]
    pub total_bytes: Option<u64>,
    #[ts(type = "number | null")]
    pub available_bytes: Option<u64>,
    #[ts(type = "number | null")]
    pub metal_limit_bytes: Option<u64>,
    #[ts(type = "number | null")]
    pub required_bytes: Option<u64>,
    #[ts(type = "number | null")]
    pub budget_bytes: Option<u64>,
}
impl From<lobo_core::local::memory::MemoryAssessment> for LocalMemory {
    fn from(a: lobo_core::local::memory::MemoryAssessment) -> Self {
        let ready = a.fits();
        Self {
            status: if ready { "ready" } else { "insufficient" }.into(),
            message: if ready {
                format!(
                    "Memory check passed · {} · {} context tokens",
                    a.model, a.ctx
                )
            } else {
                a.message()
            },
            model: a.model,
            ctx: a.ctx,
            total_bytes: Some(a.total_bytes),
            available_bytes: Some(a.available_bytes),
            metal_limit_bytes: Some(a.metal_limit_bytes),
            required_bytes: Some(a.required_bytes),
            budget_bytes: Some(a.budget_bytes),
        }
    }
}
impl LocalMemory {
    pub fn unavailable(model: String, ctx: i64, message: String) -> Self {
        Self {
            model,
            ctx,
            status: "unavailable".into(),
            message,
            total_bytes: None,
            available_bytes: None,
            metal_limit_bytes: None,
            required_bytes: None,
            budget_bytes: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../gen/")]
pub struct AppError {
    pub kind: String,
    pub message: String,
}
impl From<lobo_core::Error> for AppError {
    fn from(e: lobo_core::Error) -> Self {
        Self {
            kind: e.kind().into(),
            message: e.to_string(),
        }
    }
}
impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for AppError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Note {
    pub title: String,
    pub body: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn types_and_step_mapping() {
        assert_eq!(
            serde_json::to_value(Phase::Failed {
                message: "x".into()
            })
            .unwrap(),
            serde_json::json!({"kind":"failed", "message":"x"})
        );
        for (p, word) in [
            (Phase::Loading, "SCAN"),
            (Phase::NoConfig, "SETUP"),
            (Phase::Off, "OFF"),
            (Phase::Booting, "BOOT"),
            (Phase::Ready, "RUN"),
            (Phase::Stopping, "STOP"),
            (
                Phase::Failed {
                    message: String::new(),
                },
                "FAIL",
            ),
        ] {
            assert_eq!(p.word(), word);
        }
        assert_eq!(
            Step::steps(true)
                .iter()
                .map(|s| s.label(true))
                .collect::<Vec<_>>(),
            ["start", "metal", "model", "load", "ready"]
        );
        for (s, step) in [
            ("create", Some(Step::Rent)),
            ("image", Some(Step::Container)),
            ("boot", Some(Step::Container)),
            ("tunnel", Some(Step::Tunnel)),
            ("gpu", Some(Step::Gpu)),
            ("download", Some(Step::Download)),
            ("verify", Some(Step::Download)),
            ("load", Some(Step::Load)),
            ("ready", Some(Step::Ready)),
            ("warp", None),
        ] {
            assert_eq!(Step::from_up_phase(s), step);
        }
        assert_eq!(Step::Gpu.label(false), "gpu");
    }
    #[test]
    fn error_keeps_kind() {
        let e = lobo_core::Error::Local("x".into());
        let kind = e.kind().to_string();
        let a = AppError::from(e);
        assert_eq!(a.kind, kind);
        assert_eq!(a.message, "x");
    }

    #[test]
    fn app_types_export_without_bigint() {
        let ui = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../ui/src");
        let config = ts_rs::Config::default().with_out_dir(ui.join("proto"));
        Phase::export_all(&config).unwrap();
        Target::export_all(&config).unwrap();
        Step::export_all(&config).unwrap();
        StepMark::export_all(&config).unwrap();
        PanelState::export_all(&config).unwrap();
        AppError::export_all(&config).unwrap();
        let dir = ui.join("gen");
        let expected = [
            "AppError.ts",
            "LocalMemory.ts",
            "PanelState.ts",
            "Phase.ts",
            "Step.ts",
            "StepMark.ts",
            "Target.ts",
        ];
        let mut actual: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        actual.sort();
        assert_eq!(actual, expected);
        for name in expected {
            assert!(
                !std::fs::read_to_string(dir.join(name))
                    .unwrap()
                    .contains("bigint"),
                "{name}"
            );
        }
    }
}
