use chrono::{DateTime, TimeDelta, Utc};
use lobo_proto::*;
use lobocode_app::{
    store::Store,
    types::{Phase, Target},
};
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
        ("LOBO_BUCKET_URL", "https://pub-….r2.dev"),
        ("LOBO_MIN_MBPS", "100"),
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
        local_supported: true,
        providers: vec!["runpod".into(), "vast".into()],
        default_provider: "vast".into(),
        default_model: "q8".into(),
        local_port: 8931,
        error: None,
    };
    (c, r)
}
fn models() -> Listing {
    Listing {
        weights: "/Volumes/Extreme/_lobocode".into(),
        free_bytes: 958_902_697_984,
        models: vec![
            ModelState {
                id: "q6".into(),
                file: "Qwen3.5-27B-Uncensored-HauhauCS-Aggressive-Q6_K.gguf".into(),
                size: 22_082_528_352,
                on_disk: 22_082_528_352,
                verified: true,
            },
            ModelState {
                id: "q8".into(),
                file: "Qwen3.5-27B-Uncensored-HauhauCS-Aggressive-Q8_0.gguf".into(),
                size: 28_595_762_272,
                on_disk: 12_300_000_000,
                verified: false,
            },
        ],
        runtime: RuntimeInfo {
            version: "b11118".into(),
            present: true,
        },
    }
}
fn base() -> Store {
    let mut s = Store::new(None, true);
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
fn running(local: bool) -> Snap {
    Snap {
        pod: Some(Instance {
            provider: if local { "local" } else { "runpod" }.into(),
            id: if local { "41234" } else { "rysv8058qqhsqc" }.into(),
            status: if local { "running" } else { "RUNNING" }.into(),
            detail: if local {
                "this Mac, q6"
            } else {
                "COMMUNITY, ≥5000 Mbps"
            }
            .into(),
            cost_per_hr: if local { 0.0 } else { 0.69 },
            started_at: GoTime::from_utc(
                now() - TimeDelta::seconds(if local { 2400 } else { 8342 }),
            ),
            host_download_mbps: if local { 0 } else { 5000 },
            api_url: if local {
                "http://127.0.0.1:8931/v1"
            } else {
                ""
            }
            .into(),
            ..Default::default()
        }),
        version: Some(Manifest {
            version: if local { "b11118" } else { "2026.09.25-10" }.into(),
            git_sha: if local { "local" } else { "4b513b3" }.into(),
            ..Default::default()
        }),
        status: Some(Status {
            stage: Stage::Ready,
            uptime_s: if local { 2400 } else { 8342 },
            idle_s: if local { 166 } else { 180 },
            kill_in_s: 1634,
            kill_reason: "idle".into(),
            gpu: Some(Gpu {
                name: if local {
                    "Apple M1 Max"
                } else {
                    "NVIDIA GeForce RTX 5090"
                }
                .into(),
                vram_used_mb: if local { 23654 } else { 29316 },
                vram_total_mb: if local { 65536 } else { 32607 },
                util_pct: if local { 0 } else { 87 },
            }),
            llama: Some(Llama {
                requests_processing: if local { 0 } else { 1 },
                gen_tps: if local { 11.8 } else { 45.1 },
                prompt_tps: if local { 96.4 } else { 503.9 },
                prompt_tokens_total: if local { 41200 } else { 857350 },
                gen_tokens_total: if local { 6120 } else { 115498 },
                ..Default::default()
            }),
            model: if local { "q6" } else { "q8" }.into(),
            ctx: 65536,
            ..Default::default()
        }),
        down: false,
        at: GoTime::from_utc(now()),
    }
}
fn boot(local: bool, verify: bool) -> Store {
    let mut s = base();
    if local {
        s.choose(Target::Local);
        s.apply_models(models());
    }
    let seconds = if verify {
        25
    } else if local {
        40
    } else {
        72
    };
    let t0 = now() - TimeDelta::seconds(seconds);
    s.begin_up(t0);
    let phases: Vec<(&str, i64)> = if local {
        vec![("create", 2), ("gpu", 3), ("download", 4)]
    } else {
        vec![
            ("create", 8),
            ("image", 41),
            ("tunnel", 43),
            ("gpu", 44),
            ("download", 45),
        ]
    };
    for (p, t) in phases {
        s.handle_event(
            &UpEvent {
                phase: p.into(),
                ..Default::default()
            },
            t0 + TimeDelta::seconds(t),
        );
    }
    let detail = if verify {
        "this Mac, q6"
    } else if local {
        "llama.cpp b11118 11 MB"
    } else {
        "vast 26461230, offer 51401937, 18877 Mbps down, California, US, $0.73/h"
    };
    s.handle_event(
        &UpEvent {
            phase: if verify { "verify" } else { "download" }.into(),
            detail: detail.into(),
            download: (!verify).then_some(DownloadProgress {
                bytes: 12_400_000_000,
                total: 28_595_762_272,
                mbps: if local { 88.0 } else { 713.0 },
                ..Default::default()
            }),
            ..Default::default()
        },
        now(),
    );
    if !local {
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
                version: Some(Manifest {
                    version: "2026.09.25-10".into(),
                    git_sha: "4b513b3".into(),
                    ..Default::default()
                }),
                at: GoTime::from_utc(now()),
                ..Default::default()
            },
            now(),
        );
    }
    s
}
fn fixtures() -> Vec<(&'static str, Store)> {
    let mut out = vec![
        ("off", base()),
        ("boot", boot(false, false)),
        ("boot_local", boot(true, false)),
        ("verify_local", boot(true, true)),
        ("loading", Store::new(None, true)),
    ];
    for local in [false, true] {
        let mut ready = base();
        if local {
            ready.choose(Target::Local);
        }
        ready.apply_snap(running(local), now());
        out.push((if local { "ready_local" } else { "ready" }, ready));
        let mut fail = base();
        if local {
            fail.choose(Target::Local);
        }
        fail.begin_up(now());
        let tail = if local {
            vec!["local: port 8931 in use (LOBO_LOCAL_PORT)"]
        } else {
            vec![
                "image: download: host: download too slow: 41.2 MB/s after 20s from https://acc.r2.cloudflarestorage.com (min 100)",
                "create: bad host, renting another pod (4/4)",
                "vast 26461301, offer 51399812, 9129 Mbps down, Quebec, CA",
            ]
        };
        for detail in tail {
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
        fail.stop_failed(
            if local {
                "gpu: q8 needs 29.1 GB, this Mac allows ~48.0 GB to the GPU"
            } else {
                "gave up: 4 pods in a row landed on bad hosts (all deleted)"
            }
            .into(),
        );
        out.push((if local { "fail_local" } else { "fail" }, fail));
        let mut stop = base();
        if local {
            stop.choose(Target::Local);
        }
        stop.begin_stop();
        out.push((if local { "stopping_local" } else { "stopping" }, stop));
    }
    let mut setup = Store::new(None, true);
    setup.apply_config(
        ConfigShow {
            path: "/Users/you/.config/lobo/config.env".into(),
            ..Default::default()
        },
        Readiness {
            local_supported: true,
            ..Default::default()
        },
    );
    setup.needs_setup();
    out.push(("setup", setup));
    let mut local = base();
    local.choose(Target::Local);
    local.apply_models(models());
    out.push(("off_local", local));
    let mut local = base();
    local.choose(Target::Local);
    out.push(("off_local_nomodels", local));
    for (name, providers) in [
        ("off_nokeys", vec![]),
        ("off_one_provider", vec!["runpod".to_string()]),
    ] {
        let mut s = base();
        let (mut c, mut r) = config();
        r.providers = providers;
        r.cloud_ready = !r.providers.is_empty();
        r.default_provider = "runpod".into();
        c.set.insert("VASTAI_API_KEY".into(), false);
        c.set.insert("RUNPOD_API_KEY".into(), r.cloud_ready);
        s.apply_config(c, r);
        out.push((name, s));
    }
    let mut sha = boot(false, false);
    sha.handle_event(
        &UpEvent {
            phase: "verify".into(),
            download: Some(DownloadProgress {
                bytes: 12_400_000_000,
                total: 28_595_762_272,
                verifying: true,
                ..Default::default()
            }),
            ..Default::default()
        },
        now(),
    );
    out.push(("boot_verify_sha", sha));
    let mut fail = base();
    fail.apply_snap(running(false), now());
    fail.stop_failed("down: delete denied".into());
    out.push(("fail_pod", fail));
    let mut warning = base();
    warning.apply_snap(running(false), now());
    warning.set_warning(Some("status: connection timed out".into()));
    out.push(("ready_warning", warning));
    let mut soon = base();
    let mut snap = running(false);
    let st = snap.status.as_mut().unwrap();
    st.kill_in_s = 240;
    st.llama.as_mut().unwrap().requests_processing = 0;
    soon.apply_snap(snap, now());
    out.push(("ready_kill_soon", soon));
    out
}
#[test]
fn write_render_fixtures() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../ui/src/fixtures");
    std::fs::create_dir_all(&dir).unwrap();
    for (name, store) in fixtures() {
        let data = serde_json::json!({"name":name,"now_ms":now().timestamp_millis(),"state":store.view(now())});
        std::fs::write(
            dir.join(format!("panel_{name}.json")),
            format!("{}\n", serde_json::to_string_pretty(&data).unwrap()),
        )
        .unwrap();
    }
    let (c, r) = config();
    std::fs::write(dir.join("settings.json"),format!("{}\n",serde_json::to_string_pretty(&serde_json::json!({"config":c,"readiness":r,"models":models(),"local_supported":true})).unwrap())).unwrap();
}
#[test]
fn render_fixtures_cover_inventory() {
    let f = fixtures();
    assert_eq!(f.len(), 20);
    let names: std::collections::BTreeSet<_> = f.iter().map(|(n, _)| *n).collect();
    assert_eq!(
        names,
        [
            "off",
            "boot",
            "ready",
            "fail",
            "setup",
            "off_local",
            "boot_local",
            "verify_local",
            "ready_local",
            "fail_local",
            "loading",
            "stopping",
            "stopping_local",
            "off_nokeys",
            "off_one_provider",
            "off_local_nomodels",
            "boot_verify_sha",
            "fail_pod",
            "ready_warning",
            "ready_kill_soon",
        ]
        .into_iter()
        .collect()
    );
    for p in ["SCAN", "SETUP", "OFF", "BOOT", "RUN", "STOP", "FAIL"] {
        assert!(
            f.iter().any(|(_, s)| s.view(now()).phase.word() == p),
            "{p}"
        );
    }
    assert_eq!(
        f.iter()
            .find(|(n, _)| *n == "ready_local")
            .unwrap()
            .1
            .view(now())
            .phase,
        Phase::Ready
    );
}
