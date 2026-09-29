use std::{collections::HashSet, sync::Mutex, time::Duration};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use lobo_proto::GoTime;
use reqwest::Method;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use super::{CreateOpts, Instance, POD_NAME, Provider, on_domain};
use crate::{Error, Result, bootstrap, http};

pub const DEFAULT_BASE_URL: &str = "https://console.vast.ai/api/v0";
pub const DISK_GB: i64 = 80;

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
#[serde(default)]
pub struct Offer {
    pub id: i64,
    pub gpu_name: String,
    #[serde(rename = "dph_total")]
    pub dph: f64,
    pub inet_down: f64,
    #[serde(rename = "geolocation")]
    pub geo: String,
    #[serde(rename = "reliability2")]
    pub reliability: f64,
}
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
#[serde(default)]
pub struct Inst {
    pub id: i64,
    pub label: String,
    #[serde(rename = "actual_status")]
    pub status: String,
    #[serde(rename = "dph_total")]
    pub dph: f64,
    pub inet_down: f64,
    #[serde(rename = "geolocation")]
    pub geo: String,
    #[serde(rename = "start_date")]
    pub start: f64,
}

pub struct Client {
    pub base_url: String,
    key: String,
    hc: reqwest::Client,
}
struct RequestFailure {
    status: u16,
    code: String,
    error: Error,
}
impl RequestFailure {
    fn transport(status: u16, e: reqwest::Error) -> Self {
        Self {
            status,
            code: String::new(),
            error: e.into(),
        }
    }
}
impl Client {
    pub fn new(key: &str) -> Self {
        Self::with_base(key, DEFAULT_BASE_URL)
    }
    pub fn with_base(key: &str, base: &str) -> Self {
        Self {
            base_url: base.trim_end_matches('/').into(),
            key: key.into(),
            hc: http::client(Duration::from_secs(30)),
        }
    }
    async fn request(
        &self,
        method: Method,
        path: &str,
        body: Option<&Value>,
    ) -> std::result::Result<Vec<u8>, RequestFailure> {
        let mut req = self
            .hc
            .request(method.clone(), format!("{}{path}", self.base_url))
            .bearer_auth(&self.key)
            .header("Content-Type", "application/json");
        if let Some(body) = body {
            req = req.json(body);
        }
        let res = req
            .send()
            .await
            .map_err(|e| RequestFailure::transport(0, e))?;
        let status = res.status().as_u16();
        let bytes = res
            .bytes()
            .await
            .map_err(|e| RequestFailure::transport(status, e))?;
        let error = match status {
            404 => Error::NotFound,
            401 | 403 => Error::Api(format!(
                "vast {method} {path}: HTTP {status} (check VASTAI_API_KEY)"
            )),
            300.. => Error::Api(format!(
                "vast {method} {path}: HTTP {status}: {}",
                String::from_utf8_lossy(&bytes).trim()
            )),
            _ => return Ok(bytes.to_vec()),
        };
        let code = serde_json::from_slice::<Value>(&bytes)
            .ok()
            .and_then(|v| v["error"].as_str().map(str::to_owned))
            .unwrap_or_default();
        Err(RequestFailure {
            status,
            code,
            error,
        })
    }
    async fn json<T: serde::de::DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        body: Option<&Value>,
    ) -> Result<T> {
        let bytes = self
            .request(method.clone(), path, body)
            .await
            .map_err(|e| e.error)?;
        serde_json::from_slice(&bytes).map_err(|e| Error::Api(format!("vast {method} {path}: {e}")))
    }
    pub async fn search_offers(&self, max_dph: f64, min_mbps: i64) -> Result<Vec<Offer>> {
        #[derive(Deserialize)]
        struct Offers {
            offers: Option<Vec<Offer>>,
        }
        let v: Offers = self
            .json(
                Method::POST,
                "/bundles",
                Some(&search_query(max_dph, min_mbps)),
            )
            .await?;
        Ok(v.offers.unwrap_or_default())
    }
    pub async fn create(&self, offer_id: i64, body: &Value) -> Result<i64> {
        let path = format!("/asks/{offer_id}/");
        let bytes = match self.request(Method::PUT, &path, Some(body)).await {
            Ok(bytes) => bytes,
            Err(e) if (400..500).contains(&e.status) => {
                return Err(if e.code == "insufficient_credit" {
                    Error::NoCredit
                } else {
                    Error::Rejected(e.error.to_string())
                });
            }
            Err(e) => return Err(e.error),
        };
        #[derive(Deserialize, Default)]
        #[serde(default)]
        struct Created {
            success: bool,
            new_contract: i64,
            msg: String,
            error: String,
        }
        // A malformed successful response is uncertain, never a rejection.
        let result: Created = serde_json::from_slice(&bytes)
            .map_err(|e| Error::Api(format!("vast PUT {path}: {e}")))?;
        if !result.success || result.new_contract == 0 {
            return Err(Error::Rejected(format!(
                "vast create offer {offer_id}: {} {}",
                result.error, result.msg
            )));
        }
        Ok(result.new_contract)
    }
    pub async fn list(&self) -> Result<Vec<Inst>> {
        #[derive(Deserialize)]
        struct Instances {
            instances: Option<Vec<Inst>>,
        }
        let v: Instances = self.json(Method::GET, "/instances/", None).await?;
        Ok(v.instances.unwrap_or_default())
    }
    pub async fn get(&self, id: i64) -> Result<Inst> {
        #[derive(Deserialize)]
        struct OneInstance {
            instances: Option<Inst>,
        }
        let v: OneInstance = self
            .json(Method::GET, &format!("/instances/{id}/"), None)
            .await?;
        v.instances.ok_or(Error::NotFound)
    }
    pub async fn destroy(&self, id: i64) -> Result<()> {
        match self
            .request(Method::DELETE, &format!("/instances/{id}/"), None)
            .await
        {
            Ok(_) => Ok(()),
            Err(e) if e.error.is_not_found() => Ok(()),
            Err(e) => Err(e.error),
        }
    }
}

