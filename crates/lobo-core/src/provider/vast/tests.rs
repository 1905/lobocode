use super::*;
use crate::provider::fixture_opts;
use std::{collections::BTreeMap, sync::Arc};
use wiremock::{
    Mock, MockServer, Request, ResponseTemplate,
    matchers::{method, path},
};

async fn reply(status: u16, body: Value) -> (MockServer, Client) {
    let s = MockServer::start().await;
    Mock::given(|_: &Request| true)
        .respond_with(ResponseTemplate::new(status).set_body_json(body))
        .mount(&s)
        .await;
    let c = Client::with_base("key", &s.uri());
    (s, c)
}

#[tokio::test]
async fn bad_key_names_env() {
    for status in [401, 403] {
        let (_s, c) = reply(status, json!({})).await;
        let e = c.search_offers(1.2, 100).await.unwrap_err().to_string();
        assert!(e.contains("VASTAI_API_KEY"));
        assert!(e.contains(&status.to_string()));
    }
}

#[test]
fn search_query_shape() {
    let q = search_query(1.2, 100);
    assert_eq!(q["order"], json!([["dph_total", "asc"]]));
    assert_eq!(q["verified"]["eq"], true);
    assert_eq!(q["inet_down"]["gte"], 800);
    assert_eq!(q["limit"], 10);
    assert_eq!(q["dph_total"]["lte"], 1.2);
    assert_eq!(q["gpu_name"]["in"], json!(["RTX 5090"]));
    assert_eq!(q["reliability2"]["gte"], 0.98);
    assert_eq!(q["num_gpus"]["eq"], 1);
}

#[tokio::test]
async fn client_sends_user_agent() {
    let (_s, c) = reply(200, json!({"offers":[]})).await;
    c.search_offers(1.2, 100).await.unwrap();
    let reqs = _s.received_requests().await.unwrap();
    let r = &reqs[0];
    assert_eq!(r.method, "POST");
    assert_eq!(r.url.path(), "/bundles");
    assert_eq!(r.headers["user-agent"], http::USER_AGENT);
    assert_eq!(r.headers["Authorization"], "Bearer key");
    assert_eq!(r.headers["Content-Type"], "application/json");
}

#[tokio::test]
async fn create_no_credit() {
    let (_s, c) = reply(400, json!({"error":"insufficient_credit"})).await;
    assert!(matches!(
        c.create(7, &json!({})).await,
        Err(Error::NoCredit)
    ));
}

#[tokio::test]
async fn create_4xx_is_rejected() {
    for status in [400, 401, 403, 404, 409, 429] {
        let (_s, c) = reply(status, json!({"error":"invalid_args"})).await;
        assert!(
            matches!(c.create(7, &json!({})).await, Err(Error::Rejected(_))),
            "{status}"
        );
    }
    for body in [
        json!({"success":false,"msg":"taken"}),
        json!({"success":true}),
    ] {
        let (_s, c) = reply(200, body).await;
        assert!(matches!(
            c.create(7, &json!({})).await,
            Err(Error::Rejected(_))
        ));
    }
}

#[tokio::test]
async fn create_5xx_is_uncertain() {
    for status in [500, 502, 503] {
        let (_s, c) = reply(status, json!({})).await;
        let e = c.create(7, &json!({})).await.unwrap_err();
        assert!(!matches!(e, Error::Rejected(_) | Error::NoCredit));
    }
    let s = MockServer::start().await;
    Mock::given(method("PUT"))
        .respond_with(ResponseTemplate::new(200).set_body_string("{cut"))
        .mount(&s)
        .await;
    let e = Client::with_base("key", &s.uri())
        .create(7, &json!({}))
        .await
        .unwrap_err();
    assert!(!matches!(e, Error::Rejected(_) | Error::NoCredit));
}

#[tokio::test]
async fn get_null_instances_is_not_found() {
    let (_s, c) = reply(200, json!({"instances":null})).await;
    assert!(c.get(7).await.unwrap_err().is_not_found());
}

