use lobo_proto::{ConfigShow, DownloadProgress, Readiness, Snap};
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
    pub fn steps() -> &'static [Self] {
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
    pub fn label(self) -> &'static str {
        match self {
            Self::Rent => "rent",
            Self::Container => "container",
            Self::Tunnel => "tunnel",
            Self::Gpu => "gpu",
            Self::Download => "download",
            Self::Load => "load",
            Self::Ready => "ready",
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
        Self::steps().iter().position(|s| *s == self).unwrap()
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
    pub provider: String,
    pub model: String,
    pub snap: Option<Snap>,
    pub config: Option<ConfigShow>,
    pub readiness: Option<Readiness>,
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
    pub boot_steps: Vec<Step>,
    pub current_step: Option<Step>,
    pub endpoint: Option<String>,
    pub menu_text: String,
    pub boot_progress: f64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../gen/")]
pub struct OpenCodeInfo {
    pub path: String,
    pub endpoint: Option<String>,
    pub provider: Option<String>,
    pub model_alias: Option<String>,
    #[ts(type = "number | null")]
    pub context: Option<u64>,
    pub can_configure: bool,
    pub reason: Option<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../gen/")]
pub struct OpenCodeResult {
    pub path: String,
    pub provider: String,
    pub model_alias: String,
    pub changed: bool,
    pub message: String,
    pub warnings: Vec<String>,
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
            Step::steps().iter().map(|s| s.label()).collect::<Vec<_>>(),
            [
                "rent",
                "container",
                "tunnel",
                "gpu",
                "download",
                "load",
                "ready"
            ]
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
        assert_eq!(Step::Gpu.label(), "gpu");
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
        for name in ["Target.ts", "LocalMemory.ts"] {
            let path = ui.join("gen").join(name);
            if path.exists() {
                std::fs::remove_file(path).unwrap();
            }
        }
        let config = ts_rs::Config::default().with_out_dir(ui.join("proto"));
        Phase::export_all(&config).unwrap();
        Step::export_all(&config).unwrap();
        StepMark::export_all(&config).unwrap();
        PanelState::export_all(&config).unwrap();
        AppError::export_all(&config).unwrap();
        OpenCodeInfo::export_all(&config).unwrap();
        OpenCodeResult::export_all(&config).unwrap();
        let dir = ui.join("gen");
        let expected = [
            "AppError.ts",
            "OpenCodeInfo.ts",
            "OpenCodeResult.ts",
            "PanelState.ts",
            "Phase.ts",
            "Step.ts",
            "StepMark.ts",
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
