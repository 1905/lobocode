use std::sync::LazyLock;

pub static USER_AGENT: LazyLock<String> =
    LazyLock::new(|| format!("lobo-agent/{}", crate::VERSION));

pub fn client() -> reqwest::Client {
    // Connection drivers belong to the runtime that sent the request. A global
    // pool can reuse a socket whose runtime has stopped. Callers that repeat
    // requests retain this client for the lifetime of their operation.
    reqwest::Client::builder()
        .user_agent(USER_AGENT.as_str())
        .build()
        .expect("agent HTTP client")
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::{Mock, MockServer, ResponseTemplate, matchers::header};

    #[test]
    fn clients_do_not_share_connections_across_runtimes() {
        let runtime = || {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
        };
        let first = runtime();
        let server = first.block_on(async {
            let server = MockServer::start().await;
            Mock::given(header("user-agent", USER_AGENT.as_str()))
                .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
                .expect(2)
                .mount(&server)
                .await;
            assert_eq!(
                client()
                    .get(server.uri())
                    .send()
                    .await
                    .unwrap()
                    .text()
                    .await
                    .unwrap(),
                "ok"
            );
            // Give the connection driver time to return its socket to the pool.
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            server
        });
        // The first runtime remains alive but isn't being polled. A global pool
        // would reuse its socket and wait forever for that runtime's driver.
        runtime().block_on(async {
            let result = tokio::time::timeout(std::time::Duration::from_secs(1), async {
                client()
                    .get(server.uri())
                    .send()
                    .await
                    .unwrap()
                    .text()
                    .await
                    .unwrap()
            })
            .await
            .expect("HTTP client reused a connection owned by another runtime");
            assert_eq!(result, "ok");
            drop(server);
        });
    }

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