#[tokio::test]
async fn malformed_response_returns_error_instead_of_panicking() {
    let (_s, c) = reply(200, json!("bad gateway payload")).await;
    assert!(c.list().await.is_err());
    assert!(c.get(7).await.is_err());
    assert!(c.search_offers(1.2, 100).await.is_err());
}

#[tokio::test]
async fn destroy_404_is_gone() {
    let (_s, c) = reply(404, json!({})).await;
    c.destroy(7).await.unwrap();
}

fn opts() -> CreateOpts {
    CreateOpts {
        image: "ghcr.io/ggml-org/llama.cpp:server-cuda-b11118".into(),
        release_url: "r".into(),
        release_sha256: "s".into(),
        model_url: "m".into(),
        ctx: 65536,
        ..fixture_opts()
    }
}

#[test]
fn create_body_runtype_ssh() {
    let b = create_body(&opts());
    assert_eq!(b["runtype"], "ssh");
    assert_eq!(b["disk"], 80);
    assert_eq!(b["label"], "lobo");
    let on = b["onstart"].as_str().unwrap();
    assert!(on.starts_with("#!/bin/bash\n"));
    assert!(on.contains("CONTAINER_API_KEY"));
    assert!(!on.contains("RUNPOD"));
    assert!(!b.to_string().contains("R2_"));
    assert!(!b.to_string().contains("VASTAI_API_KEY"));
}

#[test]
fn create_body_baked() {
    let o = CreateOpts {
        image: "ghcr.io/1905/lobocode@sha256:abc".into(),
        release_url: String::new(),
        release_sha256: String::new(),
        ..opts()
    };
    let b = create_body(&o);
    assert_eq!(b["image"], o.image);
    assert!(
        !b["env"]
            .as_object()
            .unwrap()
            .contains_key("LOBO_RELEASE_URL")
    );
}

#[test]
fn create_body_matches_go() {
    let mut o = opts();
    for fixture in [
        include_str!("../../../fixtures/vast/create_body.json"),
        include_str!("../../../fixtures/vast/create_body_baked.json"),
    ] {
        assert_eq!(
            create_body(&o),
            serde_json::from_str::<Value>(fixture).unwrap()
        );
        o.release_url.clear();
        o.release_sha256.clear();
    }
}

