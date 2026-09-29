use std::{collections::BTreeMap, sync::Arc, time::Duration};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use lobo_proto::GoTime;
use reqwest::Method;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use super::{CreateOpts, Instance, POD_NAME, Provider, on_domain};
use crate::{Error, Result, bootstrap, http};

pub const GPU_TYPE: &str = "NVIDIA GeForce RTX 5090";
pub const NET_TIERS: [f64; 5] = [10000.0, 5000.0, 2500.0, 1000.0, 0.0];
const SSH_PREFIX: &str = r#"apt-get update -qq && DEBIAN_FRONTEND=noninteractive apt-get install -y -qq openssh-server >/dev/null
mkdir -p /root/.ssh /run/sshd && echo "$PUBLIC_KEY" > /root/.ssh/authorized_keys && chmod 600 /root/.ssh/authorized_keys && /usr/sbin/sshd
"#;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct RunPodTime(pub Option<DateTime<Utc>>);

impl Serialize for RunPodTime {
    fn serialize<S: Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        self.0
            .map(GoTime::from_utc)
            .unwrap_or_default()
            .serialize(s)
    }
}
impl<'de> Deserialize<'de> for RunPodTime {
    fn deserialize<D: Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        let value = Value::deserialize(d)?;
        let Some(s) = value.as_str().filter(|s| !s.is_empty()) else {
            return Ok(Self(None));
        };
        if s == "0001-01-01T00:00:00Z" {
            return Ok(Self(None));
        }
        let parsed = DateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S%.f %z %Z")
            .or_else(|_| DateTime::parse_from_rfc3339(s));
        parsed
            .map(|t| Self(Some(t.with_timezone(&Utc))))
            .map_err(|e| serde::de::Error::custom(format!("runpod: time {s:?}: {e}")))
    }
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Machine {
    pub max_download_speed_mbps: i64,
    pub max_upload_speed_mbps: i64,
    #[serde(rename = "diskThroughputMBps")]
    pub disk_throughput_mbps: i64,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Pod {
    pub id: String,
    pub name: String,
    pub desired_status: String,
    pub image_name: String,
    pub cost_per_hr: f64,
    pub last_started_at: RunPodTime,
    pub created_at: RunPodTime,
    pub gpu_count: i64,
    #[serde(deserialize_with = "null_ports")]
    pub port_mappings: BTreeMap<String, i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub machine: Option<Machine>,
}
fn null_ports<'de, D: Deserializer<'de>>(
    d: D,
) -> std::result::Result<BTreeMap<String, i64>, D::Error> {
    Ok(Option::<BTreeMap<String, i64>>::deserialize(d)?.unwrap_or_default())
}

pub fn build_create_payload(o: &CreateOpts, cloud: &str, min_download_mbps: f64) -> Value {
    let mut env = bootstrap::env(o, "runpod");
    let mut start = bootstrap::script("runpod");
    let mut ports = Vec::<&str>::new();
    if !o.ssh_pub_key.is_empty() {
        ports.push("22/tcp");
        start.insert_str(0, SSH_PREFIX);
        env.insert("PUBLIC_KEY".into(), o.ssh_pub_key.clone());
    }
    let mut payload = json!({
        "name": POD_NAME, "imageName": o.image, "gpuTypeIds": [GPU_TYPE], "gpuCount": 1,
        "cloudType": if cloud.is_empty() {"SECURE"} else {cloud},
        "containerDiskInGb": 60, "volumeInGb": 0, "ports": ports,
        "dockerEntrypoint": ["bash", "-c"], "dockerStartCmd": [start], "env": env,
    });
    if min_download_mbps > 0.0 {
        payload["minDownloadMbps"] = json!(min_download_mbps);
    }
    payload
}

#[async_trait]
pub trait RunPodApi: Send + Sync {
    async fn create(&self, o: &CreateOpts, cloud: &str, min_download_mbps: f64) -> Result<Pod>;
    async fn list(&self) -> Result<Vec<Pod>>;
    async fn get(&self, id: &str) -> Result<Pod>;
    async fn delete(&self, id: &str) -> Result<()>;
}

pub struct Client {
    pub base_url: String,
    key: String,
    hc: reqwest::Client,
}
impl Client {
    pub fn new(key: &str) -> Self {
        Self::with_base(key, "https://rest.runpod.io/v1")
    }
    pub fn with_base(key: &str, base_url: &str) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').into(),
            key: key.into(),
            hc: http::client(Duration::from_secs(30)),
        }
    }
    async fn request(&self, method: Method, path: &str, body: Option<Value>) -> Result<Vec<u8>> {
        let mut req = self
            .hc
            .request(method.clone(), format!("{}{path}", self.base_url))
            .bearer_auth(&self.key)
            .header("Content-Type", "application/json");
        if let Some(body) = body {
            req = req.json(&body);
        }
        let res = req.send().await?;
        let status = res.status();
        let bytes = res.bytes().await?;
        if status == 404 {
            return Err(Error::NotFound);
        }
        if status.as_u16() >= 300 {
            let msg = String::from_utf8_lossy(&bytes).trim().to_owned();
            if is_no_capacity(&msg) {
                return Err(Error::NoCapacity(msg));
            }
            return Err(Error::Api(format!(
                "runpod {method} {path}: HTTP {}: {msg}",
                status.as_u16()
            )));
        }
        Ok(bytes.to_vec())
    }
}
#[async_trait]
impl RunPodApi for Client {
    async fn create(&self, o: &CreateOpts, cloud: &str, min_download_mbps: f64) -> Result<Pod> {
        Ok(serde_json::from_slice(
            &self
                .request(
                    Method::POST,
                    "/pods",
                    Some(build_create_payload(o, cloud, min_download_mbps)),
                )
                .await?,
        )?)
    }
    async fn list(&self) -> Result<Vec<Pod>> {
        Ok(serde_json::from_slice::<Option<Vec<Pod>>>(
            &self.request(Method::GET, "/pods", None).await?,
        )?
        .unwrap_or_default())
    }
    async fn get(&self, id: &str) -> Result<Pod> {
        Ok(serde_json::from_slice(
            &self
                .request(Method::GET, &format!("/pods/{id}"), None)
                .await?,
        )?)
    }
    async fn delete(&self, id: &str) -> Result<()> {
        match self
            .request(Method::DELETE, &format!("/pods/{id}"), None)
            .await
        {
            Ok(_) | Err(Error::NotFound) => Ok(()),
            Err(e) => Err(e),
        }
    }
}
pub fn is_no_capacity(msg: &str) -> bool {
    let msg = msg.to_lowercase();
    [
        "no instances",
        "no longer any instances",
        "instances available",
        "not enough",
        "no gpu",
    ]
    .iter()
    .any(|s| msg.contains(s))
}

