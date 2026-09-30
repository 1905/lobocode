use super::*;
use lobo_proto::{DownloadProgress, GoTime, Instance, Llama, ModelState, ReadyInfo, Status};

fn now() -> DateTime<Utc> {
    "2026-09-29T12:00:00Z".parse().unwrap()
}
fn snap(down: bool, stage: Option<Stage>) -> Snap {
    Snap {
        down,
        at: GoTime::from_utc(now()),
        pod: (!down).then(|| Instance {
            provider: "runpod".into(),
            ..Default::default()
        }),
        status: stage.map(|stage| Status {
            stage,
            ..Default::default()
        }),
        ..Default::default()
    }
}
fn config() -> (ConfigShow, Readiness) {
    (
        ConfigShow {
            exists: true,
            ..Default::default()
        },
        Readiness {
            exists: true,
            ready: true,
            local_supported: true,
            default_provider: "runpod".into(),
            default_model: "q8".into(),
            local_port: 8931,
            ..Default::default()
        },
    )
}
fn models() -> Listing {
    Listing {
        models: vec![
            ModelState {
                id: "q6".into(),
                size: 100,
                on_disk: 100,
                ..Default::default()
            },
            ModelState {
                id: "q8".into(),
                size: 100,
                on_disk: 43,
                ..Default::default()
            },
        ],
        ..Default::default()
    }
}
#[test]
fn derive_swift_rows() {
    for (down, stage, up, current, want) in [
        (true, None, false, Phase::Loading, Phase::Off),
        (true, None, true, Phase::Booting, Phase::Booting),
        (false, None, false, Phase::Off, Phase::Booting),
        (
            false,
            Some(Stage::Download),
            false,
            Phase::Off,
            Phase::Booting,
        ),
        (
            false,
            Some(Stage::Ready),
            false,
            Phase::Booting,
            Phase::Ready,
        ),
        (
            false,
            Some(Stage::Failed),
            true,
            Phase::Booting,
            Phase::Booting,
        ),
        (
            false,
            Some(Stage::Failed),
            false,
            Phase::Booting,
            Phase::Failed {
                message: "failed".into(),
            },
        ),
        (
            true,
            None,
            false,
            Phase::Failed {
                message: "x".into(),
            },
            Phase::Failed {
                message: "x".into(),
            },
        ),
        (
            false,
            Some(Stage::Ready),
            false,
            Phase::Stopping,
            Phase::Stopping,
        ),
    ] {
        assert_eq!(Store::derive(&snap(down, stage), up, &current), want);
    }
}
#[test]
fn poll_intervals() {
    let mut s = Store::new(None, true);
    for (phase, open, closed) in [
        (Phase::Loading, 3, 3),
        (Phase::Booting, 3, 3),
        (Phase::Stopping, 3, 3),
        (Phase::Ready, 5, 15),
        (Phase::Off, 10, 30),
    ] {
        s.state.phase = phase;
        s.set_panel_open(false);
        assert_eq!(s.poll_interval().as_secs(), closed);
        s.set_panel_open(true);
        assert_eq!(s.poll_interval().as_secs(), open);
    }
}
#[test]
fn target_defaults_and_model_autopick() {
    let (bare, r) = config();
    let mut keyed = bare.clone();
    keyed.set.insert("RUNPOD_API_KEY".into(), true);
    let mut wants_local = keyed.clone();
    wants_local
        .values
        .insert("LOBO_PROVIDER".into(), "local".into());
    let models = models();
    let mut empty = models.clone();
    for m in &mut empty.models {
        m.on_disk = 0;
    }
    for (saved, cfg, listing, supported, want) in [
        (None, &wants_local, None, true, Target::Local),
        (None, &bare, Some(&models), true, Target::Local),
        (None, &bare, Some(&empty), true, Target::Cloud),
        (None, &keyed, Some(&models), true, Target::Cloud),
        (
            Some(Target::Cloud),
            &wants_local,
            Some(&models),
            true,
            Target::Cloud,
        ),
        (Some(Target::Local), &keyed, None, true, Target::Local),
        (
            Some(Target::Local),
            &wants_local,
            Some(&models),
            false,
            Target::Cloud,
        ),
    ] {
        assert_eq!(
            Store::default_target(saved, None, Some(cfg), listing, supported),
            want
        );
    }
    let mut s = Store::new(None, true);
    s.apply_config(bare.clone(), r.clone());
    s.apply_models(models.clone());
    assert_eq!(s.view(now()).model, "q6");
    assert_eq!(s.view(now()).target, Target::Local);
    s.set_model("q8".into());
    s.apply_models(models.clone());
    assert_eq!(s.view(now()).model, "q8");
    let mut cfg = bare;
    cfg.values.insert("LOBO_MODEL".into(), "q8".into());
    s.apply_config(cfg, r);
    s.apply_models(models);
    assert_eq!(s.view(now()).model, "q8");
}
#[test]
fn config_does_not_reset_running_choices() {
    let mut s = Store::new(None, true);
    s.set_provider("vast".into());
    s.set_model("q6".into());
    s.begin_up(now());
    let (c, r) = config();
    s.apply_config(c, r);
    let v = s.view(now());
    assert_eq!(v.provider, "vast");
    assert_eq!(v.model, "q6");
}
#[test]
fn up_explicit_requests_and_menu_progress() {
    let mut s = Store::new(None, true);
    s.choose(Target::Local);
    s.set_model("q6".into());
    let req = s.begin_up(now());
    assert_eq!(req.provider.as_deref(), Some("local"));
    assert_eq!(req.model.as_deref(), Some("q6"));
    assert_eq!(s.view(now()).last_detail, "starting llama.cpp…");
    for p in ["create", "gpu"] {
        s.handle_event(
            &UpEvent {
                phase: p.into(),
                ..Default::default()
            },
            now(),
        );
    }
    let v = s.view(now());
    assert_eq!(v.current_step, Some(Step::Gpu));
    assert_eq!(v.menu_text, "metal");
    assert_eq!(v.boot_progress, 0.2);
    s.handle_event(
        &UpEvent {
            phase: "verify".into(),
            ..Default::default()
        },
        now(),
    );
    assert_eq!(s.view(now()).up_phase.as_deref(), Some("verify"));
    let mut s = Store::new(None, true);
    s.set_provider("vast".into());
    let req = s.begin_up(now());
    assert_eq!(req.model.as_deref(), Some("q8"));
    assert_eq!(req.provider.as_deref(), Some("vast"));
    s.handle_event(
        &UpEvent {
            phase: "download".into(),
            download: Some(DownloadProgress {
                bytes: 50,
                total: 100,
                ..Default::default()
            }),
            ..Default::default()
        },
        now(),
    );
    let v = s.view(now());
    assert_eq!(v.menu_text, "50%");
    assert_eq!(v.boot_progress, 4.5 / 7.0);
    let notes = s.handle_event(
        &UpEvent {
            phase: "ready".into(),
            ready: Some(ReadyInfo {
                url: "https://x/v1".into(),
                cost_per_hr: 0.7,
                ..Default::default()
            }),
            ..Default::default()
        },
        now(),
    );
    assert_eq!(
        notes,
        [Note {
            title: "lobo ready".into(),
            body: "https://x/v1 · 0:00 · $0.70/h".into()
        }]
    );
    assert_eq!(s.view(now()).phase, Phase::Ready);
    assert_eq!(s.view(now()).endpoint.as_deref(), Some("https://x/v1"));
}
#[test]
fn menu_ready_freezes_processing_countdown() {
    let cases: serde_json::Value =
        serde_json::from_str(include_str!("../../../ui/src/fixtures/time_cases.json")).unwrap();
    for c in cases["kill_left"].as_array().unwrap() {
        let mut s = Store::new(None, true);
        let mut p = snap(false, Some(Stage::Ready));
        let st = p.status.as_mut().unwrap();
        st.kill_in_s = c["kill_in_s"].as_i64().unwrap();
        st.llama = Some(Llama {
            requests_processing: c["processing"].as_i64().unwrap(),
            ..Default::default()
        });
        s.apply_snap(p, now());
        assert_eq!(
            s.view(now() + chrono::TimeDelta::seconds(c["since_snap_s"].as_i64().unwrap()))
                .menu_text,
            c["menu"].as_str().unwrap()
        );
    }
    let mut s = Store::new(None, true);
    let mut p = snap(false, Some(Stage::Ready));
    p.status.as_mut().unwrap().llama = Some(Llama {
        requests_processing: 1,
        gen_tps: 45.1,
        ..Default::default()
    });
    s.apply_snap(p, now());
    assert_eq!(s.view(now()).menu_text, "45 t/s");
}
#[test]
fn endpoint_prefers_running_state() {
    let mut s = Store::new(None, true);
    let (mut c, mut r) = config();
    c.values.insert("LOBO_DOMAIN".into(), "lobo.x.cc".into());
    r.local_port = 9000;
    s.apply_config(c, r);
    s.choose(Target::Local);
    assert_eq!(
        s.view(now()).endpoint.as_deref(),
        Some("http://127.0.0.1:9000/v1")
    );
    s.choose(Target::Cloud);
    let mut p = snap(false, Some(Stage::Ready));
    p.pod.as_mut().unwrap().provider = "local".into();
    p.pod.as_mut().unwrap().api_url = "http://127.0.0.1:8931/v1".into();
    s.apply_snap(p, now());
    assert!(s.view(now()).is_local);
    assert_eq!(
        s.view(now()).endpoint.as_deref(),
        Some("http://127.0.0.1:8931/v1")
    );
}
#[test]
fn apply_snap_resume_stop_note_and_cleanup() {
    let mut s = Store::new(None, true);
    let mut p = snap(false, Some(Stage::Download));
    p.pod.as_mut().unwrap().started_at = GoTime::from_utc(now() - chrono::TimeDelta::seconds(72));
    s.apply_snap(p, now());
    let v = s.view(now());
    assert_eq!(v.boot_start_ms, Some(now().timestamp_millis() - 72000));
    assert_eq!(v.steps.len(), 5);
    assert!(v.steps.iter().all(|s| s.at_s == 72.0));
    s.apply_snap(snap(false, Some(Stage::Ready)), now());
    let v = s.view(now());
    assert!(v.steps.is_empty() && v.download.is_none() && v.boot_start_ms.is_none());
    assert_eq!(
        s.apply_snap(snap(true, None), now()),
        [Note {
            title: "lobo stopped".into(),
            body: "stopped by itself (idle or expiry)".into()
        }]
    );
    s.apply_snap(snap(false, Some(Stage::Ready)), now());
    s.begin_stop();
    assert!(s.apply_snap(snap(true, None), now()).is_empty());
    assert_eq!(s.view(now()).phase, Phase::Stopping);
    s.stop_done();
    assert_eq!(s.view(now()).phase, Phase::Off);
}
#[test]
fn stop_failure_and_dismiss_preserve_owned_cleanup() {
    let mut s = Store::new(None, true);
    s.begin_up(now());
    s.begin_stop();
    s.dismiss();
    s.apply_snap(snap(false, Some(Stage::Ready)), now());
    assert_eq!(s.view(now()).phase, Phase::Stopping);
    let notes = s.handle_event(
        &UpEvent {
            phase: "ready".into(),
            ready: Some(ReadyInfo::default()),
            ..Default::default()
        },
        now(),
    );
    assert!(notes.is_empty());
    assert_eq!(s.view(now()).phase, Phase::Stopping);
    s.up_ended(Some("cancelled"));
    s.stop_failed("down: x".into());
    s.apply_snap(snap(false, Some(Stage::Ready)), now());
    assert_eq!(
        s.view(now()).phase,
        Phase::Failed {
            message: "down: x".into()
        }
    );
    s.dismiss();
    assert_eq!(s.view(now()).phase, Phase::Off);
}
#[test]
fn up_errors_and_tail_limits() {
    let mut s = Store::new(None, true);
    s.begin_up(now());
    for i in 0..12 {
        s.handle_event(
            &UpEvent {
                phase: "load".into(),
                detail: i.to_string(),
                ..Default::default()
            },
            now(),
        );
    }
    assert_eq!(s.view(now()).log_tail.len(), 6);
    s.handle_event(
        &UpEvent {
            phase: "failed".into(),
            err: Some("a\nb".into()),
            ..Default::default()
        },
        now(),
    );
    assert_eq!(s.view(now()).log_tail.len(), 8);
    assert_eq!(
        s.up_ended(Some("boom")),
        [Note {
            title: "lobo boot failed".into(),
            body: "b".into()
        }]
    );
    s.poll_failed("offline".into());
    assert_eq!(
        s.view(now()).phase,
        Phase::Failed {
            message: "b".into()
        }
    );
    assert_eq!(s.view(now()).warning.as_deref(), Some("offline"));
    let mut s = Store::new(None, true);
    s.poll_failed("offline".into());
    assert_eq!(s.view(now()).phase, Phase::Off);
}

