use crate::{Error, Result};
use flate2::read::GzDecoder;
use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{OpenOptionsExt, PermissionsExt, symlink},
    path::{Component, Path, PathBuf},
};
use tokio_util::sync::CancellationToken;

pub const RUNTIME_VERSION: &str = "b11118";
pub struct RuntimePin {
    pub url: String,
    pub size: i64,
    pub sha256: String,
}
impl RuntimePin {
    pub fn pinned() -> Self {
        Self {
            url: format!(
                "https://github.com/ggml-org/llama.cpp/releases/download/{RUNTIME_VERSION}/llama-{RUNTIME_VERSION}-bin-macos-arm64.tar.gz"
            ),
            size: 11_205_140,
            sha256: "ca0ea3156257b21eeb11d0628f2baecd3928013a3d060e2e192042276e5b1f35".into(),
        }
    }
}
pub fn runtime_dir(weights: &Path) -> PathBuf {
    weights
        .join("runtime")
        .join(format!("llama-{RUNTIME_VERSION}"))
}
pub fn runtime_note(size: i64) -> String {
    format!("llama.cpp {RUNTIME_VERSION} {} MB", size / 1_000_000)
}
pub async fn ensure_runtime(
    weights: &Path,
    cancel: CancellationToken,
    note: &(dyn Fn(String) + Sync),
) -> Result<PathBuf> {
    ensure_runtime_with(weights, &RuntimePin::pinned(), cancel, note).await
}
pub async fn ensure_runtime_with(
    weights: &Path,
    pin: &RuntimePin,
    cancel: CancellationToken,
    note: &(dyn Fn(String) + Sync),
) -> Result<PathBuf> {
    check_cancel(&cancel)?;
    let dir = runtime_dir(weights);
    if let Ok(server) = find_server(&dir) {
        return Ok(server);
    }
    let root = dir.parent().unwrap();
    fs::create_dir_all(root)?;
    note(runtime_note(pin.size));
    let tgz = tempfile::Builder::new()
        .prefix(".llama-")
        .suffix(".tar.gz")
        .tempfile_in(root)?;
    tokio::select! {biased;
        _ = cancel.cancelled() => return Err(Error::Cancelled),
        result = lobo_agent::fetch::fetch_file(&pin.url,tgz.path(),0o600,pin.size,&pin.sha256) => result?,
    }
    check_cancel(&cancel)?;
    let temp = tempfile::Builder::new()
        .prefix(".llama-")
        .tempdir_in(root)?;
    // The blocking task owns both temporary paths. Await it through cancellation
    // so cleanup is complete before the operation can finish.
    tokio::task::spawn_blocking(move || {
        untar_with_cancel(tgz.path(), temp.path(), &cancel)?;
        find_server(temp.path())?;
        check_cancel(&cancel)?;
        fs::set_permissions(temp.path(), fs::Permissions::from_mode(0o755))?;
        if let Err(e) = fs::rename(temp.path(), &dir) {
            if let Ok(server) = find_server(&dir) {
                return Ok(server);
            }
            return Err(e.into());
        }
        find_server(&dir)
    })
    .await
    .map_err(|e| Error::Local(format!("runtime extraction worker: {e}")))?
}

