use crate::{Error, Result, http};
use chrono::{DateTime, Utc};
pub use lobo_proto::release::{
    DEFAULT_DEFAULTS, DEFAULT_LLAMA_IMAGE, DEFAULT_MODEL, Defaults, Manifest, ModelRef, Resolved,
};
use std::time::Duration;

pub mod store;
pub mod zip;
pub use store::Store;
pub use zip::{build_zip, scan_for_secrets};

pub const BUCKET: &str = "lobo";
pub const LATEST_KEY: &str = "releases/latest.json";
pub fn zip_key(version: &str) -> String {
    format!("releases/lobo-{version}.zip")
}
pub fn meta_key(version: &str) -> String {
    format!("releases/lobo-{version}.json")
}
pub fn next_version(existing: &[String], today: DateTime<Utc>) -> String {
    let day = today.format("%Y.%m.%d").to_string();
    let prefix = format!("releases/lobo-{day}-");
    let highest = existing
        .iter()
        .filter_map(|key| {
            let number = key.strip_prefix(&prefix)?.strip_suffix(".zip")?;
            if number.is_empty() || !number.bytes().all(|c| c.is_ascii_digit()) {
                return None;
            }
            number.parse::<u64>().ok()
        })
        .max()
        .unwrap_or(0);
    format!("{day}-{}", highest.saturating_add(1))
}
pub fn zip_url(r: &Resolved, bucket_url: &str) -> String {
    if r.zip_key.is_empty() {
        String::new()
    } else {
        format!("{}/{}", bucket_url.trim_end_matches('/'), r.zip_key)
    }
}
pub async fn resolve(hc: &reqwest::Client, bucket_url: &str, version: &str) -> Result<Resolved> {
    let key = if version.is_empty() {
        LATEST_KEY.to_owned()
    } else {
        meta_key(version)
    };
    let nonce = Utc::now().timestamp_nanos_opt().unwrap_or_default();
    let url = format!("{}/{key}?t={nonce}", bucket_url.trim_end_matches('/'));
    let res = hc.get(url).send().await?;
    if res.status() != 200 {
        let name = if version.is_empty() {
            "latest"
        } else {
            version
        };
        return Err(Error::Release(format!(
            "release {name}: HTTP {} from {key}",
            res.status().as_u16()
        )));
    }
    let body = res.bytes().await?;
    serde_json::from_slice(&body).map_err(|e| Error::Release(format!("release {key}: {e}")))
}
pub struct BucketReleases {
    pub bucket_url: String,
    pub hc: reqwest::Client,
}
impl BucketReleases {
    pub fn new(bucket_url: &str) -> Self {
        Self {
            bucket_url: bucket_url.into(),
            hc: http::client(Duration::from_secs(15)),
        }
    }
    pub async fn resolve(&self, version: &str) -> Result<Resolved> {
        resolve(&self.hc, &self.bucket_url, version).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::{Mock, MockServer, Request, ResponseTemplate};
    #[test]
    fn next_version_table() {
        let day = "2026-09-23T23:00:00Z".parse().unwrap();
        for (keys, want) in [
            (vec![], "2026.09.23-1"),
            (
                vec![
                    "releases/lobo-2026.09.23-1.zip",
                    "releases/lobo-2026.09.23-2.zip",
                    "releases/lobo-2026.09.23-2.json",
                ],
                "2026.09.23-3",
            ),
            (
                vec!["releases/lobo-2026.09.22-7.zip", "releases/latest.json"],
                "2026.09.23-1",
            ),
            (
                vec![
                    "releases/lobo-2026.09.23-10.zip",
                    "releases/lobo-2026.09.23-9.zip",
                ],
                "2026.09.23-11",
            ),
        ] {
            assert_eq!(
                next_version(
                    &keys.into_iter().map(str::to_owned).collect::<Vec<_>>(),
                    day
                ),
                want
            );
        }
    }
    #[test]
    fn zip_url_trims_slash_and_empty_for_baked() {
        assert_eq!(zip_url(&Resolved::default(), "https://b/"), "");
        assert_eq!(
            zip_url(
                &Resolved {
                    zip_key: zip_key("v1"),
                    ..Default::default()
                },
                "https://b///"
            ),
            "https://b/releases/lobo-v1.zip"
        );
    }
    #[tokio::test]
    async fn resolve_latest_and_version() {
        let s = MockServer::start().await;
        Mock::given(|_: &Request| true)
            .respond_with(|r: &Request| {
                assert!(
                    r.url
                        .query_pairs()
                        .any(|(k, v)| k == "t" && v.parse::<i64>().is_ok())
                );
                let version = match r.url.path() {
                    "/releases/latest.json" => "new",
                    "/releases/lobo-old.json" => "old",
                    _ => return ResponseTemplate::new(404),
                };
                ResponseTemplate::new(200).set_body_json(Resolved {
                    manifest: Manifest {
                        version: version.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                })
            })
            .mount(&s)
            .await;
        let resolver = BucketReleases::new(&s.uri());
        assert_eq!(resolver.resolve("").await.unwrap().manifest.version, "new");
        assert_eq!(
            resolver.resolve("old").await.unwrap().manifest.version,
            "old"
        );
        assert_eq!(
            resolver.resolve("missing").await.unwrap_err().to_string(),
            "release missing: HTTP 404 from releases/lobo-missing.json"
        );
    }
}
