use assert_cmd::cargo::cargo_bin_cmd;
use regex_lite::Regex;
use serde::Deserialize;
use std::{collections::BTreeMap, fs, os::unix::fs::PermissionsExt, path::Path};

#[derive(Deserialize)]
struct Case {
    name: String,
    args: Vec<String>,
    config: Option<String>,
    config_mode: String,
    #[serde(default)]
    stdin: String,
    #[serde(default)]
    weights_dir: bool,
    #[serde(default)]
    twice: bool,
    exit: i32,
    stdout: String,
    stderr: String,
    stderr_match: String,
    files_after: BTreeMap<String, String>,
}
fn normalize(s: &str, root: &Path) -> String {
    let mut out = s.replace(root.to_str().unwrap(), "$TMP");
    for (pattern, replacement) in [
        (r"\x1b\[[0-9;]*m", ""),
        (r"sk-[0-9a-f]{48}", "sk-KEY"),
        (r#""free_bytes":\d+"#, r#""free_bytes":0"#),
        (r"\(\d+\.\d GB free\)", "(N GB free)"),
        (r"(?m)^\d\d:\d\d:\d\d ", ""),
    ] {
        out = Regex::new(pattern)
            .unwrap()
            .replace_all(&out, replacement)
            .into_owned();
    }
    out
}
#[test]
fn replay_go_cli() {
    let cases: Vec<Case> = serde_json::from_str(include_str!("fixtures/go/cases.json")).unwrap();
    assert!(cases.len() >= 40);
    for case in cases {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        let config = root.join("config.env");
        if let Some(body) = &case.config {
            let mut body = body.clone();
            if case.weights_dir {
                let weights = root.join("weights");
                fs::create_dir(&weights).unwrap();
                let model = &lobo_proto::catalog::all()[0];
                fs::File::create(weights.join(&model.file))
                    .unwrap()
                    .set_len((model.size / 2) as u64)
                    .unwrap();
                body.push_str(&format!("LOBO_WEIGHTS_DIR={}\n", weights.display()));
            }
            fs::write(&config, body).unwrap();
            fs::set_permissions(
                &config,
                fs::Permissions::from_mode(u32::from_str_radix(&case.config_mode, 8).unwrap()),
            )
            .unwrap();
        }
        let command = || {
            let mut cmd = cargo_bin_cmd!("lobo");
            cmd.current_dir(&root)
                .env("HOME", root.join("home"))
                .env("XDG_CONFIG_HOME", root.join("config"))
                .env("NO_COLOR", "1")
                .env("TZ", "UTC")
                .env("TERM", "dumb")
                .env("HTTP_PROXY", "http://127.0.0.1:9")
                .env("HTTPS_PROXY", "http://127.0.0.1:9")
                .env("NO_PROXY", "")
                .arg("--config")
                .arg(&config)
                .args(&case.args)
                .write_stdin(case.stdin.clone());
            cmd
        };
        if case.twice {
            command().assert().success();
        }
        let output = command().output().unwrap();
        assert_eq!(
            output.status.code(),
            Some(case.exit),
            "{} stderr: {}",
            case.name,
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = normalize(&String::from_utf8(output.stdout).unwrap(), &root);
        let stderr = normalize(&String::from_utf8(output.stderr).unwrap(), &root);
        if case.args.contains(&"--json".into()) && !stdout.is_empty() {
            assert_eq!(
                serde_json::from_str::<serde_json::Value>(&stdout).unwrap(),
                serde_json::from_str::<serde_json::Value>(&case.stdout).unwrap(),
                "{} JSON values",
                case.name
            );
            // Key order remains part of the output contract.
            assert_eq!(stdout, case.stdout, "{} JSON bytes", case.name);
        } else {
            assert_eq!(stdout, case.stdout, "{} stdout", case.name);
        }
        match case.stderr_match.as_str() {
            "exact" | "log" => assert_eq!(stderr, case.stderr, "{} stderr", case.name),
            "prefix" => assert!(stderr.starts_with("error: "), "{}: {stderr}", case.name),
            "json_error" => assert!(
                stderr.starts_with("error: config set --stdin: want a JSON object of strings:"),
                "{}: {stderr}",
                case.name
            ),
            "os_error" => assert!(
                stderr.starts_with("error: read $TMP/config.env:"),
                "{}: {stderr}",
                case.name
            ),
            other => panic!("unknown comparison: {other}"),
        }
        for (name, want) in case.files_after {
            let got = normalize(&fs::read_to_string(root.join(&name)).unwrap(), &root);
            assert_eq!(got, want, "{} file {name}", case.name);
        }
    }
}
