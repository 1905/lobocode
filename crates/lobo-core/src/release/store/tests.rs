use super::*;
use crate::release::{Manifest, zip_key};
use std::collections::BTreeMap;
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

fn creds(endpoint: &str) -> R2Creds {
    R2Creds {
        endpoint: endpoint.into(),
        access_key: "test-access-key".into(),
        secret_key: "test-private-secret".into(),
        ..Default::default()
    }
}
fn store(endpoint: &str) -> Store {
    Store::with_endpoint_for_test(&creds(endpoint), http::client(Duration::from_secs(3))).unwrap()
}
fn release() -> Resolved {
    Resolved {
        manifest: Manifest {
            version: "2026.09.29-1".into(),
            ..Default::default()
        },
        zip_key: zip_key("2026.09.29-1"),
        zip_sha256: "abcd".into(),
    }
}
fn archive() -> tempfile::NamedTempFile {
    let f = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(f.path(), "test archive").unwrap();
    f
}

#[test]
fn presign_get_shape() {
    let s = Store::new(&creds("https://account.r2.cloudflarestorage.com")).unwrap();
    let raw = s.presign_get("models/x.gguf", Duration::from_secs(43200));
    let u: url::Url = raw.parse().unwrap();
    assert_eq!(u.host_str(), Some("account.r2.cloudflarestorage.com"));
    assert_eq!(u.path(), "/lobo/models/x.gguf");
    let q: BTreeMap<_, _> = u.query_pairs().collect();
    assert_eq!(q["X-Amz-Algorithm"], "AWS4-HMAC-SHA256");
    assert_eq!(q["X-Amz-Expires"], "43200");
    let credential = &q["X-Amz-Credential"];
    assert!(credential.starts_with("test-access-key/"));
    assert!(credential.ends_with("/auto/s3/aws4_request"));
    assert_eq!(q["X-Amz-SignedHeaders"], "host");
    assert_eq!(q["X-Amz-Signature"].len(), 64);
    assert!(q["X-Amz-Signature"].bytes().all(|b| b.is_ascii_hexdigit()));
    assert!(!raw.contains("test-private-secret"));
    assert!(Store::new(&creds("http://account.r2.cloudflarestorage.com")).is_err());
}
#[test]
fn presign_differs_per_key() {
    let s = store("https://r2.example.com");
    let a: url::Url = s.presign_get("a", Duration::from_secs(60)).parse().unwrap();
    let b: url::Url = s.presign_get("b", Duration::from_secs(60)).parse().unwrap();
    let sig = |u: &url::Url| {
        u.query_pairs()
            .find(|(k, _)| k == "X-Amz-Signature")
            .unwrap()
            .1
            .into_owned()
    };
    assert_ne!(sig(&a), sig(&b));
}

async fn publish_server(
    existing: bool,
    conflict: bool,
    failed_upload: Option<&'static str>,
) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(|_: &Request| true)
        .respond_with(move |r: &Request| {
            if r.method == "HEAD" {
                return ResponseTemplate::new(if existing { 200 } else { 404 });
            }
            if conflict && r.url.path().ends_with(".zip") {
                return ResponseTemplate::new(412);
            }
            if failed_upload.is_some_and(|suffix| r.url.path().ends_with(suffix)) {
                return ResponseTemplate::new(503);
            }
            ResponseTemplate::new(200)
        })
        .mount(&server)
        .await;
    server
}

