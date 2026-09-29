use std::time::Duration;
pub const USER_AGENT: &str = concat!("lobo/", env!("CARGO_PKG_VERSION"));
pub fn client(timeout: Duration) -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(timeout)
        .build()
        .expect("TLS client initialization")
}
#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{header_regex, method},
    };
    #[tokio::test]
    async fn client_sends_user_agent() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(header_regex("User-Agent", "^lobo/"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;
        assert_eq!(
            client(Duration::from_secs(3))
                .get(server.uri())
                .send()
                .await
                .unwrap()
                .status(),
            200
        );
    }
}
