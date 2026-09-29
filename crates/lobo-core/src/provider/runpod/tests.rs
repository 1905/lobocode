use super::*;
use crate::provider::fixture_opts;
use chrono::Timelike;
use std::sync::Mutex;
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

const FIXTURE: &str = include_str!("../../../fixtures/runpod/pod.json");

#[test]
fn pod_fixture_decodes() {
    let p: Pod = serde_json::from_str(FIXTURE).unwrap();
    assert_eq!(p.cost_per_hr, 0.69);
    assert_eq!(p.desired_status, "RUNNING");
    assert_eq!(p.last_started_at.0.unwrap().hour(), 8);
    assert_eq!(p.port_mappings["22"], 18180);
    assert_eq!(p.machine.unwrap().max_download_speed_mbps, 0);
    let p: Pod = serde_json::from_value(json!({"portMappings":null})).unwrap();
    assert!(p.port_mappings.is_empty());
}

#[test]
fn runpod_time_round_trip() {
    for src in [
        "2026-09-23 08:48:28.204 +0000 UTC",
        "2026-09-23 10:48:28.123456789 +0200 CEST",
        "2026-09-23T08:48:28Z",
    ] {
        let t: RunPodTime = serde_json::from_value(json!(src)).unwrap();
        let encoded = serde_json::to_string(&t).unwrap();
        assert_eq!(serde_json::from_str::<RunPodTime>(&encoded).unwrap(), t);
    }
    for v in [
        json!(""),
        Value::Null,
        json!(3),
        json!({}),
        json!("0001-01-01T00:00:00Z"),
    ] {
        let t: RunPodTime = serde_json::from_value(v).unwrap();
        assert_eq!(t, RunPodTime::default());
        assert_eq!(
            serde_json::to_value(t).unwrap(),
            json!("0001-01-01T00:00:00Z")
        );
    }
    assert!(
        serde_json::from_value::<RunPodTime>(json!("bad"))
            .unwrap_err()
            .to_string()
            .contains("runpod: time")
    );
}

#[test]
fn create_payload() {
    let p = build_create_payload(&fixture_opts(), "COMMUNITY", 0.0);
    assert_eq!(p["volumeInGb"], 0);
    assert_eq!(p["ports"], json!([]));
    assert_eq!(p["cloudType"], "COMMUNITY");
    assert_eq!(p["name"], "lobo");
    assert_eq!(p["gpuTypeIds"], json!([GPU_TYPE]));
    assert_eq!(p["env"].as_object().unwrap().len(), 11);
    assert_eq!(p["env"]["LOBO_EXPIRES_AT"], "2026-09-23T22:00:00Z");
    assert!(!p["env"].as_object().unwrap().contains_key("RUNPOD_API_KEY"));
    assert!(!p.to_string().contains("R2_"));
    for s in [
        "sha256sum -c",
        "exec /lobo/lobo-agent",
        "trap die ERR",
        "podTerminate",
        "$LOBO_RELEASE_URL",
    ] {
        assert!(p["dockerStartCmd"][0].as_str().unwrap().contains(s));
    }
    assert_eq!(
        build_create_payload(&fixture_opts(), "", 0.0)["cloudType"],
        "SECURE"
    );
}

#[test]
fn payload_min_download() {
    assert!(
        build_create_payload(&fixture_opts(), "SECURE", 0.0)
            .get("minDownloadMbps")
            .is_none()
    );
    assert_eq!(
        build_create_payload(&fixture_opts(), "SECURE", 5000.0)["minDownloadMbps"],
        5000.0
    );
}

#[test]
fn payload_matches_go() {
    let mut o = fixture_opts();
    let expected: Value = serde_json::from_str(include_str!(
        "../../../fixtures/runpod/payload_community.json"
    ))
    .unwrap();
    assert_eq!(build_create_payload(&o, "COMMUNITY", 0.0), expected);
    o.ssh_pub_key = "ssh-ed25519 AAAATEST".into();
    let mut expected: Value = serde_json::from_str(include_str!(
        "../../../fixtures/runpod/payload_secure_5000_ssh.json"
    ))
    .unwrap();
    // JSON has one number type; Go emits integral floats without a decimal.
    expected["minDownloadMbps"] = json!(expected["minDownloadMbps"].as_f64().unwrap());
    assert_eq!(build_create_payload(&o, "SECURE", 5000.0), expected);
}