#[derive(Default)]
struct FakeVast {
    offers: Vec<Offer>,
    taken: HashSet<i64>,
    lost_reply: HashSet<i64>,
    fail_5xx: HashSet<i64>,
    no_credit: bool,
    puts: Vec<i64>,
    put_body: Value,
    instances: BTreeMap<i64, Inst>,
    deleted: Vec<i64>,
    next_id: i64,
    cancel_on_put: Option<CancellationToken>,
    hide_until_list: usize,
    list_count: usize,
}
impl FakeVast {
    fn respond(&mut self, r: &Request) -> ResponseTemplate {
        if r.headers["Authorization"] != "Bearer key" {
            return ResponseTemplate::new(401);
        }
        let path = r.url.path();
        match (r.method.as_str(), path) {
            ("POST", "/bundles") => {
                ResponseTemplate::new(200).set_body_json(json!({"offers":self.offers}))
            }
            ("PUT", p) if p.starts_with("/asks/") => {
                let id = p
                    .trim_start_matches("/asks/")
                    .trim_end_matches('/')
                    .parse()
                    .unwrap();
                self.puts.push(id);
                self.put_body = r.body_json().unwrap();
                if let Some(c) = &self.cancel_on_put {
                    c.cancel();
                }
                if self.no_credit {
                    return ResponseTemplate::new(400)
                        .set_body_json(json!({"error":"insufficient_credit"}));
                }
                if self.taken.contains(&id) {
                    return ResponseTemplate::new(400).set_body_json(
                        json!({"error":"invalid_args","msg":"offer no longer available"}),
                    );
                }
                if self.fail_5xx.contains(&id) {
                    return ResponseTemplate::new(502);
                }
                self.next_id += 1;
                self.instances.insert(
                    self.next_id,
                    Inst {
                        id: self.next_id,
                        label: "lobo".into(),
                        status: "loading".into(),
                        dph: 0.73,
                        inet_down: 20313.0,
                        ..Default::default()
                    },
                );
                if self.lost_reply.contains(&id) {
                    return ResponseTemplate::new(502);
                }
                ResponseTemplate::new(200).set_body_json(
                    json!({"success":true,"new_contract":self.next_id,"instance_api_key":"x"}),
                )
            }
            ("GET", "/instances/") => {
                self.list_count += 1;
                let mut instances: Vec<_> = if self.list_count >= self.hide_until_list {
                    self.instances.values().cloned().collect()
                } else {
                    vec![]
                };
                instances.push(Inst {
                    id: 999,
                    label: "someone-else".into(),
                    ..Default::default()
                });
                ResponseTemplate::new(200).set_body_json(json!({"instances":instances}))
            }
            ("GET", p) if p.starts_with("/instances/") => {
                let id = p
                    .trim_start_matches("/instances/")
                    .trim_end_matches('/')
                    .parse::<i64>()
                    .unwrap();
                ResponseTemplate::new(200)
                    .set_body_json(json!({"instances":self.instances.get(&id)}))
            }
            ("DELETE", p) => {
                let id = p
                    .trim_start_matches("/instances/")
                    .trim_end_matches('/')
                    .parse::<i64>()
                    .unwrap();
                if self.instances.remove(&id).is_none() {
                    return ResponseTemplate::new(404);
                }
                self.deleted.push(id);
                ResponseTemplate::new(200).set_body_json(json!({"success":true}))
            }
            _ => ResponseTemplate::new(404),
        }
    }
}
async fn setup(mut fake: FakeVast) -> (MockServer, Arc<Mutex<FakeVast>>, VastProvider) {
    fake.next_id = 52607649;
    let state = Arc::new(Mutex::new(fake));
    let copy = state.clone();
    let s = MockServer::start().await;
    Mock::given(|_: &Request| true)
        .respond_with(move |r: &Request| copy.lock().unwrap().respond(r))
        .mount(&s)
        .await;
    let mut p = VastProvider::new(Client::with_base("key", &s.uri()), 1.2, "lobo.test");
    p.adopt_wait = Duration::from_millis(1);
    (s, state, p)
}
fn offers() -> Vec<Offer> {
    vec![
        Offer {
            id: 1,
            inet_down: 20313.0,
            dph: 0.73,
            geo: "California, US".into(),
            ..Default::default()
        },
        Offer {
            id: 2,
            inet_down: 17805.0,
            dph: 0.73,
            ..Default::default()
        },
        Offer {
            id: 3,
            inet_down: 9129.0,
            dph: 0.81,
            ..Default::default()
        },
    ]
}

#[tokio::test]
async fn rent_skips_taken_and_tried() {
    let (_s, f, p) = setup(FakeVast {
        offers: offers(),
        taken: [1].into(),
        ..Default::default()
    })
    .await;
    let notes = Mutex::new(Vec::new());
    let i = p
        .rent(&opts(), CancellationToken::new(), &|s| {
            notes.lock().unwrap().push(s)
        })
        .await
        .unwrap();
    assert_eq!(i.provider, "vast");
    assert_eq!(i.host_download_mbps, 17805);
    assert_eq!(notes.lock().unwrap().len(), 1);
    let i = p
        .rent(&opts(), CancellationToken::new(), &|_| {})
        .await
        .unwrap();
    assert!(i.detail.contains("offer 3"));
    assert!(
        p.rent(&opts(), CancellationToken::new(), &|_| {})
            .await
            .unwrap_err()
            .is_no_capacity()
    );
    assert_eq!(f.lock().unwrap().puts, [1, 2, 3]);
}

#[tokio::test]
async fn create_body_via_rent() {
    let (_s, f, p) = setup(FakeVast {
        offers: offers(),
        ..Default::default()
    })
    .await;
    p.rent(&opts(), CancellationToken::new(), &|_| {})
        .await
        .unwrap();
    assert_eq!(f.lock().unwrap().put_body, create_body(&opts()));
}

