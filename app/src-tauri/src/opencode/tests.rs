use super::*;
use crate::{backend::PreparedUp, types::OpenCodeResult};
use async_trait::async_trait;
use lobo_core::{control, local::memory::MemoryAssessment};
use lobo_proto::{ConfigShow, Instance, Listing, Readiness, Status, UpRequest};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{header, method, path},
};

pub(crate) const SECRET: &str = "fixture-private-key-never-ipc";
type SetupHook = Arc<dyn Fn() + Send + Sync>;
pub(crate) struct FixtureBackend {
    pub(crate) root: tempfile::TempDir,
    pub(crate) owner: Mutex<Option<RuntimeTarget>>,
    pub(crate) snap: Mutex<Snap>,
    pub(crate) snapshots: AtomicUsize,
    pub(crate) preparations: AtomicUsize,
    pub(crate) gated: AtomicBool,
    pub(crate) started: Notify,
    pub(crate) release: Notify,
    pub(crate) before_commit: Mutex<Option<SetupHook>>,
    pub(crate) after_validate: Mutex<Option<SetupHook>>,
    pub(crate) panic_prepare: AtomicBool,
    client: Client,
}
impl FixtureBackend {
    pub(crate) fn new(endpoint: &str, provider: &str, model: &str) -> Arc<Self> {
        let root = tempfile::tempdir().unwrap();
        fs::write(
            root.path().join("lobo.env"),
            format!("LOBO_API_KEY={SECRET}\nLOBO_MODEL=q8\nLOBO_CTX=65536\n"),
        )
        .unwrap();
        fs::write(
            root.path().join("opencode.jsonc"),
            "{ // preserve\n \"small_model\": \"custom/model\"\n}\n",
        )
        .unwrap();
        let owner = RuntimeTarget {
            provider: provider.into(),
            boot_id: "owned-boot".into(),
            instance_id: Some("owned-instance".into()),
            agent_url: None,
            api_url: Some(endpoint.into()),
            local_pid: (provider == "local").then_some(4242),
            local_start_id: (provider == "local").then_some(100),
        };
        let snap = Snap {
            down: false,
            pod: Some(Instance {
                provider: provider.into(),
                id: "owned-instance".into(),
                api_url: endpoint.into(),
                ..Default::default()
            }),
            status: Some(Status {
                boot_id: "owned-boot".into(),
                stage: Stage::Ready,
                model: model.into(),
                ctx: 4096,
                ..Default::default()
            }),
            ..Default::default()
        };
        Arc::new(Self {
            root,
            owner: Mutex::new(Some(owner)),
            snap: Mutex::new(snap),
            snapshots: AtomicUsize::new(0),
            preparations: AtomicUsize::new(0),
            gated: AtomicBool::new(false),
            started: Notify::new(),
            release: Notify::new(),
            before_commit: Mutex::new(None),
            after_validate: Mutex::new(None),
            panic_prepare: AtomicBool::new(false),
            client: client().unwrap(),
        })
    }
    pub(crate) fn destination(&self) -> PathBuf {
        self.root.path().join("opencode.jsonc")
    }
    pub(crate) fn bytes(&self) -> Vec<u8> {
        fs::read(self.destination()).unwrap()
    }
    fn unchanged(&self, bytes: &[u8]) {
        assert_eq!(self.bytes(), bytes);
        assert_eq!(fs::read_dir(self.root.path()).unwrap().count(), 2);
    }
}
#[async_trait]
impl Backend for FixtureBackend {
    fn config_path(&self) -> PathBuf {
        self.root.path().join("lobo.env")
    }
    fn opencode_path(&self) -> Result<PathBuf> {
        Ok(self.destination())
    }
    fn load_owner(&self) -> Result<Option<RuntimeTarget>> {
        Ok(self.owner.lock().unwrap().clone())
    }
    fn adopt_owner(&self, _: Option<RuntimeTarget>, _: RuntimeTarget) -> Result<()> {
        panic!("setup must not adopt")
    }
    async fn config(&self) -> Result<(ConfigShow, Readiness)> {
        Ok((ConfigShow::default(), Readiness::default()))
    }
    async fn models(&self) -> Result<Listing> {
        panic!("setup must not discover models on disk")
    }
    async fn snapshot_owned(
        &self,
        provider: &str,
        captured: Option<RuntimeTarget>,
    ) -> Result<(Option<RuntimeTarget>, Snap)> {
        self.snapshots.fetch_add(1, Ordering::SeqCst);
        let owner = self.owner.lock().unwrap().clone();
        let captured = captured.expect("setup/Stop must keep their captured owner");
        assert_eq!(provider, captured.provider);
        Ok((owner, self.snap.lock().unwrap().clone()))
    }
    async fn prepare_opencode(&self, owner: RuntimeTarget) -> Result<PreparedOpenCode> {
        self.preparations.fetch_add(1, Ordering::SeqCst);
        self.started.notify_one();
        if self.gated.load(Ordering::SeqCst) {
            self.release.notified().await;
        }
        assert!(
            !self.panic_prepare.load(Ordering::SeqCst),
            "task-owned preparation panic"
        );
        prepare(self, owner, &self.client).await
    }
    fn configure_opencode(
        &self,
        path: &Path,
        prepared: &PreparedOpenCode,
        make_default: bool,
        validate: &dyn Fn() -> Result<()>,
    ) -> Result<OpenCodeResult> {
        let hook = self.before_commit.lock().unwrap().clone();
        if let Some(hook) = hook {
            hook();
        }
        let result = configure(self, path, prepared, make_default, &|| {
            validate()?;
            let hook = self.after_validate.lock().unwrap().clone();
            if let Some(hook) = hook {
                hook();
            }
            Ok(())
        });
        // The controller must retain its guards until this entire call returns.
        if result.is_ok() {
            let hook = self.after_validate.lock().unwrap().clone();
            if let Some(hook) = hook {
                hook();
            }
        }
        result
    }
    async fn local_memory(&self, _: &str) -> Result<MemoryAssessment> {
        Err(error("fixture memory is unavailable"))
    }
    fn prepare_up(&self, _: UpRequest) -> Result<PreparedUp> {
        panic!("setup must not start")
    }
    fn up(
        &self,
        _: PreparedUp,
        _: Option<RuntimeTarget>,
        _: CancellationToken,
        _: control::OwnerSink,
    ) -> Result<control::UpOperation> {
        panic!("setup must not start")
    }
    async fn down(&self, _: RuntimeTarget) -> Result<f64> {
        *self.owner.lock().unwrap() = None;
        *self.snap.lock().unwrap() = Snap {
            down: true,
            ..Default::default()
        };
        Ok(0.0)
    }
    async fn telemetry(&self, _: RuntimeTarget) -> Result<Status> {
        panic!("setup must not sample unrelated telemetry")
    }
    async fn api_key(&self) -> Result<String> {
        panic!("setup must read the captured config revision")
    }
    async fn save(&self, _: std::collections::BTreeMap<String, String>) -> Result<()> {
        panic!("fixture save not used")
    }
}

