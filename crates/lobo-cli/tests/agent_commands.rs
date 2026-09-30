use lobo_cli::app::{App, Io, Term};
use lobo_core::{
    Error, Result,
    control::{HttpAgent, testkit},
    provider::{CreateOpts, Provider},
};
use lobo_proto::{Instance, Manifest, Stage, Status};
use std::{
    io::Write,
    os::unix::fs::PermissionsExt,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio_util::sync::CancellationToken;
use wiremock::{
    Mock, MockServer, Request, ResponseTemplate,
    matchers::{method, path},
};

#[derive(Clone, Default)]
struct Buffer(Arc<Mutex<Vec<u8>>>);
impl Write for Buffer {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(b);
        Ok(b.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
struct Endpoint(String);
#[async_trait::async_trait]
impl Provider for Endpoint {
    fn name(&self) -> &'static str {
        "local"
    }
    fn replaceable(&self) -> bool {
        false
    }
    async fn list(&self) -> Result<Vec<Instance>> {
        Ok(vec![Instance {
            provider: "local".into(),
            id: "test".into(),
            api_url: format!("{}/v1", self.0),
            agent_url: self.0.clone(),
            ..Default::default()
        }])
    }
    async fn get(&self, _: &str) -> Result<Instance> {
        Ok(self.list().await?.remove(0))
    }
    async fn rent(
        &self,
        _: &CreateOpts,
        _: CancellationToken,
        _: &(dyn Fn(String) + Sync),
    ) -> Result<Instance> {
        panic!("read-only command rented an instance")
    }
    async fn delete(&self, _: &str) -> Result<()> {
        panic!("read-only command deleted an instance")
    }
}
struct Fixture {
    tmp: tempfile::TempDir,
    app: App,
    server: MockServer,
}
impl Fixture {
    async fn new() -> Self {
        let server = MockServer::start().await;
        let tmp = tempfile::tempdir().unwrap();
        let config = tmp.path().join("config.env");
        std::fs::write(&config, "LOBO_API_KEY=sk-test\n").unwrap();
        std::fs::set_permissions(&config, std::fs::Permissions::from_mode(0o600)).unwrap();
        let mut app = App::real();
        app.term = Term::default();
        let base = server.uri();
        let clock = app.clock.clone();
        app.deps = Arc::new(move |cfg, _| {
            let mut d = testkit::deps(Default::default(), Default::default(), clock.clone());
            d.providers.clear();
            d.providers
                .insert("local".into(), Arc::new(Endpoint(base.clone())));
            let key = cfg.lobo_api_key.clone();
            d.cfg = cfg;
            d.new_agent = Arc::new(move |url| Arc::new(HttpAgent::new(url, &key)));
            Ok(d)
        });
        Self { tmp, app, server }
    }
    async fn agent(&self, version_status: u16, model: &str) {
        Mock::given(method("GET"))
            .and(path("/api/version"))
            .respond_with(
                ResponseTemplate::new(version_status).set_body_json(Manifest {
                    version: "dev-test".into(),
                    ..Default::default()
                }),
            )
            .mount(&self.server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/status"))
            .respond_with(ResponseTemplate::new(200).set_body_json(Status {
                stage: Stage::Ready,
                model: model.into(),
                ..Default::default()
            }))
            .mount(&self.server)
            .await;
    }
    async fn invoke(&self, args: &[&str]) -> (i32, String, String) {
        let out = Buffer::default();
        let err = Buffer::default();
        let config = self.tmp.path().join("config.env");
        let argv = ["lobo", "--config", config.to_str().unwrap()]
            .into_iter()
            .chain(args.iter().copied())
            .map(Into::into)
            .collect();
        let code = lobo_cli::run(
            &self.app,
            argv,
            &mut Io {
                out: Box::new(out.clone()),
                err: Box::new(err.clone()),
                input: Box::new(std::io::empty()),
            },
        )
        .await;
        let text = |b: &Buffer| String::from_utf8(b.0.lock().unwrap().clone()).unwrap();
        (code, text(&out), text(&err))
    }
}
#[tokio::test]
async fn logs_preserve_bytes_and_forward_count_and_auth() {
    let f = Fixture::new().await;
    Mock::given(method("GET"))
        .and(path("/api/logs"))
        .respond_with(|r: &Request| {
            assert_eq!(r.headers.get("Authorization").unwrap(), "Bearer sk-test");
            assert!(matches!(r.url.query(), Some("n=5" | "n=0" | "n=5000")));
            ResponseTemplate::new(200).set_body_string("last log line")
        })
        .expect(3)
        .mount(&f.server)
        .await;
    for n in ["5", "-1", "5000"] {
        let (code, out, err) = f.invoke(&["logs", "-n", n]).await;
        assert_eq!(code, 0, "{err}");
        assert_eq!(out, "last log line");
        assert_eq!(err, "");
    }
}
#[tokio::test]
async fn chat_tool_output_truncation_and_models() {
    for (model, text) in [
        ("q6", "hello".to_owned()),
        ("", "x".repeat(250)),
        ("q8", format!("{}é-tail", "a".repeat(199))),
    ] {
        let f = Fixture::new().await;
        f.agent(200, model).await;
        let reply = text.clone();
        let model = if model.is_empty() {
            lobo_core::release::DEFAULT_MODEL
        } else {
            model
        };
        let alias = lobo_proto::catalog::get(model).unwrap().alias.clone();
        Mock::given(method("POST")).and(path("/v1/chat/completions")).respond_with(move |r: &Request| {
            assert_eq!(r.headers.get("Authorization").unwrap(), "Bearer sk-test");
            let body: serde_json::Value = serde_json::from_slice(&r.body).unwrap(); assert_eq!(body["model"], alias);
            if body["stream"] == true {
                let chunk = serde_json::json!({"choices": [{"delta": {"content": reply}, "finish_reason": "stop"}]});
                ResponseTemplate::new(200).set_body_string(format!("data: {chunk}\n\ndata: [DONE]\n"))
            } else {ResponseTemplate::new(200).set_body_json(serde_json::json!({"choices": [{"finish_reason":"tool_calls", "message":{"tool_calls":[{"function":{"name":"get_weather", "arguments":"{\"location\":\"Bali\"}"}}]}}]}))}
        }).expect(2).mount(&f.server).await;
        let (code, out, err) = f.invoke(&["test"]).await;
        assert_eq!(code, 0, "{err}");
        assert_eq!(out, format!("{}\n", lobo_cli::cmd::test::preview(&text)));
        assert!(
            err.contains("✓ streamed chat release=dev-test took="),
            "{err}"
        );
        assert!(err.contains("✓ tool call: arguments is a JSON string took="));
        if text.len() > 200 {
            assert!(out.ends_with("…\n"));
            assert!(out.len() <= 204);
        }
    }
    assert_eq!(
        lobo_cli::cmd::test::preview(&format!("{}é-tail", "a".repeat(199))),
        format!("{}…", "a".repeat(199))
    );
}
#[tokio::test]
async fn agent_and_tool_failures_remain_errors() {
    let f = Fixture::new().await;
    f.agent(503, "q8").await;
    let (code, out, err) = f.invoke(&["test"]).await;
    assert_eq!(code, 1);
    assert!(out.is_empty());
    assert_eq!(
        err,
        format!(
            "error: lobo not reachable at {}/v1: /api/version: HTTP 503\n",
            f.server.uri()
        )
    );
    let f = Fixture::new().await;
    f.agent(200, "q8").await;
    Mock::given(method("POST")).respond_with(|r: &Request| {
        let body: serde_json::Value = serde_json::from_slice(&r.body).unwrap();
        if body["stream"] == true {ResponseTemplate::new(200).set_body_string("data: {\"choices\":[{\"delta\":{\"content\":\"ok\"},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n")}
        else {ResponseTemplate::new(200).set_body_string("{\"choices\":[]}")}
    }).expect(2).mount(&f.server).await;
    let (code, out, err) = f.invoke(&["test"]).await;
    assert_eq!(code, 1);
    assert_eq!(out, "ok\n");
    assert!(
        err.ends_with("error: tool call: no choices\n{\"choices\":[]}\n"),
        "{err}"
    );
}
#[tokio::test]
async fn cancelling_read_only_commands_stops_pending_http() {
    for command in ["logs", "test"] {
        let f = Fixture::new().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_secs(60)))
            .mount(&f.server)
            .await;
        let cancel = async {
            loop {
                if !f.server.received_requests().await.unwrap().is_empty() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
            f.app.cancel.cancel();
        };
        let args = [command];
        let ((code, out, err), ()) = tokio::time::timeout(Duration::from_secs(3), async {
            tokio::join!(f.invoke(&args), cancel)
        })
        .await
        .unwrap();
        assert_eq!(code, 1);
        assert!(out.is_empty());
        assert_eq!(err, format!("error: {}\n", Error::Cancelled));
    }
}
