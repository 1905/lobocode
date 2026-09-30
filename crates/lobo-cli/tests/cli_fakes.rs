#![cfg(feature = "test-fakes")]
use lobo_proto::{Snap, UpEvent};
use std::{os::unix::fs::PermissionsExt, path::Path};

fn fixture(name: &str) -> String {
    std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/go/text")
            .join(name),
    )
    .unwrap()
}
fn command(name: &str, root: &Path, args: &[&str]) -> std::process::Output {
    let config = root.join("config.env");
    std::fs::write(&config, "LOBO_API_KEY=sk\nRUNPOD_API_KEY=rp\nLOBO_DOMAIN=lobo.example.com\nCF_TUNNEL_TOKEN=tok\nLOBO_BUCKET_URL=https://pub-x.r2.dev\nLOBO_MODEL=q6\n").unwrap();
    std::fs::set_permissions(&config, std::fs::Permissions::from_mode(0o600)).unwrap();
    assert_cmd::cargo::cargo_bin_cmd!("lobo")
        .current_dir(root)
        .env("LOBO_TEST_SCENARIO", name)
        .env("NO_COLOR", "1")
        .env("TZ", "UTC")
        .env("HTTP_PROXY", "http://127.0.0.1:9")
        .env("HTTPS_PROXY", "http://127.0.0.1:9")
        .env("NO_PROXY", "")
        .arg("--config")
        .arg(config)
        .args(args)
        .output()
        .unwrap()
}
fn events(data: &str) -> Vec<UpEvent> {
    data.lines()
        .map(|s| serde_json::from_str(s).unwrap())
        .collect()
}
fn logs(data: &str) -> String {
    data.lines()
        .map(|s| {
            format!(
                "{}\n",
                if s.len() > 9 && s.as_bytes()[2] == b':' && s.as_bytes()[5] == b':' {
                    &s[9..]
                } else {
                    s
                }
            )
        })
        .collect()
}
#[test]
fn executable_json_outputs_match_go() {
    for name in ["boot", "failed"] {
        let tmp = tempfile::tempdir().unwrap();
        let output = command(name, tmp.path(), &["up", "--json", "--q6=false"]);
        let err = String::from_utf8(output.stderr).unwrap();
        assert_eq!(output.status.success(), name == "boot", "{err}");
        assert_eq!(
            events(std::str::from_utf8(&output.stdout).unwrap()),
            events(&fixture(&format!("json_up_{name}.jsonl")))
        );
        if name == "failed" {
            assert_eq!(err, "error: up failed\n");
        } else {
            assert!(tmp.path().join("boots.jsonl").exists());
            assert!(err.contains("boot timings:"));
        }
    }
    for name in ["running", "down"] {
        let tmp = tempfile::tempdir().unwrap();
        let output = command(name, tmp.path(), &["status", "--json"]);
        assert!(output.status.success());
        assert_eq!(
            serde_json::from_slice::<Snap>(&output.stdout).unwrap(),
            serde_json::from_str::<Snap>(&fixture(if name == "running" {
                "status_running.json"
            } else {
                "status_down.json"
            }))
            .unwrap()
        );
    }
    let tmp = tempfile::tempdir().unwrap();
    let output = command("running", tmp.path(), &["down", "--json"]);
    assert!(output.status.success());
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&output.stdout).unwrap(),
        serde_json::from_str::<serde_json::Value>(&fixture("down_running.json")).unwrap()
    );
}
#[test]
fn executable_plain_output_and_unknown_scenario() {
    let tmp = tempfile::tempdir().unwrap();
    let output = command("boot", tmp.path(), &["up", "--plain", "--q6=false"]);
    assert!(output.status.success());
    assert!(output.stdout.is_empty());
    let err = String::from_utf8(output.stderr).unwrap();
    let (boot, report) = err.split_once("\nboot timings:\n").unwrap();
    assert_eq!(logs(boot), logs(&fixture("plain_up_boot.txt")));
    // The scripted agent has zero timings. The representative timing table is
    // independently byte-compared with Go in cli_control.
    assert!(report.contains("model download"));
    assert!(report.contains("0 MB/s, 0 conns,"));
    let output = command("unknown", tmp.path(), &["version"]);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(output.stderr, b"error: unknown test scenario\n");
}