pub fn search_query(max_dph: f64, min_mbps: i64) -> Value {
    json!({
        "gpu_name":{"in":["RTX 5090"]}, "num_gpus":{"eq":1}, "rentable":{"eq":true}, "verified":{"eq":true},
        "reliability2":{"gte":0.98}, "disk_space":{"gte":80}, "cuda_max_good":{"gte":12.8},
        "dph_total":{"lte":max_dph}, "inet_down":{"gte":min_mbps.saturating_mul(8)},
        "order":[["dph_total","asc"]], "limit":10,
    })
}
pub fn create_body(o: &CreateOpts) -> Value {
    json!({"client_id":"me", "image":o.image, "disk":DISK_GB, "label":POD_NAME, "runtype":"ssh",
        "onstart":format!("#!/bin/bash\n{}",bootstrap::script("vast")), "env":bootstrap::env(o,"vast")})
}

pub struct VastProvider {
    pub client: Client,
    pub max_dph: f64,
    pub domain: String,
    pub adopt_wait: Duration,
    tried: Mutex<HashSet<i64>>,
}
impl VastProvider {
    pub fn new(client: Client, max_dph: f64, domain: &str) -> Self {
        Self {
            client,
            max_dph,
            domain: domain.into(),
            adopt_wait: Duration::from_secs(3),
            tried: Mutex::new(HashSet::new()),
        }
    }
    async fn adopt(&self, before: &HashSet<String>) -> Option<Instance> {
        // Cancellation cannot interrupt reconciliation of a submitted create.
        for n in 0..3 {
            if n > 0 {
                tokio::time::sleep(self.adopt_wait).await;
            }
            if let Ok(instances) = self.list().await
                && let Some(i) = instances.into_iter().find(|i| !before.contains(&i.id))
            {
                return Some(i);
            }
        }
        None
    }
}
#[async_trait]
impl Provider for VastProvider {
    fn name(&self) -> &'static str {
        "vast"
    }
    fn replaceable(&self) -> bool {
        true
    }
    async fn rent(
        &self,
        o: &CreateOpts,
        cancel: CancellationToken,
        note: &(dyn Fn(String) + Sync),
    ) -> Result<Instance> {
        let max_dph = if self.max_dph <= 0.0 {
            1.20
        } else {
            self.max_dph
        };
        let offers = tokio::select! { biased;
            _ = cancel.cancelled() => return Err(Error::Cancelled),
            offers = self.client.search_offers(max_dph, o.min_mbps) => offers?,
        };
        let before: HashSet<_> = tokio::select! { biased;
            _ = cancel.cancelled() => return Err(Error::Cancelled),
            instances = self.list() => instances?,
        }
        .into_iter()
        .map(|i| i.id)
        .collect();
        let body = create_body(o);
        for offer in offers {
            if cancel.is_cancelled() {
                return Err(Error::Cancelled);
            }
            if !self.tried.lock().unwrap().insert(offer.id) {
                continue;
            }
            let detail = format!(
                "offer {}, {:.0} Mbps down, {}",
                offer.id, offer.inet_down, offer.geo
            );
            // Await submitted creates; the owner will clean up a late success.
            match self.client.create(offer.id, &body).await {
                Ok(id) => {
                    return Ok(on_domain(
                        Instance {
                            provider: "vast".into(),
                            id: id.to_string(),
                            status: "created".into(),
                            cost_per_hr: offer.dph,
                            started_at: GoTime::from_utc(Utc::now()),
                            host_download_mbps: offer.inet_down as i64,
                            detail,
                            ..Default::default()
                        },
                        &self.domain,
                    ));
                }
                Err(Error::NoCredit) => return Err(Error::NoCredit),
                Err(e @ Error::Rejected(_)) => note(format!("offer {} unavailable: {e}", offer.id)),
                Err(e) => {
                    if let Some(mut i) = self.adopt(&before).await {
                        i.detail = detail;
                        return Ok(i);
                    }
                    return Err(Error::Api(format!(
                        "vast create offer {}: {e} (not retrying another offer: it may have been rented — check `lobo status`)",
                        offer.id
                    )));
                }
            }
        }
        Err(Error::NoCapacity(format!(
            "no untried 1× RTX 5090 offer (verified, reliability ≥0.98, ≤${max_dph:.2}/h)"
        )))
    }
    async fn list(&self) -> Result<Vec<Instance>> {
        Ok(self
            .client
            .list()
            .await?
            .into_iter()
            .filter(|i| i.label == POD_NAME)
            .map(|i| on_domain(to_instance(i), &self.domain))
            .collect())
    }
    async fn get(&self, id: &str) -> Result<Instance> {
        let id = id.parse::<i64>().map_err(|e| Error::Other(e.to_string()))?;
        Ok(on_domain(
            to_instance(self.client.get(id).await?),
            &self.domain,
        ))
    }
    async fn delete(&self, id: &str) -> Result<()> {
        let id = id.parse::<i64>().map_err(|e| Error::Other(e.to_string()))?;
        self.client.destroy(id).await
    }
}
fn to_instance(i: Inst) -> Instance {
    let started_at = if i.start > 0.0 {
        DateTime::from_timestamp(i.start as i64, 0)
            .map(GoTime::from_utc)
            .unwrap_or_default()
    } else {
        GoTime::ZERO
    };
    Instance {
        provider: "vast".into(),
        id: i.id.to_string(),
        status: i.status,
        cost_per_hr: i.dph,
        host_download_mbps: i.inet_down as i64,
        detail: format!("{:.0} Mbps down, {}", i.inet_down, i.geo),
        started_at,
        ..Default::default()
    }
}

#[cfg(test)]
mod tests;
