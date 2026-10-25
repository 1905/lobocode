use super::{BUCKET, LATEST_KEY, Resolved, meta_key};
use crate::{Error, Result, config::R2Creds, http};
use rusty_s3::{Bucket, Credentials, S3Action, UrlStyle, actions::ListObjectsV2};
use std::{path::Path, time::Duration};

pub const MODELS_BUCKET: &str = "lobocode-models";
const SIGN_TTL: Duration = Duration::from_secs(3600);
pub struct Store {
    bucket: Bucket,
    creds: Credentials,
    hc: reqwest::Client,
}
impl Store {
    pub fn new(r2: &R2Creds) -> Result<Self> {
        check_https(r2)?;
        Self::with_endpoint_for_test(r2, http::client(Duration::from_secs(300)))
    }
    pub fn models(r2: &R2Creds) -> Result<Self> {
        check_https(r2)?;
        Self::with_bucket(r2, http::client(Duration::from_secs(1800)), MODELS_BUCKET)
    }
    pub fn with_endpoint_for_test(r2: &R2Creds, hc: reqwest::Client) -> Result<Self> {
        Self::with_bucket(r2, hc, BUCKET)
    }
    pub fn with_bucket(r2: &R2Creds, hc: reqwest::Client, bucket: &str) -> Result<Self> {
        let endpoint = r2
            .endpoint
            .parse()
            .map_err(|e| Error::Release(format!("R2 endpoint: {e}")))?;
        let bucket = Bucket::new(endpoint, UrlStyle::Path, bucket.to_owned(), "auto")
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
        let cache = (content_type == "application/json").then_some("no-cache");
        let status = self
            .send_put(key, bytes, content_type, cache, immutable)
            .await?;
        if immutable && status == 412 {
            return Err(existing(r, key));
        }
        upload_status(key, status)
    }
    pub async fn put_immutable(
        &self,
        key: &str,
        bytes: Vec<u8>,
        content_type: &str,
        cache_control: &str,
    ) -> Result<()> {
        let status = self
            .send_put(key, bytes, content_type, Some(cache_control), true)
            .await?;
        if status == 412 {
            return Err(Error::Release(format!("{key} already exists")));
        }
        upload_status(key, status)
    }
    pub async fn object_len(&self, key: &str) -> Result<Option<u64>> {
        let url = self
            .bucket
            .head_object(Some(&self.creds), key)
            .sign(SIGN_TTL);
        let res = self.hc.head(url).send().await.map_err(no_url)?;
        match res.status().as_u16() {
            200 => res
                .headers()
                .get(reqwest::header::CONTENT_LENGTH)
                .and_then(|v| v.to_str().ok()?.parse().ok())
                .map(Some)
                .ok_or_else(|| Error::Release(format!("check {key}: no Content-Length"))),
            404 => Ok(None),
            status => Err(Error::Release(format!("check {key}: HTTP {status}"))),
        }
    }
    async fn send_put(
        &self,
        key: &str,
        bytes: Vec<u8>,
        content_type: &str,
        cache_control: Option<&str>,
        immutable: bool,
    ) -> Result<reqwest::StatusCode> {
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
        if let Some(cache_control) = cache_control {
            request = request.header("Cache-Control", cache_control);
        }
        Ok(request.send().await.map_err(no_url)?.status())
    }
}
fn check_https(r2: &R2Creds) -> Result<()> {
    let endpoint: url::Url = r2
        .endpoint
        .parse()
        .map_err(|e| Error::Release(format!("R2 endpoint: {e}")))?;
    if endpoint.scheme() != "https" {
        return Err(Error::Release("R2_ENDPOINT must use https".into()));
    }
    Ok(())
}
fn upload_status(key: &str, status: reqwest::StatusCode) -> Result<()> {
    if !status.is_success() {
        return Err(Error::Release(format!(
            "upload {key}: HTTP {}",
            status.as_u16()
        )));
    }
    Ok(())
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