#[tokio::test]
async fn rent_no_credit_stops_at_once() {
    let (_s, f, p) = setup(FakeVast {
        offers: offers(),
        no_credit: true,
        ..Default::default()
    })
    .await;
    assert!(matches!(
        p.rent(&opts(), CancellationToken::new(), &|_| {}).await,
        Err(Error::NoCredit)
    ));
    assert_eq!(f.lock().unwrap().puts, [1]);
}

#[tokio::test]
async fn uncertain_create_never_rents_twice() {
    for lost in [true, false] {
        let (_s, f, p) = setup(FakeVast {
            offers: offers(),
            lost_reply: if lost { [1].into() } else { HashSet::new() },
            fail_5xx: if lost { HashSet::new() } else { [1].into() },
            ..Default::default()
        })
        .await;
        let result = p.rent(&opts(), CancellationToken::new(), &|_| {}).await;
        if lost {
            let i = result.unwrap();
            assert_eq!(i.id, "52607650");
            assert!(i.detail.contains("offer 1"));
        } else {
            let e = result.unwrap_err();
            assert!(!e.is_no_capacity());
            assert!(e.to_string().contains("not retrying another offer"));
        }
        assert_eq!(f.lock().unwrap().puts, [1]);
    }
}

#[tokio::test]
async fn list_get_delete() {
    let (_s, _f, p) = setup(FakeVast {
        offers: offers(),
        ..Default::default()
    })
    .await;
    let i = p
        .rent(&opts(), CancellationToken::new(), &|_| {})
        .await
        .unwrap();
    let list = p.list().await.unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].id, i.id);
    assert_eq!(p.get(&i.id).await.unwrap().api_url, "https://lobo.test/v1");
    p.delete(&i.id).await.unwrap();
    assert!(p.get(&i.id).await.unwrap_err().is_not_found());
    p.delete(&i.id).await.unwrap();
    assert!(matches!(p.get("bad").await, Err(Error::Other(_))));
    assert!(matches!(p.delete("bad").await, Err(Error::Other(_))));
}

#[tokio::test]
async fn cancel_after_put_still_adopts_and_does_not_rent_again() {
    let cancel = CancellationToken::new();
    let (_s, f, p) = setup(FakeVast {
        offers: offers(),
        lost_reply: [1].into(),
        cancel_on_put: Some(cancel.clone()),
        hide_until_list: 4,
        ..Default::default()
    })
    .await;
    let i = p.rent(&opts(), cancel.clone(), &|_| {}).await.unwrap();
    assert!(cancel.is_cancelled());
    assert_eq!(i.id, "52607650");
    assert_eq!(f.lock().unwrap().puts, [1]);
    assert_eq!(f.lock().unwrap().list_count, 4);
}

#[tokio::test]
async fn cancel_between_offers_prevents_next_put() {
    let cancel = CancellationToken::new();
    let (_s, f, p) = setup(FakeVast {
        offers: offers(),
        taken: [1].into(),
        cancel_on_put: Some(cancel.clone()),
        ..Default::default()
    })
    .await;
    assert!(matches!(
        p.rent(&opts(), cancel, &|_| {}).await,
        Err(Error::Cancelled)
    ));
    assert_eq!(f.lock().unwrap().puts, [1]);
}

#[tokio::test]
async fn cancel_during_search_does_not_create() {
    let s = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/bundles"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_delay(Duration::from_secs(30))
                .set_body_json(json!({"offers":offers()})),
        )
        .mount(&s)
        .await;
    let p = VastProvider::new(Client::with_base("key", &s.uri()), 1.2, "test");
    let cancel = CancellationToken::new();
    let c = cancel.clone();
    let o = opts();
    let (result, ()) = tokio::join!(p.rent(&o, cancel, &|_| {}), async move {
        tokio::time::sleep(Duration::from_millis(10)).await;
        c.cancel()
    });
    assert!(matches!(result, Err(Error::Cancelled)));
    assert!(
        s.received_requests()
            .await
            .unwrap()
            .iter()
            .all(|r| r.method != "PUT")
    );
}
