use crate::types::Target;
use serde::{Deserialize, Serialize};
use std::{io::Write, path::Path};

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Prefs {
    pub target: Option<Target>,
}
impl Prefs {
    pub fn load(dir: &Path) -> Self {
        std::fs::read(dir.join("prefs.json"))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }
    pub fn save(&self, dir: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(dir)?;
        let mut f = tempfile::NamedTempFile::new_in(dir)?;
        serde_json::to_writer(&mut f, self)?;
        f.flush()?;
        f.persist(dir.join("prefs.json")).map_err(|e| e.error)?;
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn target_persists() {
        let d = tempfile::tempdir().unwrap();
        assert_eq!(Prefs::load(d.path()).target, None);
        Prefs {
            target: Some(Target::Local),
        }
        .save(d.path())
        .unwrap();
        assert_eq!(Prefs::load(d.path()).target, Some(Target::Local));
        std::fs::write(d.path().join("prefs.json"), "bad").unwrap();
        assert_eq!(Prefs::load(d.path()).target, None);
    }
}