#[test]
fn memory_selection_config_and_latest_request_discard_stale_results() {
    for change in ["model", "target", "config", "request"] {
        let mut store = Store::new(Some(Target::Local), true);
        let (cfg, readiness) = config();
        store.apply_config(cfg.clone(), readiness.clone());
        store.apply_snap(snap(true, None), now());
        let (generation, request, model) = store.begin_memory().unwrap();
        let memory: LocalMemory = lobo_core::local::memory::assess(
            &model,
            8192,
            &lobo_core::local::memory::MemorySnapshot {
                total_bytes: 64 << 30,
                available_bytes: 60 << 30,
                metal_limit_bytes: 48 << 30,
            },
        )
        .unwrap()
        .into();
        match change {
            "model" => store.set_model("q6".into()),
            "target" => store.choose(Target::Cloud),
            "config" => {
                let mut cfg = cfg;
                cfg.values.insert("LOBO_CTX".into(), "65536".into());
                store.apply_config(cfg, readiness);
            }
            "request" => {
                store.begin_memory().unwrap();
            }
            _ => unreachable!(),
        }
        store.apply_memory(generation, request, memory);
        assert_eq!(store.view(now()).local_memory, None, "{change}");
    }
}

#[test]
fn unchanged_config_polling_does_not_invalidate_captured_start() {
    let mut store = Store::new(Some(Target::Local), true);
    let (cfg, readiness) = config();
    store.apply_config(cfg.clone(), readiness.clone());
    let submission = store.submit(now());
    store.apply_config(cfg.clone(), readiness.clone());
    assert!(store.submission_valid(&submission));
    store.invalidate_config();
    assert!(!store.submission_valid(&submission));
}

