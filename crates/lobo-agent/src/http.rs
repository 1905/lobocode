use std::sync::LazyLock;

pub static USER_AGENT: LazyLock<String> =
    LazyLock::new(|| format!("lobo-agent/{}", crate::VERSION));

pub fn client() -> reqwest::Client {
    static CLIENT: LazyLock<reqwest::Client> = LazyLock::new(|| {
        reqwest::Client::builder()
            .user_agent(USER_AGENT.as_str())
            .build()
            .expect("agent HTTP client")
    });
    CLIENT.clone()
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::{Mock, MockServer, ResponseTemplate, matchers::header};

    #[tokio::test]
    async fn client_sends_user_agent() {
        let server = MockServer::start().await;
        Mock::given(header("user-agent", USER_AGENT.as_str()))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&server)
            .await;
        assert_eq!(
            client().get(server.uri()).send().await.unwrap().status(),
            200
        );
    }
}