pub(crate) async fn serve_models(server: &MockServer, model: &str, count: u64) {
    Mock::given(method("GET"))
        .and(path("/v1/models"))
        .and(header("authorization", format!("Bearer {SECRET}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            serde_json::json!({"data":[{"id":lobo_proto::catalog::get(model).unwrap().alias}]}),
        ))
        .expect(count)
        .mount(server)
        .await;
}
async fn prepared(backend: &FixtureBackend) -> Result<PreparedOpenCode> {
    backend
        .prepare_opencode(backend.load_owner()?.unwrap())
        .await
}

#[tokio::test]
async fn authenticates_only_model_get_for_actual_local_and_cloud_q6_q8_context() {
    for (provider, model) in [
        ("local", "q6"),
        ("local", "q8"),
        ("runpod", "q6"),
        ("vast", "q8"),
    ] {
        let server = MockServer::start().await;
        serve_models(&server, model, 1).await;
        let b = FixtureBackend::new(&format!("{}/v1", server.uri()), provider, model);
        let p = prepared(&b).await.unwrap();
        assert_eq!(p.binding.context, 4096);
        assert_eq!(
            p.binding.model_alias,
            lobo_proto::catalog::get(model).unwrap().alias
        );
        assert_eq!(
            p.binding.provider,
            if provider == "local" {
                "lobo-local"
            } else {
                "lobo"
            }
        );
        assert_eq!(b.snapshots.load(Ordering::SeqCst), 2);
        let result = configure(&*b, &b.destination(), &p, false, &|| Ok(())).unwrap();
        assert!(result.changed);
        let text = String::from_utf8(b.bytes()).unwrap();
        assert!(text.contains("// preserve"));
        assert!(text.contains("custom/model"));
        assert!(text.contains("4096"));
        assert!(!text.contains(SECRET));
        let ipc = serde_json::to_string(&result).unwrap();
        assert!(!ipc.contains(SECRET));
        assert_eq!(result.message, SAVED);
        let requests = server.received_requests().await.unwrap();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].method.as_str(), "GET");
        assert_eq!(requests[0].url.path(), "/v1/models");
        assert!(requests[0].body.is_empty());
    }
}
#[tokio::test]
async fn authentication_model_and_parser_errors_are_bounded_and_write_nothing() {
    for response in [
        ResponseTemplate::new(401).set_body_string(SECRET),
        ResponseTemplate::new(403).set_body_string(SECRET),
        ResponseTemplate::new(500).set_body_string(SECRET),
        ResponseTemplate::new(200).set_body_string(format!("{{bad:{SECRET}")),
        ResponseTemplate::new(200).set_body_json(serde_json::json!({"data":[{"id":"wrong"}]})),
        ResponseTemplate::new(200).set_body_string("x".repeat(MODELS_LIMIT + 1)),
    ] {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/models"))
            .respond_with(response)
            .expect(1)
            .mount(&server)
            .await;
        let b = FixtureBackend::new(&format!("{}/v1", server.uri()), "local", "q6");
        let original = b.bytes();
        let Err(e) = prepared(&b).await else {
            panic!("discovery must fail")
        };
        assert!(!serde_json::to_string(&e).unwrap().contains(SECRET));
        assert!(e.message.len() < 160);
        b.unchanged(&original);
    }
}
#[tokio::test]
async fn empty_key_and_unsafe_endpoints_send_nothing() {
    let server = MockServer::start().await;
    let b = FixtureBackend::new(&format!("{}/v1", server.uri()), "local", "q6");
    fs::write(b.config_path(), "LOBO_API_KEY=\n").unwrap();
    let original = b.bytes();
    assert!(prepared(&b).await.is_err());
    b.unchanged(&original);
    assert!(server.received_requests().await.unwrap().is_empty());
    for url in [
        "file:///v1",
        "http://key:secret@localhost/v1",
        "http://localhost/v1?q=x",
        "http://localhost/v1#x",
        "http://localhost/",
    ] {
        assert!(models_url(url).is_err());
    }
}

