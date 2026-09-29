use super::*;
use flate2::{Compression, write::GzEncoder};
use sha2::{Digest, Sha256};
use std::sync::Mutex;
use wiremock::{Mock, MockServer, ResponseTemplate, matchers::any};

#[derive(Clone)]
struct Entry<'a> {
    name: &'a str,
    body: &'a [u8],
    link: &'a str,
    kind: u8,
    mode: u32,
}
fn file<'a>(name: &'a str, body: &'a [u8], mode: u32) -> Entry<'a> {
    Entry {
        name,
        body,
        link: "",
        kind: b'0',
        mode,
    }
}
fn link<'a>(name: &'a str, target: &'a str) -> Entry<'a> {
    Entry {
        name,
        body: b"",
        link: target,
        kind: b'2',
        mode: 0o777,
    }
}
fn good_tree() -> Vec<Entry<'static>> {
    vec![
        file("build/bin/llama-server", b"#!/bin/sh\n", 0o755),
        file("build/bin/libllama.0.dylib", b"lib", 0o644),
        link("build/bin/libllama.dylib", "libllama.0.dylib"),
    ]
}
fn tgz(entries: &[Entry<'_>]) -> Vec<u8> {
    let mut tar = tar::Builder::new(GzEncoder::new(Vec::new(), Compression::fast()));
    for e in entries {
        let mut h = tar::Header::new_gnu();
        h.set_entry_type(tar::EntryType::new(e.kind));
        h.set_mode(e.mode);
        h.set_size(e.body.len() as u64);
        // Deliberately bypass set_path's validation to make malicious fixtures.
        h.as_mut_bytes()[..100].fill(0);
        h.as_mut_bytes()[..e.name.len()].copy_from_slice(e.name.as_bytes());
        h.as_mut_bytes()[157..257].fill(0);
        h.as_mut_bytes()[157..157 + e.link.len()].copy_from_slice(e.link.as_bytes());
        h.set_cksum();
        tar.append(&h, e.body).unwrap();
    }
    tar.into_inner().unwrap().finish().unwrap()
}
async fn serve(body: Vec<u8>) -> (MockServer, RuntimePin) {
    let s = MockServer::start().await;
    let pin = RuntimePin {
        url: s.uri() + "/llama.tar.gz",
        size: body.len() as i64,
        sha256: hex::encode(Sha256::digest(&body)),
    };
    Mock::given(any())
        .respond_with(ResponseTemplate::new(200).set_body_bytes(body))
        .mount(&s)
        .await;
    (s, pin)
}
fn unpack(entries: &[Entry<'_>], dst: &Path) -> Result<()> {
    let src = tempfile::NamedTempFile::new().unwrap();
    fs::write(src.path(), tgz(entries)).unwrap();
    untar(src.path(), dst)
}
#[test]
fn untar_keeps_modes_and_symlinks() {
    let tmp = tempfile::tempdir().unwrap();
    unpack(&good_tree(), tmp.path()).unwrap();
    assert_eq!(
        fs::metadata(tmp.path().join("build/bin/llama-server"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o755
    );
    assert_eq!(
        fs::metadata(tmp.path().join("build/bin/libllama.0.dylib"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o644
    );
    assert_eq!(
        fs::read_link(tmp.path().join("build/bin/libllama.dylib")).unwrap(),
        Path::new("libllama.0.dylib")
    );
}
#[test]
fn untar_refuses_unsafe_paths() {
    for entries in [
        vec![file("../evil", b"x", 0o644)],
        vec![file("/tmp/evil", b"x", 0o644)],
        vec![link("a", "../../x")],
        vec![link("a", "/etc/passwd")],
        vec![link("a", "."), link("escape", "a/..")],
        vec![link("a", "."), file("a/injected", b"x", 0o644)],
    ] {
        let tmp = tempfile::tempdir().unwrap();
        assert!(unpack(&entries, tmp.path()).is_err());
    }
}
#[test]
fn find_server_needs_exec_bit() {
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("llama-server");
    fs::write(&p, b"x").unwrap();
    fs::set_permissions(&p, fs::Permissions::from_mode(0o644)).unwrap();
    assert!(
        find_server(tmp.path())
            .unwrap_err()
            .to_string()
            .contains("no llama-server")
    );
    fs::set_permissions(&p, fs::Permissions::from_mode(0o100)).unwrap();
    assert_eq!(find_server(tmp.path()).unwrap(), p);
    let other = tempfile::tempdir().unwrap();
    symlink(tmp.path(), other.path().join("link")).unwrap();
    assert!(find_server(other.path()).is_err());
}
#[tokio::test]
async fn ensure_fetches_once() {
    let (s, pin) = serve(tgz(&good_tree())).await;
    let tmp = tempfile::tempdir().unwrap();
    let notes = Mutex::new(Vec::new());
    for _ in 0..2 {
        let p = ensure_runtime_with(tmp.path(), &pin, CancellationToken::new(), &|s| {
            notes.lock().unwrap().push(s)
        })
        .await
        .unwrap();
        assert_eq!(p, runtime_dir(tmp.path()).join("build/bin/llama-server"));
    }
    assert_eq!(s.received_requests().await.unwrap().len(), 1);
    assert_eq!(notes.lock().unwrap().len(), 1);
    assert_eq!(fs::read_dir(tmp.path().join("runtime")).unwrap().count(), 1);
}
#[test]
fn runtime_note_size() {
    assert_eq!(
        runtime_note(RuntimePin::pinned().size),
        "llama.cpp b11118 11 MB"
    );
}

#[tokio::test]
#[ignore = "downloads the pinned 11 MB llama.cpp archive"]
async fn pinned_runtime_archive_extracts() {
    let tmp = tempfile::tempdir().unwrap();
    let server = ensure_runtime(tmp.path(), CancellationToken::new(), &|_| {})
        .await
        .unwrap();
    assert!(server.is_file());
    assert!(server.metadata().unwrap().permissions().mode() & 0o111 != 0);
}
#[tokio::test]
async fn ensure_rejects_table() {
    for (entries, bad_sha, error) in [
        (good_tree(), true, "sha256"),
        (vec![file("../evil", b"x", 0o644)], false, "unsafe path"),
        (vec![file("/tmp/evil", b"x", 0o644)], false, "unsafe path"),
        (vec![link("a", "../../x")], false, "unsafe symlink"),
        (vec![link("a", "/etc/passwd")], false, "unsafe symlink"),
        (
            vec![file("bin/other", b"x", 0o755)],
            false,
            "no llama-server",
        ),
    ] {
        let (_s, mut pin) = serve(tgz(&entries)).await;
        if bad_sha {
            pin.sha256 = "0".repeat(64);
        }
        let tmp = tempfile::tempdir().unwrap();
        let e = ensure_runtime_with(tmp.path(), &pin, CancellationToken::new(), &|_| {})
            .await
            .unwrap_err();
        assert!(e.to_string().contains(error), "{e}");
        assert_eq!(fs::read_dir(tmp.path().join("runtime")).unwrap().count(), 0);
    }
}
#[tokio::test]
async fn ensure_short_body_fails() {
    let (_s, mut pin) = serve(tgz(&good_tree())).await;
    pin.size += 1;
    let tmp = tempfile::tempdir().unwrap();
    assert!(
        ensure_runtime_with(tmp.path(), &pin, CancellationToken::new(), &|_| {})
            .await
            .unwrap_err()
            .to_string()
            .contains("size")
    );
}
#[tokio::test]
async fn cancelled_download_removes_staging() {
    let (_s, pin) = serve(tgz(&good_tree())).await;
    let tmp = tempfile::tempdir().unwrap();
    let cancel = CancellationToken::new();
    let other = cancel.clone();
    assert!(matches!(
        ensure_runtime_with(tmp.path(), &pin, cancel, &|_| other.cancel()).await,
        Err(Error::Cancelled)
    ));
    assert_eq!(fs::read_dir(tmp.path().join("runtime")).unwrap().count(), 0);
}
#[test]
fn cancelled_extraction_stops_before_writing() {
    let src = tempfile::NamedTempFile::new().unwrap();
    fs::write(src.path(), tgz(&good_tree())).unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let cancel = CancellationToken::new();
    cancel.cancel();
    assert!(matches!(
        untar_with_cancel(src.path(), tmp.path(), &cancel),
        Err(Error::Cancelled)
    ));
    assert_eq!(fs::read_dir(tmp.path()).unwrap().count(), 0);
}
