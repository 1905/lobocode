use chrono::{DateTime, TimeDelta, Utc};
use lobo_proto::*;
use lobocode_app::store::Store;
use std::collections::BTreeMap;
fn now() -> DateTime<Utc> {
    "2026-09-29T12:00:00Z".parse().unwrap()
}
fn config() -> (ConfigShow, Readiness) {
    let values: BTreeMap<String, String> = [
        ("RUNPOD_API_KEY", "rpa_…a1b2"),
        ("VASTAI_API_KEY", "3f9c…c0de"),
        ("LOBO_CONNECTION", "ssh"),
        ("LOBO_CLOUD_PORT", "8933"),
        ("LOBO_API_KEY", "sk-9…7e4d"),
        ("LOBO_PROVIDER", "vast"),
        ("LOBO_MODEL", "q6"),
    ]
    .into_iter()
    .map(|(k, v)| (k.into(), v.into()))
    .collect();
    let c = ConfigShow {
        path: "/Users/you/.config/lobo/config.env".into(),
        exists: true,
        set: values.keys().map(|k| (k.clone(), true)).collect(),
        values,
    };
    let r = Readiness {
        exists: true,
        cloud_ready: true,
        ready: true,
        providers: vec!["runpod".into(), "vast".into()],
        default_provider: "vast".into(),
        default_model: "q6".into(),
        ..Default::default()
    };
    (c, r)
}
fn base() -> Store {
    let mut s = Store::new();
    let (c, r) = config();
    s.apply_config(c, r);
    s.apply_snap(
        Snap {
            down: true,
            at: GoTime::from_utc(now()),
            ..Default::default()
        },
        now(),
    );
    s
}
fn running() -> Snap {
    Snap {
        pod: Some(Instance {
            provider: "runpod".into(),
            id: "rysv8058qqhsqc".into(),
            status: "RUNNING".into(),
            detail: "COMMUNITY, ≥5000 Mbps".into(),
            cost_per_hr: 0.69,
            started_at: GoTime::from_utc(now() - TimeDelta::seconds(8342)),
            host_download_mbps: 5000,
            api_url: "http://127.0.0.1:8933/v1".into(),
            ..Default::default()
        }),
        version: Some(Manifest {
            version: "2026.09.25-10".into(),
            git_sha: "4b513b3".into(),
            ..Default::default()
        }),
        status: Some(Status {
            stage: Stage::Ready,
            uptime_s: 8342,
            idle_s: 180,
            kill_in_s: 1634,
            kill_reason: "idle".into(),
            gpu: Some(Gpu {
                name: "NVIDIA GeForce RTX 5090".into(),
                vram_used_mb: 23347,
                vram_total_mb: 32607,
                util_pct: 87,
            }),
            llama: Some(Llama {
                requests_processing: 1,
                gen_tps: 45.1,
                prompt_tps: 503.9,
                prompt_tokens_total: 857350,
                gen_tokens_total: 115498,
                ..Default::default()
            }),
            model: "q6".into(),
            ctx: 65536,
            ..Default::default()
        }),
        down: false,
        at: GoTime::from_utc(now()),
    }
}
fn boot() -> Store {
    let mut s = base();
    let t0 = now() - TimeDelta::seconds(72);
    s.begin_up(t0);
    for (phase, seconds) in [
        ("create", 8),
        ("image", 41),
        ("tunnel", 43),
        ("gpu", 44),
        ("download", 45),
    ] {
        s.handle_event(
            &UpEvent {
                phase: phase.into(),
                ..Default::default()
            },
            t0 + TimeDelta::seconds(seconds),
        );
    }
    s.handle_event(
        &UpEvent {
            phase: "download".into(),
            detail: "vast 26461230, offer 51401937, 18877 Mbps down, California, US, $0.73/h"
                .into(),
            download: Some(DownloadProgress {
                bytes: 12_400_000_000,
                total: 22_082_528_352,
                verifying: true,
                source: "Docker image".into(),
                ..Default::default()
            }),
            ..Default::default()
        },
        now(),
    );
    s.apply_snap(
        Snap {
            pod: Some(Instance {
                provider: "vast".into(),
                id: "26461230".into(),
                status: "running".into(),
                detail: "offer 51401937, 18877 Mbps down, California, US".into(),
                cost_per_hr: 0.73,
                started_at: GoTime::from_utc(t0),
                host_download_mbps: 18877,
                ..Default::default()
            }),
            at: GoTime::from_utc(now()),
            ..Default::default()
        },
        now(),
    );
    s
}
fn fixtures() -> Vec<(&'static str, Store)> {
    let mut ready = base();
    ready.apply_snap(running(), now());
    let mut fail = base();
    fail.begin_up(now());
    for detail in [
        "Docker image model shard failed SHA-256 verification; instance deleted",
        "create: bad host, renting another pod (4/4)",
        "vast 26461301, offer 51399812, 9129 Mbps down, Quebec, CA",
    ] {
        fail.handle_event(
            &UpEvent {
                phase: "failed".into(),
                detail: detail.into(),
                ..Default::default()
            },
            now(),
        );
    }
    fail.up_ended(None);
    fail.stop_failed("Startup failed on four GPU hosts. All four instances were deleted.".into());
    let mut stopping = base();
    stopping.begin_stop();
    let mut out = vec![
        ("off", base()),
        ("boot", boot()),
        ("loading", Store::new()),
        ("ready", ready),
        ("fail", fail),
        ("stopping", stopping),
    ];
    let mut setup = Store::new();
    setup.apply_config(
        ConfigShow {
            path: "/Users/you/.config/lobo/config.env".into(),
            ..Default::default()
        },
        Readiness {
            default_provider: "runpod".into(),
            default_model: "q6".into(),
            ..Default::default()
        },
    );
    setup.needs_setup();
    out.push(("setup", setup));
    for (name, providers) in [
        ("off_nokeys", vec![]),
        ("off_one_provider", vec!["runpod".to_string()]),
    ] {
        let mut s = base();
        let (mut c, mut r) = config();
        r.providers = providers;
        r.cloud_ready = !r.providers.is_empty();
        r.ready = r.cloud_ready;
        r.default_provider = "runpod".into();
        c.values.insert("LOBO_PROVIDER".into(), "runpod".into());
        c.set.insert("VASTAI_API_KEY".into(), false);
        c.set.insert("RUNPOD_API_KEY".into(), r.cloud_ready);
        s.apply_config(c, r);
        out.push((name, s));
    }
    let mut sha = boot();
    sha.handle_event(
        &UpEvent {
            phase: "verify".into(),
            download: Some(DownloadProgress {
                bytes: 12_400_000_000,
                total: 22_082_528_352,
                verifying: true,
                ..Default::default()
            }),
            ..Default::default()
        },
        now(),
    );
    out.push(("boot_verify_sha", sha));
    let mut fail = base();
    fail.apply_snap(running(), now());
    fail.stop_failed("down: delete denied".into());
    out.push(("fail_pod", fail));
    let mut warning = base();
    warning.apply_snap(running(), now());
    warning.set_warning(Some("status: connection timed out".into()));
    out.push(("ready_warning", warning));
    let mut soon = base();
    let mut snap = running();
    let st = snap.status.as_mut().unwrap();
    st.kill_in_s = 240;
    st.llama.as_mut().unwrap().requests_processing = 0;
    soon.apply_snap(snap, now());
    out.push(("ready_kill_soon", soon));
    out
}
pub fn write_render_fixtures() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../ui/src/fixtures");
    std::fs::create_dir_all(&dir).unwrap();
    // Remove only obsolete app-local fixtures owned by this generator.
    for entry in std::fs::read_dir(&dir).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with("panel_") && name.ends_with(".json") && name.contains("local") {
            std::fs::remove_file(entry.path()).unwrap();
        }
    }
    for (name, store) in fixtures() {
        let data = serde_json::json!({"name":name,"now_ms":now().timestamp_millis(),"state":store.view(now())});
        std::fs::write(
            dir.join(format!("panel_{name}.json")),
            format!("{}\n", serde_json::to_string_pretty(&data).unwrap()),
        )
        .unwrap();
    }
    let (c, r) = config();
    std::fs::write(
        dir.join("settings.json"),
        format!(
            "{}\n",
            serde_json::to_string_pretty(&serde_json::json!({"config":c,"readiness":r})).unwrap()
        ),
    )
    .unwrap();
}
#[test]
fn generate_render_fixtures() {
    write_render_fixtures();
}
#[test]
fn render_fixtures_cover_inventory() {
    let fixtures = fixtures();
    let names: std::collections::BTreeSet<_> = fixtures.iter().map(|(n, _)| *n).collect();
    assert_eq!(
        names,
        [
            "off",
            "boot",
            "ready",
            "fail",
            "setup",
            "loading",
            "stopping",
            "off_nokeys",
            "off_one_provider",
            "boot_verify_sha",
            "fail_pod",
            "ready_warning",
            "ready_kill_soon"
        ]
        .into_iter()
        .collect()
    );
    for phase in ["SCAN", "SETUP", "OFF", "BOOT", "RUN", "STOP", "FAIL"] {
        assert!(
            fixtures
                .iter()
                .any(|(_, s)| s.view(now()).phase.word() == phase),
            "{phase}"
        );
    }
    for (_, store) in fixtures {
        let value = serde_json::to_value(store.view(now())).unwrap();
        for field in ["target", "is_local", "models", "local_memory"] {
            assert!(value.get(field).is_none(), "{field}");
        }
    }
}
