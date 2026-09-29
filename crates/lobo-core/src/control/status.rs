use super::*;
use lobo_proto::{GoTime, Snap};
pub async fn snapshot(d: &Deps) -> Result<Snap> {
    let (instances, error) = list_all(d).await;
    if let Some(e) = error {
        return Err(e);
    }
    let Some(instance) = instances.into_iter().next() else {
        return Ok(Snap {
            down: true,
            at: GoTime::from_utc(d.clock.now()),
            ..Default::default()
        });
    };
    let at = GoTime::from_utc(d.clock.now());
    if instance.provider != "local" && d.cfg.domain.is_empty() {
        return Ok(Snap {
            pod: Some(instance),
            at,
            ..Default::default()
        });
    }
    let agent = (d.new_agent)(&instance.agent_url);
    let status = agent.status().await.ok();
    let version = agent.version().await.ok();
    Ok(Snap {
        pod: Some(instance),
        at,
        status,
        version,
        down: false,
    })
}
pub async fn down(d: &Deps) -> Result<f64> {
    let mut operation = d
        .operations
        .acquire(&tokio_util::sync::CancellationToken::new())
        .await?;
    let (instances, error) = list_all(d).await;
    let mut errors = vec![];
    let mut spent = 0.0;
    if let Some(e) = error {
        errors.push(Error::Other(format!("list: {e}")));
    }
    // Keep the initial list for spend, even if reconciliation deletes one now.
    if let Err(e) = super::cleanup::pending(d, &mut operation).await {
        errors.push(e);
    }
    for instance in instances {
        if let Some(start) = instance.started_at.0 {
            spent += instance.cost_per_hr
                * (d.clock
                    .now()
                    .signed_duration_since(start)
                    .num_milliseconds() as f64
                    / 3_600_000.0);
        }
        let result = match d.providers.get(&instance.provider) {
            Some(p) => p.delete(&instance.id).await,
            None => Err(Error::Other("instance provider is not configured".into())),
        };
        if let Err(e) = result {
            errors.push(Error::Other(format!(
                "delete {} {}: {e}",
                instance.provider, instance.id
            )));
        }
    }
    let wait = if d.poll.is_zero() {
        Duration::from_secs(2)
    } else {
        d.poll
    };
    let mut left = vec![];
    for n in 0..5 {
        if n > 0 {
            tokio::time::sleep(wait).await;
        }
        let (instances, error) = list_all(d).await;
        left.clear();
        if let Some(e) = error {
            left.push(Error::Other(format!("list after delete: {e}")));
        }
        if !instances.is_empty() {
            left.push(Error::Other(format!(
                "lobo instances still listed after delete: {}",
                instances.len()
            )));
        }
        if left.is_empty() {
            break;
        }
    }
    errors.extend(left);
    if errors.is_empty() {
        Ok(spent)
    } else {
        Err(Error::Multi(errors))
    }
}
pub async fn target(d: &Deps) -> Result<(Arc<dyn AgentApi>, String)> {
    let (instances, error) = list_all(d).await;
    if let Some(instance) = instances.first() {
        return Ok(((d.new_agent)(&instance.agent_url), instance.api_url.clone()));
    }
    if let Some(e) = error {
        return Err(e);
    }
    if d.cfg.domain.is_empty() {
        return Err(Error::Other(
            "nothing running, and LOBO_DOMAIN is empty: start one with `lobo up`".into(),
        ));
    }
    Ok((
        (d.new_agent)(&format!("https://{}", d.cfg.domain)),
        format!("https://{}/v1", d.cfg.domain),
    ))
}
