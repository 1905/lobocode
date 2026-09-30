//! Run explicitly in the task-owned Dell container. No model or GPU is used.
use axum::{
    Router,
    body::{Body, Bytes},
    http::{HeaderMap, StatusCode},
    response::Response,
    routing::get,
};
use futures_util::StreamExt;
use lobo_core::{bootstrap, connection::Manager, provider::CreateOpts};
use lobo_proto::Instance;
use std::{
    fs,
    net::{Ipv4Addr, TcpListener},
    os::unix::fs::PermissionsExt,
    path::Path,
    process::Stdio,
    time::Duration,
};

fn port_pair() -> u16 {
    for p in (18000..22000).step_by(2) {
        if let Ok(_a) = TcpListener::bind((Ipv4Addr::LOCALHOST, p))
            && TcpListener::bind((Ipv4Addr::LOCALHOST, p + 1)).is_ok()
        {
            return p;
        }
    }
    panic!("no free pair");
}
async fn reply(headers: HeaderMap) -> Response {
    if headers.get("authorization").and_then(|v| v.to_str().ok()) != Some("Bearer fixture") {
        return Response::builder()
            .status(StatusCode::UNAUTHORIZED)
            .body(Body::empty())
            .unwrap();
    }
    Response::new(Body::from("fixture-ok"))
}
async fn stream() -> Response {
    let chunks = futures_util::stream::unfold(0, |n| async move {
        if n == 3 {
            return None;
        }
        if n > 0 {
            tokio::time::sleep(Duration::from_millis(600)).await;
        }
        let text = [
            "data: {\"choices\":[{\"delta\":{\"content\":null}}]}\n\n",
            "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"function\":{\"name\":\"weather\",\"arguments\":\"{}\"}}]}}]}\n\n",
            "data: [DONE]\n\n",
        ][n];
        Some((
            Ok::<_, std::io::Error>(Bytes::from_static(text.as_bytes())),
            n + 1,
        ))
    });
    Response::builder()
        .header("content-type", "text/event-stream")
        .body(Body::from_stream(chunks))
        .unwrap()
}
async fn slow() -> &'static str {
    tokio::time::sleep(Duration::from_secs(105)).await;
    "first token after 105 seconds"
}
async fn wait_http(url: &str) {
    for _ in 0..150 {
        if reqwest::get(url).await.is_ok() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("forward did not start: {url}");
}
fn sshd_stop() {
    if let Ok(text) = fs::read_to_string("/lobo/cloud-ssh/sshd.pid")
        && let Ok(pid) = text.trim().parse::<i32>()
    {
        let _ = nix::sys::signal::kill(
            nix::unistd::Pid::from_raw(pid),
            nix::sys::signal::Signal::SIGTERM,
        );
    }
}
fn address(root: &Path, port: u16) {
    fs::write(
        root.join("address.json"),
        serde_json::to_vec(&serde_json::json!({"host":"127.0.0.1", "port":port})).unwrap(),
    )
    .unwrap();
}
async fn close_ssh(root: &Path) {
    let output = tokio::process::Command::new("ssh")
        .args(["-F", "/dev/null", "-S"])
        .arg(root.join("ssh.sock"))
        .args(["-O", "exit", "owned"])
        .output()
        .await
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
async fn master(root: &Path) -> String {
    let output = tokio::process::Command::new("ssh")
        .args(["-F", "/dev/null", "-S"])
        .arg(root.join("ssh.sock"))
        .args(["-O", "check", "owned"])
        .output()
        .await
        .unwrap();
    assert!(output.status.success());
    String::from_utf8_lossy(&output.stderr).into()
}
struct Cleanup(std::path::PathBuf);
impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = fs::remove_file(self.0.join("desired.json"));
        sshd_stop();
    }
}
#[tokio::test]
#[ignore = "Dell Docker only: needs root and OpenSSH server; takes about two minutes"]
async fn detached_ssh_streaming_reconnect_and_failures() {
    assert_eq!(std::env::consts::OS, "linux", "never run this on the Mac");
    assert!(
        !Path::new("/lobo/cloud-ssh").exists(),
        "use a fresh task-owned container fixture"
    );
    let tmp = tempfile::Builder::new()
        .prefix("lobo-cloud-")
        .tempdir()
        .unwrap();
    // A space in the configuration path must work in native app installations.
    let cfg = tmp.path().join("config file.env");
    let manager = Manager::new(
        cfg.clone(),
        env!("CARGO_BIN_EXE_lobo-core-testchild").into(),
        port_pair(),
    )
    .unwrap();
    manager.preflight().unwrap();
    let _cleanup = Cleanup(manager.root.clone());
    let mut opts = CreateOpts {
        boot_id: "0123456789abcdef".into(),
        expires_at: chrono::Utc::now() + chrono::TimeDelta::minutes(10),
        ..Default::default()
    };
    manager.prepare(&mut opts).await.unwrap();
    assert!(!manager.root.join(&opts.boot_id).join("host").exists());
    assert_eq!(
        fs::metadata(manager.root.join(&opts.boot_id).join("client"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    let script = format!("set -e\n{}", bootstrap::SSH_START);
    let out = tokio::process::Command::new("bash")
        .arg("-c")
        .arg(&script)
        .envs(bootstrap::env(&opts, "runpod"))
        .stdin(Stdio::null())
        .output()
        .await
        .unwrap();
    assert!(
        out.status.success(),
        "bootstrap: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let routes = Router::new()
        .route("/", get(reply))
        .route("/stream", get(stream))
        .route("/slow", get(slow));
    let mut servers = vec![];
    for port in [8080, 8081] {
        let listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, port))
            .await
            .unwrap();
        let app = routes.clone();
        servers.push(tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        }));
    }
    address(&manager.root, 2222);
    let mut instance = Instance {
        provider: "runpod".into(),
        id: "fixture".into(),
        ..Default::default()
    };
    manager.start(&mut instance, &opts).await.unwrap();
    let base = instance.api_url.trim_end_matches("/v1").to_owned();
    wait_http(&base).await;
    let client = reqwest::Client::new();
    assert_eq!(client.get(&base).send().await.unwrap().status(), 401);
    for url in [&base, &instance.agent_url] {
        assert_eq!(
            client
                .get(url)
                .bearer_auth("fixture")
                .send()
                .await
                .unwrap()
                .text()
                .await
                .unwrap(),
            "fixture-ok"
        );
    }
    let before = tokio::time::Instant::now();
    let mut chunks = client
        .get(format!("{base}/stream"))
        .send()
        .await
        .unwrap()
        .bytes_stream();
    let first = chunks.next().await.unwrap().unwrap();
    assert!(
        before.elapsed() < Duration::from_millis(500),
        "SSE was buffered"
    );
    assert!(String::from_utf8_lossy(&first).contains("null"));
    let mut text = String::new();
    while let Some(chunk) = chunks.next().await {
        text.push_str(&String::from_utf8_lossy(&chunk.unwrap()));
    }
    assert!(text.contains("tool_calls") && text.contains("[DONE]"));
    assert!(before.elapsed() >= Duration::from_millis(1100));
    let reopened = Manager::new(cfg, manager.exe.clone(), manager.port).unwrap();
    let pid = master(&manager.root).await;
    reopened.attach(&mut instance).await.unwrap();
    assert_eq!(
        pid,
        master(&manager.root).await,
        "reopen duplicated the SSH process"
    );
    close_ssh(&manager.root).await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    wait_http(&base).await;
    assert_ne!(
        pid,
        master(&manager.root).await,
        "disconnect did not reconnect"
    );
    // A killed helper must reclaim its orphaned SSH child through the private socket.
    let owner: serde_json::Value =
        serde_json::from_slice(&fs::read(manager.root.join("helper.json")).unwrap()).unwrap();
    assert_eq!(owner["boot_id"], opts.boot_id);
    let orphan_master = master(&manager.root).await;
    nix::sys::signal::kill(
        nix::unistd::Pid::from_raw(owner["pid"].as_i64().unwrap() as i32),
        nix::sys::signal::Signal::SIGKILL,
    )
    .unwrap();
    tokio::time::sleep(Duration::from_millis(250)).await;
    reopened.attach(&mut instance).await.unwrap();
    tokio::time::sleep(Duration::from_secs(2)).await;
    wait_http(&base).await;
    assert_ne!(orphan_master, master(&manager.root).await);
    eprintln!(
        "SSH authentication, progressive SSE, tool calls, detached reopen and reconnect passed"
    );
    let text = tokio::time::timeout(
        Duration::from_secs(115),
        client.get(format!("{base}/slow")).send(),
    )
    .await
    .unwrap()
    .unwrap()
    .text()
    .await
    .unwrap();
    assert_eq!(text, "first token after 105 seconds");
    eprintln!("105-second first response passed without proxy timeout");
    // A different trusted pin must never be accepted automatically.
    let known = manager.root.join(&opts.boot_id).join("known_hosts");
    let saved = fs::read(&known).unwrap();
    fs::write(
        &known,
        format!("lobo-{} {}", opts.boot_id, opts.connection_public_key),
    )
    .unwrap();
    close_ssh(&manager.root).await;
    tokio::time::sleep(Duration::from_secs(5)).await;
    let err = reopened
        .attach(&mut instance)
        .await
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("Host key verification failed") || err.contains("REMOTE HOST IDENTIFICATION"),
        "{err}"
    );
    fs::write(&known, &saved).unwrap();
    reopened.stop("runpod", "fixture").await.unwrap();
    assert!(!manager.root.join("desired.json").exists());
    assert!(!manager.root.join(&opts.boot_id).exists());
    TcpListener::bind((Ipv4Addr::LOCALHOST, manager.port)).unwrap();
    TcpListener::bind((Ipv4Addr::LOCALHOST, manager.port + 1)).unwrap();
    // A fresh client key is not authorized on the existing fixture server.
    let mut wrong = CreateOpts {
        boot_id: "fedcba9876543210".into(),
        expires_at: opts.expires_at,
        ..Default::default()
    };
    manager.prepare(&mut wrong).await.unwrap();
    let original_pin = String::from_utf8(saved).unwrap();
    fs::write(
        manager.root.join(&wrong.boot_id).join("known_hosts"),
        original_pin.replace(&opts.boot_id, &wrong.boot_id),
    )
    .unwrap();
    manager.start(&mut instance, &wrong).await.unwrap();
    tokio::time::sleep(Duration::from_secs(5)).await;
    let error = reopened
        .attach(&mut instance)
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("Permission denied (publickey)"), "{error}");
    reopened.stop("runpod", "fixture").await.unwrap();
    eprintln!("helper crash recovery and unauthorized client key rejection passed");
    let occupied = TcpListener::bind((Ipv4Addr::LOCALHOST, manager.port)).unwrap();
    assert!(
        manager
            .preflight()
            .unwrap_err()
            .to_string()
            .contains("occupied")
    );
    drop(occupied);
    for server in servers {
        server.abort();
        let _ = server.await;
    }
    sshd_stop();
    fs::remove_dir_all("/lobo/cloud-ssh").unwrap();
    eprintln!("host-key rejection, port conflict and owned cleanup passed");
}

