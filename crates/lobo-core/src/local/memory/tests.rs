use super::*;
fn ample() -> MemorySnapshot {
    MemorySnapshot {
        total_bytes: 128 * GIB,
        available_bytes: 100 * GIB,
        metal_limit_bytes: 96 * GIB,
    }
}
#[test]
fn q6_q8_exact_requirement() {
    for model in ["q6", "q8"] {
        let a = assess(model, 65536, &ample()).unwrap();
        let size = lobo_proto::catalog::get(model).unwrap().size as u64;
        assert_eq!(a.required_bytes, size + 2_281_701_376 + 4 * GIB);
        assert_eq!(a.model, model);
        assert_eq!(a.ctx, 65536);
    }
}
#[test]
fn context_padding_and_maximum() {
    let weight = lobo_proto::catalog::get("q6").unwrap().size as u64;
    for (ctx, padded) in [
        (1, 256),
        (255, 256),
        (256, 256),
        (257, 512),
        (262144, 262144),
    ] {
        assert_eq!(
            assess("q6", ctx, &ample()).unwrap().required_bytes,
            weight + padded * 34_816 + 4 * GIB
        );
    }
}
#[test]
fn unsupported_model_and_context_fail() {
    for model in ["", "q2", "Q6"] {
        assert!(assess(model, 1, &ample()).is_err());
    }
    for ctx in [i64::MIN, -1, 0, 262145, i64::MAX] {
        assert!(assess("q6", ctx, &ample()).is_err());
    }
}
#[test]
fn physical_and_metal_limits() {
    let mut s = ample();
    s.available_bytes = 40 * GIB;
    assert_eq!(assess("q6", 1, &s).unwrap().budget_bytes, 36 * GIB);
    s.metal_limit_bytes = 30 * GIB;
    assert_eq!(assess("q6", 1, &s).unwrap().budget_bytes, 30 * GIB);
    s.available_bytes = 3 * GIB;
    assert_eq!(assess("q6", 1, &s).unwrap().budget_bytes, 0);
    s.available_bytes = 0;
    assert!(!assess("q6", 1, &s).unwrap().fits());
}
#[test]
fn exact_boundary_and_one_byte_less() {
    let mut s = ample();
    let need = assess("q8", 65536, &s).unwrap().required_bytes;
    s.available_bytes = need + SYSTEM_RESERVE;
    let a = assess("q8", 65536, &s).unwrap();
    assert!(a.fits());
    a.ensure_fit().unwrap();
    s.available_bytes -= 1;
    let a = assess("q8", 65536, &s).unwrap();
    assert!(!a.fits());
    let error = a.ensure_fit().unwrap_err().to_string();
    for want in ["q8", "65536", "GiB", "Close other apps", "Cloud"] {
        assert!(error.contains(want), "{error}");
    }
    s.available_bytes = 100 * GIB;
    s.metal_limit_bytes = need;
    assert!(assess("q8", 65536, &s).unwrap().fits());
    s.metal_limit_bytes -= 1;
    assert!(!assess("q8", 65536, &s).unwrap().fits());
}
#[test]
fn invalid_measurements_fail() {
    for s in [
        MemorySnapshot {
            total_bytes: 0,
            ..ample()
        },
        MemorySnapshot {
            available_bytes: 129 * GIB,
            ..ample()
        },
        MemorySnapshot {
            metal_limit_bytes: 0,
            ..ample()
        },
    ] {
        assert!(assess("q6", 1, &s).is_err());
    }
}
#[test]
fn page_accounting_counts_only_unreclaimable_pages() {
    let s = from_pages(100 * 16384, 16384, 20, 5, 10, 7, 80 * 16384).unwrap();
    assert_eq!(s.available_bytes, 68 * 16384);
    assert_eq!(s.total_bytes, 100 * 16384);
    assert_eq!(s.metal_limit_bytes, 80 * 16384);
}
#[test]
fn invalid_page_counters_and_overflow_fail() {
    for args in [
        (100, 0, 1, 0, 0, 0, 80),
        (0, 1, 0, 0, 0, 0, 80),
        (100, 1, 1, 2, 0, 0, 80),
        (100, 1, 101, 0, 0, 0, 80),
        (u64::MAX, 2, u64::MAX, 0, 0, 0, 80),
        (u64::MAX, 1, u64::MAX, 0, 1, 0, 80),
        (u64::MAX, 1, 0, 0, u64::MAX, 1, 80),
        (100, 1, 0, 0, 0, 0, 0),
    ] {
        let (t, p, a, r, w, c, m) = args;
        assert!(from_pages(t, p, a, r, w, c, m).is_err(), "{args:?}");
    }
}
#[test]
fn serialized_byte_counts_stay_exact() {
    let a = assess("q6", 1, &ample()).unwrap();
    let serialized = serde_json::to_value(&a).unwrap();
    assert_eq!(
        serialized["required_bytes"].as_u64(),
        Some(a.required_bytes)
    );
    assert_eq!(
        serde_json::from_value::<MemoryAssessment>(serialized).unwrap(),
        a
    );
}

#[test]
fn unknown_or_invalid_probe_preserves_model_and_context() {
    let probes: [MemoryProbe; 2] = [
        Arc::new(|| Err(Error::Local("native read failed".into()))),
        Arc::new(|| {
            Ok(MemorySnapshot {
                metal_limit_bytes: 0,
                ..ample()
            })
        }),
    ];
    for probe in probes {
        let message = inspect_with("q8", 65536, &probe).unwrap_err().to_string();
        for want in ["q8", "65536", "Close other apps", "Cloud"] {
            assert!(message.contains(want), "{message}");
        }
    }
}
#[cfg(target_os = "macos")]
#[test]
fn native_snapshot_reads_without_loading_a_model() {
    if super::super::platform::supported().is_err() {
        assert!(snapshot().is_err());
        return;
    }
    // Hosted macOS runners can expose no Metal device. Check the native
    // capability independently; only that specific failure is acceptable.
    if objc2_metal::MTLCreateSystemDefaultDevice().is_none() {
        let error = snapshot().unwrap_err();
        assert!(
            matches!(&error, Error::Local(message)
            if message == "Mac memory measurement unavailable: Metal device unavailable"),
            "{error}"
        );
        return;
    }
    let s = snapshot().unwrap();
    assert!(s.total_bytes > 0);
    assert!(s.available_bytes <= s.total_bytes);
    assert!(s.metal_limit_bytes > 0);
    let a = assess("q6", 65536, &s).unwrap();
    assert_eq!(a.total_bytes, s.total_bytes);
}
