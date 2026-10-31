use crate::{testutil::*, *};
use serde_json::{Value, json};
use ts_rs::TS;

#[test]
fn gotime_rfc3339_nano_and_offsets() {
    for (input, expected) in [
        ("0001-01-01T00:00:00Z", "0001-01-01T00:00:00Z"),
        ("2026-09-29T10:00:00.5Z", "2026-09-29T10:00:00.5Z"),
        (
            "2026-09-29T10:00:00.123456789+03:00",
            "2026-09-29T10:00:00.123456789+03:00",
        ),
        ("2026-09-29T10:00:00.100Z", "2026-09-29T10:00:00.1Z"),
        ("2026-09-29T10:00:00Z", "2026-09-29T10:00:00Z"),
    ] {
        let t: GoTime = serde_json::from_value(json!(input)).unwrap();
        assert_eq!(serde_json::to_value(&t).unwrap(), json!(expected));
    }
    assert!(serde_json::from_value::<GoTime>(json!("garbage")).is_err());
    assert!(
        serde_json::from_value::<GoTime>(json!("0001-01-01T00:00:00Z"))
            .unwrap()
            .is_zero()
    );
    let dt = chrono::DateTime::parse_from_rfc3339("2026-09-29T10:00:00Z")
        .unwrap()
        .to_utc();
    assert_eq!(
        serde_json::to_value(GoTime::from_utc(dt)).unwrap(),
        json!("2026-09-29T10:00:00Z")
    );
}

#[test]
fn agent_fixtures() {
    for name in [
        "status_ready.json",
        "status_booting.json",
        "status_offset_time.json",
    ] {
        round_trip::<Status>(name);
    }
}

#[test]
fn agent_missing_unknown_and_nullable_fields() {
    let s: Status = serde_json::from_value(json!({"stage":"warp", "future_field": 1})).unwrap();
    assert_eq!(s.stage, Stage::Unknown);
    assert_eq!(
        serde_json::from_value::<Status>(json!({})).unwrap(),
        Status::default()
    );
    let boot = round_trip::<Status>("status_booting.json");
    let out = serde_json::to_value(boot).unwrap();
    assert!(out.get("boot_id").is_none());
    assert_eq!(out["gpu"], Value::Null);
    assert_eq!(out["timings"]["ready_at"], "0001-01-01T00:00:00Z");
    for stage in [
        Stage::Boot,
        Stage::Tunnel,
        Stage::Gpu,
        Stage::Verify,
        Stage::Download,
        Stage::Load,
        Stage::Ready,
        Stage::Failed,
        Stage::Terminating,
    ] {
        assert_eq!(serde_json::to_value(stage).unwrap(), stage.as_str());
    }
}

#[test]
fn release_fixtures_and_go_pin() {
    round_trip::<Manifest>("manifest.json");
    round_trip::<Resolved>("resolved.json");
    // Retire this source drift guard at P6. Keep the generated fixture tests.
    assert!(
        include_str!("../../../internal/release/manifest.go")
            .contains(&format!("DefaultLlamaImage = \"{DEFAULT_LLAMA_IMAGE}\""))
    );
    assert_eq!(DEFAULT_MODEL, "qwen38-q6");
    assert_eq!(DEFAULT_MODEL, DEFAULT_CLOUD_MODEL);
    assert_eq!(
        DEFAULT_DEFAULTS,
        Defaults {
            ctx: 65536,
            idle_min: 30,
            max_hours: 12
        }
    );
}

#[test]
fn control_fixtures() {
    for name in [
        "up_event_progress.json",
        "up_event_ready.json",
        "up_event_error.json",
    ] {
        round_trip::<UpEvent>(name);
    }
    let snap = round_trip::<Snap>("snap_running.json");
    assert!(snap.pod.unwrap().api_url.is_empty());
    round_trip::<Snap>("snap_down.json");
}

#[test]
fn instance_urls_are_never_serialized() {
    let instance = Instance {
        api_url: "private-api".into(),
        agent_url: "private-agent".into(),
        ..Default::default()
    };
    let out = serde_json::to_string(&instance).unwrap();
    assert!(!out.contains("private"));
    assert!(!out.contains("api_url"));
    assert!(!out.contains("agent_url"));
}

#[test]
fn legacy_ready_event() {
    let legacy: ReadyInfo = serde_json::from_value(fixture("boots_ready_legacy.json")).unwrap();
    assert!(legacy.provider.is_empty());
    assert!(legacy.timings.is_some());
}

#[test]
fn config_fixture_remains_readable() {
    round_trip::<ConfigShow>("config_show.json");
}

