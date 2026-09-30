//! Check bundled GGUF shards without fetching or copying model weights at boot.
use crate::{Error, Result, runner::Download};
use async_trait::async_trait;
use lobo_proto::{DownloadProgress, catalog::Model};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use tokio::io::AsyncReadExt;
use tokio_util::sync::CancellationToken;

pub const MAX_SHARDS: usize = 8;
pub const MAX_SHARD_BYTES: u64 = 5_000_000_000;
const MAX_MANIFEST_BYTES: u64 = 64 * 1024;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Shard {
    pub file: String,
    pub size: u64,
    pub sha256: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub model: String,
    pub source_sha256: String,
    pub shards: Vec<Shard>,
}
pub struct ImageModel {
    root: PathBuf,
    manifest: Manifest,
}
impl ImageModel {
    pub async fn load(root: &Path, manifest_path: &Path, model: &Model) -> Result<Self> {
        let root_meta = tokio::fs::symlink_metadata(root).await?;
        let metadata = tokio::fs::symlink_metadata(manifest_path).await?;
        if !root_meta.is_dir() || !metadata.is_file() || metadata.len() > MAX_MANIFEST_BYTES {
            return Err(Error::msg(
                "Docker image model directory or manifest is invalid",
            ));
        }
        // Bound the read too, so a changing manifest cannot allocate unbounded memory.
        let mut bytes = Vec::new();
        tokio::fs::File::open(manifest_path)
            .await?
            .take(MAX_MANIFEST_BYTES + 1)
            .read_to_end(&mut bytes)
            .await?;
        if bytes.len() as u64 > MAX_MANIFEST_BYTES {
            return Err(Error::msg("Docker image model manifest is too large"));
        }
        let manifest: Manifest = serde_json::from_slice(&bytes)
            .map_err(|e| Error::msg(format!("Docker image model manifest: {e}")))?;
        if manifest.model != model.id
            || manifest.source_sha256 != model.sha256
            || manifest.shards.is_empty()
            || manifest.shards.len() > MAX_SHARDS
        {
            return Err(Error::msg(
                "Docker image does not contain the selected model",
            ));
        }
        let count = manifest.shards.len();
        for (index, shard) in manifest.shards.iter().enumerate() {
            let expected = format!("model-{:05}-of-{count:05}.gguf", index + 1);
            if shard.file != expected
                || shard.size < 24
                || shard.size >= MAX_SHARD_BYTES
                || shard.sha256.len() != 64
                || !shard
                    .sha256
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            {
                return Err(Error::msg("Docker image has an invalid model manifest"));
            }
        }
        Ok(Self {
            root: root.into(),
            manifest,
        })
    }
    pub fn model_path(&self) -> PathBuf {
        self.root.join(&self.manifest.shards[0].file)
    }
}
#[async_trait]
impl Download for ImageModel {
    async fn run(
        &self,
        cancel: CancellationToken,
        on_progress: &(dyn Fn(DownloadProgress) + Sync),
    ) -> Result<()> {
        let total: u64 = self.manifest.shards.iter().map(|s| s.size).sum();
        let mut done = 0u64;
        let mut buffer = vec![0u8; 1024 * 1024];
        if cancel.is_cancelled() {
            return Err(Error::Cancelled);
        }
        on_progress(DownloadProgress {
            total: total as i64,
            verifying: true,
            source: "Docker image".into(),
            ..Default::default()
        });
        for shard in &self.manifest.shards {
            if cancel.is_cancelled() {
                return Err(Error::Cancelled);
            }
            let path = self.root.join(&shard.file);
            let meta = tokio::fs::symlink_metadata(&path).await?;
            if !meta.is_file() || meta.len() != shard.size {
                return Err(Error::msg(format!(
                    "Docker image model shard is missing or has the wrong size: {}",
                    shard.file
                )));
            }
            let mut file = tokio::fs::File::open(path).await?;
            let mut hash = Sha256::new();
            let mut header = [0u8; 24];
            tokio::select! { biased; _ = cancel.cancelled() => return Err(Error::Cancelled), result = file.read_exact(&mut header) => {result?;} };
            if &header[..4] != b"GGUF"
                || !matches!(u32::from_le_bytes(header[4..8].try_into().unwrap()), 2 | 3)
            {
                return Err(Error::msg(
                    "Docker image model shard has an invalid GGUF header",
                ));
            }
            hash.update(header);
            let mut shard_done = header.len() as u64;
            done += shard_done;
            on_progress(DownloadProgress {
                bytes: done as i64,
                total: total as i64,
                verifying: true,
                source: "Docker image".into(),
                ..Default::default()
            });
            loop {
                let n = tokio::select! { biased; _ = cancel.cancelled() => return Err(Error::Cancelled), n = file.read(&mut buffer) => n? };
                if n == 0 {
                    break;
                }
                hash.update(&buffer[..n]);
                shard_done += n as u64;
                if shard_done > shard.size {
                    return Err(Error::msg(
                        "Docker image model shard changed during verification",
                    ));
                }
                done += n as u64;
                on_progress(DownloadProgress {
                    bytes: done as i64,
                    total: total as i64,
                    verifying: true,
                    source: "Docker image".into(),
                    ..Default::default()
                });
            }
            if shard_done != shard.size || hex::encode(hash.finalize()) != shard.sha256 {
                return Err(Error::msg(format!(
                    "Docker image model shard failed SHA-256 verification: {}",
                    shard.file
                )));
            }
        }
        if cancel.is_cancelled() {
            return Err(Error::Cancelled);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    struct Fixture {
        dir: tempfile::TempDir,
        model: Model,
        manifest: Manifest,
    }
    impl Fixture {
        async fn new() -> Self {
            let dir = tempfile::tempdir().unwrap();
            let model = lobo_proto::catalog::get("q6").unwrap().clone();
            let mut manifest = Manifest {
                model: model.id.clone(),
                source_sha256: model.sha256.clone(),
                shards: vec![],
            };
            for i in 1..=2 {
                let file = format!("model-{i:05}-of-00002.gguf");
                let mut bytes = b"GGUF\x03\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0".to_vec();
                bytes.resize(2 * 1024 * 1024, i as u8);
                tokio::fs::write(dir.path().join(&file), &bytes)
                    .await
                    .unwrap();
                manifest.shards.push(Shard {
                    file,
                    size: bytes.len() as u64,
                    sha256: hex::encode(Sha256::digest(bytes)),
                });
            }
            Self {
                dir,
                model,
                manifest,
            }
        }
        async fn load(&self) -> Result<ImageModel> {
            let path = self.dir.path().join("model.json");
            tokio::fs::write(&path, serde_json::to_vec(&self.manifest).unwrap())
                .await
                .unwrap();
            ImageModel::load(self.dir.path(), &path, &self.model).await
        }
    }

    #[tokio::test]
    async fn shards_report_progress_and_load_only_the_first_path() {
        let f = Fixture::new().await;
        let bundled = f.load().await.unwrap();
        assert_eq!(
            bundled.model_path(),
            f.dir.path().join(&f.manifest.shards[0].file)
        );
        let progress = Mutex::new(Vec::new());
        bundled
            .run(CancellationToken::new(), &|p| {
                progress.lock().unwrap().push(p)
            })
            .await
            .unwrap();
        let progress = progress.into_inner().unwrap();
        assert!(progress.windows(2).all(|p| p[0].bytes <= p[1].bytes));
        assert!(
            progress
                .iter()
                .all(|p| p.verifying && p.source == "Docker image")
        );
        assert_eq!(progress.first().unwrap().bytes, 0);
        assert_eq!(progress.last().unwrap().bytes, 4 * 1024 * 1024);
        assert_eq!(progress.last().unwrap().total, 4 * 1024 * 1024);
        assert_eq!(
            std::fs::read_dir(f.dir.path()).unwrap().count(),
            3,
            "verification must not merge/copy shards"
        );
    }

    #[tokio::test]
    async fn verification_cancels_between_reads() {
        let f = Fixture::new().await;
        let bundled = f.load().await.unwrap();
        let cancel = CancellationToken::new();
        let seen = Mutex::new(0);
        assert!(matches!(
            bundled
                .run(cancel.clone(), &|p| {
                    *seen.lock().unwrap() = p.bytes;
                    if p.bytes > 24 {
                        cancel.cancel();
                    }
                })
                .await,
            Err(Error::Cancelled)
        ));
        assert_eq!(*seen.lock().unwrap(), 1024 * 1024 + 24);
    }

    #[tokio::test]
    async fn rejects_invalid_manifest_paths_sizes_hashes_and_catalog_identity() {
        let mut f = Fixture::new().await;
        for name in [
            "../model.gguf",
            "/tmp/model.gguf",
            "model-00002-of-00002.gguf",
            "model-00001-of-00001.gguf",
        ] {
            f.manifest.shards[0].file = name.into();
            assert!(f.load().await.is_err(), "accepted {name}");
        }
        f.manifest.shards[0].file = "model-00001-of-00002.gguf".into();
        let size = f.manifest.shards[0].size;
        for bad in [0, MAX_SHARD_BYTES, u64::MAX] {
            f.manifest.shards[0].size = bad;
            assert!(f.load().await.is_err(), "accepted size {bad}");
        }
        f.manifest.shards[0].size = size;
        let hash = f.manifest.shards[0].sha256.clone();
        for bad in ["z".repeat(64), "0".repeat(63)] {
            f.manifest.shards[0].sha256 = bad;
            assert!(f.load().await.is_err());
        }
        f.manifest.shards[0].sha256 = hash;
        f.manifest.source_sha256 = "0".repeat(64);
        assert!(f.load().await.is_err());
        f.manifest.source_sha256 = f.model.sha256.clone();
        f.manifest.model = "q8".into();
        assert!(f.load().await.is_err());
        f.manifest.model = f.model.id.clone();
        f.manifest.shards.clear();
        assert!(f.load().await.is_err());
    }

    #[tokio::test]
    async fn rejects_missing_wrong_size_symlink_directory_and_non_gguf_content() {
        let mut f = Fixture::new().await;
        let path = f.dir.path().join(&f.manifest.shards[0].file);
        let original = tokio::fs::read(&path).await.unwrap();
        tokio::fs::remove_file(&path).await.unwrap();
        assert!(
            f.load()
                .await
                .unwrap()
                .run(CancellationToken::new(), &|_| {})
                .await
                .is_err()
        );
        tokio::fs::write(&path, b"short").await.unwrap();
        assert!(
            f.load()
                .await
                .unwrap()
                .run(CancellationToken::new(), &|_| {})
                .await
                .is_err()
        );
        tokio::fs::remove_file(&path).await.unwrap();
        std::os::unix::fs::symlink(f.dir.path().join(&f.manifest.shards[1].file), &path).unwrap();
        assert!(
            f.load()
                .await
                .unwrap()
                .run(CancellationToken::new(), &|_| {})
                .await
                .is_err()
        );
        tokio::fs::remove_file(&path).await.unwrap();
        tokio::fs::create_dir(&path).await.unwrap();
        assert!(
            f.load()
                .await
                .unwrap()
                .run(CancellationToken::new(), &|_| {})
                .await
                .is_err()
        );
        tokio::fs::remove_dir(&path).await.unwrap();
        let mut invalid = original;
        invalid[..4].copy_from_slice(b"NOPE");
        f.manifest.shards[0].sha256 = hex::encode(Sha256::digest(&invalid));
        tokio::fs::write(&path, invalid).await.unwrap();
        assert!(
            f.load()
                .await
                .unwrap()
                .run(CancellationToken::new(), &|_| {})
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn rejects_symlink_or_oversized_manifest() {
        let f = Fixture::new().await;
        f.load().await.unwrap();
        let real = f.dir.path().join("model.json");
        let link = f.dir.path().join("link.json");
        std::os::unix::fs::symlink(&real, &link).unwrap();
        assert!(
            ImageModel::load(f.dir.path(), &link, &f.model)
                .await
                .is_err()
        );
        tokio::fs::write(&real, vec![b' '; MAX_MANIFEST_BYTES as usize + 1])
            .await
            .unwrap();
        assert!(
            ImageModel::load(f.dir.path(), &real, &f.model)
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn bundled_model_verifies_without_network_and_rejects_corruption() {
        let tmp = tempfile::tempdir().unwrap();
        let model = lobo_proto::catalog::get("q6").unwrap();
        let bytes = b"GGUF\x03\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0fixture";
        let file = "model-00001-of-00001.gguf";
        let manifest = Manifest {
            model: "q6".into(),
            source_sha256: model.sha256.clone(),
            shards: vec![Shard {
                file: file.into(),
                size: bytes.len() as u64,
                sha256: hex::encode(Sha256::digest(bytes)),
            }],
        };
        let metadata = tmp.path().join("model.json");
        tokio::fs::write(&metadata, serde_json::to_vec(&manifest).unwrap())
            .await
            .unwrap();
        tokio::fs::write(tmp.path().join(file), bytes)
            .await
            .unwrap();
        let bundled = ImageModel::load(tmp.path(), &metadata, model)
            .await
            .unwrap();
        assert_eq!(bundled.model_path(), tmp.path().join(file));
        bundled
            .run(CancellationToken::new(), &|p| {
                assert!(p.verifying);
                assert_eq!(p.source, "Docker image");
            })
            .await
            .unwrap();
        let mut corrupted = bytes.to_vec();
        *corrupted.last_mut().unwrap() ^= 1;
        tokio::fs::write(tmp.path().join(file), corrupted)
            .await
            .unwrap();
        assert!(
            bundled
                .run(CancellationToken::new(), &|_| {})
                .await
                .is_err()
        );
        assert!(
            ImageModel::load(
                tmp.path(),
                &metadata,
                lobo_proto::catalog::get("q8").unwrap()
            )
            .await
            .is_err()
        );
        let cancel = CancellationToken::new();
        cancel.cancel();
        assert!(matches!(
            bundled.run(cancel, &|_| {}).await,
            Err(Error::Cancelled)
        ));
        tokio::fs::remove_file(tmp.path().join(file)).await.unwrap();
        assert!(
            bundled
                .run(CancellationToken::new(), &|_| {})
                .await
                .is_err()
        );
    }
}
