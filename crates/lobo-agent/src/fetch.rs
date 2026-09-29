use crate::{Error, Result};
use futures_util::StreamExt;
use sha2::{Digest, Sha256};
use std::{path::Path, time::Duration};
use tokio::io::AsyncWriteExt;

pub async fn fetch_file(url: &str, dst: &Path, mode: u32, size: i64, sha: &str) -> Result<()> {
    tokio::time::timeout(Duration::from_secs(300), async {
        let label = crate::source::redact_url(url);
        let response = crate::http::client()
            .get(url)
            .send()
            .await
            .map_err(|e| Error::msg(e.without_url().to_string()))?;
        if response.status() != 200 {
            return Err(Error::msg(format!(
                "fetch {label}: HTTP {}",
                response.status().as_u16()
            )));
        }
        let mut file = tokio::fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .mode(mode)
            .open(dst)
            .await?;
        let mut stream = response.bytes_stream();
        let mut n = 0_i64;
        let mut hash = Sha256::new();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| Error::msg(e.without_url().to_string()))?;
            let len = if size >= 0 {
                chunk
                    .len()
                    .min((size.saturating_add(1) - n).max(0) as usize)
            } else {
                chunk.len()
            };
            file.write_all(&chunk[..len]).await?;
            hash.update(&chunk[..len]);
            n += len as i64;
            if size >= 0 && n > size {
                break;
            }
        }
        file.flush().await?;
        if size >= 0 && n != size {
            return Err(Error::msg(format!("fetch {label}: size {n}, want {size}")));
        }
        let got = hex::encode(hash.finalize());
        if !sha.is_empty() && got != sha {
            return Err(Error::msg(format!(
                "fetch {label}: sha256 {got}, want {sha}"
            )));
        }
        Ok(())
    })
    .await
    .map_err(|_| Error::msg("fetch: timeout after 5m0s"))?
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use wiremock::{Mock, MockServer, ResponseTemplate, matchers::any};
    #[tokio::test]
    async fn fetch_file_ok_sets_mode() {
        let server = MockServer::start().await;
        Mock::given(any())
            .respond_with(ResponseTemplate::new(200).set_body_string("hello"))
            .mount(&server)
            .await;
        let temp = tempfile::tempdir().unwrap();
        let dst = temp.path().join("binary");
        fetch_file(
            &server.uri(),
            &dst,
            0o755,
            5,
            &hex::encode(Sha256::digest(b"hello")),
        )
        .await
        .unwrap();
        assert_eq!(std::fs::read(&dst).unwrap(), b"hello");
        assert_eq!(
            std::fs::metadata(dst).unwrap().permissions().mode() & 0o777,
            0o755
        );
    }
    #[tokio::test]
    async fn fetch_file_404_creates_nothing() {
        let server = MockServer::start().await;
        let temp = tempfile::tempdir().unwrap();
        let dst = temp.path().join("missing");
        assert!(
            fetch_file(&server.uri(), &dst, 0o644, -1, "")
                .await
                .is_err()
        );
        assert!(!dst.exists());
    }
    #[tokio::test]
    async fn fetch_file_size_and_sha_mismatch() {
        let server = MockServer::start().await;
        Mock::given(any())
            .respond_with(ResponseTemplate::new(200).set_body_string("hello"))
            .mount(&server)
            .await;
        let temp = tempfile::tempdir().unwrap();
        let dst = temp.path().join("body");
        for (size, sha, part) in [
            (4, "", "size 5, want 4"),
            (10, "", "size 5, want 10"),
            (5, "bad", "sha256"),
        ] {
            assert!(
                fetch_file(&server.uri(), &dst, 0o644, size, sha)
                    .await
                    .unwrap_err()
                    .to_string()
                    .contains(part)
            );
        }
    }
}