async fn server() -> MockServer {
    let s = MockServer::start().await;
    Mock::given(|_: &Request| true)
        .respond_with(|r: &Request| {
            if r.headers.get("Authorization").unwrap() != "Bearer key" {
                return ResponseTemplate::new(401);
            }
            if r.url.path() == "/pods/gone" {
                return ResponseTemplate::new(404);
            }
            match r.method.as_str() {
                "POST" => {
                    if r.body_json::<Value>().unwrap()["cloudType"] == "SECURE" {
                        ResponseTemplate::new(200).set_body_string(FIXTURE)
                    } else {
                        ResponseTemplate::new(500).set_body_string(
                            "create pod: There are no instances currently available",
                        )
                    }
                }
                "GET" if r.url.path() == "/pods" => {
                    ResponseTemplate::new(200).set_body_string(format!("[{FIXTURE}]"))
                }
                "GET" => ResponseTemplate::new(200).set_body_string(FIXTURE),
                _ => ResponseTemplate::new(200),
            }
        })
        .mount(&s)
        .await;
    s
}

#[tokio::test]
async fn client_rest_calls() {
    let s = server().await;
    let c = Client::with_base("key", &s.uri());
    assert!(matches!(
        c.create(&fixture_opts(), "COMMUNITY", 0.0).await,
        Err(Error::NoCapacity(_))
    ));
    assert!(
        !c.create(&fixture_opts(), "SECURE", 0.0)
            .await
            .unwrap()
            .id
            .is_empty()
    );
    assert_eq!(c.list().await.unwrap().len(), 1);
    assert!(!c.get("x").await.unwrap().id.is_empty());
    assert!(matches!(c.get("gone").await, Err(Error::NotFound)));
    c.delete("gone").await.unwrap();
    c.delete("x").await.unwrap();
    let e = Client::with_base("wrong", &s.uri())
        .list()
        .await
        .unwrap_err()
        .to_string();
    assert_eq!(e, "runpod GET /pods: HTTP 401: ");
}

#[tokio::test]
async fn client_sends_user_agent() {
    let s = server().await;
    let c = Client::with_base("key", &s.uri());
    c.create(&fixture_opts(), "SECURE", 0.0).await.unwrap();
    c.list().await.unwrap();
    c.get("x").await.unwrap();
    c.delete("x").await.unwrap();
    for r in s.received_requests().await.unwrap() {
        assert_eq!(r.headers["user-agent"], http::USER_AGENT);
        assert_eq!(r.headers["content-type"], "application/json");
    }
}

#[test]
fn is_no_capacity_table() {
    for text in [
        "No instances",
        "no longer any instances",
        "no instances available",
        "not enough memory",
        "NO GPU",
    ] {
        assert!(is_no_capacity(text));
    }
    for text in ["unauthorized", "bad key", "internal error"] {
        assert!(!is_no_capacity(text));
    }
}

#[derive(Default)]
struct Fake {
    calls: Mutex<Vec<(String, f64)>>,
    succeed_at: usize,
    pods: Vec<Pod>,
    cancel: Option<CancellationToken>,
}
#[async_trait]
impl RunPodApi for Fake {
    async fn create(&self, _: &CreateOpts, cloud: &str, tier: f64) -> Result<Pod> {
        let mut calls = self.calls.lock().unwrap();
        calls.push((cloud.into(), tier));
        if let Some(c) = &self.cancel {
            c.cancel();
        }
        if calls.len() == self.succeed_at {
            Ok(Pod {
                id: "p1".into(),
                ..Default::default()
            })
        } else {
            Err(Error::NoCapacity("sold out".into()))
        }
    }
    async fn list(&self) -> Result<Vec<Pod>> {
        Ok(self.pods.clone())
    }
    async fn get(&self, _: &str) -> Result<Pod> {
        Ok(self.pods[0].clone())
    }
    async fn delete(&self, _: &str) -> Result<()> {
        Ok(())
    }
}
fn provider(api: Arc<Fake>) -> RunPodProvider {
    RunPodProvider {
        api,
        domain: "lobo.test".into(),
    }
}

