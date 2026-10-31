//! Shared persistent control state. Existing operation paths remain unchanged.
use std::path::{Path, PathBuf};

pub fn state_dir() -> PathBuf {
    let xdg = std::env::var("XDG_STATE_HOME").ok();
    let home = std::env::var_os("HOME").map(PathBuf::from);
    state_dir_from(xdg.as_deref(), home.as_deref())
}
fn state_dir_from(xdg: Option<&str>, home: Option<&Path>) -> PathBuf {
    let base = match xdg.filter(|v| !v.is_empty()) {
        Some(path) => PathBuf::from(path),
        None => home.unwrap_or_else(|| Path::new(".")).join(".local/state"),
    };
    base.join("lobo")
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn operation_directory_preserves_existing_location() {
        assert_eq!(
            state_dir_from(Some("/state"), Some(Path::new("/home"))),
            PathBuf::from("/state/lobo")
        );
        assert_eq!(
            state_dir_from(Some(""), Some(Path::new("/home"))),
            PathBuf::from("/home/.local/state/lobo")
        );
        assert_eq!(
            state_dir_from(None, None),
            PathBuf::from("./.local/state/lobo")
        );
    }
}
