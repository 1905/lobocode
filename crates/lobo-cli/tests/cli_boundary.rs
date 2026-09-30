use clap::FromArgMatches;
use lobo_cli::{
    app::{App, Io, Term},
    cli::{Cli, Cmd},
    wizard::{
        Prompter,
        prompt::{PromptError, PromptResult},
    },
};
use lobo_core::config::Laptop;
use std::{
    io::Write,
    path::Path,
    sync::{Arc, Mutex},
};

#[test]
fn gen_key_reuse_rotate_and_permissions() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.env");
    std::fs::write(&path, "LOBO_LOCAL_PORT=9000\n").unwrap();
    let invoke = |rotate| {
        let mut c = assert_cmd::cargo::cargo_bin_cmd!("lobo");
        c.current_dir(dir.path())
            .arg("--config")
            .arg(&path)
            .arg("gen-api-key");
        if rotate {
            c.arg("--rotate");
        }
        let output = c.assert().success().get_output().clone();
        let out = String::from_utf8(output.stdout).unwrap();
        let err = String::from_utf8(output.stderr).unwrap();
        assert!(out.starts_with("wrote "));
        assert!(err.contains("no cloud provider configured"));
        let key = lobo_core::config::values(&path).unwrap()["LOBO_API_KEY"].clone();
        let file = dir.path().join("opencode.lobo.json");
        assert_eq!(
            std::fs::metadata(file).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        (key, err)
    };
    let (key, err) = invoke(false);
    assert!(err.contains("new LOBO_API_KEY written"));
    assert_eq!(key.len(), 51);
    let (same, err) = invoke(false);
    assert_eq!(same, key);
    assert!(err.contains("keeping existing LOBO_API_KEY"));
    assert_ne!(invoke(true).0, key);
}

