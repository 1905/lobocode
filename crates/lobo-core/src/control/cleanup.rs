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
pub(crate) async fn pending(d: &Deps, guard: &mut OperationGuard<'_>) -> Result<()> {
    let Some(mut pending) = guard.pending().cloned() else {
        return Ok(());
    };
    if pending.instance_id.is_none() {
        let instance = tokio::time::timeout(CLEANUP_TIMEOUT, adopt(d, &pending))
            .await
            .map_err(|_| pending.unresolved("reconciliation timed out"))??;
        pending.instance_id = Some(instance.id);
        guard.record(pending.clone())?;
    }
    let provider = d
        .providers
        .get(&pending.provider)
        .ok_or_else(|| pending.unresolved("provider is no longer configured"))?;
    delete_verified(&**provider, pending.instance_id.as_deref().unwrap(), d.poll)
        .await
        .map_err(|e| pending.unresolved(e.to_string()))?;
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
        let local = Arc::new(FakeLocal::default());
        local.state.lock().unwrap().running = vec![
            Instance {
                provider: "local".into(),
                id: "old".into(),
                ..Default::default()
            },
            Instance {
                provider: "local".into(),
                id: "new".into(),
                ..Default::default()
            },
        ];
        d.providers.insert("local".into(), local.clone());
        let state = OperationState::memory();
        let mut g = state.acquire(&CancellationToken::new()).await.unwrap();
        g.record(PendingCreate {
            provider: "local".into(),
            boot_id: "b".into(),
            before: vec!["old".into()],
            instance_id: None,
        })
        .unwrap();
        pending(&d, &mut g).await.unwrap();
        assert!(g.pending().is_none());
        let s = local.state.lock().unwrap();
        assert_eq!(s.deleted, vec!["new"]);
        assert_eq!(s.running.len(), 1);
    }
    #[tokio::test]
    async fn ambiguous_listing_deletes_nothing() {
        let mut d = setup();
        let local = Arc::new(FakeLocal::default());
        local.state.lock().unwrap().running = vec![
            Instance {
                provider: "local".into(),
                id: "one".into(),
                ..Default::default()
            },
            Instance {
                provider: "local".into(),
                id: "two".into(),
                ..Default::default()
            },
        ];
        d.providers.insert("local".into(), local.clone());
        let mut g = d
            .operations
            .acquire(&CancellationToken::new())
            .await
            .unwrap();
        g.record(PendingCreate {
            provider: "local".into(),
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
        assert!(local.state.lock().unwrap().deleted.is_empty());
    }
}
