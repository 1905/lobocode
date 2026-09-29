use crate::{Error, Result};
use std::time::Duration;
use tokio_util::sync::CancellationToken;

pub async fn wait_healthy(base: &str, poll: Duration, cancel: CancellationToken) -> Result<()> {
    let client = crate::http::client();
    loop {
        tokio::select! {
            biased;
            () = cancel.cancelled() => return Err(Error::Cancelled),
            response = client.get(format!("{}/health", base.trim_end_matches('/'))).timeout(Duration::from_secs(3)).send() => {
                if response.is_ok_and(|r| r.status() == 200) { return Ok(()); }
            }
        }
        tokio::select! {
            () = cancel.cancelled() => return Err(Error::Cancelled),
            () = tokio::time::sleep(poll) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    use wiremock::{Mock, MockServer, Request, ResponseTemplate, matchers::path};
    #[tokio::test]
    async fn wait_healthy_polls_until_200() {
        let server = MockServer::start().await;
        let count = Arc::new(AtomicUsize::new(0));
        let calls = count.clone();
        Mock::given(path("/health"))
            .respond_with(move |_: &Request| {
                ResponseTemplate::new(if calls.fetch_add(1, Ordering::Relaxed) < 2 {
                    503
                } else {
                    200
                })
            })
            .mount(&server)
            .await;
        wait_healthy(
            &server.uri(),
            Duration::from_millis(10),
            CancellationToken::new(),
        )
        .await
        .unwrap();
        assert_eq!(count.load(Ordering::Relaxed), 3);
    }
    #[tokio::test]
    async fn wait_healthy_cancel() {
        let token = CancellationToken::new();
        token.cancel();
        assert!(matches!(
            wait_healthy("http://127.0.0.1:1", Duration::from_secs(10), token).await,
            Err(Error::Cancelled)
        ));
    }
}
