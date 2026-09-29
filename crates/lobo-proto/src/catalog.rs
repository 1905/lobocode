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

pub fn get(id: &str) -> Result<&'static Model, UnknownModel> {
    all()
        .iter()
        .find(|m| m.id == id)
        .ok_or_else(|| UnknownModel {
            id: id.to_owned(),
            valid: all()
                .iter()
                .map(|m| m.id.as_str())
                .collect::<Vec<_>>()
                .join(", "),
        })
}

impl Model {
    pub fn url(&self, bucket_url: &str) -> String {
        format!("{}/models/{}", bucket_url.trim_end_matches('/'), self.file)
    }
}

pub fn min_free_mib(model_bytes: i64) -> i64 {
    (model_bytes >> 20) + 2560
}
