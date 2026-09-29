use super::{AgentApi, Error, Manifest, Result, Status};
use std::time::Duration;
pub struct HttpAgent {
    pub base: String,
    key: String,
    http: reqwest::Client,
}
impl HttpAgent {
    pub fn new(base: &str, key: &str) -> Self {
        Self {
            base: base.trim_end_matches('/').into(),
            key: key.into(),
            http: crate::http::client(Duration::from_secs(5)),
        }
    }
    pub fn on_domain(domain: &str, key: &str) -> Self {
        Self::new(&format!("https://{domain}"), key)
    }
    async fn get(&self, path: &str, auth: bool) -> Result<Vec<u8>> {
        let mut req = self.http.get(format!("{}{path}", self.base));
        if auth {
            req = req.bearer_auth(&self.key);
        }
        let response = req.send().await?;
        if response.status() != 200 {
            return Err(Error::Other(format!(
                "{path}: HTTP {}",
                response.status().as_u16()
            )));
        }
        Ok(response.bytes().await?.to_vec())
    }
}
#[async_trait::async_trait]
impl AgentApi for HttpAgent {
    async fn status(&self) -> Result<Status> {
        Ok(serde_json::from_slice(
            &self.get("/api/status", false).await?,
        )?)
    }
    async fn version(&self) -> Result<Manifest> {
        Ok(serde_json::from_slice(
            &self.get("/api/version", false).await?,
        )?)
    }
    async fn logs(&self, n: usize) -> Result<String> {
        Ok(String::from_utf8_lossy(&self.get(&format!("/api/logs?n={n}"), true).await?).into())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::{Mock, MockServer, Request, ResponseTemplate, matchers::any};
    #[tokio::test]
    async fn http_agent_url() {
        let s = MockServer::start().await;
        Mock::given(any())
            .respond_with(|r: &Request| match r.url.path() {
                "/api/version" => {
                    assert!(!r.headers.contains_key("Authorization"));
                    ResponseTemplate::new(200)
                        .set_body_json(serde_json::json!({"version":"dev","git_sha":"abc"}))
                }
                "/api/status" => ResponseTemplate::new(200).set_body_json(Status::default()),
                "/api/logs"
                    if r.headers
                        .get("Authorization")
                        .is_some_and(|v| v == "Bearer sk-x") =>
                {
                    assert_eq!(r.url.query(), Some("n=7"));
                    ResponseTemplate::new(200).set_body_string("line\n")
                }
                _ => ResponseTemplate::new(401),
            })
            .mount(&s)
            .await;
        let agent = HttpAgent::new(&(s.uri() + "///"), "sk-x");
        assert_eq!(agent.base, s.uri());
        assert_eq!(agent.version().await.unwrap().git_sha, "abc");
        assert_eq!(agent.status().await.unwrap(), Status::default());
        assert_eq!(agent.logs(7).await.unwrap(), "line\n");
        assert_eq!(
            HttpAgent::new(&s.uri(), "wrong")
                .logs(7)
                .await
                .unwrap_err()
                .to_string(),
            "/api/logs?n=7: HTTP 401"
        );
        assert_eq!(
            HttpAgent::on_domain("lobo.example.com", "k").base,
            "https://lobo.example.com"
        );
    }
}