#[tokio::test]
async fn publish_order_zip_meta_latest() {
    let server = publish_server(false, false, None).await;
    let s = store(&server.uri());
    let zip = archive();
    let r = release();
    s.publish(zip.path(), &r).await.unwrap();
    let reqs = server.received_requests().await.unwrap();
    let calls: Vec<_> = reqs
        .iter()
        .map(|r| format!("{} {}", r.method, r.url.path()))
        .collect();
    assert_eq!(
        calls,
        [
            "HEAD /lobo/releases/lobo-2026.09.29-1.zip",
            "HEAD /lobo/releases/lobo-2026.09.29-1.json",
            "PUT /lobo/releases/lobo-2026.09.29-1.zip",
            "PUT /lobo/releases/lobo-2026.09.29-1.json",
            "PUT /lobo/releases/latest.json"
        ]
    );
    for request in &reqs {
        let q: BTreeMap<_, _> = request.url.query_pairs().collect();
        assert!(q.contains_key("X-Amz-Signature"));
        assert_eq!(request.headers["user-agent"], http::USER_AGENT);
        if request.method == "PUT" && !request.url.path().ends_with("latest.json") {
            assert_eq!(request.headers["if-none-match"], "*");
            assert!(q["X-Amz-SignedHeaders"].contains("if-none-match"));
        }
    }
    assert_eq!(reqs[2].headers["content-type"], "application/zip");
    assert_eq!(reqs[2].body, b"test archive");
    for request in &reqs[3..] {
        assert_eq!(request.headers["cache-control"], "no-cache");
        assert_eq!(request.headers["content-type"], "application/json");
        assert_eq!(request.body_json::<Resolved>().unwrap(), r);
    }
    assert!(!reqs[4].headers.contains_key("if-none-match"));
}

#[tokio::test]
async fn publish_refuses_existing_release() {
    let server = publish_server(true, false, None).await;
    let e = store(&server.uri())
        .publish(archive().path(), &release())
        .await
        .unwrap_err();
    assert!(e.to_string().contains("already exists"));
    assert!(
        server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .all(|r| r.method == "HEAD")
    );
}

#[tokio::test]
async fn publish_version_never_moves_latest() {
    let server = MockServer::start().await;
    Mock::given(|_: &Request| true)
        .respond_with(|r: &Request| {
            assert!(!r.url.path().ends_with("latest.json"));
            ResponseTemplate::new(if r.method == "HEAD" { 404 } else { 200 })
        })
        .mount(&server)
        .await;
    store(&server.uri())
        .publish_version(archive().path(), &release())
        .await
        .unwrap();
    assert_eq!(server.received_requests().await.unwrap().len(), 4);
}

#[tokio::test]
async fn conditional_conflict_never_moves_latest() {
    let server = publish_server(false, true, None).await;
    let e = store(&server.uri())
        .publish(archive().path(), &release())
        .await
        .unwrap_err();
    assert!(e.to_string().contains("already exists"));
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 3);
    assert!(
        requests
            .iter()
            .all(|r| !r.url.path().ends_with("latest.json"))
    );
}

#[tokio::test]
async fn failed_upload_never_moves_latest() {
    for suffix in [".zip", "lobo-2026.09.29-1.json"] {
        let server = publish_server(false, false, Some(suffix)).await;
        assert!(
            store(&server.uri())
                .publish(archive().path(), &release())
                .await
                .is_err()
        );
        assert!(
            server
                .received_requests()
                .await
                .unwrap()
                .iter()
                .all(|r| !r.url.path().ends_with("latest.json"))
        );
    }
}

#[tokio::test]
async fn list_follows_continuation() {
    let server = MockServer::start().await;
    Mock::given(|_: &Request| true).respond_with(|r: &Request| {
        let q: BTreeMap<_,_> = r.url.query_pairs().collect(); assert_eq!(q["prefix"],"releases/"); assert_eq!(q["list-type"],"2");
        assert!(q.contains_key("X-Amz-Signature"));
        let (key,tail) = if q.get("continuation-token").is_some_and(|v|v == "page 2+") {
            ("releases%2Fb.zip","")
        } else {("releases%2Fa.zip","<NextContinuationToken>page 2+</NextContinuationToken>")};
        ResponseTemplate::new(200).set_body_string(format!(r#"<ListBucketResult xmlns="http://s3.amazonaws.com/doc/2006-03-01/"><Contents><Key>{key}</Key><ETag>abc</ETag><LastModified>2026-09-29T00:00:00Z</LastModified><Size>1</Size></Contents>{tail}<EncodingType>url</EncodingType></ListBucketResult>"#))
    }).mount(&server).await;
    assert_eq!(
        store(&server.uri()).list_release_keys().await.unwrap(),
        ["releases/a.zip", "releases/b.zip"]
    );
    assert_eq!(server.received_requests().await.unwrap().len(), 2);
}
