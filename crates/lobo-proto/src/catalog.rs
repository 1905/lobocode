use serde::{Deserialize, Serialize};
use std::sync::LazyLock;
use ts_rs::TS;

pub const CHUNK_SIZE: i64 = 256 << 20;

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct Model {
    pub id: String,
    pub file: String,
    pub sha256: String,
    pub alias: String,
    #[ts(type = "number")]
    pub size: i64,
    pub chunk_sha: Vec<String>,
    pub repo: String,
    pub name: String,
    pub quant: String,
    pub local: bool,
    /// Supported on rented GPUs.
    pub gpu: bool,
}

static CATALOG: LazyLock<Vec<Model>> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../catalog.json")).expect("invalid built-in model catalog")
});

#[derive(Debug, thiserror::Error)]
#[error("unknown model {id:?}, valid: {valid}")]
pub struct UnknownModel {
    pub id: String,
    pub valid: String,
}

pub fn all() -> &'static [Model] {
    &CATALOG
}

pub fn ids() -> Vec<&'static str> {
    all().iter().map(|m| m.id.as_str()).collect()
}

pub fn get(id: &str) -> Result<&'static Model, UnknownModel> {
    all()
        .iter()
        .find(|m| m.id == id)
        .ok_or_else(|| UnknownModel {
            id: id.to_owned(),
            valid: ids().join(", "),
        })
}

impl Model {
    pub fn url(&self, base: &str) -> String {
        format!("{}/models/{}", base.trim_end_matches('/'), self.sha256)
    }
    /// The original GGUF on Hugging Face; it serves byte ranges.
    pub fn hf_url(&self) -> String {
        format!(
            "https://huggingface.co/{}/resolve/main/{}",
            self.repo, self.file
        )
    }
}

pub fn min_free_mib(model_bytes: i64) -> i64 {
    (model_bytes >> 20) + 2560
}