#[derive(Clone, Default)]
struct Buffer(Arc<Mutex<Vec<u8>>>);
impl Write for Buffer {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(b);
        Ok(b.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
impl Buffer {
    fn text(&self) -> String {
        String::from_utf8(self.0.lock().unwrap().clone()).unwrap()
    }
}
async fn run(app: &App, args: &[&str]) -> (i32, String, String) {
    let out = Buffer::default();
    let err = Buffer::default();
    let mut io = Io {
        out: Box::new(out.clone()),
        err: Box::new(err.clone()),
        input: Box::new(std::io::empty()),
    };
    let code = lobo_cli::run(
        app,
        std::iter::once("lobo")
            .chain(args.iter().copied())
            .map(Into::into)
            .collect(),
        &mut io,
    )
    .await;
    (code, out.text(), err.text())
}
#[tokio::test]
async fn run_errors_and_root_help() {
    let mut app = App::real();
    app.term = Term::default();
    for args in [vec!["nosuch"], vec!["up", "--bogus"]] {
        let (code, out, err) = run(&app, &args).await;
        assert_eq!(code, 1);
        assert!(out.is_empty());
        assert!(err.starts_with("error: "));
        assert_eq!(err.lines().count(), 1);
    }
    for args in [vec![], vec!["help"], vec!["-h"], vec!["--help"]] {
        let mut all = vec!["--config", "/home/u/.config/lobo/config.env"];
        all.extend(args);
        let (code, out, err) = run(&app, &all).await;
        assert_eq!(code, 0);
        assert_eq!(err, "");
        assert_eq!(
            out,
            include_str!("goldens/help_noconfig.golden").replace(
                "lobo dev\n",
                &format!("lobo {}\n", lobo_cli::build_info::VERSION)
            )
        );
    }
}

#[tokio::test]
async fn completion_emits_scripts_for_all_supported_shells() {
    for (shell, marker) in [
        ("bash", "complete -F"),
        ("zsh", "#compdef lobo"),
        ("fish", "complete -c lobo"),
        ("powershell", "Register-ArgumentCompleter"),
    ] {
        let (code, out, err) = run(&App::real(), &["completion", shell]).await;
        assert_eq!(code, 0, "{shell}: {err}");
        assert!(err.is_empty());
        assert!(out.contains(marker), "{shell}: {out}");
    }
}
#[tokio::test]
async fn local_run_flags_and_config_forwarding() {
    let seen = Arc::new(Mutex::new(None));
    let recorder = seen.clone();
    let mut app = App::real();
    app.supervise = Arc::new(move |cfg, _| {
        *recorder.lock().unwrap() = Some(cfg);
        Box::pin(async { Ok(()) })
    });
    for args in [
        vec![
            "--model",
            "q8",
            "--ctx",
            "65536",
            "--idle-min",
            "20",
            "--boot-id",
            "b1",
            "--port",
            "9000",
            "--api-port",
            "9001",
        ],
        vec!["--ctx", "4096", "--idle-min", "5"],
    ] {
        let mut all = vec!["--config", "rel/override.env", "local", "run"];
        all.extend(&args);
        let (code, _, err) = run(&app, &all).await;
        assert_eq!(code, 0, "{err}");
        let cfg = seen.lock().unwrap().take().unwrap();
        assert_eq!(cfg.config_path, Some("rel/override.env".into()));
        assert_eq!(cfg.version.version, lobo_cli::build_info::VERSION);
        assert_eq!(cfg.version.git_sha, lobo_cli::build_info::COMMIT);
        if args[0] == "--model" {
            assert_eq!(
                (
                    cfg.model.as_str(),
                    cfg.ctx,
                    cfg.idle_min,
                    cfg.boot_id.as_str(),
                    cfg.port,
                    cfg.api_port
                ),
                ("q8", 65536, 20, "b1", 9000, 9001)
            );
        } else {
            assert_eq!(
                (
                    cfg.model.as_str(),
                    cfg.ctx,
                    cfg.idle_min,
                    cfg.port,
                    cfg.api_port
                ),
                ("q6", 4096, 5, 8931, 8932)
            );
        }
    }
    for (args, want) in [
        (vec!["--model", "q2", "--ctx", "1", "--idle-min", "1"], "q2"),
        (vec!["--idle-min", "1"], "--ctx"),
        (vec!["--ctx", "1"], "--idle-min"),
        (
            vec![
                "--ctx",
                "1",
                "--idle-min",
                "1",
                "--port",
                "9000",
                "--api-port",
                "9000",
            ],
            "--api-port",
        ),
        (vec!["--ctx", "1", "--idle-min", "1", "x"], "error:"),
        (
            vec!["--ctx", "1", "--idle-min", "1", "--port", "70000"],
            "--port: want 1-65535, got 70000",
        ),
        (
            vec!["--ctx", "1", "--idle-min", "1", "--port", "-1"],
            "--port: want 1-65535, got -1",
        ),
    ] {
        let mut all = vec!["local", "run"];
        all.extend(args);
        let (code, _, err) = run(&app, &all).await;
        assert_eq!(code, 1);
        assert!(err.contains(want), "{err}");
        assert!(seen.lock().unwrap().is_none());
    }
    assert!(
        lobo_cli::cli::command()
            .find_subcommand("local")
            .unwrap()
            .is_hide_set()
    );
}
#[test]
fn defaults_keep_explicit_false_and_flag_precedence() {
    let cfg = Laptop {
        runpod_api_key: "rp".into(),
        model: "q6".into(),
        ctx: "100".into(),
        cf_tunnel_token: "tok".into(),
        domain: "lobo.test".into(),
        bucket_url: "https://b.test".into(),
        ..Default::default()
    };
    for (flag, model) in [("--q6=false", ""), ("--q6=true", "q6")] {
        let m = lobo_cli::cli::command()
            .try_get_matches_from(["lobo", "up", flag, "--ctx", "8192"])
            .unwrap();
        let changed = lobo_cli::cli::changed(&m);
        let Some(Cmd::Up(a)) = Cli::from_arg_matches(&m).unwrap().cmd else {
            panic!()
        };
        let opts =
            lobo_cli::cmd::up::prepare(&App::real(), &a, &changed, &cfg, Path::new("config"))
                .unwrap();
        assert_eq!(opts.model, model);
        assert_eq!(opts.ctx, 8192);
    }
}
#[tokio::test]
async fn gates_reject_before_dependencies() {
    let mut app = App::real();
    app.local_supported = || Err(lobo_core::Error::Local("unsupported".into()));
    app.deps = Arc::new(|_, _| panic!("precheck must run first"));
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("config");
    for (command, body, needle) in [
        (
            "up",
            "LOBO_API_KEY=sk\nRUNPOD_API_KEY=rp\n",
            "LOBO_BUCKET_URL",
        ),
        (
            "down",
            "LOBO_API_KEY=sk\n",
            "RUNPOD_API_KEY or VASTAI_API_KEY",
        ),
        (
            "status",
            "LOBO_API_KEY=sk\n",
            "RUNPOD_API_KEY or VASTAI_API_KEY",
        ),
        ("release", "LOBO_API_KEY=sk\n", "R2_"),
    ] {
        std::fs::write(&path, body).unwrap();
        let (code, _, err) = run(&app, &["--config", path.to_str().unwrap(), command]).await;
        assert_eq!(code, 1);
        assert!(err.contains(needle), "{err}");
    }
}

struct WizardAnswer {
    abort: bool,
    fail: bool,
    discard: bool,
}
impl Prompter for WizardAnswer {
    fn note(&mut self, _: &str, _: &str) -> PromptResult<()> {
        if self.fail {
            Err(PromptError::Failed(anyhow::anyhow!("terminal read failed")))
        } else if self.abort {
            Err(PromptError::Aborted)
        } else {
            Ok(())
        }
    }
    fn secret(
        &mut self,
        _: &str,
        _: &str,
        _: &dyn Fn(&str) -> Result<(), String>,
    ) -> PromptResult<String> {
        Ok(String::new())
    }
    fn text(
        &mut self,
        _: &str,
        _: &str,
        s: &str,
        _: &dyn Fn(&str) -> Result<(), String>,
    ) -> PromptResult<String> {
        Ok(s.into())
    }
    fn select(
        &mut self,
        title: &str,
        _: &str,
        _: &[(&str, &str)],
        cur: &str,
    ) -> PromptResult<String> {
        Ok(if title.ends_with("LOBO API key") {
            "new"
        } else {
            cur
        }
        .into())
    }
    fn confirm(&mut self, _: &str, _: &str, _: &str, _: &str, _: bool) -> PromptResult<bool> {
        Ok(!self.discard)
    }
}
#[tokio::test]
async fn config_wizard_save_abort_discard_and_failure() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("config");
    for (abort, fail, discard) in [
        (true, false, false),
        (false, false, true),
        (false, true, false),
        (false, false, false),
    ] {
        std::fs::write(&path, "LOBO_API_KEY=sk-old\nKEEP_THIS=yes\n").unwrap();
        let mut app = App::real();
        app.term = Term {
            stdout_tty: true,
            stdin_tty: true,
            ..Default::default()
        };
        app.local_supported = || Ok(());
        app.prompter = Arc::new(move || {
            Box::new(WizardAnswer {
                abort,
                fail,
                discard,
            })
        });
        let (code, out, err) = run(&app, &["--config", path.to_str().unwrap(), "config"]).await;
        if fail {
            assert_eq!(code, 1);
            assert_eq!(err, "error: terminal read failed\n");
        } else {
            assert_eq!(code, 0, "{err}");
        }
        let values = lobo_core::config::values(&path).unwrap();
        assert_eq!(values["KEEP_THIS"], "yes");
        if abort || discard || fail {
            assert_eq!(values["LOBO_API_KEY"], "sk-old");
            if !fail {
                assert_eq!(out, "nothing saved\n");
            }
        } else {
            assert_ne!(values["LOBO_API_KEY"], "sk-old");
            assert!(out.starts_with("saved "));
            assert!(out.contains("new LOBO_API_KEY:"));
        }
    }
}
