//! Resolve public tags again for every new rental; never cache a prior image.
use crate::{Error, Result};
use async_trait::async_trait;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::time::Duration;

pub const REPOSITORY: &str = "1905/lobocode";
const ACCEPT: &str = "application/vnd.oci.image.manifest.v1+json, application/vnd.oci.image.index.v1+json, application/vnd.docker.distribution.manifest.v2+json, application/vnd.docker.distribution.manifest.list.v2+json";

#[async_trait]
pub trait ImageResolver: Send + Sync {
    async fn latest(&self, model: &str) -> Result<String>;
}
pub struct PublicImages {
    client: reqwest::Client,
    base: String,
}
impl Default for PublicImages {
    fn default() -> Self {
        Self::new()
    }
}
impl PublicImages {
    pub fn new() -> Self {
        Self {
            client: crate::http::client(Duration::from_secs(30)),
            base: "https://ghcr.io".into(),
        }
    }
}
#[async_trait]
impl ImageResolver for PublicImages {
    async fn latest(&self, model: &str) -> Result<String> {
        lobo_proto::catalog::get(model).map_err(|e| Error::Config(e.to_string()))?;
        #[derive(Deserialize)]
        struct Token {
            token: String,
        }
        let token = self
            .client
            .get(format!("{}/token", self.base))
            .query(&[
                ("service", "ghcr.io"),
                ("scope", &format!("repository:{REPOSITORY}:pull")),
            ])
            .header("Cache-Control", "no-cache")
            .send()
            .await?;
        if !token.status().is_success() {
            return Err(Error::Other(format!(
                "cannot access the public GPU image: HTTP {}",
                token.status()
            )));
        }
        let token: Token = token.json().await?;
        let mut response = self
            .client
            .get(format!(
                "{}/v2/{REPOSITORY}/manifests/latest-{model}",
                self.base
            ))
            .bearer_auth(token.token)
            .header("Accept", ACCEPT)
            .header("Cache-Control", "no-cache, no-store")
            .query(&[(
                "t",
                chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default(),
            )])
            .send()
            .await?;
        if !response.status().is_success() {
            return Err(Error::Other(format!(
                "cannot resolve latest {model} GPU image: HTTP {}; no GPU was rented",
                response.status()
            )));
        }
        let expected = response
            .headers()
            .get("Docker-Content-Digest")
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);
        const MAX_MANIFEST_BYTES: usize = 4 * 1024 * 1024;
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await? {
            if bytes.len().saturating_add(chunk.len()) > MAX_MANIFEST_BYTES {
                return Err(Error::Other(
                    "public GPU image manifest is too large".into(),
                ));
            }
            bytes.extend_from_slice(&chunk);
        }
        let manifest: serde_json::Value = serde_json::from_slice(&bytes)?;
        let populated = |key: &str| manifest[key].as_array().is_some_and(|v| !v.is_empty());
        if manifest["schemaVersion"] != 2 || !(populated("layers") || populated("manifests")) {
            return Err(Error::Other(
                "public GPU image returned an invalid manifest".into(),
            ));
        }
        let digest = format!("sha256:{}", hex::encode(Sha256::digest(&bytes)));
        if expected.as_ref().is_some_and(|v| v != &digest) {
            return Err(Error::Other(
                "public GPU image manifest hash does not match".into(),
            ));
        }
        Ok(format!("ghcr.io/{REPOSITORY}@{digest}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{header, path},
    };
    #[tokio::test]
    async fn latest_is_refetched_and_registry_failure_never_returns_stale_image() {
        let server = MockServer::start().await;
        let client = PublicImages {
            base: server.uri(),
            ..PublicImages::new()
        };
        let mut previous = String::new();
        for n in 0..3 {
            server.reset().await;
            Mock::given(path("/token"))
                .respond_with(
                    ResponseTemplate::new(200)
                        .set_body_json(serde_json::json!({"token":"anonymous"})),
                )
                .mount(&server)
                .await;
            Mock::given(path("/v2/1905/lobocode/manifests/latest-q8"))
                .and(header("Authorization", "Bearer anonymous"))
                .and(|r: &wiremock::Request| r.headers.get("cache-control").is_some_and(|h| {
                    h.to_str().is_ok_and(|v| v.contains("no-cache") && v.contains("no-store"))
                }))
                .respond_with(if n == 2 {
                    ResponseTemplate::new(503)
                } else {
                    ResponseTemplate::new(200).set_body_json(
                        serde_json::json!({"schemaVersion":2,"layers":[{"digest":"sha256:fixture"}],"generation":n}),
                    )
                })
                .expect(1)
                .mount(&server)
                .await;
            let result = client.latest("q8").await;
            if n == 2 {
                assert!(result.is_err());
            } else {
                let image = result.unwrap();
                assert!(image.starts_with("ghcr.io/1905/lobocode@sha256:"));
                assert_ne!(image, previous);
                previous = image;
            }
        }
    }
    #[tokio::test]
    async fn refuses_wrong_manifest_digest() {
        let server = MockServer::start().await;
        let client = PublicImages {
            base: server.uri(),
            ..PublicImages::new()
        };
        Mock::given(path("/token"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({"token":"anonymous"})),
            )
            .mount(&server)
            .await;
        Mock::given(path("/v2/1905/lobocode/manifests/latest-q6"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("Docker-Content-Digest", "sha256:bad")
                    .set_body_json(serde_json::json!({"schemaVersion":2,"layers":[{"digest":"sha256:fixture"}]})),
            )
            .mount(&server)
            .await;
        assert!(
            client
                .latest("q6")
                .await
                .unwrap_err()
                .to_string()
                .contains("hash")
        );
    }
    #[tokio::test]
    async fn rejects_invalid_manifest_and_unknown_model() {
        let server = MockServer::start().await;
        let client = PublicImages {
            base: server.uri(),
            ..PublicImages::new()
        };
        assert!(client.latest("q2").await.is_err());
        assert!(server.received_requests().await.unwrap().is_empty());
        Mock::given(path("/token"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({"token":"anonymous"})),
            )
            .mount(&server)
            .await;
        Mock::given(path("/v2/1905/lobocode/manifests/latest-q6"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({"schemaVersion":2,"layers":[]})),
            )
            .mount(&server)
            .await;
        assert!(
            client
                .latest("q6")
                .await
                .unwrap_err()
                .to_string()
                .contains("invalid manifest")
        );
    }
}
