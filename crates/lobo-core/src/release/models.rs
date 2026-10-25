use super::Store;
use crate::{Error, Result};
use lobo_proto::catalog::Model;
use sha2::{Digest, Sha256};
use std::{path::Path, time::Duration};
use tokio::io::{AsyncReadExt, BufReader};

pub const LICENSE: &str = include_str!("../../../../docker/pod/Apache-2.0.txt");

const TEXT: &str = "text/plain; charset=utf-8";
const CHUNK_CACHE: &str = "public, max-age=31536000, immutable";
const ATTEMPTS: u32 = 3;

pub fn model_txt(m: &Model) -> String {
    format!(
        "Model: {repo}\nSource: https://huggingface.co/{repo}\nOriginal GGUF SHA-256: {sha}\nLicense: Apache-2.0\n\nThis model is split into 256 MiB byte chunks; concatenating them in order restores the original GGUF.\n",
        repo = m.repo,
        sha = m.sha256,
    )
}
/// Uploads the model as content-addressed chunks and returns the sha256 of each one.
/// The whole file is verified before the first request: keys are immutable and
/// re-runs skip existing ones, so a wrong chunk would otherwise be pinned forever.
pub async fn publish_model(
    store: &Store,
    m: &Model,
    file: &Path,
    chunk: i64,
    note: &mut (dyn FnMut(String) + Send),
) -> Result<Vec<String>> {
    let size = tokio::fs::metadata(file).await?.len();
    if i64::try_from(size).ok() != Some(m.size) {
        return Err(Error::Release(format!(
            "{}: size {size}, catalog says {}",
            file.display(),
            m.size
        )));
    }
    let mut src = BufReader::with_capacity(4 << 20, tokio::fs::File::open(file).await?);
    let mut hash = Sha256::new();
    let mut buf = vec![0; 4 << 20];
    loop {
        let n = src.read(&mut buf).await?;
        if n == 0 {
            break;
        }
        hash.update(&buf[..n]);
    }
    let got = hex::encode(hash.finalize());
    if got != m.sha256 {
        return Err(Error::Release(format!(
            "{}: sha256 {got}, catalog says {}",
            file.display(),
            m.sha256
        )));
    }
    let prefix = format!("models/{}", m.sha256);
    for (name, text) in [
        ("LICENSE.txt", LICENSE.to_owned()),
        ("MODEL.txt", model_txt(m)),
    ] {
        let key = format!("{prefix}/{name}");
        if store.object_len(&key).await?.is_none() {
            store
                .put_immutable(&key, text.into_bytes(), TEXT, CHUNK_CACHE)
                .await?;
        }
    }
    let chunk = u64::try_from(chunk).map_err(|_| Error::Release("chunk size".into()))?;
    let mut src = tokio::fs::File::open(file).await?;
    let mut table = Vec::new();
    let (mut uploaded, mut skipped) = (0u32, 0u32);
    let mut bytes = 0u64;
    for (i, offset) in (0..size).step_by(chunk as usize).enumerate() {
        let len = chunk.min(size - offset);
        let mut data = vec![0; len as usize];
        src.read_exact(&mut data).await?;
        table.push(hex::encode(Sha256::digest(&data)));
        let key = format!("{prefix}/{i:03}");
        match store.object_len(&key).await? {
            Some(have) if have == len => skipped += 1,
            Some(have) => {
                return Err(Error::Release(format!(
                    "{key}: existing object is {have} bytes, expected {len}"
                )));
            }
            None => {
                put_chunk(store, &key, data).await?;
                uploaded += 1;
                bytes += len;
            }
        }
        if (i + 1) % 10 == 0 {
            note(format!(
                "chunk {}: uploaded={uploaded} skipped={skipped}",
                i + 1
            ));
        }
    }
    note(format!(
        "uploaded={uploaded} skipped={skipped} bytes={bytes}"
    ));
    Ok(table)
}
async fn put_chunk(store: &Store, key: &str, data: Vec<u8>) -> Result<()> {
    let mut attempt = 1;
    loop {
        let err = match store
            .put_immutable(key, data.clone(), "application/octet-stream", CHUNK_CACHE)
            .await
        {
            Ok(()) => return Ok(()),
            Err(e) => e,
        };
        let conflict = matches!(&err, Error::Release(m) if m.ends_with("already exists"));
        if conflict || attempt == ATTEMPTS {
            return Err(err);
        }
        tokio::time::sleep(Duration::from_secs(u64::from(attempt) * 2)).await;
        attempt += 1;
    }
}

#[cfg(test)]
mod tests;