#[test]
fn catalog_contains_only_supported_cloud_model() {
    assert_eq!(catalog::ids(), [DEFAULT_MODEL]);
    for m in catalog::all() {
        assert!(m.gpu);
        assert!(!m.local);
        assert_eq!(m.sha256.len(), 64);
        assert_eq!(
            m.url("https://pub-x.r2.dev/"),
            format!("https://pub-x.r2.dev/models/{}", m.sha256)
        );
        assert_eq!(
            m.chunk_sha.len() as i64,
            (m.size + catalog::CHUNK_SIZE - 1) / catalog::CHUNK_SIZE
        );
        assert!(
            m.chunk_sha
                .iter()
                .all(|sha| sha.len() == 64 && sha.bytes().all(|b| b.is_ascii_hexdigit()))
        );
    }
    assert_eq!(
        catalog::get("x").unwrap_err().to_string(),
        "unknown model \"x\", valid: qwen38-q6"
    );
    for retired in ["q6", "q8"] {
        assert_eq!(
            catalog::get(retired).unwrap_err().to_string(),
            format!("unknown model {retired:?}, valid: qwen38-q6")
        );
    }
    assert_eq!(catalog::min_free_mib(22430999840), 23951);
}

#[test]
fn catalog_cloud_entry_qwen38() {
    let m = catalog::get("qwen38-q6").unwrap();
    assert_eq!(m.alias, "qwen3.8-27b-abliterated-q6");
    assert_eq!(m.repo, "huihui-ai/Huihui-Qwen3.8-27B-abliterated-GGUF");
    assert_eq!(m.file, "Huihui-Qwen3.8-27B-abliterated-Q6_K.gguf");
    assert_eq!(m.size, 22430999840);
    assert_eq!(
        m.sha256,
        "a5c159519d7bdba523578977d649dbfc48f2e47277ec77edd77670003b5b2492"
    );
    assert_eq!(
        (m.name.as_str(), m.quant.as_str(), m.local),
        ("qwen3.8-27b abliterated", "Q6_K", false)
    );
    let gpu: Vec<_> = catalog::all()
        .iter()
        .filter(|m| m.gpu)
        .map(|m| m.id.as_str())
        .collect();
    assert_eq!(gpu, [DEFAULT_CLOUD_MODEL]);
    assert_eq!(m.chunk_sha.len(), 84);
    assert_eq!(
        m.hf_url(),
        "https://huggingface.co/huihui-ai/Huihui-Qwen3.8-27B-abliterated-GGUF/resolve/main/Huihui-Qwen3.8-27B-abliterated-Q6_K.gguf"
    );
    assert_eq!(catalog::ids(), ["qwen38-q6"]);
    assert_eq!(catalog::get(DEFAULT_CLOUD_MODEL).unwrap().id, "qwen38-q6");
    assert!(
        catalog::get("x")
            .unwrap_err()
            .to_string()
            .ends_with("valid: qwen38-q6")
    );
}

#[test]
fn export_bindings_schema() {
    // Explicit export and a private output directory avoid dependence on parallel test order.
    let dir = std::env::temp_dir().join(format!("lobo-proto-types-{}", std::process::id()));
    let config = ts_rs::Config::default().with_out_dir(&dir);
    macro_rules! export {
        ($($t:ty),+ $(,)?) => {
            $(<$t>::export_all(&config).unwrap();)+
        };
    }
    export!(
        GoTime,
        Stage,
        DownloadProgress,
        Gpu,
        Host,
        Llama,
        Timings,
        Status,
        ModelRef,
        Defaults,
        Manifest,
        Resolved,
        Instance,
        ReadyInfo,
        UpEvent,
        Snap,
        ConfigShow,
        UpRequest,
        Readiness,
        catalog::Model
    );
    let mut names = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect::<Vec<_>>();
    names.sort();
    let mut expected = [
        "GoTime",
        "Stage",
        "DownloadProgress",
        "Gpu",
        "Host",
        "Llama",
        "Timings",
        "Status",
        "ModelRef",
        "Defaults",
        "Manifest",
        "Resolved",
        "Instance",
        "ReadyInfo",
        "UpEvent",
        "Snap",
        "ConfigShow",
        "UpRequest",
        "Readiness",
        "Model",
    ]
    .map(|s| format!("{s}.ts"));
    expected.sort();
    assert_eq!(names, expected);
    for name in names {
        let ts = std::fs::read_to_string(dir.join(name)).unwrap();
        assert!(!ts.contains("bigint"), "{ts}");
    }
    assert!(
        std::fs::read_to_string(dir.join("GoTime.ts"))
            .unwrap()
            .contains("= string;")
    );
    let ts = std::fs::read_to_string(dir.join("Instance.ts")).unwrap();
    assert!(!ts.contains("api_url"));
    assert!(!ts.contains("agent_url"));
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn app_request_and_readiness_wire() {
    assert_eq!(
        serde_json::to_value(UpRequest::default()).unwrap(),
        serde_json::json!({"provider":null,"model":null,"ctx":null,"source":null,"cloud":null})
    );
    let value = serde_json::json!({"exists":true,"cloud_ready":true,"ready":true,"local_supported":false,"providers":["runpod"],"default_provider":"runpod","default_model":"qwen38-q6","local_port":0,"error":null});
    let r: Readiness = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(serde_json::to_value(r).unwrap(), value);
}

#[test]
fn legacy_readiness_wire_remains_readable() {
    let value = serde_json::json!({"exists":true,"cloud_ready":false,"ready":true,"local_supported":true,"providers":[],"default_provider":"local","default_model":"q6","local_port":9000,"error":null});
    let r: Readiness = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(serde_json::to_value(r).unwrap(), value);
}
