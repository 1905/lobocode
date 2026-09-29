use crate::{LogSource, Runner};
use axum::{
    Router,
    extract::{RawQuery, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use bytes::Bytes;
use std::sync::Arc;
#[derive(Clone)]
struct Api {
    runner: Arc<Runner>,
    key: String,
    logs: LogSource,
    version: Bytes,
}
pub fn router(runner: Arc<Runner>, api_key: String, logs: LogSource, version: Bytes) -> Router {
    Router::new()
        .route("/api/version", get(version_route))
        .route("/api/status", get(status))
        .route("/api/logs", get(logs_route))
        .with_state(Api {
            runner,
            key: api_key,
            logs,
            version,
        })
}
async fn version_route(State(api): State<Api>) -> impl IntoResponse {
    ([(header::CONTENT_TYPE, "application/json")], api.version)
}
async fn status(State(api): State<Api>) -> Response {
    match serde_json::to_vec(&api.runner.status()) {
        Ok(mut body) => {
            body.push(b'\n');
            ([(header::CONTENT_TYPE, "application/json")], body).into_response()
        }
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}
async fn logs_route(
    State(api): State<Api>,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    if headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        != Some(&format!("Bearer {}", api.key))
    {
        return (StatusCode::UNAUTHORIZED, "unauthorized\n").into_response();
    }
    let n = query
        .as_deref()
        .and_then(|q| {
            url::form_urlencoded::parse(q.as_bytes())
                .find(|(k, _)| k == "n")
                .and_then(|(_, v)| v.parse::<i64>().ok())
        })
        .filter(|n| *n > 0)
        .unwrap_or(200)
        .min(1000) as usize;
    (
        [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        api.logs.tail(n).join("\n") + "\n",
    )
        .into_response()
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LogRing, testutil::*};
    #[tokio::test]
    async fn api_routes() {
        let runner = Runner::new(fake_deps(Arc::new(FakeKiller::default())), cfg());
        let logs = Arc::new(LogRing::new(2000));
        for i in 0..1500 {
            logs.write(format!("line {i}\n").as_bytes());
        }
        let version = Bytes::from_static(b"{ \"version\": \"dev\" }\n");
        let app = router(runner, "secret".into(), logs, version.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let task = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let client = crate::http::client();
        let response = client
            .get(format!("{base}/api/version"))
            .send()
            .await
            .unwrap();
        assert_eq!(response.headers()["Content-Type"], "application/json");
        assert_eq!(response.bytes().await.unwrap(), version);
        let status_body = client
            .get(format!("{base}/api/status"))
            .send()
            .await
            .unwrap()
            .bytes()
            .await
            .unwrap();
        assert!(status_body.ends_with(b"\n"));
        let status: serde_json::Value = serde_json::from_slice(&status_body).unwrap();
        assert!(status["gpu"].is_null());
        assert!(status["llama"].is_null());
        assert_eq!(status["stage"], "boot");
        let keys: std::collections::BTreeSet<_> = status
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(
            keys,
            [
                "boot_id",
                "stage",
                "stage_detail",
                "download",
                "uptime_s",
                "idle_s",
                "kill_in_s",
                "kill_reason",
                "expires_at",
                "gpu",
                "host",
                "llama",
                "metrics_failures",
                "model",
                "ctx",
                "timings"
            ]
            .into_iter()
            .collect()
        );
        for key in ["", "wrong"] {
            let response = client
                .get(format!("{base}/api/logs"))
                .bearer_auth(key)
                .send()
                .await
                .unwrap();
            assert_eq!(response.status(), 401);
            assert_eq!(response.text().await.unwrap(), "unauthorized\n");
        }
        for (n, count) in [
            ("5", 5),
            ("5000", 1000),
            ("bad", 200),
            ("-1", 200),
            ("0", 200),
            ("", 200),
        ] {
            let text = client
                .get(format!("{base}/api/logs?n={n}"))
                .bearer_auth("secret")
                .send()
                .await
                .unwrap()
                .text()
                .await
                .unwrap();
            assert_eq!(text.lines().count(), count);
            assert!(text.ends_with("line 1499\n"));
        }
        task.abort();
        let _ = task.await;
    }
}
