use super::*;
use crate::provider::fixture_opts;
use std::{fs, os::unix::fs::PermissionsExt, path::Path, process::Command};

#[test]
fn script_per_provider() {
    for (provider, credential) in [("runpod", "podTerminate"), ("vast", "$CONTAINER_API_KEY")] {
        let s = script(provider);
        for required in [
            credential,
            "trap die ERR",
            "sha256sum -c",
            "exec /lobo/lobo-agent",
            "$LOBO_RELEASE_URL",
        ] {
            assert!(s.contains(required), "{provider}: missing {required}");
        }
    }
    assert!(!script("vast").contains("RUNPOD"));
    assert!(!script("runpod").contains("CONTAINER_API_KEY"));
    assert!(script("unknown").contains("terminate() {\n  \n}"));
}

#[test]
fn script_matches_go_bytes() {
    assert_eq!(
        script("runpod"),
        include_str!("../../fixtures/bootstrap/script_runpod.sh")
    );
    assert_eq!(
        script("vast"),
        include_str!("../../fixtures/bootstrap/script_vast.sh")
    );
}

#[test]
fn script_steps_time_bounded() {
    for provider in ["runpod", "vast"] {
        let s = script(provider);
        assert!(s.contains("timeout 300 bash -c 'apt-get"));
        assert!(s.contains("timeout 300 curl"));
        assert!(s.contains("-m 15"));
    }
}

#[test]
fn script_execfail_then_die() {
    let s = script("vast");
    assert!(s.contains("shopt -s execfail\nset +e\nexec /lobo/lobo-agent\ndie"));
}

#[test]
fn script_terminate_retries_30_then_sleeps() {
    let s = script("runpod");
    assert!(s.contains("seq 1 30"));
    assert!(s.contains("done\n  sleep 600\n  exit 1"));
}

#[test]
fn env_keys() {
    let mut o = fixture_opts();
    o.model_fallback = "f".into();
    o.min_mbps = 100;
    o.dl_conns = 32;
    let e = env(&o, "vast");
    assert_eq!(
        e.keys().map(String::as_str).collect::<Vec<_>>().join(" "),
        "CF_TUNNEL_TOKEN LOBO_API_KEY LOBO_BOOT_TIMEOUT LOBO_CTX LOBO_DL_CONNS LOBO_EXPIRES_AT LOBO_IDLE_MIN LOBO_MIN_MBPS LOBO_MODEL LOBO_MODEL_URL LOBO_MODEL_URL_FALLBACK LOBO_PROVIDER LOBO_RELEASE_SHA256 LOBO_RELEASE_URL"
    );
    assert_eq!(e["LOBO_PROVIDER"], "vast");
    assert_eq!(e["LOBO_EXPIRES_AT"], "2026-09-23T22:00:00Z");
}

#[test]
fn env_baked_omits_release() {
    let e = env(
        &CreateOpts {
            model_url: "m".into(),
            release_sha256: "unused".into(),
            ..Default::default()
        },
        "runpod",
    );
    assert!(!e.contains_key("LOBO_RELEASE_URL"));
    assert!(!e.contains_key("LOBO_RELEASE_SHA256"));
}

#[test]
fn env_matches_go() {
    let mut o = fixture_opts();
    o.boot_id = "boot-fixed".into();
    o.model_ssh_key = "base64-fixed".into();
    o.model_host_key = "ssh-ed25519 AAAAPIN".into();
    o.model_fallback = "https://fallback/m".into();
    o.dl_conns = 8;
    o.min_mbps = 100;
    for fixture in [
        include_str!("../../fixtures/bootstrap/env_full.json"),
        include_str!("../../fixtures/bootstrap/env_baked.json"),
    ] {
        let expected: BTreeMap<String, String> = serde_json::from_str(fixture).unwrap();
        assert_eq!(env(&o, "runpod"), expected);
        o.release_url.clear();
        o.release_sha256.clear();
    }
}

#[test]
fn env_never_has_account_secrets() {
    for k in env(&fixture_opts(), "vast").keys() {
        assert!(!k.starts_with("R2_"));
        assert_ne!(k, "RUNPOD_API_KEY");
        assert_ne!(k, "VASTAI_API_KEY");
    }
}

