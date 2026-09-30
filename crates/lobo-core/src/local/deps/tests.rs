use super::*;
use lobo_agent::LogRing;
use sha2::{Digest, Sha256};
use std::{fs, os::unix::fs::PermissionsExt, sync::atomic::AtomicUsize};
use wiremock::{Mock, MockServer, Request, ResponseTemplate, matchers::any};

fn config(w: &Path) -> MacConfig {
    MacConfig {
        weights: w.into(),
        llama_server: PathBuf::new(),
        api_key: "sk-x".into(),
        port: 8931,
        ctx: 65536,
        model: lobo_proto::catalog::get("q6").unwrap().clone(),
        base_env: vec![],
    }
}
fn deps(w: &Path) -> MacDeps {
    let mut d = MacDeps::new(
        config(w),
        Arc::new(LogRing::new(100)),
        CancellationToken::new(),
    );
    d.memory = Arc::new(|| Ok(ample_memory()));
    d
}
fn executable(path: &Path, body: &str) {
    // A sibling test can fork while this fixture is open for writing. CLOEXEC
    // leaves a brief writable reference in that child, making exec return
    // ETXTBSY. Keep the writable descriptor out of the shared test process.
    let status = std::process::Command::new("/bin/sh")
        .args(["-c", "printf '%s' \"$1\" > \"$2\"", "fixture-writer"])
        .arg(format!("#!/bin/sh\n{body}\n"))
        .arg(path)
        .status()
        .unwrap();
    assert!(status.success());
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

#[tokio::test]
async fn check_gpu_table() {
    let need = super::super::memory::assess("q8", 65536, &ample_memory())
        .unwrap()
        .required_bytes;
    for (budget, script, want) in [
        (100 << 30, "echo 'MTL0: Apple M1 Max'", ""),
        (need, "echo 'MTL0: Apple M1 Max'", ""),
        (
            need - 1,
            "echo 'MTL0: Apple M1 Max'",
            "q8 with 65536 context tokens requires",
        ),
        (
            100 << 30,
            "echo 'Available devices:'; echo 'ggml_metal_init: error: failed'",
            "no Metal device",
        ),
    ] {
        let tmp = tempfile::tempdir().unwrap();
        let mut d = deps(tmp.path());
        let server = tmp.path().join("server");
        executable(&server, script);
        d.cfg.llama_server = server;
        d.cfg.model = lobo_proto::catalog::get("q8").unwrap().clone();
        d.memory = Arc::new(move || {
            Ok(super::super::memory::MemorySnapshot {
                available_bytes: budget + (4 << 30),
                metal_limit_bytes: 110 << 30,
                ..ample_memory()
            })
        });
        let result = d.check_gpu(&CancellationToken::new()).await;
        if want.is_empty() {
            result.unwrap();
        } else {
            assert!(result.unwrap_err().to_string().contains(want));
        }
    }
}
#[tokio::test]
async fn gpu_metrics_vm_stat() {
    let tmp = tempfile::tempdir().unwrap();
    let mut d = deps(tmp.path());
    d.host = host_gpu(
        &|name| {
            assert_eq!(name, "machdep.cpu.brand_string");
            Ok("Apple M1 Max".into())
        },
        &|| Ok(64 << 30),
    )
    .map_err(|e| e.to_string());
    d.vm_stat = Arc::new(|| {
        Box::pin(async {
            Ok("Mach Virtual Memory Statistics: (page size of 16384 bytes)\nPages free: 19024.\nPages active: 1000000.\nPages wired down: 300000.\nPages occupied by compressor: 214000.\n".into())
        })
    });
    let g = d.gpu().await.unwrap();
    assert_eq!(g.name, "Apple M1 Max");
    assert_eq!(g.vram_used_mb, 23656);
    assert_eq!(g.vram_total_mb, 65536);
    assert_eq!(g.util_pct, 0);
    for bad in [
        "garbage",
        "Mach (page size of 16384 bytes)\nPages active: 1.\n",
    ] {
        assert!(parse_vm_stat(bad).is_err());
    }
}

async fn serve(body: Vec<u8>) -> MockServer {
    let s = MockServer::start().await;
    Mock::given(any())
        .respond_with(move |r: &Request| {
            assert_eq!(r.url.path(), "/t.gguf");
            if let Some(range) = r.headers.get("Range") {
                let range = range.to_str().unwrap().strip_prefix("bytes=").unwrap();
                let (start, end) = range.split_once('-').unwrap();
                let start: usize = start.parse().unwrap();
                let end: usize = if end.is_empty() {
                    body.len() - 1
                } else {
                    end.parse().unwrap()
                };
                ResponseTemplate::new(206)
                    .insert_header(
                        "Content-Range",
                        format!("bytes {start}-{end}/{}", body.len()),
                    )
                    .set_body_bytes(body[start..=end].to_vec())
            } else {
                ResponseTemplate::new(200).set_body_bytes(body.clone())
            }
        })
        .mount(&s)
        .await;
    s
}
#[tokio::test]
async fn download_table() {
    let body = b"gguf".repeat(4096);
    let good = Model {
        id: "t".into(),
        file: "t.gguf".into(),
        size: body.len() as i64,
        sha256: hex::encode(Sha256::digest(&body)),
        ..Default::default()
    };
    for (name, existing, marker, want_hits, bad, want_verifying) in [
        ("complete marker", 2, "valid", 0, false, false),
        ("complete no marker", 2, "", 0, false, true),
        ("stale mtime", 2, "stale", 0, false, true),
        ("old format", 2, "old", 0, false, true),
        ("wrong sha marker", 2, "wrong", 0, false, true),
        ("missing", 0, "", 1, false, false),
        ("short", 1, "old", 2, false, false),
        ("bad download", 0, "", 1, true, false),
        ("bad disk", 2, "", 0, true, true),
    ] {
        let s = serve(body.clone()).await;
        let tmp = tempfile::tempdir().unwrap();
        let w = tmp.path();
        let mut d = deps(w);
        d.hf_base = s.uri() + "/";
        d.cfg.model = good.clone();
        if bad {
            d.cfg.model.sha256 = "0".repeat(64);
        }
        let dst = w.join("t.gguf");
        match existing {
            1 => fs::write(&dst, &body[..100]).unwrap(),
            2 => fs::write(&dst, &body).unwrap(),
            _ => {}
        }
        match marker {
            "valid" => models::write_marker(w, &good).unwrap(),
            "old" => {
                fs::write(models::marker_path(w, "t.gguf"), good.sha256.clone() + "\n").unwrap()
            }
            "stale" => fs::write(
                models::marker_path(w, "t.gguf"),
                format!("{} {} 0\n", good.sha256, good.size),
            )
            .unwrap(),
            "wrong" => fs::write(
                models::marker_path(w, "t.gguf"),
                models::marker_line(&"a".repeat(64), &dst.metadata().unwrap()),
            )
            .unwrap(),
            _ => {}
        }
        let verifying = AtomicBool::new(false);
        let result = d
            .download(CancellationToken::new(), &|p| {
                if p.verifying {
                    verifying.store(true, Ordering::Relaxed);
                }
            })
            .await;
        if bad {
            assert!(result.unwrap_err().to_string().contains("sha256"), "{name}");
            assert_eq!(fs::read_dir(w.join(".bad")).unwrap().count(), 1);
            assert!(!dst.exists());
            assert!(!w.join("t.gguf.bad").exists());
        } else {
            result.unwrap();
            assert_eq!(fs::read(&dst).unwrap(), body);
        }
        assert_eq!(
            s.received_requests().await.unwrap().len(),
            want_hits,
            "{name}"
        );
        assert_eq!(
            models::marker_valid(w, &d.cfg.model, dst.metadata().ok().as_ref()),
            !bad,
            "{name}"
        );
        assert_eq!(verifying.load(Ordering::Relaxed), want_verifying, "{name}");
    }
}
#[tokio::test]
async fn start_llama_args_env() {
    let tmp = tempfile::tempdir().unwrap();
    let server = tmp.path().join("server");
    executable(
        &server,
        "echo \"args: $*\"; echo \"key: $LLAMA_API_KEY\"; echo \"arg: $LLAMA_ARG_HOST\"; echo \"rp: $RUNPOD_API_KEY\"; echo \"lobo: $LOBO_API_KEY\"",
    );
    let logs = Arc::new(LogRing::new(100));
    let mut cfg = config(Path::new("/w"));
    let model = cfg.model.clone();
    cfg.llama_server = server;
    cfg.base_env = vec![
        ("LLAMA_ARG_HOST".into(), "0.0.0.0".into()),
        ("RUNPOD_API_KEY".into(), "secret-rp".into()),
        ("LOBO_API_KEY".into(), "secret-lobo".into()),
    ];
    let mut d = MacDeps::new(cfg, logs.clone(), CancellationToken::new());
    d.memory = Arc::new(|| Ok(ample_memory()));
    Llama::start(&d).await.unwrap().await.unwrap().unwrap();
    let out = logs.tail(100).join("\n") + "\n";
    for want in [
        format!(
            "args: -m /w/{} --alias {} --host 127.0.0.1 --port 8931 ",
            model.file, model.alias
        ),
        "-c 65536".into(),
        "key: sk-x".into(),
        "arg: \n".into(),
        "rp: \n".into(),
        "lobo: \n".into(),
    ] {
        assert!(out.contains(&want), "missing {want:?}: {out}");
    }
    assert!(!out.contains("secret-"));
    assert!(d.pid.load(Ordering::Acquire) > 0);
    assert!(d.wait_llama(Duration::from_secs(5)).await);
    assert!(deps(tmp.path()).wait_llama(Duration::ZERO).await);
}
#[tokio::test]
async fn wait_healthy_polls() {
    let count = Arc::new(AtomicUsize::new(0));
    let c = count.clone();
    let s = MockServer::start().await;
    Mock::given(any())
        .respond_with(move |r: &Request| {
            assert_eq!(r.url.path(), "/health");
            ResponseTemplate::new(if c.fetch_add(1, Ordering::Relaxed) == 0 {
                503
            } else {
                200
            })
        })
        .mount(&s)
        .await;
    let tmp = tempfile::tempdir().unwrap();
    let mut d = deps(tmp.path());
    d.llama_url = s.uri();
    d.poll = Duration::from_millis(1);
    Llama::wait_healthy(&d, CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(count.load(Ordering::Relaxed), 2);
}
#[tokio::test]
async fn new_deps_wiring() {
    let tmp = tempfile::tempdir().unwrap();
    let stop = CancellationToken::new();
    let (d, _mac) = new_deps(
        config(tmp.path()),
        Arc::new(LogRing::new(100)),
        stop.clone(),
    );
    let mut tunnel = d
        .tunnel
        .start(CancellationToken::new(), CancellationToken::new())
        .await
        .unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(50), &mut tunnel)
            .await
            .is_err()
    );
    assert!(
        d.metrics
            .host()
            .await
            .unwrap_err()
            .to_string()
            .contains("not collected on macOS")
    );
    d.killer.kill_self(CancellationToken::new()).await.unwrap();
    assert!(stop.is_cancelled());
}
#[tokio::test]
async fn stop_token_kills_llama_and_waits_for_exit() {
    let tmp = tempfile::tempdir().unwrap();
    let mut cfg = config(tmp.path());
    // A dedicated script consumes llama arguments then execs one long-lived child.
    cfg.llama_server = tmp.path().join("server");
    executable(&cfg.llama_server, "exec /bin/sleep 30");
    let stop = CancellationToken::new();
    let mut d = MacDeps::new(cfg, Arc::new(LogRing::new(100)), stop.clone());
    d.memory = Arc::new(|| Ok(ample_memory()));
    let exit = Llama::start(&d).await.unwrap();
    assert!(!d.wait_llama(Duration::ZERO).await);
    stop.cancel();
    assert!(d.wait_llama(Duration::from_secs(3)).await);
    assert!(exit.await.unwrap().is_err());
    assert!(!super::super::state::alive(
        d.pid.load(Ordering::Acquire) as i32
    ));
}

