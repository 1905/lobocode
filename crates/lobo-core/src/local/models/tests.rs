use super::*;

#[test]
fn free_bytes_nearest_missing_and_home() {
    let dir = tempfile::tempdir().unwrap();
    assert!(free_bytes_nearest(&dir.path().join("missing/nested/weights")).is_some());
    assert!(free_bytes_nearest(Path::new("~/")).is_some());
    assert!(!dir.path().join("missing").exists());
}
use std::fs::FileTimes;

#[test]
fn marker_path_has_suffix() {
    assert_eq!(
        marker_path(Path::new("/w"), "a.gguf"),
        Path::new("/w/a.gguf.sha256-ok")
    );
}
#[test]
fn marker_valid_table() {
    let m = Model {
        id: "t".into(),
        file: "t.gguf".into(),
        sha256: "b".repeat(64),
        size: 10,
        ..Default::default()
    };
    for case in [
        "written",
        "valid",
        "missing",
        "stale mtime",
        "old format",
        "wrong sha",
        "wrong size",
        "touched",
        "changed",
        "gone",
    ] {
        let tmp = tempfile::tempdir().unwrap();
        let w = tmp.path();
        let path = w.join(&m.file);
        fs::write(&path, b"gguf bytes").unwrap();
        let meta = fs::metadata(&path).unwrap();
        let line = marker_line(&m.sha256, &meta);
        let marker = match case {
            "written" | "touched" | "changed" | "gone" => {
                write_marker(w, &m).unwrap();
                None
            }
            "valid" => Some(line.clone()),
            "missing" => None,
            "old format" => Some(m.sha256.clone() + "\n"),
            "wrong sha" => Some(line.replacen(&m.sha256, &"c".repeat(64), 1)),
            "wrong size" => Some(line.replacen(" 10 ", " 9 ", 1)),
            "stale mtime" => Some(format!("{} 10 0\n", m.sha256)),
            _ => unreachable!(),
        };
        if let Some(marker) = marker {
            fs::write(marker_path(w, &m.file), marker).unwrap();
        }
        if case == "changed" {
            fs::write(&path, b"GGUF BYTES").unwrap();
        }
        if case == "touched" || case == "changed" {
            fs::File::open(&path)
                .unwrap()
                .set_times(FileTimes::new().set_modified(
                    std::time::SystemTime::now() + std::time::Duration::from_secs(3600),
                ))
                .unwrap();
        }
        if case == "gone" {
            fs::remove_file(&path).unwrap();
        }
        let meta = fs::metadata(&path).ok();
        assert_eq!(
            marker_valid(w, &m, meta.as_ref()),
            matches!(case, "written" | "valid"),
            "{case}"
        );
        assert!(fs::read_dir(w).unwrap().all(|e| {
            !e.unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".sha256-ok-")
        }));
    }
}
#[test]
fn list_states() {
    let tmp = tempfile::tempdir().unwrap();
    let w = tmp.path();
    let q6 = catalog::get("q6").unwrap();
    let q8 = catalog::get("q8").unwrap();
    fs::File::create(w.join(&q6.file))
        .unwrap()
        .set_len(q6.size as u64)
        .unwrap();
    write_marker(w, q6).unwrap();
    fs::write(w.join(&q8.file), b"short").unwrap();
    let l = list(w).unwrap();
    assert!(l.free_bytes > 0);
    assert_eq!(l.models.len(), 2);
    assert!(l.models[0].verified);
    assert_eq!(l.models[0].on_disk, q6.size);
    assert_eq!(l.models[1].on_disk, 5);
    assert!(!l.models[1].verified);
    assert!(!l.runtime.present);
    assert_eq!(l.runtime.version, runtime::RUNTIME_VERSION);
    fs::remove_file(w.join(&q8.file)).unwrap();
    fs::write(marker_path(w, &q8.file), b"").unwrap();
    let l = list(w).unwrap();
    assert_eq!(l.models[1].on_disk, 0);
    assert!(!l.models[1].verified);
    let bin = runtime::runtime_dir(w).join("build/bin/llama-server");
    fs::create_dir_all(bin.parent().unwrap()).unwrap();
    fs::write(&bin, b"").unwrap();
    fs::set_permissions(&bin, fs::Permissions::from_mode(0o755)).unwrap();
    assert!(list(w).unwrap().runtime.present);
}
#[test]
fn list_missing_weights() {
    let tmp = tempfile::tempdir().unwrap();
    let l = list(&tmp.path().join("missing")).unwrap();
    assert_eq!(l.free_bytes, 0);
    assert_eq!(l.models.len(), 2);
    assert!(l.models.iter().all(|m| m.on_disk == 0 && !m.verified));
}
#[test]
fn list_json_shape() {
    let l = Listing {
        weights: "/w".into(),
        free_bytes: 7,
        models: vec![ModelState {
            id: "q6".into(),
            file: "f".into(),
            size: 2,
            on_disk: 1,
            verified: true,
        }],
        runtime: RuntimeInfo {
            version: "b1".into(),
            present: true,
        },
    };
    assert_eq!(
        serde_json::to_string(&l).unwrap(),
        r#"{"weights":"/w","free_bytes":7,"models":[{"id":"q6","file":"f","size":2,"on_disk":1,"verified":true}],"runtime":{"version":"b1","present":true}}"#
    );
}
#[test]
fn marker_written_by_go_is_valid() {
    let tmp = tempfile::tempdir().unwrap();
    let w = tmp.path();
    let m = Model {
        file: "x".into(),
        size: 5,
        sha256: "abc".into(),
        ..Default::default()
    };
    fs::write(w.join("x"), b"hello").unwrap();
    let meta = fs::metadata(w.join("x")).unwrap();
    let nanos = meta
        .modified()
        .unwrap()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    fs::write(marker_path(w, "x"), format!("abc 5 {nanos}\n")).unwrap();
    assert!(marker_valid(w, &m, Some(&meta)));
}