fn check_cancel(cancel: &CancellationToken) -> Result<()> {
    if cancel.is_cancelled() {
        Err(Error::Cancelled)
    } else {
        Ok(())
    }
}
fn relative(path: &Path) -> Option<PathBuf> {
    if path.as_os_str().is_empty() {
        return None;
    }
    let mut out = PathBuf::new();
    for c in path.components() {
        match c {
            Component::Normal(s) => out.push(s),
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    return None;
                }
            }
            _ => return None,
        }
    }
    Some(out)
}
fn directories(root: &Path, rel: &Path) -> Result<()> {
    let mut current = root.to_path_buf();
    for c in rel.components() {
        current.push(c);
        match fs::symlink_metadata(&current) {
            Ok(m) if m.is_dir() && !m.file_type().is_symlink() => {}
            Ok(_) => {
                return Err(Error::Local(format!(
                    "unsafe path through non-directory {}",
                    current.display()
                )));
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => fs::create_dir(&current)?,
            Err(e) => return Err(e.into()),
        }
    }
    Ok(())
}
pub fn untar(src: &Path, dst: &Path) -> Result<()> {
    untar_with_cancel(src, dst, &CancellationToken::new())
}
fn untar_with_cancel(src: &Path, dst: &Path, cancel: &CancellationToken) -> Result<()> {
    let mut archive = tar::Archive::new(GzDecoder::new(fs::File::open(src)?));
    let mut links = Vec::new();
    let mut buffer = [0u8; 64 * 1024];
    for entry in archive.entries()? {
        check_cancel(cancel)?;
        let mut entry = entry?;
        if entry.header().entry_type().is_pax_global_extensions() {
            continue;
        }
        let name = entry.path()?.into_owned();
        let rel = relative(&name).ok_or_else(|| Error::Local(format!("unsafe path {name:?}")))?;
        let path = dst.join(&rel);
        let mode = entry.header().mode()? & 0o777;
        let kind = entry.header().entry_type();
        if kind.is_dir() {
            directories(dst, &rel)?;
            fs::set_permissions(&path, fs::Permissions::from_mode(mode | 0o700))?;
        } else if kind.is_file() {
            directories(dst, rel.parent().unwrap_or_else(|| Path::new("")))?;
            let mut out = OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&path)?;
            loop {
                check_cancel(cancel)?;
                let n = entry.read(&mut buffer)?;
                if n == 0 {
                    break;
                }
                out.write_all(&buffer[..n])?;
            }
            out.set_permissions(fs::Permissions::from_mode(mode))?;
        } else if kind.is_symlink() {
            let link = entry
                .link_name()?
                .ok_or_else(|| Error::Local(format!("unsafe symlink {name:?}: missing target")))?
                .into_owned();
            let combined = rel.parent().unwrap_or_else(|| Path::new("")).join(&link);
            if link.is_absolute() || relative(&combined).is_none() {
                return Err(Error::Local(format!("unsafe symlink {name:?} -> {link:?}")));
            }
            links.push((rel, link));
        } else {
            return Err(Error::Local(format!(
                "unsupported tar entry {name:?} (type {:?})",
                kind.as_byte()
            )));
        }
    }
    // Deferred links cannot redirect extraction. Validate their resolved targets
    // too: two individually relative links can otherwise escape the tree.
    for (rel, link) in &links {
        check_cancel(cancel)?;
        directories(dst, rel.parent().unwrap_or_else(|| Path::new("")))?;
        symlink(link, dst.join(rel))?;
    }
    let root = fs::canonicalize(dst)?;
    for (rel, link) in links {
        check_cancel(cancel)?;
        let target = fs::canonicalize(dst.join(&rel))
            .map_err(|e| Error::Local(format!("unsafe symlink {rel:?} -> {link:?}: {e}")))?;
        if !target.starts_with(&root) {
            return Err(Error::Local(format!("unsafe symlink {rel:?} -> {link:?}")));
        }
    }
    Ok(())
}
pub fn find_server(dir: &Path) -> Result<PathBuf> {
    fn visit(path: &Path) -> std::io::Result<Option<PathBuf>> {
        let meta = fs::symlink_metadata(path)?;
        if meta.is_file()
            && path.file_name().is_some_and(|n| n == "llama-server")
            && meta.permissions().mode() & 0o111 != 0
        {
            return Ok(Some(path.to_owned()));
        }
        if meta.is_dir() {
            let mut paths = fs::read_dir(path)?
                .map(|e| e.map(|e| e.path()))
                .collect::<std::io::Result<Vec<_>>>()?;
            paths.sort();
            for child in paths {
                if let Some(p) = visit(&child)? {
                    return Ok(Some(p));
                }
            }
        }
        Ok(None)
    }
    visit(dir)?.ok_or_else(|| Error::Local(format!("no llama-server in {}", dir.display())))
}

#[cfg(test)]
mod tests;
