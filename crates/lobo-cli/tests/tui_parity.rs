use chrono::{DateTime, TimeDelta, Utc};
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use lobo_cli::tui::{Flow, status::*, styles::to_plain, up::*};
use lobo_core::control::testkit;
use lobo_proto::{
    DownloadProgress, GoTime, Gpu, Host, Instance, Llama, Manifest, Snap, Stage, Status, UpEvent,
};

fn golden(name: &str, text: String) {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/goldens")
        .join(format!("{name}.golden"));
    assert_eq!(text, std::fs::read_to_string(path).unwrap(), "{name}");
    insta::assert_snapshot!(name, text);
}
async fn up_until(script: Vec<Option<Status>>, phase: &str) -> UpState {
    let t0: DateTime<Utc> = "2026-09-25T10:00:00Z".parse().unwrap();
    let mut state = UpState::default();
    for (i, e) in testkit::events(script, &["SECURE"])
        .await
        .iter()
        .enumerate()
    {
        let at = t0 + TimeDelta::seconds(i as i64 * 7);
        state.apply_at(e, at);
        state.at = at + TimeDelta::seconds(5);
        if e.phase == phase {
            break;
        }
    }
    state
}
#[tokio::test]
async fn up_goldens() {
    for phase in ["download", "ready"] {
        golden(
            &format!("up_{phase}"),
            to_plain(&render_up(
                &up_until(testkit::boot_script(), phase).await,
                "*",
            )),
        );
    }
    let failed = vec![
        None,
        Some(Status {
            stage: Stage::Tunnel,
            ..Default::default()
        }),
        Some(Status {
            stage: Stage::Failed,
            stage_detail: "download: sha256 mismatch".into(),
            ..Default::default()
        }),
    ];
    golden(
        "up_failed",
        to_plain(&render_up(&up_until(failed, "failed").await, "*")),
    );
}
#[tokio::test]
async fn terminal_buffers() {
    use lobo_cli::tui::run::{draw_status, draw_up};
    use ratatui::{Terminal, backend::TestBackend};
    let mut model = UpModel::new();
    model.state = up_until(testkit::boot_script(), "ready").await;
    let mut terminal = Terminal::new(TestBackend::new(120, 20)).unwrap();
    terminal.draw(|frame| draw_up(frame, &model)).unwrap();
    insta::assert_snapshot!("up_ready_terminal", terminal.backend());
    let mut model = StatusModel::default();
    model.update(StatusMsg::Snap(Ok(snap())));
    let mut terminal = Terminal::new(TestBackend::new(80, 40)).unwrap();
    terminal
        .draw(|frame| draw_status(frame, &model, &Utc))
        .unwrap();
    insta::assert_snapshot!("status_ready_terminal", terminal.backend());
}
fn snap() -> Snap {
    let at: DateTime<Utc> = "2026-09-23T12:00:00Z".parse().unwrap();
    Snap {
        at: GoTime::from_utc(at),
        pod: Some(Instance {
            provider: "runpod".into(),
            id: "3ab00vd3rf1575".into(),
            status: "RUNNING".into(),
            cost_per_hr: 0.69,
            started_at: GoTime::from_utc(at - TimeDelta::minutes(90)),
            ..Default::default()
        }),
        version: Some(Manifest {
            version: "2026.09.23-1".into(),
            git_sha: "feaaaa5".into(),
            llama_image: "ghcr.io/ggml-org/llama.cpp:server-cuda-b11118".into(),
            ..Default::default()
        }),
        status: Some(Status {
            stage: Stage::Ready,
            model: "q8".into(),
            ctx: 8192,
            idle_s: 312,
            kill_in_s: 1488,
            kill_reason: "idle".into(),
            expires_at: GoTime::from_utc(at + TimeDelta::hours(10)),
            gpu: Some(Gpu {
                name: "NVIDIA GeForce RTX 5090".into(),
                vram_used_mb: 30112,
                vram_total_mb: 32607,
                util_pct: 87,
            }),
            host: Some(Host {
                load1: 1.2,
                load5: 0.9,
                load15: 0.7,
                mem_used_mb: 9120,
                mem_total_mb: 450560,
            }),
            llama: Some(Llama {
                requests_processing: 1,
                prompt_tokens_total: 182340,
                gen_tokens_total: 21044,
                prompt_tps: 2410.5,
                gen_tps: 48.3,
                ..Default::default()
            }),
            ..Default::default()
        }),
        ..Default::default()
    }
}
#[test]
fn status_goldens() {
    golden("status_ready", to_plain(&render_status(&snap(), &Utc)));
    let mut dl = snap();
    dl.status = Some(Status {
        stage: Stage::Download,
        model: "q8".into(),
        ctx: 8192,
        kill_in_s: 43000,
        kill_reason: "expired".into(),
        expires_at: GoTime(Some(dl.at.0.unwrap() + TimeDelta::hours(12))),
        download: DownloadProgress {
            bytes: 12357400000,
            total: 28595762272,
            mbps: 51.3,
            ..Default::default()
        },
        ..Default::default()
    });
    golden("status_downloading", to_plain(&render_status(&dl, &Utc)));
    let mut na = snap();
    let st = na.status.as_mut().unwrap();
    st.gpu = None;
    st.host = None;
    st.llama = None;
    st.metrics_failures = 4;
    golden(
        "status_metrics_unavailable",
        to_plain(&render_status(&na, &Utc)),
    );
    golden(
        "status_down",
        to_plain(&render_status(
            &Snap {
                down: true,
                at: dl.at,
                ..Default::default()
            },
            &Utc,
        )),
    );
    let mut local = snap();
    let p = local.pod.as_mut().unwrap();
    p.provider = "local".into();
    p.id = "74585".into();
    p.cost_per_hr = 0.0;
    local.version.as_mut().unwrap().llama_image.clear();
    let st = local.status.as_mut().unwrap();
    st.gpu = Some(Gpu {
        name: "Apple M1 Max".into(),
        vram_used_mb: 41230,
        vram_total_mb: 65536,
        ..Default::default()
    });
    st.host = None;
    golden("status_local", to_plain(&render_status(&local, &Utc)));
}
#[test]
fn up_state_timers_and_terminal_events() {
    let at: DateTime<Utc> = "2026-09-25T10:00:00Z".parse().unwrap();
    let mut s = UpState::default();
    s.apply_at(
        &UpEvent {
            phase: "create".into(),
            ..Default::default()
        },
        at,
    );
    s.apply_at(
        &UpEvent {
            phase: "image".into(),
            ..Default::default()
        },
        at + TimeDelta::seconds(2),
    );
    s.at = at + TimeDelta::seconds(20);
    let text = to_plain(&render_up(&s, "*"));
    for want in ["0:18", "re-rent at 6:00", "2s"] {
        assert!(text.contains(want), "{text}");
    }
    s.apply_at(
        &UpEvent {
            phase: "create".into(),
            detail: "bad host, renting another pod (2/4)".into(),
            ..Default::default()
        },
        at + TimeDelta::minutes(6),
    );
    assert_eq!(s.took("image"), None);
    for phase in ["failed", "terminated", "cancelled"] {
        s.apply_at(
            &UpEvent {
                phase: phase.into(),
                err: Some("boom".into()),
                done: true,
                ..Default::default()
            },
            at + TimeDelta::minutes(7),
        );
        assert_eq!(s.phase, "create");
        assert_eq!(s.err.as_deref(), Some("boom"));
        assert!(s.done);
    }
}
#[test]
fn models_keep_errors_and_handle_keys() {
    let mut m = StatusModel::default();
    assert!(to_plain(&m.view(&Utc)).contains("loading…"));
    assert_eq!(
        m.update(StatusMsg::Snap(Err("runpod 502".into()))),
        Flow::Continue
    );
    let text = to_plain(&m.view(&Utc));
    assert!(text.contains("runpod 502"));
    assert!(text.contains("retrying every 2s"));
    m.update(StatusMsg::Snap(Ok(snap())));
    m.update(StatusMsg::Snap(Err("blip".into())));
    assert_eq!(
        m.snap.as_ref().unwrap().pod.as_ref().unwrap().id,
        "3ab00vd3rf1575"
    );
    let text = to_plain(&m.view(&Utc));
    assert!(text.contains("refresh failed: blip"));
    assert!(text.contains("q quit"));
    let _ = render_status(&Snap::default(), &Utc);
    for key in [
        KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE),
        KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
        KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
    ] {
        assert_eq!(m.update(StatusMsg::Key(key)), Flow::Quit);
    }
    assert_eq!(
        m.update(StatusMsg::Key(KeyEvent::new(
            KeyCode::Char('x'),
            KeyModifiers::NONE
        ))),
        Flow::Continue
    );
    assert_eq!(
        m.update(StatusMsg::Snap(Ok(Snap {
            down: true,
            ..Default::default()
        }))),
        Flow::Quit
    );
    let at = Utc::now();
    let mut up = UpModel::new();
    let before = to_plain(&up.view());
    up.update(
        UpMsg::Event(Some(UpEvent {
            phase: "image".into(),
            ..Default::default()
        })),
        at,
    );
    up.update(UpMsg::Tick(at + TimeDelta::seconds(1)), at);
    assert_ne!(to_plain(&up.view()), before);
    assert_eq!(up.state.at, at + TimeDelta::seconds(1));
    assert_eq!(up.update(UpMsg::Event(None), at), Flow::Quit);
    for key in [
        KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE),
        KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
    ] {
        let mut up = UpModel::new();
        assert_eq!(up.update(UpMsg::Key(key), at), Flow::Quit);
        assert!(up.interrupted);
        // Cleanup belongs to the owned operation, so a key alone cannot report it complete.
        assert_eq!(up.err(), None);
    }
    assert_eq!(
        up.update(
            UpMsg::Event(Some(UpEvent {
                done: true,
                ..Default::default()
            })),
            at
        ),
        Flow::Quit
    );
    let mut released = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE);
    released.kind = KeyEventKind::Release;
    assert_eq!(m.update(StatusMsg::Key(released)), Flow::Continue);
}
