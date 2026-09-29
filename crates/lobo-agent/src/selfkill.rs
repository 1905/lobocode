use crate::{Error, Result};
use async_trait::async_trait;
use serde_json::{Value, json};
use std::{sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;

#[async_trait]
pub trait PodApi: Send + Sync {
    async fn terminate(&self) -> Result<()>;
    async fn gone(&self) -> Result<bool>;
}
pub struct RetryKiller {
    pub api: Arc<dyn PodApi>,
}
impl RetryKiller {
    pub async fn kill_self(&self, cancel: CancellationToken) -> Result<()> {
        let mut backoff = Duration::from_secs(2);
        for attempt in 1.. {
            let result = tokio::select! {
                biased; _ = cancel.cancelled() => return Err(Error::Cancelled),
                r = tokio::time::timeout(Duration::from_secs(10), async {
                    self.api.terminate().await?; self.api.gone().await
                }) => r.unwrap_or_else(|_| Err(Error::msg("terminate attempt timed out"))),
            };
            if matches!(result, Ok(true)) {
                tracing::info!(service = "lobo-agent", attempt, "pod terminated");
                return Ok(());
            }
            tracing::warn!(service="lobo-agent", attempt, error = ?result, retry_in_s=backoff.as_secs(), "terminate not confirmed");
            tokio::select! { biased; _=cancel.cancelled()=>return Err(Error::Cancelled), _=tokio::time::sleep(backoff)=>{} }
            backoff = (backoff * 2).min(Duration::from_secs(60));
        }
        unreachable!()
    }
}
pub struct RunPodSelf {
    pub pod_id: String,
    pub key: String,
    pub url: String,
}
impl RunPodSelf {
    pub fn new(pod_id: &str, key: &str) -> Self {
        Self {
            pod_id: pod_id.into(),
            key: key.into(),
            url: "https://api.runpod.io/graphql".into(),
        }
    }
    async fn gql(&self, query: String) -> Result<Value> {
        let response = crate::http::client()
            .post(&self.url)
            .bearer_auth(&self.key)
            .timeout(Duration::from_secs(10))
            .json(&json!({"query":query}))
            .send()
            .await
            .map_err(http_error)?;
        if response.status() != reqwest::StatusCode::OK {
            return Err(Error::msg(format!(
                "runpod graphql: HTTP {}",
                response.status().as_u16()
            )));
        }
        let value: Value = response.json().await.map_err(http_error)?;
        if let Some(e) = value
            .get("errors")
            .and_then(|v| v.as_array())
            .and_then(|a| a.first())
        {
            return Err(Error::msg(format!(
                "runpod graphql: {}",
                e["message"].as_str().unwrap_or("unknown error")
            )));
        }
        Ok(value["data"].clone())
    }
}
fn http_error(e: reqwest::Error) -> Error {
    Error::msg(e.without_url().to_string())
}
#[async_trait]
impl PodApi for RunPodSelf {
    async fn terminate(&self) -> Result<()> {
        let id = serde_json::to_string(&self.pod_id).expect("string JSON");
        self.gql(format!("mutation {{ podTerminate(input:{{podId:{id}}}) }}"))
            .await?;
        Ok(())
    }
    async fn gone(&self) -> Result<bool> {
        let id = serde_json::to_string(&self.pod_id).expect("string JSON");
        let data = self
            .gql(format!(
                "{{ pod(input:{{podId:{id}}}) {{ desiredStatus }} }}"
            ))
            .await?;
        #[derive(serde::Deserialize)]
        struct Data {
            pod: Option<Pod>,
        }
        #[derive(serde::Deserialize)]
        struct Pod {
            #[serde(rename = "desiredStatus", default)]
            desired_status: String,
        }
        let data: Data = serde_json::from_value(data).map_err(|e| Error::msg(e.to_string()))?;
        Ok(data.pod.is_none_or(|p| p.desired_status == "TERMINATED"))
    }
}
pub struct VastSelf {
    pub id: i64,
    pub key: String,
    pub base_url: String,
}
impl VastSelf {
    pub fn new(id: &str, key: &str) -> Result<Self> {
        Ok(Self {
            id: id
                .parse()
                .map_err(|e| Error::msg(format!("CONTAINER_ID: {e}")))?,
            key: key.into(),
            base_url: "https://console.vast.ai/api/v0".into(),
        })
    }
    async fn request(&self, method: reqwest::Method) -> Result<reqwest::Response> {
        let response = crate::http::client()
            .request(
                method,
                format!(
                    "{}/instances/{}/",
                    self.base_url.trim_end_matches('/'),
                    self.id
                ),
            )
            .bearer_auth(&self.key)
            .timeout(Duration::from_secs(30))
            .send()
            .await
            .map_err(http_error)?;
        if response.status().is_success() || response.status() == reqwest::StatusCode::NOT_FOUND {
            Ok(response)
        } else {
            Err(Error::msg(format!(
                "vast: HTTP {}",
                response.status().as_u16()
            )))
        }
    }
}
#[async_trait]
impl PodApi for VastSelf {
    async fn terminate(&self) -> Result<()> {
        self.request(reqwest::Method::DELETE).await?;
        Ok(())
    }
    async fn gone(&self) -> Result<bool> {
        let response = self.request(reqwest::Method::GET).await?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(true);
        }
        let data: Value = response.json().await.map_err(http_error)?;
        Ok(data["instances"].is_null())
    }
}
pub fn self_api(
    provider: &str,
    rp_id: &str,
    rp_key: &str,
    vast_id: &str,
    vast_key: &str,
) -> Option<Arc<dyn PodApi>> {
    if provider == "vast" {
        if vast_id.is_empty() || vast_key.is_empty() {
            None
        } else {
            VastSelf::new(vast_id, vast_key)
                .ok()
                .map(|v| Arc::new(v) as Arc<dyn PodApi>)
        }
    } else if rp_id.is_empty() || rp_key.is_empty() {
        None
    } else {
        Some(Arc::new(RunPodSelf::new(rp_id, rp_key)))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{body_string_contains, header, method, path},
    };
    struct FakeApi {
        terms: AtomicUsize,
        fail: usize,
        gone_after: usize,
    }
    #[async_trait]
    impl PodApi for FakeApi {
        async fn terminate(&self) -> Result<()> {
            let n = self.terms.fetch_add(1, Ordering::SeqCst);
            if n < self.fail {
                Err(Error::msg("retry"))
            } else {
                Ok(())
            }
        }
        async fn gone(&self) -> Result<bool> {
            Ok(self.terms.load(Ordering::SeqCst) >= self.gone_after)
        }
    }
    #[tokio::test(start_paused = true)]
    async fn kill_self_retries() {
        let api = Arc::new(FakeApi {
            terms: AtomicUsize::new(0),
            fail: 2,
            gone_after: 1,
        });
        let start = tokio::time::Instant::now();
        RetryKiller { api: api.clone() }
            .kill_self(CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(api.terms.load(Ordering::SeqCst), 3);
        assert_eq!(start.elapsed(), Duration::from_secs(6));
    }
    #[tokio::test(start_paused = true)]
    async fn kill_self_waits_for_gone() {
        let api = Arc::new(FakeApi {
            terms: AtomicUsize::new(0),
            fail: 0,
            gone_after: 3,
        });
        RetryKiller { api: api.clone() }
            .kill_self(CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(api.terms.load(Ordering::SeqCst), 3);
    }
    #[tokio::test(start_paused = true)]
    async fn kill_self_cancel() {
        let api = Arc::new(FakeApi {
            terms: AtomicUsize::new(0),
            fail: 10,
            gone_after: 20,
        });
        let cancel = CancellationToken::new();
        let c = cancel.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(1)).await;
            c.cancel();
        });
        assert!(matches!(
            RetryKiller { api: api.clone() }.kill_self(cancel).await,
            Err(Error::Cancelled)
        ));
        assert_eq!(api.terms.load(Ordering::SeqCst), 1);
    }
    #[tokio::test]
    async fn runpod_self() {
        let server = MockServer::start().await;
        let mut api = RunPodSelf::new("pod1", "podkey");
        api.url = server.uri();
        Mock::given(method("POST"))
            .and(body_string_contains(
                "mutation { podTerminate(input:{podId:",
            ))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(json!({"data":{"podTerminate":null}})),
            )
            .expect(1)
            .mount(&server)
            .await;
        api.terminate().await.unwrap();
        server.verify().await;
        for (body, expected) in [
            (
                json!({"data":{"pod":{"desiredStatus":"RUNNING"}}}),
                Some(false),
            ),
            (json!({"data":{"pod":null}}), Some(true)),
            (
                json!({"data":{"pod":{"desiredStatus":"TERMINATED"}}}),
                Some(true),
            ),
            (json!({"errors":[{"message":"denied"}]}), None),
        ] {
            server.reset().await;
            Mock::given(method("POST"))
                .and(body_string_contains("desiredStatus"))
                .respond_with(ResponseTemplate::new(200).set_body_json(body))
                .mount(&server)
                .await;
            assert_eq!(api.gone().await.ok(), expected);
        }
    }
    #[tokio::test]
    async fn runpod_self_sends_user_agent() {
        let server = MockServer::start().await;
        let mut api = RunPodSelf::new("pod", "podkey");
        api.url = server.uri();
        Mock::given(method("POST"))
            .and(header("Authorization", "Bearer podkey"))
            .and(header("User-Agent", crate::http::USER_AGENT.as_str()))
            .and(header("Content-Type", "application/json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data":{"pod":null}})))
            .expect(2)
            .mount(&server)
            .await;
        api.terminate().await.unwrap();
        assert!(api.gone().await.unwrap());
    }
    #[tokio::test]
    async fn vast_self_terminate_and_gone() {
        let server = MockServer::start().await;
        let mut api = VastSelf::new("42", "podkey").unwrap();
        api.base_url = server.uri();
        for (status, body, expected) in [
            (200, json!({"instances":{"id":42}}), Some(false)),
            (200, json!({"instances":null}), Some(true)),
            (404, json!({}), Some(true)),
            (403, json!({}), None),
            (401, json!({}), None),
        ] {
            server.reset().await;
            Mock::given(path("/instances/42/"))
                .respond_with(ResponseTemplate::new(status).set_body_json(body))
                .mount(&server)
                .await;
            assert_eq!(api.gone().await.ok(), expected);
            assert_eq!(api.terminate().await.is_ok(), expected.is_some());
        }
    }
    #[tokio::test]
    async fn vast_self_sends_user_agent() {
        let server = MockServer::start().await;
        let mut api = VastSelf::new("42", "podkey").unwrap();
        api.base_url = server.uri();
        Mock::given(path("/instances/42/"))
            .and(header("Authorization", "Bearer podkey"))
            .and(header("User-Agent", crate::http::USER_AGENT.as_str()))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"instances":null})))
            .expect(2)
            .mount(&server)
            .await;
        api.terminate().await.unwrap();
        assert!(api.gone().await.unwrap());
    }
    #[test]
    fn self_api_per_provider() {
        for p in ["runpod", "", "other"] {
            assert!(self_api(p, "p", "k", "", "").is_some());
            assert!(self_api(p, "", "", "1", "v").is_none());
        }
        assert!(self_api("vast", "p", "k", "", "").is_none());
        assert!(self_api("vast", "", "", "1", "v").is_some());
        assert!(self_api("vast", "", "", "bad", "v").is_none());
        let clean = crate::process::CleanEnv::filter(
            [
                ("RUNPOD_API_KEY", "k"),
                ("CONTAINER_API_KEY", "v"),
                ("PATH", "/bin"),
            ]
            .into_iter()
            .map(|(k, v)| (k.into(), v.into())),
        );
        assert_eq!(clean, vec![("PATH".into(), "/bin".into())]);
    }
}
