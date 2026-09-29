use super::{BUCKET, LATEST_KEY, Resolved, meta_key};
use crate::{Error, Result, config::R2Creds, http};
use rusty_s3::{Bucket, Credentials, S3Action, UrlStyle, actions::ListObjectsV2};
use std::{path::Path, time::Duration};

const SIGN_TTL: Duration = Duration::from_secs(3600);
pub struct Store {
    bucket: Bucket,
    creds: Credentials,
    hc: reqwest::Client,
}
impl Store {
    pub fn new(r2: &R2Creds) -> Result<Self> {
        let endpoint: url::Url = r2
            .endpoint
            .parse()
            .map_err(|e| Error::Release(format!("R2 endpoint: {e}")))?;
        if endpoint.scheme() != "https" {
            return Err(Error::Release("R2_ENDPOINT must use https".into()));
        }
        Self::with_endpoint_for_test(r2, http::client(Duration::from_secs(300)))
    }
    pub fn with_endpoint_for_test(r2: &R2Creds, hc: reqwest::Client) -> Result<Self> {
        let endpoint = r2
            .endpoint
            .parse()
            .map_err(|e| Error::Release(format!("R2 endpoint: {e}")))?;
        let bucket = Bucket::new(endpoint, UrlStyle::Path, BUCKET, "auto")
            .map_err(|e| Error::Release(format!("R2 endpoint: {e}")))?;
        Ok(Self {
            bucket,
            creds: Credentials::new(&r2.access_key, &r2.secret_key),
            hc,
        })
    }
    pub fn presign_get(&self, key: &str, ttl: Duration) -> String {
        self.bucket
            .get_object(Some(&self.creds), key)
            .sign(ttl)
            .into()
    }
    pub async fn list_release_keys(&self) -> Result<Vec<String>> {
        let mut keys = Vec::new();
        let mut token: Option<String> = None;
        loop {
            let mut list = self.bucket.list_objects_v2(Some(&self.creds));
            list.with_prefix("releases/");
            if let Some(token) = &token {
                list.with_continuation_token(token.as_str());
            }
            let res = self
                .hc
                .get(list.sign(SIGN_TTL))
                .send()
                .await
                .map_err(no_url)?;
            if !res.status().is_success() {
                return Err(Error::Release(format!(
                    "list releases: HTTP {}",
                    res.status().as_u16()
                )));
            }
            let text = res.text().await.map_err(no_url)?;
            let result = ListObjectsV2::parse_response(&text)
                .map_err(|e| Error::Release(format!("list releases: {e}")))?;
            keys.extend(result.contents.into_iter().map(|o| o.key));
            let Some(next) = result.next_continuation_token else {
                return Ok(keys);
            };
            if token.as_ref() == Some(&next) {
                return Err(Error::Release(
                    "list releases: repeated continuation token".into(),
                ));
            }
            token = Some(next);
        }
    }
    pub async fn publish_version(&self, zip: &Path, r: &Resolved) -> Result<()> {
        let meta = meta_key(&r.manifest.version);
        for key in [&r.zip_key, &meta] {
            let url = self
                .bucket
                .head_object(Some(&self.creds), key)
                .sign(SIGN_TTL);
            let status = self.hc.head(url).send().await.map_err(no_url)?.status();
            match status.as_u16() {
                200 => return Err(existing(r, key)),
                404 => {}
                status => return Err(Error::Release(format!("check {key}: HTTP {status}"))),
            }
        }
        let bytes = tokio::fs::read(zip).await?;
        self.put(&r.zip_key, bytes, "application/zip", true, r)
            .await?;
        self.put(
            &meta,
            serde_json::to_vec_pretty(r)?,
            "application/json",
            true,
            r,
        )
        .await
    }
    pub async fn publish(&self, zip: &Path, r: &Resolved) -> Result<()> {
        self.publish_version(zip, r).await?;
        self.put(
            LATEST_KEY,
            serde_json::to_vec_pretty(r)?,
            "application/json",
            false,
            r,
        )
        .await
    }
    async fn put(
        &self,
        key: &str,
        bytes: Vec<u8>,
        content_type: &str,
        immutable: bool,
        r: &Resolved,
    ) -> Result<()> {
        let mut action = self.bucket.put_object(Some(&self.creds), key);
        if immutable {
            action.headers_mut().insert("if-none-match", "*");
        }
        let mut request = self
            .hc
            .put(action.sign(SIGN_TTL))
            .header("Content-Type", content_type)
            .body(bytes);
        if immutable {
            request = request.header("If-None-Match", "*");
        }
        if content_type == "application/json" {
            request = request.header("Cache-Control", "no-cache");
        }
        let status = request.send().await.map_err(no_url)?.status();
        if immutable && status == 412 {
            return Err(existing(r, key));
        }
        if !status.is_success() {
            return Err(Error::Release(format!(
                "upload {key}: HTTP {}",
                status.as_u16()
            )));
        }
        Ok(())
    }
}
fn existing(r: &Resolved, key: &str) -> Error {
    Error::Release(format!(
        "release {} already exists ({key}); run make release again",
        r.manifest.version
    ))
}
fn no_url(e: reqwest::Error) -> Error {
    Error::Http(e.without_url())
}

#[cfg(test)]
mod tests;
