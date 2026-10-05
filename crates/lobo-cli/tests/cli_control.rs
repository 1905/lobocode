use chrono::{TimeDelta, Utc};
use lobo_cli::{
    app::{App, Io, Term},
    bootlog::report_boot,
    cmd::up::{json_up, plain_up},
    logfmt::{SharedWriter, ZerologConsole},
};
use lobo_core::{
    clock::FixedClock,
    control::testkit::{self, FakeAgent, FakeRunPod},
    provider::runpod::Pod,
};
use lobo_proto::{ReadyInfo, Stage, Status, UpEvent};
use std::{
    io::Write,
    os::unix::fs::PermissionsExt,
    path::Path,
    sync::{Arc, Mutex},
    time::Duration,
};
use tracing::instrument::WithSubscriber;

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
fn fixture(name: &str) -> String {
    std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/go/text")
            .join(name),
    )
    .unwrap()
}
fn json_lines(text: &str) -> Vec<serde_json::Value> {
    text.lines()
        .map(|s| {
            let mut value = serde_json::from_str(s).unwrap();
            normalize_numbers(&mut value);
            value
        })
        .collect()
}
fn normalize_numbers(value: &mut serde_json::Value) {
    use serde_json::Value;
    match value {
        Value::Number(n) if n.is_f64() => {
            let f = n.as_f64().unwrap();
            // Only canonicalize exactly representable integral floats. Integer
            // fields stay integers, including values beyond f64's exact range.
            if f.fract() == 0.0 && f.abs() <= 9_007_199_254_740_992.0 {
                *value = Value::from(f as i64);
            }
        }
        Value::Array(items) => items.iter_mut().for_each(normalize_numbers),
        Value::Object(fields) => fields.values_mut().for_each(normalize_numbers),
        _ => {}
    }
}
fn no_timestamps(s: &str) -> String {
    s.lines()
        .map(|line| {
            let line = if line.len() > 9 && line.as_bytes()[2] == b':' && line.as_bytes()[5] == b':'
            {
                &line[9..]
            } else {
                line
            };
            format!("{line}\n")
        })
        .collect()
}
fn events(name: &str) -> tokio::sync::mpsc::Receiver<UpEvent> {
    let items: Vec<UpEvent> = fixture(name)
        .lines()
        .map(|s| serde_json::from_str(s).unwrap())
        .collect();
    let (tx, rx) = tokio::sync::mpsc::channel(items.len());
    for e in items {
        tx.try_send(e).unwrap();
    }
    rx
}
#[tokio::test]
async fn consumers_match_go() {
    for name in ["boot", "failed"] {
        let input = format!("json_up_{name}.jsonl");
        let mut ready = None;
        let mut out = Vec::new();
        let result = json_up(&mut events(&input), &mut out, &mut ready).await;
        assert_eq!(result.is_ok(), name == "boot");
        if name == "failed" {
            assert_eq!(result.unwrap_err().to_string(), "up failed");
        }
        assert_eq!(
            json_lines(std::str::from_utf8(&out).unwrap()),
            json_lines(&fixture(&input))
        );
        assert_eq!(ready.is_some(), name == "boot");
        let buffer = Buffer::default();
        let subscriber = tracing_subscriber::fmt()
            .event_format(ZerologConsole {
                color: false,
                tz: chrono::FixedOffset::east_opt(0).unwrap(),
            })
            .with_writer(SharedWriter(Arc::new(Mutex::new(Box::new(buffer.clone())))))
            .finish();
        let result = plain_up(&mut events(&input), &mut None)
            .with_subscriber(subscriber)
            .await;
        assert_eq!(result.is_ok(), name == "boot");
        assert_eq!(
            no_timestamps(&buffer.text()),
            no_timestamps(&fixture(&format!("plain_up_{name}.txt")))
        );
    }
}
#[test]
fn boot_report_and_append_match_go() {
    let want: serde_json::Value = serde_json::from_str(&fixture("boots_line.json")).unwrap();
    let mut ready: ReadyInfo = serde_json::from_value(want["ready"].clone()).unwrap();
    let now = "2026-09-25T10:00:00Z".parse().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("boots.jsonl");
    let mut out = Vec::new();
    for _ in 0..2 {
        report_boot(&mut out, Some(&ready), "r2", 4, &path, now).unwrap();
    }
    assert_eq!(
        String::from_utf8(out).unwrap(),
        fixture("report_boot.txt").repeat(2)
    );
    let data = std::fs::read_to_string(&path).unwrap();
    assert_eq!(json_lines(&data), vec![want.clone(), want]);
    for line in data.lines() {
        let indices = ["\"at\":", "\"conns\":", "\"ready\":", "\"source\":"]
            .map(|key| line.find(key).unwrap());
        assert!(indices.windows(2).all(|p| p[0] < p[1]));
    }
    let none = tmp.path().join("none");
    let mut out = Vec::new();
    report_boot(&mut out, None, "", 0, &none, now).unwrap();
    ready.timings = None;
    report_boot(&mut out, Some(&ready), "", 0, &none, now).unwrap();
    assert!(out.is_empty());
    assert!(!none.exists());
}
struct Fixture {
    tmp: tempfile::TempDir,
    app: App,
    rp: Arc<FakeRunPod>,
}
impl Fixture {
    fn new(script: Vec<Option<Status>>, running: bool) -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let config = tmp.path().join("config.env");
        std::fs::write(&config, "LOBO_API_KEY=sk\nRUNPOD_API_KEY=rp\nLOBO_DOMAIN=lobo.example.com\nCF_TUNNEL_TOKEN=tok\nLOBO_BUCKET_URL=https://pub-x.r2.dev\n").unwrap();
        std::fs::set_permissions(&config, std::fs::Permissions::from_mode(0o600)).unwrap();
        let rp = Arc::new(FakeRunPod::default());
        let at = "2026-09-25T10:00:00Z"
            .parse::<chrono::DateTime<Utc>>()
            .unwrap();
        if running {
            rp.state.lock().unwrap().pods.push(Pod {
                id: "pod1".into(),
                name: "lobo".into(),
                desired_status: "RUNNING".into(),
                cost_per_hr: 0.69,
                created_at: lobo_core::provider::runpod::RunPodTime(Some(at - TimeDelta::hours(1))),
                ..Default::default()
            });
        }
        let ag = Arc::new(FakeAgent::new(script));
        let recorder = rp.clone();
        let mut app = App::real();
        app.term = Term::default();
        app.boot_log = tmp.path().join("boots.jsonl");
        app.clock = Arc::new(FixedClock(at));
        let clock = app.clock.clone();
        app.deps =
            Arc::new(move |_, _| Ok(testkit::deps(recorder.clone(), ag.clone(), clock.clone())));
        Self { tmp, app, rp }
    }
    async fn invoke(&self, args: &[&str]) -> (i32, String, String) {
        let out = Buffer::default();
        let err = Buffer::default();
        let code = self
            .invoke_with(args, Box::new(out.clone()), Box::new(err.clone()))
            .await;
        (code, out.text(), err.text())
    }
    async fn invoke_with(
        &self,
        args: &[&str],
        out: Box<dyn Write + Send>,
        err: Box<dyn Write + Send>,
    ) -> i32 {
        let config = self.tmp.path().join("config.env");
        let argv = ["lobo", "--config", config.to_str().unwrap()]
            .into_iter()
            .chain(args.iter().copied())
            .map(Into::into)
            .collect();
        lobo_cli::run(
            &self.app,
            argv,
            &mut Io {
                out,
                err,
                input: Box::new(std::io::empty()),
            },
        )
        .await
    }
}
#[tokio::test]
async fn up_command_modes_defaults_and_report() {
    for flag in ["--json", "--plain", ""] {
        let mut f = Fixture::new(testkit::boot_script(), false);
        let rp = f.rp.clone();
        let agent = Arc::new(FakeAgent::new(testkit::boot_script()));
        f.app.deps = Arc::new(move |_, _| {
            Ok(testkit::deps(
                rp.clone(),
                agent.clone(),
                Arc::new(FixedClock("2026-09-23T10:00:00Z".parse().unwrap())),
            ))
        });
        let args = if flag.is_empty() {
            vec!["up", "--q6=false"]
        } else {
            vec!["up", flag, "--q6=false"]
        };
        let (code, out, err) = f.invoke(&args).await;
        assert_eq!(code, 0, "{err}");
        let s = f.rp.state.lock().unwrap();
        assert_eq!(s.created[0].opts.model, "q8");
        assert_eq!(s.created[0].opts.ctx, 65536);
        assert_eq!(s.created[0].opts.image, "public-image-q8@sha256:fixture");
        assert!(s.deleted.is_empty());
        // Frozen Go output still checks the event contract. Rust cloud starts
        // use built-in defaults and resolve an image before an agent version exists.
        // Rust also reports initial agent reachability. Keep the frozen consumer
        // fixtures unchanged, and assert this intentional producer difference.
        // A fixed clock keeps this command test independent of clock-read counts.
        let adapt =
            |text: String| text.replace(", release 2026.09.23-1, q8 ctx 8192", ", q8 ctx 65536");
        if flag == "--json" {
            let mut expected = json_lines(&adapt(fixture("json_up_boot.jsonl")));
            assert_eq!(expected[1]["phase"], "image");
            expected[1]["detail"] = "agent unreachable (other); ready unconfirmed".into();
            expected.last_mut().unwrap()["ready"]["elapsed_ns"] = 0.into();
            assert_eq!(json_lines(&out), expected);
        } else {
            assert!(out.is_empty());
            let expected = adapt(fixture("plain_up_boot.txt"))
                .replace("boot=15000", "boot=0")
                .replace(
                    "INF up phase=image",
                    "INF up detail=\"agent unreachable (other); ready unconfirmed\" phase=image",
                );
            assert!(
                no_timestamps(&err).starts_with(&no_timestamps(&expected)),
                "{err}"
            );
        }
        assert!(err.contains("boot timings:"));
        assert!(f.app.boot_log.exists());
    }
    let f = Fixture::new(testkit::boot_script(), false);
    let (code, _, err) = f.invoke(&["up", "--q6", "--json"]).await;
    assert_eq!(code, 0, "{err}");
    assert_eq!(f.rp.state.lock().unwrap().created[0].opts.model, "q6");
    let f = Fixture::new(testkit::boot_script(), false);
    let (code, _, err) = f.invoke(&["up", "--ssh", "/does/not/exist"]).await;
    assert_eq!(code, 1);
    assert!(!err.is_empty());
    assert!(f.rp.state.lock().unwrap().created.is_empty());
}
#[tokio::test]
async fn status_and_down_commands_match_go() {
    for running in [false, true] {
        let f = Fixture::new(
            vec![Some(Status {
                stage: Stage::Ready,
                ..Default::default()
            })],
            running,
        );
        let (code, out, err) = f.invoke(&["status", "--json"]).await;
        assert_eq!(code, 0, "{err}");
        assert_eq!(
            json_lines(&out),
            json_lines(&fixture(if running {
                "status_running.json"
            } else {
                "status_down.json"
            }))
        );
        let (code, out, err) = f.invoke(&["status", "--once"]).await;
        assert_eq!(code, 0, "{err}");
        assert!(out.starts_with("lobo status"));
        assert!(!out.contains('\x1b'));
        let (code, out, err) = f.invoke(&["down", "--json"]).await;
        assert_eq!(code, 0, "{err}");
        if running {
            assert_eq!(json_lines(&out), json_lines(&fixture("down_running.json")));
            assert_eq!(f.rp.state.lock().unwrap().deleted, ["pod1"]);
        }
        let (code, _, err) = f.invoke(&["down"]).await;
        assert_eq!(code, 0);
        assert_eq!(
            no_timestamps(&err),
            "INF down: no lobo pods left spent=$0.00\n"
        );
    }
    let mut f = Fixture::new(vec![Some(Status::default())], true);
    f.app.term.stdout_tty = true;
    let (_, out, _) = f.invoke(&["status", "--once"]).await;
    assert!(out.contains('\x1b'));
    f.app.term.no_color = true;
    let (_, out, _) = f.invoke(&["status", "--once"]).await;
    assert!(!out.contains('\x1b'));
}
#[tokio::test]
async fn failed_up_keeps_json_error_and_reports_nonzero() {
    let f = Fixture::new(
        vec![Some(Status {
            stage: Stage::Failed,
            stage_detail: "download: sha256 mismatch".into(),
            ..Default::default()
        })],
        false,
    );
    let (code, out, err) = f.invoke(&["up", "--json"]).await;
    assert_eq!(code, 1);
    assert_eq!(err, "error: up failed\n");
    let events = json_lines(&out);
    assert!(
        events.last().unwrap()["err"]
            .as_str()
            .unwrap()
            .contains("sha256 mismatch")
    );
    // Failed cloud agents own self-deletion; preserve that Go behavior.
    assert!(f.rp.state.lock().unwrap().deleted.is_empty());
}

