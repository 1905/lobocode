use std::process::Command;

#[test]
fn check_image_uses_fixture_without_credentials_or_network() {
    use sha2::{Digest, Sha256};
    let dir = tempfile::tempdir().unwrap();
    let models = dir.path().join("models");
    std::fs::create_dir(&models).unwrap();
    let bytes = b"GGUF\x03\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0fixture";
    let file = "model-00001-of-00001.gguf";
    std::fs::write(models.join(file), bytes).unwrap();
    let manifest = lobo_agent::image_model::Manifest {
        model: "q6".into(),
        source_sha256: lobo_proto::catalog::get("q6").unwrap().sha256.clone(),
        shards: vec![lobo_agent::image_model::Shard {
            file: file.into(),
            size: bytes.len() as u64,
            sha256: hex::encode(Sha256::digest(bytes)),
        }],
    };
    std::fs::write(
        dir.path().join("model.json"),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
    let check = |model: &str| {
        Command::new(env!("CARGO_BIN_EXE_lobo-agent"))
            .env_clear()
            .args(["check-image", "--model", model, "--root"])
            .arg(dir.path())
            .output()
            .unwrap()
    };
    let success = check("q6");
    assert!(
        success.status.success(),
        "{}",
        String::from_utf8_lossy(&success.stderr)
    );
    assert!(String::from_utf8_lossy(&success.stdout).contains("bundled model verified"));
    assert!(!check("q8").status.success());
    assert!(!check("invalid").status.success());
    std::fs::remove_file(models.join(file)).unwrap();
    assert!(!check("q6").status.success());
}

/// Execute the entrypoint in a private fixture tree. Never modify /lobo or /app.
#[test]
fn image_entrypoint_terminates_both_providers_when_required_content_is_missing() {
    use std::os::unix::fs::PermissionsExt;
    fn executable(path: &std::path::Path, text: &str) {
        std::fs::write(path, text).unwrap();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    for (provider, rejected) in [
        ("runpod", false),
        ("vast", false),
        ("runpod", true),
        ("vast", true),
    ] {
        for missing in ["agent", "runtime", "manifest", "tunnel", "none"] {
            if rejected && missing != "agent" {
                continue;
            }
            let fixture = tempfile::tempdir().unwrap();
            let root = fixture.path();
            let lobo = root.join("lobo");
            let app = root.join("app");
            let commands = root.join("commands");
            std::fs::create_dir_all(lobo.join("bin")).unwrap();
            std::fs::create_dir(&app).unwrap();
            std::fs::create_dir(&commands).unwrap();
            if missing != "agent" {
                executable(
                    &lobo.join("lobo-agent"),
                    "#!/bin/sh\nprintf started > \"$FIXTURE_AGENT_START\"\nexit 0\n",
                );
            }
            if missing != "runtime" {
                executable(&app.join("llama-server"), "#!/bin/sh\nexit 0\n");
            }
            if missing != "manifest" {
                std::fs::write(lobo.join("model.json"), "{}").unwrap();
            }
            if missing != "tunnel" {
                executable(&lobo.join("bin/cloudflared"), "#!/bin/sh\nexit 0\n");
            }
            // The fake client records the actual argument bytes. It never calls a network.
            executable(
                &commands.join("curl"),
                r#"#!/bin/bash
printf '%s\0' "$@" >> "$FIXTURE_REQUEST"
if [[ "$LOBO_PROVIDER" = runpod ]]; then
  if [[ "$FIXTURE_REJECT" = 1 ]]; then printf '{"errors":[{"message":"fixture rejection"}]}';
  else printf '{"data":{"podTerminate":null}}'; fi
else
  if [[ "$FIXTURE_REJECT" = 1 ]]; then printf 503; else printf 200; fi
fi
"#,
            );
            // A failed cleanup retry must not hang a fixture test for ten minutes.
            executable(&commands.join("sleep"), "#!/bin/sh\nexit 0\n");
            let script = include_str!("../../../docker/pod/start.sh")
                .replace("/lobo/", &format!("{}/", lobo.display()))
                .replace("/app/", &format!("{}/", app.display()));
            let entrypoint = root.join("start.sh");
            std::fs::write(&entrypoint, script).unwrap();
            let request = root.join("request");
            let agent_start = root.join("agent-started");
            let out = Command::new("/bin/bash")
                .arg(&entrypoint)
                .env_clear()
                .env("PATH", format!("{}:/usr/bin:/bin", commands.display()))
                .env("LOBO_PROVIDER", provider)
                .env("LOBO_CONNECTION", "cloudflare")
                .env("RUNPOD_POD_ID", "pod\"with\\quotes")
                .env("RUNPOD_API_KEY", "fixture-runpod-key")
                .env("CONTAINER_ID", "1234")
                .env("CONTAINER_API_KEY", "fixture-vast-key")
                .env("FIXTURE_REJECT", if rejected { "1" } else { "0" })
                .env("FIXTURE_REQUEST", &request)
                .env("FIXTURE_AGENT_START", &agent_start)
                .output()
                .unwrap();
            if missing == "none" {
                assert!(
                    out.status.success(),
                    "{provider}: {}",
                    String::from_utf8_lossy(&out.stderr)
                );
                assert_eq!(std::fs::read(&agent_start).unwrap(), b"started");
                assert!(
                    !request.exists(),
                    "healthy entrypoint must not request provider cleanup"
                );
                continue;
            }
            assert_eq!(out.status.code(), Some(1), "{provider}/{missing}");
            assert!(
                !agent_start.exists(),
                "missing required content must fail before agent launch"
            );
            let stderr = String::from_utf8_lossy(&out.stderr);
            if rejected {
                assert!(
                    stderr.contains("manual cleanup required"),
                    "{provider}/{missing}: {stderr}"
                );
                assert!(!stderr.contains("termination accepted"));
            } else {
                assert!(
                    stderr.contains("termination accepted"),
                    "{provider}/{missing}: {stderr}"
                );
            }
            let args: Vec<_> = std::fs::read(&request)
                .unwrap()
                .split(|b| *b == 0)
                .filter(|b| !b.is_empty())
                .map(|b| String::from_utf8(b.to_vec()).unwrap())
                .collect();
            let endpoint = if provider == "runpod" {
                "https://api.runpod.io/graphql"
            } else {
                "https://console.vast.ai/api/v0/instances/1234/"
            };
            assert_eq!(
                args.iter().filter(|s| s.as_str() == endpoint).count(),
                if rejected { 30 } else { 1 }
            );
            if provider == "runpod" {
                assert!(args.iter().any(|s| s == "https://api.runpod.io/graphql"));
                assert!(
                    args.iter()
                        .any(|s| s == "Authorization: Bearer fixture-runpod-key")
                );
                let payload = &args[args.iter().position(|s| s == "-d").unwrap() + 1];
                let json: serde_json::Value = serde_json::from_str(payload).unwrap();
                let id = serde_json::to_string("pod\"with\\quotes").unwrap();
                assert_eq!(
                    json["query"],
                    format!("mutation {{ podTerminate(input:{{podId:{id}}}) }}")
                );
            } else {
                assert!(args.iter().any(|s| s == "DELETE"));
                assert!(
                    args.iter()
                        .any(|s| s == "https://console.vast.ai/api/v0/instances/1234/")
                );
                assert!(
                    args.iter()
                        .any(|s| s == "Authorization: Bearer fixture-vast-key")
                );
            }
        }
    }
}

#[test]
fn bin_fatal_without_creds_exits_1() {
    let out = Command::new(env!("CARGO_BIN_EXE_lobo-agent"))
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("lobo-agent fatal: config:"), "{err}");
    assert!(err.contains("cannot self-terminate"), "{err}");
}
#[test]
fn bin_version() {
    let out = Command::new(env!("CARGO_BIN_EXE_lobo-agent"))
        .env_clear()
        .arg("version")
        .output()
        .unwrap();
    assert!(out.status.success());
    assert_eq!(
        String::from_utf8(out.stdout).unwrap(),
        format!("{}\n", lobo_agent::VERSION)
    );
}