#[tokio::test]
async fn rent_walks_tiers_and_notes() {
    let api = Arc::new(Fake {
        succeed_at: 7,
        ..Default::default()
    });
    let p = provider(api.clone());
    let notes = Mutex::new(Vec::new());
    let mut o = fixture_opts();
    o.cloud = "secure".into();
    let i = p
        .rent(&o, CancellationToken::new(), &|s| {
            notes.lock().unwrap().push(s)
        })
        .await
        .unwrap();
    assert_eq!(
        *api.calls.lock().unwrap(),
        vec![
            ("SECURE".into(), 10000.0),
            ("SECURE".into(), 5000.0),
            ("SECURE".into(), 2500.0),
            ("SECURE".into(), 1000.0),
            ("SECURE".into(), 0.0),
            ("COMMUNITY".into(), 10000.0),
            ("COMMUNITY".into(), 5000.0)
        ]
    );
    assert_eq!(
        *notes.lock().unwrap(),
        ["no 5090 in SECURE at any network speed"]
    );
    assert_eq!(i.detail, "COMMUNITY, host ≥5000 Mbps");
    assert_eq!(i.api_url, "https://lobo.test/v1");
    let api = Arc::new(Fake::default());
    let p = provider(api.clone());
    assert!(
        p.rent(&fixture_opts(), CancellationToken::new(), &|_| {})
            .await
            .unwrap_err()
            .is_no_capacity()
    );
    assert_eq!(api.calls.lock().unwrap().len(), 5);
    assert!(
        api.calls
            .lock()
            .unwrap()
            .iter()
            .all(|(c, _)| c == "COMMUNITY")
    );
}

#[tokio::test]
async fn get_terminated_is_not_found() {
    let p = provider(Arc::new(Fake {
        pods: vec![Pod {
            desired_status: "terminated".into(),
            ..Default::default()
        }],
        ..Default::default()
    }));
    assert!(p.get("x").await.unwrap_err().is_not_found());
}

#[tokio::test]
async fn list_filters_by_name() {
    let p = provider(Arc::new(Fake {
        pods: vec![
            Pod {
                name: "lobo".into(),
                id: "ours".into(),
                ..Default::default()
            },
            Pod {
                name: "other".into(),
                ..Default::default()
            },
        ],
        ..Default::default()
    }));
    let list = p.list().await.unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].id, "ours");
}

#[test]
fn started_at_falls_back_to_created() {
    let t = "2026-09-23T08:00:00Z".parse().unwrap();
    let i = to_instance(Pod {
        created_at: RunPodTime(Some(t)),
        ..Default::default()
    });
    assert_eq!(i.started_at, GoTime::from_utc(t));
}

#[tokio::test]
async fn cancellation_prevents_next_create_and_preserves_success() {
    for succeed_at in [0, 1] {
        let cancel = CancellationToken::new();
        let api = Arc::new(Fake {
            cancel: Some(cancel.clone()),
            succeed_at,
            ..Default::default()
        });
        let p = provider(api.clone());
        let result = p.rent(&fixture_opts(), cancel, &|_| {}).await;
        if succeed_at == 1 {
            assert_eq!(result.unwrap().id, "p1");
        } else {
            assert!(matches!(result, Err(Error::Cancelled)));
        }
        assert_eq!(api.calls.lock().unwrap().len(), 1);
    }
    let cancel = CancellationToken::new();
    cancel.cancel();
    let api = Arc::new(Fake::default());
    assert!(matches!(
        provider(api.clone())
            .rent(&fixture_opts(), cancel, &|_| {})
            .await,
        Err(Error::Cancelled)
    ));
    assert!(api.calls.lock().unwrap().is_empty());
}

#[tokio::test]
async fn create_rejection_is_distinct_from_ambiguous_response() {
    for (status, body, kind) in [
        (401, "unauthorized", "rejected"),
        (500, "upstream failed", "provider_api"),
        (200, "broken json", "json"),
    ] {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::any())
            .respond_with(wiremock::ResponseTemplate::new(status).set_body_string(body))
            .mount(&server)
            .await;
        let client = Client::with_base("k", &server.uri());
        assert_eq!(
            client
                .create(&fixture_opts(), "COMMUNITY", 0.0)
                .await
                .unwrap_err()
                .kind(),
            kind
        );
    }
}