#[tokio::test]
async fn rotated_saved_key_does_not_repair_with_a_running_old_key() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/models"))
        .and(header("authorization", "Bearer rotated-saved-key"))
        .respond_with(ResponseTemplate::new(401).set_body_string(SECRET))
        .expect(1)
        .mount(&server)
        .await;
    let b = FixtureBackend::new(&format!("{}/v1", server.uri()), "local", "q6");
    fs::write(b.config_path(), "LOBO_API_KEY=rotated-saved-key\n").unwrap();
    let original = b.bytes();
    let Err(e) = prepared(&b).await else {
        panic!("wrong running key must fail")
    };
    assert_eq!(
        e.message,
        "The runtime rejected the saved API key. Repair Lobocode access first."
    );
    b.unchanged(&original);
    assert_eq!(
        fs::read_to_string(b.config_path()).unwrap(),
        "LOBO_API_KEY=rotated-saved-key\n"
    );
}

#[test]
fn captured_key_parser_keeps_dotenv_semantics_and_sanitizes_bad_source() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("lobo.env");
    fs::write(
        &path,
        "PREFIX=fixture\nexport LOBO_API_KEY=\"${PREFIX}-key\"\n",
    )
    .unwrap();
    assert_eq!(existing_key(&path).unwrap().1, "fixture-key");
    fs::write(&path, format!("LOBO_API_KEY=\"{SECRET}")).unwrap();
    let Err(error) = existing_key(&path) else {
        panic!("invalid dotenv must fail")
    };
    assert!(!serde_json::to_string(&error).unwrap().contains(SECRET));
    fs::write(&path, b"LOBO_API_KEY=\xff").unwrap();
    assert!(existing_key(&path).is_err());
}

