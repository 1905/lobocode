//! Operations scoped to the runtime recorded by the native app.
use super::*;
use lobo_proto::Snap;
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct RuntimeTarget {
    pub provider: String,
    pub instance_id: Option<String>,
    pub boot_id: String,
    pub agent_url: Option<String>,
    pub api_url: Option<String>,
    pub local_pid: Option<i32>,
    pub local_start_id: Option<u64>,
}
pub type OwnerSink = Arc<dyn Fn(RuntimeTarget) -> Result<()> + Send + Sync>;
pub fn up_app(
    d: Deps,
    opts: UpOpts,
    previous: Option<RuntimeTarget>,
    cancel: CancellationToken,
    owner: OwnerSink,
) -> UpOperation {
    super::up::up_scoped(
        d,
        opts,
        cancel,
        super::up::OperationScope::App {
            previous: previous.map(Box::new),
            owner,
            current: None,
        },
    )
}
fn provider<'a>(d: &'a Deps, name: &str) -> Result<&'a Arc<dyn Provider>> {
    d.providers
        .get(name)
        .ok_or_else(|| Error::Config(format!("provider {name} is not configured")))
}
fn nonempty(value: &str) -> Option<String> {
    (!value.is_empty()).then(|| value.into())
}
fn check_boot(target: &RuntimeTarget) -> Result<()> {
    if target.boot_id.is_empty() {
        return Err(Error::Other("runtime boot identity is missing".into()));
    }
    Ok(())
}
pub async fn discover_app(d: &Deps, name: &str) -> Result<Option<RuntimeTarget>> {
    let p = provider(d, name)?;
    let mut instances = p.list().await?;
    if instances.len() > 1 {
        return Err(Error::Other(format!(
            "ambiguous runtime ownership for {name}"
        )));
    }
    let Some(mut i) = instances.pop() else {
        return Ok(None);
    };
    if i.provider != name || i.id.is_empty() {
        return Err(Error::Other(
            "provider returned an invalid runtime identity".into(),
        ));
    }
    let mut local_pid = None;
    let mut local_start_id = None;
    let boot_id = if name == "local" {
        let (boot, pid, start) = p
            .runtime_identity(&i.id)
            .await?
            .ok_or_else(|| Error::Other("local runtime identity cannot be verified".into()))?;
        local_pid = Some(pid);
        local_start_id = Some(start);
        boot
    } else if let Some((boot, port)) = d
        .connection
        .as_ref()
        .map(|c| c.recorded(name, &i.id))
        .transpose()?
        .flatten()
    {
        let agent_port = port.checked_add(1).filter(|_| port > 0).ok_or_else(|| {
            Error::Config("Saved cloud connection has no valid adjacent agent port.".into())
        })?;
        i.agent_url = format!("http://127.0.0.1:{agent_port}");
        i.api_url = format!("http://127.0.0.1:{port}/v1");
        boot
    } else {
        // Legacy domains can report another provider's runtime. A domain
        // response does not prove ownership of the listed instance ID.
        return Err(Error::Other(
            "Cloud runtime has no matching saved connection identity.".into(),
        ));
    };
    let target = RuntimeTarget {
        provider: name.into(),
        instance_id: Some(i.id),
        boot_id,
        agent_url: nonempty(&i.agent_url),
        api_url: nonempty(&i.api_url),
        local_pid,
        local_start_id,
    };
    check_boot(&target)?;
    Ok(Some(target))
}
async fn owned_instance(d: &Deps, target: &RuntimeTarget) -> Result<Option<Instance>> {
    check_boot(target)?;
    let Some(id) = &target.instance_id else {
        return Ok(None);
    };
    let p = provider(d, &target.provider)?;
    let mut i = match p.get(id).await {
        Ok(i) => i,
        Err(Error::NotFound) => return Ok(None),
        Err(e) => return Err(e),
    };
    if i.provider != target.provider || i.id != *id {
        return Err(Error::Other("provider returned a different runtime".into()));
    }
    if target.provider == "local" {
        let Some((boot, pid, start)) = p.runtime_identity(id).await? else {
            return Ok(None);
        };
        if boot != target.boot_id
            || Some(pid) != target.local_pid
            || Some(start) != target.local_start_id
        {
            return Ok(None);
        }
    } else if let Some(connection) = &d.connection {
        connection.attach_owned(&mut i, &target.boot_id).await?;
    }
    Ok(Some(i))
}
pub async fn snapshot_app(d: &Deps, target: &RuntimeTarget) -> Result<Snap> {
    let at = lobo_proto::GoTime::from_utc(d.clock.now());
    let Some(instance) = owned_instance(d, target).await? else {
        return Ok(Snap {
            down: true,
            at,
            ..Default::default()
        });
    };
    let agent = (d.new_agent)(&instance.agent_url);
    let status = match agent.status().await {
        Ok(s) if s.boot_id == target.boot_id => Some(s),
        Ok(_) => return Err(Error::Other("runtime boot identity changed".into())),
        Err(_) => None,
    };
    let version = if status.is_some() {
        agent.version().await.ok()
    } else {
        None
    };
    Ok(Snap {
        pod: Some(instance),
        at,
        status,
        version,
        down: false,
    })
}
pub async fn down_app(d: &Deps, target: &RuntimeTarget) -> Result<f64> {
    check_boot(target)?;
    let mut guard = d.operations.acquire(&CancellationToken::new()).await?;
    if guard.pending().is_some_and(|pending| {
        pending.provider == target.provider && pending.boot_id == target.boot_id
    }) {
        cleanup::pending_owned(d, &mut guard, target.local_start_id).await?;
    }
    let Some(id) = target.instance_id.as_deref() else {
        return Ok(0.0);
    };
    let p = provider(d, &target.provider)?;
    let instance = match p.get(id).await {
        Ok(i) => Some(i),
        Err(Error::NotFound) => None,
        Err(e) => return Err(e),
    };
    if let Some(i) = &instance {
        if i.provider != target.provider || i.id != id {
            return Err(Error::Other("provider returned a different runtime".into()));
        }
        if target.provider == "local" {
            let Some((boot, pid, start)) = p.runtime_identity(id).await? else {
                return Ok(0.0);
            };
            if boot != target.boot_id
                || Some(pid) != target.local_pid
                || Some(start) != target.local_start_id
            {
                return Ok(0.0);
            }
        }
        cleanup::delete_owned_verified(&**p, id, &target.boot_id, target.local_start_id, d.poll)
            .await?;
    }
    if target.provider != "local"
        && let Some(connection) = &d.connection
    {
        connection
            .stop_owned(&target.provider, id, &target.boot_id)
            .await?;
    }
    Ok(instance
        .and_then(|i| {
            i.started_at.0.map(|start| {
                i.cost_per_hr
                    * d.clock
                        .now()
                        .signed_duration_since(start)
                        .num_milliseconds() as f64
                    / 3_600_000.0
            })
        })
        .unwrap_or(0.0))
}
pub async fn sample_app(d: &Deps, target: &RuntimeTarget) -> Result<Status> {
    check_boot(target)?;
    let endpoint = target
        .agent_url
        .as_deref()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| Error::Other("runtime agent endpoint is missing".into()))?;
    let status = (d.new_agent)(endpoint).status().await?;
    if status.boot_id != target.boot_id {
        return Err(Error::Other("runtime boot identity changed".into()));
    }
    Ok(status)
}
#[cfg(test)]
mod tests;
