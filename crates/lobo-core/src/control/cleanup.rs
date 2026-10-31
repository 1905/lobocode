use super::{
    Deps, Error, Result,
    operation_state::{OperationGuard, PendingCreate},
};
use crate::provider::{Instance, Provider};
use std::time::Duration;
pub const CLEANUP_TIMEOUT: Duration = Duration::from_secs(120);
const RECONCILE_WAIT: Duration = Duration::from_secs(3);

// Read-only reconciliation can continue after cancellation. It never starts a
// second create. A missing record in one list does not prove the POST failed.
pub(crate) async fn adopt(d: &Deps, pending: &PendingCreate) -> Result<Instance> {
    if !matches!(pending.provider.as_str(), "runpod" | "vast") {
        return Err(pending.unresolved("provider is no longer supported"));
    }
    let p = d
        .providers
        .get(&pending.provider)
        .ok_or_else(|| pending.unresolved("provider is no longer configured"))?;
    let mut detail = "no new instance is visible; refusing another create".to_owned();
    for attempt in 0..3 {
        if attempt > 0 {
            tokio::time::sleep(RECONCILE_WAIT).await;
        }
        match p.list().await {
            Ok(list) => {
                let mut candidates = list.into_iter().filter(|i| {
                    i.provider == pending.provider
                        && !i.id.is_empty()
                        && !pending.before.contains(&i.id)
                });
                if let Some(instance) = candidates.next() {
                    if candidates.next().is_some() {
                        return Err(
                            pending.unresolved("multiple new instances; ownership is uncertain")
                        );
                    }
                    return Ok(instance);
                }
            }
            Err(e) => detail = format!("cannot reconcile: {e}"),
        }
    }
    Err(pending.unresolved(detail))
}
/// App reconciliation needs proof of boot ownership, not only a new list ID.
/// Shared legacy domains do not bind an agent response to a provider instance.
pub(crate) async fn adopt_owned(d: &Deps, pending: &PendingCreate) -> Result<Instance> {
    let instance = adopt(d, pending).await?;
    let proven = d
        .connection
        .as_ref()
        .map(|c| c.recorded(&pending.provider, &instance.id))
        .transpose()?
        .flatten()
        .is_some_and(|(boot, _)| !boot.is_empty() && boot == pending.boot_id);
    if !proven {
        return Err(pending.unresolved(
            "new instance boot ownership cannot be proved; refusing deletion or another create",
        ));
    }
    Ok(instance)
}
pub(crate) async fn delete_verified(
    provider: &dyn Provider,
    id: &str,
    poll: Duration,
) -> Result<()> {
    let operation = async {
        provider.delete(id).await?;
        for n in 0..5 {
            if n > 0 {
                tokio::time::sleep(if poll.is_zero() {
                    Duration::from_secs(2)
                } else {
                    poll
                })
                .await;
            }
            match provider.get(id).await {
                Err(Error::NotFound) => return Ok(()),
                Err(e) => return Err(e),
                Ok(_) => {}
            }
        }
        Err(Error::Other(format!(
            "instance {id} is still present after delete"
        )))
    };
    tokio::time::timeout(CLEANUP_TIMEOUT, operation)
        .await
        .map_err(|_| Error::Other(format!("cleanup timed out for {} {id}", provider.name())))?
}
pub(crate) async fn delete_owned_verified(
    provider: &dyn Provider,
    id: &str,
    boot_id: &str,
    poll: Duration,
) -> Result<()> {
    let operation = async {
        provider.delete_owned(id, boot_id).await?;
        for n in 0..5 {
            if n > 0 {
                tokio::time::sleep(if poll.is_zero() {
                    Duration::from_secs(2)
                } else {
                    poll
                })
                .await;
            }
            match provider.get(id).await {
                Err(Error::NotFound) => return Ok(()),
                Err(e) => return Err(e),
                Ok(_) => {}
            }
        }
        Err(Error::Other(format!(
            "owned instance {id} is still present after delete"
        )))
    };
    tokio::time::timeout(CLEANUP_TIMEOUT, operation)
        .await
        .map_err(|_| Error::Other(format!("cleanup timed out for {} {id}", provider.name())))?
}
pub(crate) async fn pending_owned(d: &Deps, guard: &mut OperationGuard<'_>) -> Result<()> {
    pending_scoped(d, guard, true).await
}
pub(crate) async fn pending(d: &Deps, guard: &mut OperationGuard<'_>) -> Result<()> {
    pending_scoped(d, guard, false).await
}
async fn pending_scoped(d: &Deps, guard: &mut OperationGuard<'_>, owned: bool) -> Result<()> {
    let Some(mut pending) = guard.pending().cloned() else {
        return Ok(());
    };
    if !matches!(pending.provider.as_str(), "runpod" | "vast") {
        return Err(pending.unresolved("provider is no longer supported"));
    }
    if pending.instance_id.is_none() {
        let reconcile = async {
            if owned {
                adopt_owned(d, &pending).await
            } else {
                adopt(d, &pending).await
            }
        };
        let instance = tokio::time::timeout(CLEANUP_TIMEOUT, reconcile)
            .await
            .map_err(|_| pending.unresolved("reconciliation timed out"))??;
        pending.instance_id = Some(instance.id);
        guard.record(pending.clone())?;
    }
    let provider = d
        .providers
        .get(&pending.provider)
        .ok_or_else(|| pending.unresolved("provider is no longer configured"))?;
    let id = pending.instance_id.as_deref().unwrap();
    if owned {
        delete_owned_verified(&**provider, id, &pending.boot_id, d.poll).await
    } else {
        delete_verified(&**provider, id, d.poll).await
    }
    .map_err(|e| pending.unresolved(e.to_string()))?;
    if let Some(connection) = &d.connection {
        if owned {
            connection
                .stop_owned(&pending.provider, id, &pending.boot_id)
                .await?;
        } else {
            connection.stop(&pending.provider, id).await?;
        }
    }
    if let Some(connection) = &d.connection {
        connection.discard_keys(&pending.boot_id)?;
    }
    guard.clear()
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        clock::SystemClock,
        control::{OperationState, testkit::*},
    };
    use std::sync::Arc;
    use tokio_util::sync::CancellationToken;
    fn setup() -> Deps {
        deps(
            Arc::new(FakeRunPod::default()),
            Arc::new(FakeAgent::default()),
            Arc::new(SystemClock),
        )
    }
    #[tokio::test(start_paused = true)]
    async fn empty_listing_keeps_uncertain_record() {
        let d = setup();
        let mut g = d
            .operations
            .acquire(&CancellationToken::new())
            .await
            .unwrap();
        g.record(PendingCreate {
            provider: "runpod".into(),
            boot_id: "b".into(),
            before: vec![],
            instance_id: None,
        })
        .unwrap();
        let e = pending(&d, &mut g).await.unwrap_err();
        assert_eq!(e.kind(), "unresolved_create");
        assert!(g.pending().is_some());
    }
    #[tokio::test]
    async fn reconcile_deletes_single_new_instance_only() {
        let mut d = setup();
        let local = Arc::new(RecordingProvider::new("vast"));
        local.state.lock().unwrap().instances = vec![
            Instance {
                provider: "vast".into(),
                id: "old".into(),
                ..Default::default()
            },
            Instance {
                provider: "vast".into(),
                id: "new".into(),
                ..Default::default()
            },
        ];
        d.providers.insert("vast".into(), local.clone());
        let state = OperationState::memory();
        let mut g = state.acquire(&CancellationToken::new()).await.unwrap();
        g.record(PendingCreate {
            provider: "vast".into(),
            boot_id: "b".into(),
            before: vec!["old".into()],
            instance_id: None,
        })
        .unwrap();
        pending(&d, &mut g).await.unwrap();
        assert!(g.pending().is_none());
        let s = local.state.lock().unwrap();
        assert!(s.calls.contains(&"delete:new".into()));
        assert_eq!(s.instances.len(), 1);
    }
    #[tokio::test]
    async fn ambiguous_listing_deletes_nothing() {
        let mut d = setup();
        let local = Arc::new(RecordingProvider::new("vast"));
        local.state.lock().unwrap().instances = vec![
            Instance {
                provider: "vast".into(),
                id: "one".into(),
                ..Default::default()
            },
            Instance {
                provider: "vast".into(),
                id: "two".into(),
                ..Default::default()
            },
        ];
        d.providers.insert("vast".into(), local.clone());
        let mut g = d
            .operations
            .acquire(&CancellationToken::new())
            .await
            .unwrap();
        g.record(PendingCreate {
            provider: "vast".into(),
            boot_id: "b".into(),
            before: vec![],
            instance_id: None,
        })
        .unwrap();
        assert!(
            pending(&d, &mut g)
                .await
                .unwrap_err()
                .to_string()
                .contains("multiple")
        );
        assert!(!local.calls().iter().any(|call| call.starts_with("delete:")));
    }
}
