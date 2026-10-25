use super::*;
use crate::{config::R2Creds, http};
use sha2::{Digest, Sha256};
use std::time::Duration;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path},
};

const DATA: &[u8] = b"0123456789";

fn sha(b: &[u8]) -> String {
    hex::encode(Sha256::digest(b))
}
fn model(data: &[u8]) -> Model {
    Model {
        sha256: sha(data),
        size: data.len() as i64,
        file: "m.gguf".into(),
        repo: "org/repo".into(),
        ..Default::default()
    }
}
fn store(endpoint: &str) -> Store {
    let r2 = R2Creds {
        endpoint: endpoint.into(),
        access_key: "test-access-key".into(),
        secret_key: "test-private-secret".into(),
        ..Default::default()
    };
    Store::with_bucket(&r2, http::client(Duration::from_secs(3)), "models").unwrap()
}
fn file(data: &[u8]) -> tempfile::NamedTempFile {
    let f = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(f.path(), data).unwrap();
    f
}
fn key(m: &Model, name: &str) -> String {
    format!("/models/models/{}/{name}", m.sha256)
}
async fn mount(server: &MockServer, verb: &str, p: String, resp: ResponseTemplate) {
    Mock::given(method(verb))
        .and(path(p))
        .respond_with(resp)
        .mount(server)
        .await;
}
async fn run(server: &MockServer, m: &Model, data: &[u8]) -> Result<Vec<String>> {
    let f = file(data);
    publish_model(&store(&server.uri()), m, f.path(), 4, &mut |_| {}).await
}
async fn mount_missing(server: &MockServer) {
    Mock::given(method("HEAD"))
        .respond_with(ResponseTemplate::new(404))
        .mount(server)
        .await;
    Mock::given(method("PUT"))
        .respond_with(ResponseTemplate::new(200))
        .mount(server)
        .await;
}
async fn server_all_missing() -> MockServer {
    let server = MockServer::start().await;
    mount_missing(&server).await;
    server
}

#[tokio::test]
async fn publish_rejects_wrong_file_before_any_request() {
    let m = model(DATA);
    for bad in [&b"012345678"[..], &b"012345678X"[..]] {
        let server = server_all_missing().await;
        assert!(run(&server, &m, bad).await.is_err());
        assert!(server.received_requests().await.unwrap().is_empty());
    }
}
#[tokio::test]
async fn publish_uploads_texts_and_chunks_with_headers() {
    let m = model(DATA);
    let server = server_all_missing().await;
    let table = run(&server, &m, DATA).await.unwrap();
    assert_eq!(
        table,
        [&DATA[0..4], &DATA[4..8], &DATA[8..10]].map(sha).to_vec()
    );
    let reqs = server.received_requests().await.unwrap();
    let puts: Vec<_> = reqs.iter().filter(|r| r.method.as_str() == "PUT").collect();
    let paths: Vec<_> = puts.iter().map(|r| r.url.path().to_owned()).collect();
    let want: Vec<_> = ["LICENSE.txt", "MODEL.txt", "000", "001", "002"]
        .iter()
        .map(|n| key(&m, n))
        .collect();
    assert_eq!(paths, want);
    for (r, body) in puts[2..]
        .iter()
        .zip([&DATA[0..4], &DATA[4..8], &DATA[8..10]])
    {
        let h = |n: &str| r.headers.get(n).unwrap().to_str().unwrap().to_owned();
        assert_eq!(h("if-none-match"), "*");
        assert_eq!(h("cache-control"), "public, max-age=31536000, immutable");
        assert_eq!(h("content-type"), "application/octet-stream");
        assert_eq!(r.body, body);
    }
}
#[tokio::test]
async fn publish_skips_existing_chunks_but_still_returns_full_table() {
    let m = model(DATA);
    let server = MockServer::start().await;
    mount(
        &server,
        "HEAD",
        key(&m, "000"),
        ResponseTemplate::new(200).insert_header("content-length", "4"),
    )
    .await;
    mount_missing(&server).await;
    let table = run(&server, &m, DATA).await.unwrap();
    assert_eq!(table.len(), 3);
    let reqs = server.received_requests().await.unwrap();
    assert!(
        !reqs
            .iter()
            .any(|r| r.method.as_str() == "PUT" && r.url.path() == key(&m, "000"))
    );
}
#[tokio::test]
async fn publish_aborts_on_existing_chunk_with_wrong_length() {
    let m = model(DATA);
    let server = MockServer::start().await;
    mount(
        &server,
        "HEAD",
        key(&m, "001"),
        ResponseTemplate::new(200).insert_header("content-length", "3"),
    )
    .await;
    mount_missing(&server).await;
    let err = run(&server, &m, DATA).await.unwrap_err().to_string();
    assert!(err.contains("001"), "{err}");
}
#[tokio::test]
async fn publish_reports_conflict() {
    let m = model(DATA);
    let server = MockServer::start().await;
    mount(&server, "PUT", key(&m, "002"), ResponseTemplate::new(412)).await;
    mount_missing(&server).await;
    let err = run(&server, &m, DATA).await.unwrap_err().to_string();
    assert!(
        err.contains("002") && err.contains("already exists"),
        "{err}"
    );
}
#[test]
fn model_txt_names_repo_sha_and_packaging() {
    let m = model(DATA);
    let t = model_txt(&m);
    for want in [
        "Model: org/repo".to_owned(),
        "Source: https://huggingface.co/org/repo".to_owned(),
        format!("Original GGUF SHA-256: {}", m.sha256),
        "License: Apache-2.0".to_owned(),
        "split into 256 MiB byte chunks; concatenating them in order restores the original GGUF"
            .to_owned(),
    ] {
        assert!(t.contains(&want), "{want}");
    }
}