#[tokio::test]
async fn scoped_connection_mutations_preserve_foreign_owner_and_keys() {
    let tmp = tempfile::tempdir().unwrap();
    let manager =
        Manager::new(tmp.path().join("lobo.env"), "/missing-helper".into(), 28900).unwrap();
    fs::create_dir_all(&manager.root).unwrap();
    let original = serde_json::to_vec(&serde_json::json!({
        "provider": "vast", "id": "foreign", "boot_id": "0123456789abcdef",
        "config_path": manager.config_path, "port": 28900,
        "expires_at": chrono::Utc::now() + chrono::TimeDelta::minutes(10),
    }))
    .unwrap();
    let desired = manager.root.join("desired.json");
    fs::write(&desired, &original).unwrap();
    let keys = manager.root.join("0123456789abcdef");
    fs::create_dir(&keys).unwrap();
    fs::write(keys.join("client"), "private fixture key").unwrap();
    for (provider, id, boot) in [
        ("runpod", "ours", "fedcba9876543210"),
        ("vast", "foreign", "fedcba9876543210"),
    ] {
        manager.stop_owned(provider, id, boot).await.unwrap();
        let mut i = Instance {
            provider: provider.into(),
            id: id.into(),
            ..Default::default()
        };
        assert!(manager.attach_owned(&mut i, boot).await.is_err());
        assert!(
            manager
                .start_owned(
                    &mut i,
                    &CreateOpts {
                        boot_id: boot.into(),
                        ..Default::default()
                    }
                )
                .await
                .is_err()
        );
        assert_eq!(fs::read(&desired).unwrap(), original);
        assert_eq!(
            fs::read_to_string(keys.join("client")).unwrap(),
            "private fixture key"
        );
    }
}