fn executable(dir: &Path, name: &str, body: &str) {
    let p = dir.join(name);
    fs::write(&p, format!("#!/bin/sh\n{body}\n")).unwrap();
    fs::set_permissions(p, fs::Permissions::from_mode(0o755)).unwrap();
}

fn bash_available() -> bool {
    Command::new("bash").arg("--version").output().is_ok()
}

#[test]
fn script_baked_skips_apt_and_zip() {
    if !bash_available() {
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    executable(dir, "apt-get", "echo apt >> \"$CALL_LOG\"");
    executable(dir, "curl", "echo curl >> \"$CALL_LOG\"");
    executable(
        dir,
        "lobo-agent",
        "echo \"agent $LOBO_T_APT $LOBO_T_ZIP $LOBO_T_BOOT0\" >> \"$CALL_LOG\"",
    );
    let log = dir.join("calls.log");
    let output = Command::new("bash")
        .arg("-c")
        .arg(script("runpod").replace(
            "/lobo/lobo-agent",
            &dir.join("lobo-agent").to_string_lossy(),
        ))
        .env_clear()
        .env("PATH", format!("{}:/usr/bin:/bin", dir.display()))
        .env("CALL_LOG", &log)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let calls = fs::read_to_string(log).unwrap();
    let fields: Vec<_> = calls.split_whitespace().collect();
    assert_eq!(fields.len(), 4);
    assert_eq!(fields[0], "agent");
    assert_eq!(fields[1], fields[3]);
    assert_eq!(fields[2], fields[3]);
}

fn run_script(provider: &str, release_fails: bool, answer: &str) -> (String, String) {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    let log = dir.join("calls.log");
    for (name, body) in [
        ("apt-get", "exit 0"),
        ("timeout", "shift; exec \"$@\""),
        ("sleep", "exit 0"),
        ("sha256sum", "exit 0"),
        ("unzip", "exit 0"),
        (
            "curl",
            r#"echo "$*" >> "$CALL_LOG"
case "$*" in *lobo-release*) [ "$RELEASE_FAILS" = 1 ] && exit 22; exit 0 ;; esac
printf '%s' "$TERM_ANSWER""#,
        ),
    ] {
        executable(dir, name, body);
    }
    // A task-owned missing binary makes exec failure deterministic and cannot
    // accidentally execute a real /lobo/lobo-agent on the test host.
    let script = script(provider).replace(
        "/lobo/lobo-agent",
        &dir.join("missing-agent").to_string_lossy(),
    );
    let out = Command::new("bash")
        .arg("-c")
        .arg(script)
        .env_clear()
        .env("PATH", format!("{}:/usr/bin:/bin", dir.display()))
        .env("CALL_LOG", &log)
        .env("RELEASE_FAILS", if release_fails { "1" } else { "0" })
        .env("TERM_ANSWER", answer)
        .env("LOBO_RELEASE_URL", "https://x/lobo-release.zip")
        .env("LOBO_RELEASE_SHA256", "abc")
        .env("RUNPOD_POD_ID", "p1")
        .env("RUNPOD_API_KEY", "k")
        .env("CONTAINER_ID", "7")
        .env("CONTAINER_API_KEY", "k")
        .output()
        .unwrap();
    assert!(!out.status.success());
    (
        fs::read_to_string(log).unwrap(),
        String::from_utf8(out.stderr).unwrap(),
    )
}

#[test]
fn script_terminates_on_failure() {
    if !bash_available() {
        return;
    }
    for (provider, fails, answer, needle, count, accepted) in [
        (
            "runpod",
            true,
            r#"{"data":{"podTerminate":null}}"#,
            "podTerminate",
            1,
            true,
        ),
        (
            "runpod",
            true,
            r#"{"errors":[{"message":"nope"}]}"#,
            "podTerminate",
            30,
            false,
        ),
        ("vast", true, "500", "-X DELETE", 30, false),
        ("vast", true, "404", "-X DELETE", 1, true),
        ("vast", false, "200", "-X DELETE", 1, true),
    ] {
        let (calls, stderr) = run_script(provider, fails, answer);
        assert_eq!(calls.matches(needle).count(), count, "{provider}: {stderr}");
        assert_eq!(stderr.contains("terminate accepted"), accepted, "{stderr}");
    }
}
