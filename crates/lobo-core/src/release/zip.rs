use super::Manifest;
use crate::{Error, Result};
use ::zip::{CompressionMethod, ZipArchive, ZipWriter, write::SimpleFileOptions};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Write},
    path::Path,
};

pub fn build_zip(agent_bin: &Path, m: &Manifest, out: &Path) -> Result<String> {
    let bin = fs::read(agent_bin)?;
    let manifest = serde_json::to_vec_pretty(m)?;
    let mut writer = ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (name, mode, data) in [
        ("lobo-agent", 0o755, bin),
        ("release.json", 0o644, manifest),
    ] {
        writer
            .start_file(
                name,
                SimpleFileOptions::default()
                    .compression_method(CompressionMethod::Deflated)
                    .unix_permissions(mode),
            )
            .map_err(|e| Error::Release(e.to_string()))?;
        writer.write_all(&data)?;
    }
    let bytes = writer
        .finish()
        .map_err(|e| Error::Release(e.to_string()))?
        .into_inner();
    fs::write(out, &bytes)?;
    Ok(hex::encode(Sha256::digest(&bytes)))
}

pub fn scan_for_secrets(zip: &Path, secrets: &BTreeMap<String, String>) -> Result<()> {
    let mut archive =
        ZipArchive::new(fs::File::open(zip)?).map_err(|e| Error::Release(e.to_string()))?;
    for n in 0..archive.len() {
        let mut entry = archive
            .by_index(n)
            .map_err(|e| Error::Release(e.to_string()))?;
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes)?;
        for (name, value) in secrets {
            if value.len() >= 8 && bytes.windows(value.len()).any(|w| w == value.as_bytes()) {
                return Err(Error::Release(format!(
                    "release: {name} found in {}, refusing to publish",
                    entry.name()
                )));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn build_zip_and_scan() {
        let tmp = tempfile::tempdir().unwrap();
        let bin = tmp.path().join("agent");
        let zip = tmp.path().join("r.zip");
        fs::write(&bin, "ELF binary bytes").unwrap();
        let manifest = Manifest {
            version: "2026.09.23-1".into(),
            git_sha: "abc1234".into(),
            ..Default::default()
        };
        let sha = build_zip(&bin, &manifest, &zip).unwrap();
        assert_eq!(sha, hex::encode(Sha256::digest(fs::read(&zip).unwrap())));
        let mut archive = ZipArchive::new(fs::File::open(&zip).unwrap()).unwrap();
        assert_eq!(archive.len(), 2);
        {
            let f = archive.by_index(0).unwrap();
            assert_eq!(f.name(), "lobo-agent");
            assert_eq!(f.unix_mode().unwrap() & 0o777, 0o755);
        }
        {
            let mut f = archive.by_index(1).unwrap();
            assert_eq!(f.name(), "release.json");
            assert_eq!(f.unix_mode().unwrap() & 0o777, 0o644);
            let mut bytes = Vec::new();
            f.read_to_end(&mut bytes).unwrap();
            assert_eq!(
                serde_json::from_slice::<Manifest>(&bytes).unwrap(),
                manifest
            );
        }
        scan_for_secrets(
            &zip,
            &[
                ("LOBO_API_KEY".into(), "sk-notinzip-123".into()),
                ("SHORT".into(), "ELF".into()),
            ]
            .into(),
        )
        .unwrap();
        let e = scan_for_secrets(
            &zip,
            &[("R2_SECRET_KEY".into(), "binary bytes".into())].into(),
        )
        .unwrap_err()
        .to_string();
        assert!(e.contains("R2_SECRET_KEY"));
        assert!(!e.contains("binary bytes"));
    }
    #[test]
    fn scan_catches_laptop_secret_values() {
        let tmp = tempfile::tempdir().unwrap();
        let bin = tmp.path().join("agent");
        let zip = tmp.path().join("r.zip");
        let mut laptop = crate::config::Laptop::default();
        laptop.r2.secret_key = "private-secret-value".into();
        fs::write(&bin, format!("embedded {}", laptop.r2.secret_key)).unwrap();
        build_zip(&bin, &Manifest::default(), &zip).unwrap();
        let e = scan_for_secrets(&zip, &laptop.secret_values())
            .unwrap_err()
            .to_string();
        assert!(e.contains("R2_SECRET_KEY"));
        assert!(!e.contains(&laptop.r2.secret_key));
    }
}