struct SlowApi {
    inner: Arc<FakeRunPod>,
    create_started: tokio::sync::Notify,
    delete_started: tokio::sync::Notify,
    create_delay: Duration,
    delete_delay: Duration,
    fail_delete: bool,
}
#[async_trait::async_trait]
impl lobo_core::provider::runpod::RunPodApi for SlowApi {
    async fn create(
        &self,
        o: &lobo_core::provider::CreateOpts,
        cloud: &str,
        mbps: f64,
    ) -> lobo_core::Result<Pod> {
        self.create_started.notify_one();
        tokio::time::sleep(self.create_delay).await;
        self.inner.create(o, cloud, mbps).await
    }
    async fn list(&self) -> lobo_core::Result<Vec<Pod>> {
        self.inner.list().await
    }
    async fn get(&self, id: &str) -> lobo_core::Result<Pod> {
        self.inner.get(id).await
    }
    async fn delete(&self, id: &str) -> lobo_core::Result<()> {
        self.delete_started.notify_one();
        tokio::time::sleep(self.delete_delay).await;
        if self.fail_delete {
            return Err(lobo_core::Error::Other("delete denied".into()));
        }
        self.inner.delete(id).await
    }
}
fn slow(
    f: &mut Fixture,
    create_delay: Duration,
    delete_delay: Duration,
    fail_delete: bool,
) -> Arc<SlowApi> {
    let api = Arc::new(SlowApi {
        inner: f.rp.clone(),
        create_started: Default::default(),
        delete_started: Default::default(),
        create_delay,
        delete_delay,
        fail_delete,
    });
    let injected = api.clone();
    let rp = f.rp.clone();
    let clock = f.app.clock.clone();
    f.app.deps = Arc::new(move |_, _| {
        let mut d = testkit::deps(
            rp.clone(),
            Arc::new(FakeAgent::new(vec![None])),
            clock.clone(),
        );
        d.providers.insert(
            "runpod".into(),
            Arc::new(lobo_core::provider::runpod::RunPodProvider {
                api: injected.clone(),
                domain: "lobo.example.com".into(),
            }),
        );
        Ok(d)
    });
    api
}
#[tokio::test(start_paused = true)]
async fn cancel_awaits_a_121_second_create_and_reports_cleanup_failure() {
    for fail_delete in [false, true] {
        let mut f = Fixture::new(vec![None], false);
        let api = slow(
            &mut f,
            Duration::from_secs(121),
            Duration::ZERO,
            fail_delete,
        );
        let cancel = f.app.cancel.clone();
        let before = tokio::time::Instant::now();
        let cancelled = async {
            api.create_started.notified().await;
            cancel.cancel();
        };
        let ((code, out, err), ()) = tokio::join!(f.invoke(&["up", "--json"]), cancelled);
        assert!(before.elapsed() >= Duration::from_secs(121));
        assert_eq!(code, 1);
        let state = f.rp.state.lock().unwrap();
        assert_eq!(state.created.len(), 1);
        if fail_delete {
            assert!(err.contains("delete denied"), "{err}");
            assert!(!err.contains("cleanup completed"));
            assert_eq!(state.pods.len(), 1);
            assert_eq!(json_lines(&out).last().unwrap()["phase"], "failed");
        } else {
            assert!(state.pods.is_empty());
            assert_eq!(state.deleted, ["pod1"]);
            assert_eq!(
                err,
                "error: interrupted: startup cancelled and cleanup completed\n"
            );
            assert_eq!(json_lines(&out).last().unwrap()["phase"], "cancelled");
        }
    }
}
struct BrokenOutput;
impl Write for BrokenOutput {
    fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
        Err(std::io::Error::new(
            std::io::ErrorKind::BrokenPipe,
            "test output closed",
        ))
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
#[tokio::test(start_paused = true)]
async fn broken_output_still_waits_for_owned_cleanup() {
    let mut f = Fixture::new(vec![None], false);
    slow(&mut f, Duration::ZERO, Duration::from_secs(10), false);
    let err = Buffer::default();
    let before = tokio::time::Instant::now();
    let code = f
        .invoke_with(
            &["up", "--json"],
            Box::new(BrokenOutput),
            Box::new(err.clone()),
        )
        .await;
    assert_eq!(code, 1);
    assert!(err.text().contains("test output closed"), "{}", err.text());
    assert!(before.elapsed() >= Duration::from_secs(10));
    let state = f.rp.state.lock().unwrap();
    assert!(state.pods.is_empty());
    assert_eq!(state.deleted, ["pod1"]);
}
#[tokio::test(start_paused = true)]
async fn down_keeps_issued_deletes_owned_after_ctrl_c() {
    let mut f = Fixture::new(vec![None], true);
    let api = slow(&mut f, Duration::ZERO, Duration::from_secs(121), false);
    let cancel = f.app.cancel.clone();
    let before = tokio::time::Instant::now();
    let cancelled = async {
        api.delete_started.notified().await;
        cancel.cancel();
    };
    let ((code, out, err), ()) = tokio::join!(f.invoke(&["down", "--json"]), cancelled);
    assert_eq!(code, 0, "{err}");
    assert!(before.elapsed() >= Duration::from_secs(121));
    assert_eq!(json_lines(&out), json_lines(&fixture("down_running.json")));
    assert!(f.rp.state.lock().unwrap().pods.is_empty());
}
