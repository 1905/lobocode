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
    assert_eq!(DEFAULT_MODEL, "q8");
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
fn local_and_config_fixtures() {
    round_trip::<Listing>("listing.json");
    assert!(
        round_trip::<Listing>("listing_empty.json")
            .models
            .is_empty()
    );
    round_trip::<LocalState>("local_state.json");
    round_trip::<ConfigShow>("config_show.json");
}

#[test]
fn catalog_go_parity() {
    for (id, suffix, alias, size) in [
        ("q8", "Q8_0", "qwen3.5-27b-uncensored-q8", 28595762272),
        ("q6", "Q6_K", "qwen3.5-27b-uncensored-q6", 22082528352),
    ] {
        let m = catalog::get(id).unwrap();
        assert_eq!(
            m.file,
            format!("Qwen3.5-27B-Uncensored-HauhauCS-Aggressive-{suffix}.gguf")
        );
        assert_eq!(m.alias, alias);
        assert_eq!(m.size, size);
        assert_eq!(m.sha256.len(), 64);
        assert_eq!(
            m.url("https://pub-x.r2.dev/"),
            format!("https://pub-x.r2.dev/models/{}", m.file)
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
        "unknown model \"x\", valid: q6, q8"
    );
    assert_eq!(
        catalog::all()
            .iter()
            .map(|m| m.id.as_str())
            .collect::<Vec<_>>(),
        ["q6", "q8"]
    );
    assert!((29275..31602).contains(&catalog::min_free_mib(28595762272)));
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
        ModelState,
        RuntimeInfo,
        Listing,
        LocalState,
        ConfigShow,
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
        "ModelState",
        "RuntimeInfo",
        "Listing",
        "LocalState",
        "ConfigShow",
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
