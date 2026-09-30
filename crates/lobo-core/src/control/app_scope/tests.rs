use super::*;
use crate::{clock::SystemClock, control::testkit::*};
use std::sync::Mutex;

fn target(provider: &str, id: &str) -> RuntimeTarget {
    RuntimeTarget {
        provider: provider.into(),
        instance_id: Some(id.into()),
        boot_id: "owned".into(),
        agent_url: Some("http://owned-agent".into()),
        api_url: Some("http://owned-api/v1".into()),
        local_pid: None,
        local_start_id: None,
    }
}
fn instance(provider: &str, id: &str) -> Instance {
    Instance {
        provider: provider.into(),
        id: id.into(),
        agent_url: "http://owned-agent".into(),
        api_url: "http://owned-api/v1".into(),
        ..Default::default()
    }
}
fn setup() -> (
    Deps,
    Arc<RecordingProvider>,
    Arc<RecordingProvider>,
    Arc<RecordingProvider>,
) {
    let mut d = deps(
        Arc::new(FakeRunPod::default()),
        Arc::new(FakeAgent::new(vec![Some(Status {
            boot_id: "owned".into(),
            ..Default::default()
        })])),
        Arc::new(SystemClock),
    );
    let local = Arc::new(RecordingProvider::new("local"));
    let runpod = Arc::new(RecordingProvider::new("runpod"));
    let vast = Arc::new(RecordingProvider::new("vast"));
    d.providers = [
        ("local".into(), local.clone() as Arc<dyn Provider>),
        ("runpod".into(), runpod.clone()),
        ("vast".into(), vast.clone()),
    ]
    .into();
    runpod.state.lock().unwrap().broken = true;
    vast.state.lock().unwrap().broken = true;
    (d, local, runpod, vast)
}
#[tokio::test]
async fn local_discovery_status_and_stop_never_call_cloud() {
    let (d, local, rp, vast) = setup();
    assert_eq!(discover_app(&d, "local").await.unwrap(), None);
    assert!(
        snapshot_app(&d, &target("local", "missing"))
            .await
            .unwrap()
            .down
    );
    assert_eq!(
        down_app(&d, &target("local", "missing")).await.unwrap(),
        0.0
    );
    assert!(rp.calls().is_empty());
    assert!(vast.calls().is_empty());
    assert!(!local.calls().is_empty());
}
#[tokio::test]
async fn cloud_stop_deletes_only_recorded_instance() {
    let (d, local, rp, vast) = setup();
    rp.state.lock().unwrap().instances =
        vec![instance("runpod", "ours"), instance("runpod", "foreign")];
    down_app(&d, &target("runpod", "ours")).await.unwrap();
    assert_eq!(rp.state.lock().unwrap().instances[0].id, "foreign");
    assert!(!rp.calls().contains(&"list".into()));
    assert!(local.calls().is_empty());
    assert!(vast.calls().is_empty());
}
#[tokio::test]
async fn foreign_pending_survives_scoped_stop_byte_for_byte() {
    let (mut d, _, rp, _) = setup();
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("operation.json");
    d.operations = Arc::new(OperationState::persistent(path.clone()));
    let foreign = operation_state::PendingCreate {
        provider: "runpod".into(),
        boot_id: "foreign".into(),
        before: vec![],
        instance_id: Some("foreign".into()),
    };
    d.operations
        .acquire(&CancellationToken::new())
        .await
        .unwrap()
        .record(foreign)
        .unwrap();
    let bytes = std::fs::read(&path).unwrap();
    rp.state.lock().unwrap().instances =
        vec![instance("runpod", "ours"), instance("runpod", "foreign")];
    down_app(&d, &target("runpod", "ours")).await.unwrap();
    assert_eq!(std::fs::read(path).unwrap(), bytes);
    assert_eq!(rp.state.lock().unwrap().instances[0].id, "foreign");
}
#[tokio::test]
async fn ambiguous_discovery_never_selects_first() {
    let (d, _, rp, _) = setup();
    {
        let mut s = rp.state.lock().unwrap();
        s.broken = false;
        s.instances = vec![instance("runpod", "one"), instance("runpod", "two")];
    }
    assert!(
        discover_app(&d, "runpod")
            .await
            .unwrap_err()
            .to_string()
            .contains("ambiguous")
    );
}
#[tokio::test]
async fn direct_sample_never_lists_or_touches_connections() {
    let (d, local, rp, vast) = setup();
    assert_eq!(
        sample_app(&d, &target("local", "ours"))
            .await
            .unwrap()
            .boot_id,
        "owned"
    );
    assert!(local.calls().is_empty());
    assert!(rp.calls().is_empty());
    assert!(vast.calls().is_empty());
}
#[tokio::test]
async fn direct_sample_rejects_changed_or_empty_boot() {
    let (d, _, _, _) = setup();
    let mut t = target("local", "ours");
    t.boot_id = "other".into();
    assert!(sample_app(&d, &t).await.is_err());
    t.boot_id.clear();
    assert!(sample_app(&d, &t).await.is_err());
}

