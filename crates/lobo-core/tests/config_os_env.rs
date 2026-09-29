#[test]
fn load_laptop_ignores_os_env() {
    // This integration test owns its process and starts no other threads.
    unsafe {
        std::env::set_var("RUNPOD_API_KEY", "from-os");
    }
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config");
    std::fs::write(&path, "LOBO_API_KEY=sk-test\n").unwrap();
    let cfg = lobo_core::config::load_laptop(&path).unwrap();
    assert!(cfg.runpod_api_key.is_empty());
    assert!(
        cfg.require_provider_key()
            .unwrap_err()
            .to_string()
            .contains("RUNPOD_API_KEY")
    );
}