pub struct RunPodProvider {
    pub api: Arc<dyn RunPodApi>,
    pub domain: String,
}
#[async_trait]
impl Provider for RunPodProvider {
    fn name(&self) -> &'static str {
        "runpod"
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
        let clouds: &[&str] = if o.cloud == "secure" {
            &["SECURE", "COMMUNITY"]
        } else {
            &["COMMUNITY"]
        };
        let mut last_error = Error::NoCapacity(String::new());
        for cloud in clouds {
            for tier in NET_TIERS {
                if cancel.is_cancelled() {
                    return Err(Error::Cancelled);
                }
                // Await the request even after cancellation. Its successful
                // result belongs to the core worker until cleanup completes.
                match self.api.create(o, cloud, tier).await {
                    Ok(pod) => {
                        let mut i = on_domain(to_instance(pod), &self.domain);
                        i.detail = cloud.to_string();
                        if tier > 0.0 {
                            i.detail.push_str(&format!(", host ≥{tier:.0} Mbps"));
                        }
                        return Ok(i);
                    }
                    Err(e @ Error::NoCapacity(_)) => last_error = e,
                    Err(e) => return Err(e),
                }
            }
            note(format!("no 5090 in {cloud} at any network speed"));
        }
        Err(last_error)
    }
    async fn list(&self) -> Result<Vec<Instance>> {
        Ok(self
            .api
            .list()
            .await?
            .into_iter()
            .filter(|p| p.name == POD_NAME)
            .map(|p| on_domain(to_instance(p), &self.domain))
            .collect())
    }
    async fn get(&self, id: &str) -> Result<Instance> {
        let p = self.api.get(id).await?;
        if p.desired_status.eq_ignore_ascii_case("TERMINATED") {
            return Err(Error::NotFound);
        }
        Ok(on_domain(to_instance(p), &self.domain))
    }
    async fn delete(&self, id: &str) -> Result<()> {
        self.api.delete(id).await
    }
}
fn to_instance(p: Pod) -> Instance {
    Instance {
        provider: "runpod".into(),
        id: p.id,
        status: p.desired_status,
        cost_per_hr: p.cost_per_hr,
        started_at: p
            .last_started_at
            .0
            .or(p.created_at.0)
            .map(GoTime::from_utc)
            .unwrap_or_default(),
        host_download_mbps: p
            .machine
            .map(|m| m.max_download_speed_mbps)
            .unwrap_or_default(),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests;