#[tokio::test]
async fn rotated_key_after_authentication_rejects_final_commit() {
    let server = MockServer::start().await;
    serve_models(&server, "q6", 1).await;
    let b = FixtureBackend::new(&format!("{}/v1", server.uri()), "local", "q6");
    let original = b.bytes();
    let p = prepared(&b).await.unwrap();
    fs::write(b.config_path(), "LOBO_API_KEY=rotated-saved-key\n").unwrap();
    assert!(configure(&*b, &b.destination(), &p, true, &|| Ok(())).is_err());
    assert_eq!(b.bytes(), original);
    assert_eq!(
        fs::read_dir(b.root.path().join("opencode-keys"))
            .unwrap()
            .count(),
        0
    );
    assert!(
        !fs::read_dir(b.root.path()).unwrap().any(|e| e
            .unwrap()
            .file_name()
            .to_string_lossy()
            .contains("backup"))
    );
}
#[tokio::test]
async fn redirects_never_forward_authorization_even_on_the_same_host() {
    let server = MockServer::start().await;
    Mock::given(path("/v1/models"))
        .respond_with(
            ResponseTemplate::new(302)
                .insert_header("location", format!("{}/redirected", server.uri())),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(path("/redirected"))
        .respond_with(ResponseTemplate::new(200))
        .expect(0)
        .mount(&server)
        .await;
    let b = FixtureBackend::new(&format!("{}/v1", server.uri()), "local", "q6");
    let original = b.bytes();
    assert!(prepared(&b).await.is_err());
    b.unchanged(&original);
}

#[tokio::test]
async fn cross_host_redirect_sends_nothing_to_the_destination() {
    let source = MockServer::start().await;
    let destination = MockServer::start().await;
    Mock::given(path("/v1/models"))
        .respond_with(
            ResponseTemplate::new(307)
                .insert_header("location", format!("{}/v1/models", destination.uri())),
        )
        .expect(1)
        .mount(&source)
        .await;
    let b = FixtureBackend::new(&format!("{}/v1", source.uri()), "local", "q6");
    assert!(prepared(&b).await.is_err());
    assert!(destination.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn chunked_body_without_content_length_still_has_a_hard_limit() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut connection, _) = listener.accept().await.unwrap();
        let mut request = Vec::new();
        loop {
            let mut buffer = [0; 512];
            let count = connection.read(&mut buffer).await.unwrap();
            if count == 0 {
                break;
            }
            request.extend_from_slice(&buffer[..count]);
            if request.windows(4).any(|window| window == b"\r\n\r\n") {
                break;
            }
        }
        assert!(String::from_utf8_lossy(&request).starts_with("GET /v1/models HTTP/1.1\r\n"));
        connection
            .write_all(
                b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n",
            )
            .await
            .unwrap();
        let chunk = vec![b'x'; 4096];
        for _ in 0..17 {
            if connection.write_all(b"1000\r\n").await.is_err()
                || connection.write_all(&chunk).await.is_err()
                || connection.write_all(b"\r\n").await.is_err()
            {
                return;
            }
        }
        let _ = connection.write_all(b"0\r\n\r\n").await;
    });
    let b = FixtureBackend::new(&format!("http://{address}/v1"), "local", "q6");
    let original = b.bytes();
    let Err(e) = prepared(&b).await else {
        panic!("streaming body must fail")
    };
    assert_eq!(e.message, "The model discovery response is too large.");
    b.unchanged(&original);
    server.await.unwrap();
}
#[tokio::test]
async fn short_client_timeout_returns_safe_error_without_writes() {
    let server = MockServer::start().await;
    Mock::given(path("/v1/models"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_delay(Duration::from_secs(1))
                .set_body_string(SECRET),
        )
        .expect(1)
        .mount(&server)
        .await;
    let b = FixtureBackend::new(&format!("{}/v1", server.uri()), "local", "q6");
    let client = Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_millis(20))
        .build()
        .unwrap();
    let original = b.bytes();
    let Err(e) = prepare(&*b, b.load_owner().unwrap().unwrap(), &client).await else {
        panic!("must time out")
    };
    assert!(!e.message.contains(SECRET));
    b.unchanged(&original);
}
#[tokio::test]
async fn stale_boot_model_context_endpoint_or_key_during_http_rejects_before_files() {
    for change in 0..5 {
        let server = MockServer::start().await;
        let b = FixtureBackend::new(&format!("{}/v1", server.uri()), "local", "q6");
        let original = b.bytes();
        let mutation = b.clone();
        Mock::given(path("/v1/models")).respond_with(move |_: &wiremock::Request| {
            match change {
                0 => mutation.snap.lock().unwrap().status.as_mut().unwrap().boot_id = "other-boot".into(),
                1 => mutation.snap.lock().unwrap().status.as_mut().unwrap().model = "q8".into(),
                2 => mutation.snap.lock().unwrap().status.as_mut().unwrap().ctx = 8192,
                3 => mutation.snap.lock().unwrap().pod.as_mut().unwrap().api_url = "http://other/v1".into(),
                _ => fs::write(mutation.config_path(), "LOBO_API_KEY=rotated-key\n").unwrap(),
            }
            ResponseTemplate::new(200).set_body_json(serde_json::json!({"data":[{"id":lobo_proto::catalog::get("q6").unwrap().alias}]}))
        }).expect(1).mount(&server).await;
        assert!(prepared(&b).await.is_err());
        b.unchanged(&original);
    }
}
#[tokio::test]
async fn revision_catches_external_atomic_rotate_and_restore_before_commit() {
    let server = MockServer::start().await;
    serve_models(&server, "q6", 1).await;
    let b = FixtureBackend::new(&format!("{}/v1", server.uri()), "local", "q6");
    let original = b.bytes();
    let p = prepared(&b).await.unwrap();
    let raw = fs::read(b.config_path()).unwrap();
    let replacement = b.root.path().join("replacement.env");
    fs::write(&replacement, &raw).unwrap();
    fs::rename(replacement, b.config_path()).unwrap();
    assert!(configure(&*b, &b.destination(), &p, true, &|| Ok(())).is_err());
    assert_eq!(b.bytes(), original);
    assert!(
        !fs::read_dir(b.root.path()).unwrap().any(|e| e
            .unwrap()
            .file_name()
            .to_string_lossy()
            .contains("backup"))
    );
    assert_eq!(
        fs::read_dir(b.root.path().join("opencode-keys"))
            .unwrap()
            .count(),
        0
    );
}
#[tokio::test]
async fn authenticated_noop_revalidates_and_creates_no_key_or_backup_churn() {
    let server = MockServer::start().await;
    serve_models(&server, "q6", 2).await;
    let b = FixtureBackend::new(&format!("{}/v1", server.uri()), "local", "q6");
    let p = prepared(&b).await.unwrap();
    configure(&*b, &b.destination(), &p, true, &|| Ok(())).unwrap();
    let before = b.bytes();
    let entries = fs::read_dir(b.root.path()).unwrap().count();
    let p = prepared(&b).await.unwrap();
    let validations = AtomicUsize::new(0);
    let outcome = configure(&*b, &b.destination(), &p, true, &|| {
        validations.fetch_add(1, Ordering::SeqCst);
        Ok(())
    })
    .unwrap();
    assert!(!outcome.changed);
    assert_eq!(validations.load(Ordering::SeqCst), 1);
    assert_eq!(b.bytes(), before);
    assert_eq!(fs::read_dir(b.root.path()).unwrap().count(), entries);
    assert_eq!(
        fs::read_dir(b.root.path().join("opencode-keys"))
            .unwrap()
            .count(),
        1
    );
    assert!(configure(&*b, &b.destination(), &p, true, &|| Err(error("stale"))).is_err());
    assert_eq!(b.bytes(), before);
}
#[test]
fn file_picker_validation_refuses_missing_symlink_and_wrong_extension() {
    let root = tempfile::tempdir().unwrap();
    let valid = root.path().join("existing.jsonc");
    fs::write(&valid, "{}").unwrap();
    assert!(config_path(&valid, true).is_ok());
    assert!(config_path(&root.path().join("missing.json"), true).is_err());
    assert!(config_path(&root.path().join("new.jsonc"), false).is_ok());
    let wrong = root.path().join("config.env");
    fs::write(&wrong, "{}").unwrap();
    assert!(config_path(&wrong, true).is_err());
    let link = root.path().join("link.json");
    std::os::unix::fs::symlink(valid, &link).unwrap();
    assert!(config_path(&link, true).is_err());
}
#[tokio::test]
async fn restrictions_are_static_preserved_and_checked_before_commit() {
    let server = MockServer::start().await;
    serve_models(&server, "q6", 1).await;
    let b = FixtureBackend::new(&format!("{}/v1", server.uri()), "local", "q6");
    fs::write(b.destination(), "{\n\"disabled_providers\":[\"lobo-local\"],\n\"enabled_providers\":[],\n\"experimental\":{\"policies\":[\"secret policy\"]}\n}").unwrap();
    let p = prepared(&b).await.unwrap();
    let result = configure(&*b, &b.destination(), &p, false, &|| Ok(())).unwrap();
    assert_eq!(result.warnings.len(), 2);
    assert!(
        !serde_json::to_string(&result)
            .unwrap()
            .contains("secret policy")
    );
    let source = String::from_utf8(b.bytes()).unwrap();
    assert!(source.contains("\"disabled_providers\":[\"lobo-local\"]"));
    assert!(source.contains("\"experimental\":{\"policies\":[\"secret policy\"]}"));
}