#[tokio::test]
async fn local_discovery_records_process_identity_and_rejects_replacement() {
    let (d, local, rp, vast) = setup();
    {
        let mut s = local.state.lock().unwrap();
        s.instances.push(instance("local", "4242"));
        s.boot_id = "owned".into();
        s.start_id = 100;
    }
    let t = discover_app(&d, "local").await.unwrap().unwrap();
    assert_eq!(t.local_pid, Some(4242));
    assert_eq!(t.local_start_id, Some(100));
    assert_eq!(
        snapshot_app(&d, &t).await.unwrap().status.unwrap().boot_id,
        "owned"
    );
    local.state.lock().unwrap().start_id += 1;
    assert!(snapshot_app(&d, &t).await.unwrap().down);
    down_app(&d, &t).await.unwrap();
    assert!(!local.calls().iter().any(|c| c.starts_with("delete:")));
    assert!(rp.calls().is_empty());
    assert!(vast.calls().is_empty());
}
#[tokio::test]
async fn local_replacement_at_delete_boundary_is_not_signalled() {
    let (d, local, _, _) = setup();
    {
        let mut s = local.state.lock().unwrap();
        s.instances.push(instance("local", "4242"));
        s.boot_id = "owned".into();
        s.start_id = 100;
    }
    let t = discover_app(&d, "local").await.unwrap().unwrap();
    local.state.lock().unwrap().swap_on_delete = true;
    down_app(&d, &t).await.unwrap();
    assert_eq!(local.state.lock().unwrap().instances.len(), 1);
    assert!(!local.calls().iter().any(|c| c.starts_with("delete:")));
}
#[tokio::test]
async fn stop_rejects_empty_boot_before_provider_calls() {
    let (d, local, rp, vast) = setup();
    let mut t = target("runpod", "ours");
    t.boot_id.clear();
    assert!(down_app(&d, &t).await.is_err());
    assert!(local.calls().is_empty());
    assert!(rp.calls().is_empty());
    assert!(vast.calls().is_empty());
}
type StartupDeps = (
    Deps,
    Arc<RecordingProvider>,
    Arc<RecordingProvider>,
    Arc<RecordingProvider>,
    OwnerSink,
    Arc<Mutex<Vec<RuntimeTarget>>>,
);
fn startup_deps() -> StartupDeps {
    let (mut d, local, rp, vast) = setup();
    let agent = Arc::new(FakeAgent::new(vec![Some(Status {
        stage: lobo_proto::Stage::Ready,
        ..Default::default()
    })]));
    let ag = agent.clone();
    d.new_agent = Arc::new(move |_| ag.clone());
    let targets = Arc::new(Mutex::new(Vec::new()));
    let owned = targets.clone();
    let owner = Arc::new(move |target: RuntimeTarget| {
        let mut s = agent.state.lock().unwrap();
        s.script[0].as_mut().unwrap().boot_id = target.boot_id.clone();
        owned.lock().unwrap().push(target);
        Ok(())
    });
    (d, local, rp, vast, owner, targets)
}
fn local_opts() -> UpOpts {
    UpOpts {
        provider: "local".into(),
        ..Default::default()
    }
}
#[tokio::test]
async fn local_start_ignores_broken_cloud_keys_and_records_owner() {
    let (d, local, rp, vast, owner, targets) = startup_deps();
    let mut op = up_app(d, local_opts(), None, CancellationToken::new(), owner);
    op.wait().await.unwrap();
    let targets = targets.lock().unwrap();
    assert!(targets[0].instance_id.is_none());
    assert!(!targets[0].boot_id.is_empty());
    let ready = targets.last().unwrap();
    assert_eq!(ready.instance_id.as_deref(), Some("4242"));
    assert_eq!(ready.local_pid, Some(4242));
    assert_eq!(ready.local_start_id, Some(100));
    assert_eq!(ready.agent_url.as_deref(), Some(LOCAL_AGENT_URL));
    assert_eq!(local.state.lock().unwrap().created.len(), 1);
    assert!(rp.calls().is_empty());
    assert!(vast.calls().is_empty());
}
#[tokio::test]
async fn foreign_pending_blocks_start_without_any_provider_calls() {
    for (pending_provider, previous_provider, previous_boot) in [
        ("runpod", "local", "foreign"),
        ("local", "local", "other"),
        ("runpod", "runpod", "foreign"),
    ] {
        let (mut d, local, rp, vast, owner, _) = startup_deps();
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("operation.json");
        d.operations = Arc::new(OperationState::persistent(path.clone()));
        d.operations
            .acquire(&CancellationToken::new())
            .await
            .unwrap()
            .record(operation_state::PendingCreate {
                provider: pending_provider.into(),
                boot_id: "foreign".into(),
                before: vec![],
                instance_id: Some("4242".into()),
            })
            .unwrap();
        let bytes = std::fs::read(&path).unwrap();
        let mut previous = target(previous_provider, "4242");
        previous.boot_id = previous_boot.into();
        let mut op = up_app(
            d,
            local_opts(),
            Some(previous),
            CancellationToken::new(),
            owner,
        );
        assert!(
            op.wait()
                .await
                .unwrap_err()
                .to_string()
                .contains("another runtime")
        );
        assert_eq!(std::fs::read(path).unwrap(), bytes);
        assert!(local.calls().is_empty());
        assert!(rp.calls().is_empty());
        assert!(vast.calls().is_empty());
    }
}
#[tokio::test]
async fn owner_record_failure_before_create_never_rents() {
    let (d, local, _, _, _, _) = startup_deps();
    let mut op = up_app(
        d,
        local_opts(),
        None,
        CancellationToken::new(),
        Arc::new(|_| Err(Error::Other("owner record failed".into()))),
    );
    assert!(
        op.wait()
            .await
            .unwrap_err()
            .to_string()
            .contains("owner record failed")
    );
    assert!(local.state.lock().unwrap().created.is_empty());
}
#[tokio::test]
async fn owner_record_failure_after_create_retains_worker_cleanup() {
    let (d, local, _, _, _, _) = startup_deps();
    let state = d.operations.clone();
    let mut op = up_app(
        d,
        local_opts(),
        None,
        CancellationToken::new(),
        Arc::new(|t| {
            if t.instance_id.is_some() {
                Err(Error::Other("owner update failed".into()))
            } else {
                Ok(())
            }
        }),
    );
    assert!(
        op.wait()
            .await
            .unwrap_err()
            .to_string()
            .contains("owner update failed")
    );
    assert!(local.state.lock().unwrap().instances.is_empty());
    assert!(
        state
            .acquire(&CancellationToken::new())
            .await
            .unwrap()
            .pending()
            .is_none()
    );
}
#[tokio::test(start_paused = true)]
async fn cancellation_before_create_has_no_provider_calls() {
    let (d, local, rp, vast, owner, _) = startup_deps();
    let cancel = CancellationToken::new();
    cancel.cancel();
    let mut op = up_app(d, local_opts(), None, cancel, owner);
    assert!(matches!(op.wait().await, Err(Error::Cancelled)));
    assert!(local.calls().is_empty());
    assert!(rp.calls().is_empty());
    assert!(vast.calls().is_empty());
}
#[tokio::test(start_paused = true)]
async fn cancelled_or_panicked_start_cleans_only_its_local_owner() {
    for panic in [false, true] {
        let (d, local, rp, vast, owner, _) = startup_deps();
        {
            let mut s = local.state.lock().unwrap();
            s.cancel_on_rent = !panic;
            s.panic_after_create = panic;
        }
        let mut op = up_app(d, local_opts(), None, CancellationToken::new(), owner);
        let error = op.wait().await.unwrap_err();
        if panic {
            assert!(error.to_string().contains("scoped worker exploded"));
        } else {
            assert!(matches!(error, Error::Cancelled));
        }
        assert!(local.state.lock().unwrap().instances.is_empty());
        assert!(rp.calls().is_empty());
        assert!(vast.calls().is_empty());
    }
}
#[tokio::test(start_paused = true)]
async fn scoped_cancelled_uncertain_create_is_reconciled_without_rerent() {
    let (d, local, rp, vast, owner, _) = startup_deps();
    {
        let mut s = local.state.lock().unwrap();
        s.cancel_on_rent = true;
        s.uncertain_create = true;
    }
    let mut op = up_app(d, local_opts(), None, CancellationToken::new(), owner);
    assert!(matches!(op.wait().await, Err(Error::Cancelled)));
    assert_eq!(local.state.lock().unwrap().created.len(), 1);
    assert!(local.state.lock().unwrap().instances.is_empty());
    assert!(rp.calls().is_empty());
    assert!(vast.calls().is_empty());
}
#[tokio::test]
async fn cancelled_start_does_not_delete_replacement_local_identity() {
    let (d, local, rp, vast, owner, _) = startup_deps();
    {
        let mut s = local.state.lock().unwrap();
        s.cancel_on_rent = true;
        s.swap_on_delete = true;
    }
    let mut op = up_app(d, local_opts(), None, CancellationToken::new(), owner);
    assert!(matches!(op.wait().await, Err(Error::Cancelled)));
    assert_eq!(local.state.lock().unwrap().instances.len(), 1);
    assert!(!local.calls().iter().any(|c| c.starts_with("delete:")));
    assert!(rp.calls().is_empty());
    assert!(vast.calls().is_empty());
}