fn ample_memory() -> super::super::memory::MemorySnapshot {
    super::super::memory::MemorySnapshot {
        total_bytes: 128 << 30,
        available_bytes: 100 << 30,
        metal_limit_bytes: 96 << 30,
    }
}
#[tokio::test]
async fn rejected_memory_never_runs_device_probe() {
    for unknown in [false, true] {
        let tmp = tempfile::tempdir().unwrap();
        let mut d = deps(tmp.path());
        let marker = tmp.path().join("device-ran");
        d.cfg.llama_server = tmp.path().join("server");
        executable(
            &d.cfg.llama_server,
            &format!("touch '{}'; echo MTL0", marker.display()),
        );
        d.memory = Arc::new(move || {
            if unknown {
                return Err(Error::Local(
                    "fixture native measurement unavailable".into(),
                ));
            }
            Ok(super::super::memory::MemorySnapshot {
                available_bytes: 10 << 30,
                ..ample_memory()
            })
        });
        let error = d
            .check_gpu(&CancellationToken::new())
            .await
            .unwrap_err()
            .to_string();
        assert!(
            error.contains(if unknown {
                "measurement unavailable"
            } else {
                "GiB"
            }),
            "{error}"
        );
        assert!(!marker.exists());
    }
}
#[tokio::test]
async fn declining_memory_after_gpu_check_prevents_inference() {
    let tmp = tempfile::tempdir().unwrap();
    let mut d = deps(tmp.path());
    let marker = tmp.path().join("inference-ran");
    d.cfg.llama_server = tmp.path().join("server");
    executable(
        &d.cfg.llama_server,
        &format!(
            "if [ \"$1\" = --list-devices ]; then echo MTL0; else touch '{}'; fi",
            marker.display()
        ),
    );
    let count = Arc::new(AtomicUsize::new(0));
    let c = count.clone();
    d.memory = Arc::new(move || {
        Ok(super::super::memory::MemorySnapshot {
            available_bytes: if c.fetch_add(1, Ordering::SeqCst) == 0 {
                100 << 30
            } else {
                10 << 30
            },
            ..ample_memory()
        })
    });
    d.check_gpu(&CancellationToken::new()).await.unwrap();
    assert!(Llama::start(&d).await.is_err());
    assert_eq!(count.load(Ordering::SeqCst), 2);
    assert_eq!(d.pid.load(Ordering::Acquire), 0);
    assert!(!d.llama_started.load(Ordering::Acquire));
    assert!(d.wait_llama(Duration::ZERO).await);
    assert!(!marker.exists());
}
#[tokio::test]
async fn unknown_memory_before_inference_has_no_process() {
    let tmp = tempfile::tempdir().unwrap();
    let mut d = deps(tmp.path());
    d.memory = Arc::new(|| {
        Err(Error::Local(
            "fixture native measurement unavailable".into(),
        ))
    });
    let error = Llama::start(&d).await.err().unwrap().to_string();
    assert!(error.contains("measurement unavailable"), "{error}");
    assert_eq!(d.pid.load(Ordering::Acquire), 0);
    assert!(!d.llama_started.load(Ordering::Acquire));
    assert!(d.wait_llama(Duration::ZERO).await);
}

#[tokio::test]
async fn cancelled_gpu_check_never_reads_memory_or_device() {
    let tmp = tempfile::tempdir().unwrap();
    let mut d = deps(tmp.path());
    d.memory = Arc::new(|| panic!("cancelled startup must not inspect memory"));
    let cancel = CancellationToken::new();
    cancel.cancel();
    assert!(matches!(d.check_gpu(&cancel).await, Err(Error::Cancelled)));
}
