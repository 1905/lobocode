use serde::{Deserialize, Serialize};
use std::path::Path;

// Legacy target preferences are ignored. Reading never rewrites the user's file.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Prefs {}
impl Prefs {
    pub fn load(dir: &Path) -> Self {
        std::fs::read(dir.join("prefs.json"))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn legacy_local_preference_is_ignored_and_preserved() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("prefs.json");
        let original = b"{\"target\":\"local\"}";
        std::fs::write(&path, original).unwrap();
        let prefs = Prefs::load(root.path());
        assert_eq!(serde_json::to_value(prefs).unwrap(), serde_json::json!({}));
        assert_eq!(std::fs::read(path).unwrap(), original);
    }
}