fn saved_connection(
    d: &mut Deps,
    path: &std::path::Path,
) -> (std::path::PathBuf, Vec<u8>, std::path::PathBuf) {
    let manager =
        crate::connection::Manager::new(path.join("lobo.env"), "/missing-helper".into(), 28920)
            .unwrap();
    std::fs::create_dir_all(&manager.root).unwrap();
    let bytes = serde_json::to_vec(&serde_json::json!({
        "provider": "vast", "id": "foreign", "boot_id": "0123456789abcdef",
        "config_path": manager.config_path, "port": 28920,
        "expires_at": chrono::Utc::now() + chrono::TimeDelta::minutes(10),
    }))
    .unwrap();
    let desired = manager.root.join("desired.json");
    std::fs::write(&desired, &bytes).unwrap();
    let keys = manager.root.join("0123456789abcdef");
    std::fs::create_dir(&keys).unwrap();
    let key = keys.join("client");
    std::fs::write(&key, "foreign key").unwrap();
    d.connection = Some(Arc::new(manager));
    (desired, bytes, key)
}
#[tokio::test]
async fn local_start_status_stop_preserve_saved_cloud_connection() {
    let (mut d, _, rp, vast, owner, _) = startup_deps();
    let tmp = tempfile::tempdir().unwrap();
    let (path, bytes, key) = saved_connection(&mut d, tmp.path());
    let mut op = up_app(
        d.clone(),
        local_opts(),
        None,
        CancellationToken::new(),
        owner,
    );
    op.wait().await.unwrap();
    let t = discover_app(&d, "local").await.unwrap().unwrap();
    assert!(!snapshot_app(&d, &t).await.unwrap().down);
    down_app(&d, &t).await.unwrap();
    assert_eq!(std::fs::read(path).unwrap(), bytes);
    assert_eq!(std::fs::read_to_string(key).unwrap(), "foreign key");
    assert!(rp.calls().is_empty());
    assert!(vast.calls().is_empty());
}
#[tokio::test]
async fn cloud_stop_needs_no_agent_and_preserves_foreign_connection() {
    let (mut d, local, rp, vast) = setup();
    let tmp = tempfile::tempdir().unwrap();
    let (path, bytes, key) = saved_connection(&mut d, tmp.path());
    d.new_agent = Arc::new(|_| panic!("Stop must not access an agent"));
    rp.state.lock().unwrap().instances = vec![instance("runpod", "ours")];
    down_app(&d, &target("runpod", "ours")).await.unwrap();
    assert!(rp.state.lock().unwrap().instances.is_empty());
    assert_eq!(std::fs::read(path).unwrap(), bytes);
    assert_eq!(std::fs::read_to_string(key).unwrap(), "foreign key");
    assert!(local.calls().is_empty());
    assert!(vast.calls().is_empty());
}
#[tokio::test(start_paused = true)]
async fn unproven_cloud_uncertain_create_keeps_journal_and_never_deletes_candidate() {
    let (d, local, rp, vast, _, _) = startup_deps();
    {
        let mut s = rp.state.lock().unwrap();
        s.broken = false;
        s.uncertain_create = true;
    }
    let state = d.operations.clone();
    let mut op = up_app(
        d,
        UpOpts {
            provider: "runpod".into(),
            ..Default::default()
        },
        None,
        CancellationToken::new(),
        Arc::new(|_| Ok(())),
    );
    assert_eq!(op.wait().await.unwrap_err().kind(), "unresolved_create");
    assert_eq!(rp.state.lock().unwrap().created.len(), 1);
    assert_eq!(rp.state.lock().unwrap().instances.len(), 1);
    assert!(!rp.calls().iter().any(|c| c.starts_with("delete:")));
    assert!(
        state
            .acquire(&CancellationToken::new())
            .await
            .unwrap()
            .pending()
            .unwrap()
            .instance_id
            .is_none()
    );
    assert!(local.calls().is_empty());
    assert!(vast.calls().is_empty());
}
#[tokio::test]
async fn unproven_local_candidate_is_not_adopted_or_deleted() {
    let (d, local, _, _) = setup();
    {
        let mut s = local.state.lock().unwrap();
        s.instances.push(instance("local", "4242"));
        s.boot_id = "foreign".into();
        s.start_id = 100;
    }
    let pending = operation_state::PendingCreate {
        provider: "local".into(),
        boot_id: "ours".into(),
        before: vec![],
        instance_id: None,
    };
    assert_eq!(
        cleanup::adopt_owned(&d, &pending).await.unwrap_err().kind(),
        "unresolved_create"
    );
    assert_eq!(local.state.lock().unwrap().instances.len(), 1);
    assert!(!local.calls().iter().any(|c| c.starts_with("delete:")));
}
#[tokio::test]
async fn matching_pending_local_owner_is_recovered_before_new_start() {
    let (d, local, rp, vast, owner, _) = startup_deps();
    {
        let mut s = local.state.lock().unwrap();
        s.instances.push(instance("local", "4242"));
        s.boot_id = "owned".into();
        s.start_id = 100;
    }
    let previous = discover_app(&d, "local").await.unwrap().unwrap();
    d.operations
        .acquire(&CancellationToken::new())
        .await
        .unwrap()
        .record(operation_state::PendingCreate {
            provider: "local".into(),
            boot_id: "owned".into(),
            before: vec![],
            instance_id: Some("4242".into()),
        })
        .unwrap();
    let mut op = up_app(
        d,
        local_opts(),
        Some(previous),
        CancellationToken::new(),
        owner,
    );
    op.wait().await.unwrap();
    assert_eq!(local.state.lock().unwrap().created.len(), 1);
    assert_eq!(local.state.lock().unwrap().instances.len(), 1);
    assert!(local.calls().contains(&"owned-delete:4242:owned".into()));
    assert!(rp.calls().is_empty());
    assert!(vast.calls().is_empty());
}
