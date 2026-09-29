use super::runtime;
use crate::Result;
use lobo_proto::{
    Listing, ModelState, RuntimeInfo,
    catalog::{self, Model},
};
use std::{
    fs,
    io::Write,
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::{Path, PathBuf},
};

pub const HF_BASE: &str =
    "https://huggingface.co/HauhauCS/Qwen3.5-27B-Uncensored-HauhauCS-Aggressive/resolve/main/";
pub fn marker_path(weights: &Path, file: &str) -> PathBuf {
    weights.join(format!("{file}.sha256-ok"))
}
pub fn marker_line(sha: &str, meta: &fs::Metadata) -> String {
    let nanos = i128::from(meta.mtime()) * 1_000_000_000 + i128::from(meta.mtime_nsec());
    format!("{sha} {} {nanos}\n", meta.len())
}
pub fn marker_valid(weights: &Path, m: &Model, meta: Option<&fs::Metadata>) -> bool {
    let Some(meta) = meta else { return false };
    if i64::try_from(meta.len()).ok() != Some(m.size) {
        return false;
    }
    fs::read_to_string(marker_path(weights, &m.file))
        .is_ok_and(|s| s == marker_line(&m.sha256, meta))
}
pub fn write_marker(weights: &Path, m: &Model) -> Result<()> {
    let meta = fs::metadata(weights.join(&m.file))?;
    let mut temp = tempfile::Builder::new()
        .prefix(".sha256-ok-")
        .tempfile_in(weights)?;
    temp.write_all(marker_line(&m.sha256, &meta).as_bytes())?;
    temp.flush()?;
    temp.as_file()
        .set_permissions(fs::Permissions::from_mode(0o644))?;
    temp.persist(marker_path(weights, &m.file))
        .map_err(|e| e.error)?;
    Ok(())
}
pub fn list(weights: &Path) -> Result<Listing> {
    let mut listing = Listing {
        weights: weights.to_string_lossy().into(),
        free_bytes: free_space(weights)?,
        runtime: RuntimeInfo {
            version: runtime::RUNTIME_VERSION.into(),
            present: runtime::find_server(&runtime::runtime_dir(weights)).is_ok(),
        },
        ..Default::default()
    };
    for m in catalog::all() {
        let meta = fs::metadata(weights.join(&m.file)).ok();
        listing.models.push(ModelState {
            id: m.id.clone(),
            file: m.file.clone(),
            size: m.size,
            on_disk: meta.as_ref().map(|m| m.len() as i64).unwrap_or_default(),
            verified: marker_valid(weights, m, meta.as_ref()),
        });
    }
    Ok(listing)
}
pub fn free_space(dir: &Path) -> Result<u64> {
    match nix::sys::statvfs::statvfs(dir) {
        // Darwin's fsblkcnt_t is u32; Linux uses u64.
        #[allow(clippy::useless_conversion)]
        Ok(s) => Ok(u64::from(s.blocks_available()).saturating_mul(s.fragment_size())),
        Err(nix::errno::Errno::ENOENT) => Ok(0),
        Err(e) => Err(std::io::Error::from(e).into()),
    }
}

#[cfg(test)]
mod tests;