#[test]
fn immutable_runtime_is_private_and_controls_running_identity() {
    let mut store = Store::new(Some(Target::Cloud), true);
    let owner = lobo_core::control::RuntimeTarget {
        provider: "local".into(),
        instance_id: Some("4242".into()),
        boot_id: "private-boot".into(),
        local_pid: Some(4242),
        local_start_id: Some(100),
        agent_url: None,
        api_url: None,
    };
    store.set_runtime(Some(owner.clone()));
    let mut status = snap(false, Some(Stage::Ready));
    status.pod.as_mut().unwrap().provider = "local".into();
    status.pod.as_mut().unwrap().id = "4242".into();
    status.status.as_mut().unwrap().boot_id = "private-boot".into();
    store.apply_snap(status, now());
    store.handle_event(
        &UpEvent {
            phase: "create".into(),
            detail: "local 4242 is starting".into(),
            err: Some("boot private-boot".into()),
            ..Default::default()
        },
        now(),
    );
    store.choose(Target::Cloud);
    store.set_provider("vastai".into());
    assert_eq!(store.runtime(), Some(owner));
    assert!(store.view(now()).is_local);
    let json = serde_json::to_string(&store.view(now())).unwrap();
    assert!(json.contains("private-boot"));
    assert!(json.contains("4242"));
    assert!(!json.contains("local_start_id"));
    assert!(!json.contains("local_pid"));
    assert!(!json.contains("\"runtime\""));
    assert!(!json.contains("runtime_generation"));
    store.set_runtime(None);
    let json = serde_json::to_string(&store.view(now())).unwrap();
    assert!(json.contains("private-boot"));
    assert!(json.contains("4242"));
}
