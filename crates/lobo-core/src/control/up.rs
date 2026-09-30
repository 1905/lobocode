use super::{
    cleanup,
    operation_state::{OperationGuard, PendingCreate},
    *,
};
use crate::{provider::CreateOpts, release};
use chrono::{DateTime, Utc};
use futures_util::FutureExt;
use lobo_proto::{ReadyInfo, Stage, UpEvent, catalog};
use std::{future::Future, panic::AssertUnwindSafe, sync::Mutex};
use tokio::{sync::mpsc, task::JoinHandle};
use tokio_util::sync::CancellationToken;

pub struct UpOperation {
    cancel: CancellationToken,
    events: Option<mpsc::Receiver<UpEvent>>,
    worker: Option<JoinHandle<Result<()>>>,
}
impl UpOperation {
    pub fn take_events(&mut self) -> Option<mpsc::Receiver<UpEvent>> {
        self.events.take()
    }
    pub fn cancel(&self) {
        self.cancel.cancel();
    }
    /// Retains the worker if this wait future is dropped (for example by timeout).
    /// Completion is consumed once; subsequent waits report that explicitly.
    pub async fn wait(&mut self) -> Result<()> {
        let worker = self
            .worker
            .as_mut()
            .ok_or_else(|| Error::Other("up completion was already consumed".into()))?;
        let result = worker.await;
        self.worker = None;
        result.map_err(|e| Error::Other(format!("up worker: {e}")))?
    }
}
impl Drop for UpOperation {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}
struct Events {
    tx: mpsc::Sender<UpEvent>,
    terminal: Mutex<Option<mpsc::OwnedPermit<UpEvent>>>,
}
struct PreparedConnection {
    manager: Arc<crate::connection::Manager>,
    boot_id: String,
    keep: bool,
}
impl Drop for PreparedConnection {
    fn drop(&mut self) {
        if !self.keep {
            let _ = self.manager.discard_keys(&self.boot_id);
        }
    }
}
impl Events {
    fn emit(&self, event: UpEvent) {
        // Reserve one slot for completion. Slow consumers can miss progress,
        // but cannot block cancellation, cleanup, or the final event.
        if event.done {
            if let Some(permit) = self.terminal.lock().unwrap().take() {
                permit.send(event);
            }
        } else {
            let _ = self.tx.try_send(event);
        }
    }
    fn finished(&self) -> bool {
        self.terminal.lock().unwrap().is_none()
    }

    fn phase(&self, phase: &str, detail: String) {
        self.emit(UpEvent {
            phase: phase.into(),
            detail,
            ..Default::default()
        });
    }
    fn fail(&self, phase: &str, detail: String, error: Error) -> Error {
        self.emit(UpEvent {
            phase: phase.into(),
            detail,
            err: Some(error.to_string()),
            done: true,
            ..Default::default()
        });
        error
    }
}
pub(crate) enum OperationScope {
    Cli,
    App {
        previous: Option<Box<RuntimeTarget>>,
        owner: OwnerSink,
        current: Option<Box<RuntimeTarget>>,
    },
}
impl OperationScope {
    fn owned(&self) -> bool {
        matches!(self, Self::App { .. })
    }
    fn record_owner(&mut self, target: RuntimeTarget) -> Result<()> {
        if let Self::App { owner, current, .. } = self {
            *current = Some(Box::new(target.clone()));
            owner(target)?;
        }
        Ok(())
    }
    async fn recover(
        &self,
        d: &Deps,
        guard: &mut OperationGuard<'_>,
        selected: &str,
    ) -> Result<()> {
        let Some(pending) = guard.pending() else {
            return Ok(());
        };
        match self {
            Self::Cli => cleanup::pending(d, guard).await,
            Self::App { previous, .. } => {
                if !previous.as_ref().is_some_and(|t| {
                    !t.boot_id.is_empty()
                        && t.provider == selected
                        && pending.provider == t.provider
                        && pending.boot_id == t.boot_id
                }) {
                    return Err(Error::Other(
                        "another runtime owns the pending operation; resolve it before starting"
                            .into(),
                    ));
                }
                cleanup::pending_owned(d, guard, previous.as_ref().and_then(|t| t.local_start_id))
                    .await
            }
        }
    }
    async fn cleanup(&self, d: &Deps, guard: &mut OperationGuard<'_>) -> Result<()> {
        match self {
            Self::Cli => cleanup::pending(d, guard).await,
            Self::App { current, .. } => {
                if let Some(pending) = guard.pending() {
                    if !current.as_ref().is_some_and(|t| {
                        pending.provider == t.provider && pending.boot_id == t.boot_id
                    }) {
                        return Err(Error::Other(
                            "pending operation belongs to another runtime".into(),
                        ));
                    }
                    cleanup::pending_owned(
                        d,
                        guard,
                        current.as_ref().and_then(|t| t.local_start_id),
                    )
                    .await?;
                }
                Ok(())
            }
        }
    }
}
pub fn up(d: Deps, o: UpOpts, cancel: CancellationToken) -> UpOperation {
    up_scoped(d, o, cancel, OperationScope::Cli)
}
pub(crate) fn up_scoped(
    d: Deps,
    o: UpOpts,
    cancel: CancellationToken,
    mut scope: OperationScope,
) -> UpOperation {
    let cancel = cancel.child_token();
    let worker_cancel = cancel.clone();
    let (tx, rx) = mpsc::channel(16);
    let worker = tokio::spawn(async move {
        let permit = tx
            .clone()
            .try_reserve_owned()
            .expect("fresh event channel has capacity");
        let events = Events {
            tx,
            terminal: Mutex::new(Some(permit)),
        };
        let result = async {
            let mut ownership = d.operations.acquire(&worker_cancel).await?;
            // Recover a prior crash/uncertain response before allowing a create.
            let selected = if o.provider.is_empty() {
                "runpod"
            } else {
                &o.provider
            };
            scope.recover(&d, &mut ownership, selected).await?;
            let result = AssertUnwindSafe(run(
                &d,
                o,
                &worker_cancel,
                &events,
                &mut ownership,
                &mut scope,
            ))
            .catch_unwind()
            .await;
            let panicked = result.is_err();
            let result = match result {
                Ok(result) => result,
                Err(payload) => {
                    let message = payload
                        .downcast_ref::<&str>()
                        .copied()
                        .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
                        .unwrap_or("unknown panic");
                    Err(Error::Other(format!("up worker panicked: {message}")))
                }
            };
            if panicked
                || (worker_cancel.is_cancelled() && !events.finished())
                || (scope.owned() && result.is_err())
            {
                scope.cleanup(&d, &mut ownership).await?;
                if worker_cancel.is_cancelled() {
                    return Err(Error::Cancelled);
                }
            }
            result
        };
        tokio::pin!(result);
        // Closing the receiver cancels the operation but does not drop its worker.
        let result = tokio::select! {biased;
            _=events.tx.closed()=>{worker_cancel.cancel();result.await},
            result=&mut result=>result,
        };
        if let Err(e) = &result
            && !events.finished()
        {
            events.emit(UpEvent {
                phase: if matches!(e, Error::Cancelled) {
                    "cancelled"
                } else {
                    "failed"
                }
                .into(),
                err: Some(e.to_string()),
                done: true,
                ..Default::default()
            });
        }
        result
    });
    UpOperation {
        cancel,
        events: Some(rx),
        worker: Some(worker),
    }
}
async fn read<T>(cancel: &CancellationToken, future: impl Future<Output = Result<T>>) -> Result<T> {
    tokio::select! {biased; _=cancel.cancelled()=>Err(Error::Cancelled),r=future=>r}
}
fn check_cancel(cancel: &CancellationToken) -> Result<()> {
    if cancel.is_cancelled() {
        Err(Error::Cancelled)
    } else {
        Ok(())
    }
}
fn expires(start: DateTime<Utc>, life: Duration) -> Result<DateTime<Utc>> {
    chrono::TimeDelta::from_std(life)
        .ok()
        .and_then(|d| start.checked_add_signed(d))
        .ok_or_else(|| Error::Other("bad options: max-life is out of range".into()))
}
async fn run(
    d: &Deps,
    mut o: UpOpts,
    cancel: &CancellationToken,
    events: &Events,
    ownership: &mut OperationGuard<'_>,
    scope: &mut OperationScope,
) -> Result<()> {
    check_cancel(cancel)?;
    let start = d.clock.now();
    if o.provider.is_empty() {
        o.provider = "runpod".into();
    }
    let provider = d.providers.get(&o.provider).ok_or_else(|| {
        Error::Other(if scope.owned() {
            format!(
                "Provider {} is not configured. Check its settings.",
                o.provider
            )
        } else {
            format!(
                "provider {:?} is not configured (key missing in the config? run `lobo config`)",
                o.provider
            )
        })
    })?;
    let (instances, error) = if scope.owned() {
        (read(cancel, provider.list()).await?, None)
    } else {
        tokio::select! {biased; _=cancel.cancelled()=>return Err(Error::Cancelled),r=list_all(d)=>r}
    };
    if let Some(e) = error {
        return Err(e);
    }
    if let Some(i) = instances.first() {
        return Err(Error::AlreadyRunning(if scope.owned() {
            format!(
                "{} runtime {} is already running. Stop it before starting again.",
                i.provider, i.id
            )
        } else {
            format!(
                "lobo already running: {} {} ({}). Run `lobo down` first",
                i.provider, i.id, i.status
            )
        }));
    }
    let builtin = |version: String, llama_image: String| release::Resolved {
        manifest: Manifest {
            version,
            llama_image,
            model: release::ModelRef {
                id: release::DEFAULT_MODEL.into(),
                ..Default::default()
            },
            defaults: release::DEFAULT_DEFAULTS,
            ..Default::default()
        },
        ..Default::default()
    };
    let rel = if o.provider == "local" {
        builtin("local".into(), String::new())
    } else {
        if let Some(connection) = &d.connection {
            connection.preflight()?;
        }
        if !o.release.is_empty() {
            return Err(Error::Config(
                "cloud starts use the latest public image; --release is no longer supported".into(),
            ));
        }
        if !o.source.is_empty() {
            return Err(Error::Config(
                "cloud models are included in the Docker image; --source is no longer supported"
                    .into(),
            ));
        }
        if o.conns != 0 {
            return Err(Error::Config(
                "cloud models are included in the Docker image; --conns is no longer supported"
                    .into(),
            ));
        }
        if !o.ssh_key.is_empty() {
            return Err(Error::Config(
                "cloud SSH is managed automatically; --ssh is no longer supported".into(),
            ));
        }
        if o.model.is_empty() {
            o.model = release::DEFAULT_MODEL.into();
        }
        catalog::get(&o.model).map_err(|e| Error::Other(e.to_string()))?;
        // Re-resolve on every start. Never fall back to a cached tag or config image.
        let image = if o.image.is_empty() {
            read(cancel, d.images.latest(&o.model)).await?
        } else {
            o.image.clone() // Explicit CLI development override only.
        };
        builtin("unknown".into(), image)
    };
    let defaults = &rel.manifest.defaults;
    if o.model.is_empty() {
        o.model = rel.manifest.model.id.clone();
    }
    if o.ctx == 0 {
        o.ctx = defaults.ctx;
    }
    if o.idle_min == 0 {
        o.idle_min = defaults.idle_min;
    }
    if o.max_life.is_zero() {
        o.max_life = Duration::from_secs(
            u64::try_from(defaults.max_hours)
                .ok()
                .and_then(|h| h.checked_mul(3600))
                .ok_or_else(|| Error::Other("bad options: max-life is out of range".into()))?,
        );
    }
    if o.timeout.is_zero() {
        o.timeout = Duration::from_secs(40 * 60);
    }
    if o.ctx < 512 || o.idle_min < 1 || o.max_life < Duration::from_secs(60) {
        return Err(Error::Other(format!(
            "bad options: ctx {} (min 512), idle-min {} (min 1), max-life {:?} (min 1m)",
            o.ctx, o.idle_min, o.max_life
        )));
    }
    let model = catalog::get(&o.model).map_err(|e| Error::Other(e.to_string()))?;
    let co = if o.provider == "local" {
        CreateOpts {
            lobo_api_key: d.cfg.lobo_api_key.clone(),
            model: model.id.clone(),
            ctx: o.ctx,
            idle_min: o.idle_min,
            expires_at: expires(start, o.max_life)?,
            ..Default::default()
        }
    } else {
        read(cancel, cloud_opts(d, &mut o, model, &rel, start)).await?
    };
    for attempt in 1..=MAX_GPU_RETRIES {
        check_cancel(cancel)?;
        let retry = boot(
            d,
            &**provider,
            &o,
            co.clone(),
            attempt,
            &rel.manifest.version,
            start,
            cancel,
            events,
            ownership,
            scope,
        )
        .await?;
        if !retry {
            return Ok(());
        }
        check_cancel(cancel)?;
        if attempt == MAX_GPU_RETRIES {
            return Err(Error::Other(format!(
                "gave up: {attempt} pods in a row landed on bad hosts (all deleted)"
            )));
        }
        events.phase(
            "create",
            format!(
                "bad host, renting another pod ({}/{MAX_GPU_RETRIES})",
                attempt + 1
            ),
        );
    }
    unreachable!()
}
async fn cloud_opts(
    d: &Deps,
    o: &mut UpOpts,
    m: &catalog::Model,
    rel: &release::Resolved,
    start: DateTime<Utc>,
) -> Result<CreateOpts> {
    Ok(CreateOpts {
        image: rel.manifest.llama_image.clone(),
        image_model: true,
        lobo_api_key: d.cfg.lobo_api_key.clone(),
        cf_tunnel_token: d.cfg.cf_tunnel_token.clone(),
        model: m.id.clone(),
        ctx: o.ctx,
        idle_min: o.idle_min,
        expires_at: expires(start, o.max_life)?,
        ssh_pub_key: o.ssh_key.clone(),
        cloud: o.cloud.clone(),
        min_mbps: if o.min_mbps > 0 {
            o.min_mbps
        } else {
            d.cfg
                .min_mbps
                .parse::<i64>()
                .ok()
                .filter(|n| *n > 0)
                .unwrap_or(100)
        },
        ..Default::default()
    })
}
#[allow(clippy::too_many_arguments)]
async fn boot(
    d: &Deps,
    p: &dyn Provider,
    o: &UpOpts,
    mut co: CreateOpts,
    attempt: u32,
    release_version: &str,
    start: DateTime<Utc>,
    cancel: &CancellationToken,
    events: &Events,
    ownership: &mut OperationGuard<'_>,
    scope: &mut OperationScope,
) -> Result<bool> {
    use rand::RngExt;
    co.boot_id = hex::encode(rand::rng().random::<[u8; 8]>());
    scope.record_owner(RuntimeTarget {
        provider: p.name().into(),
        boot_id: co.boot_id.clone(),
        instance_id: None,
        agent_url: None,
        api_url: None,
        local_pid: None,
        local_start_id: None,
    })?;
    let mut prepared = (p.name() != "local")
        .then(|| d.connection.clone())
        .flatten()
        .map(|manager| PreparedConnection {
            manager,
            boot_id: co.boot_id.clone(),
            keep: false,
        });
    if p.name() != "local"
        && let Some(connection) = &d.connection
    {
        read(cancel, connection.prepare(&mut co)).await?;
    }
    let before = read(cancel, p.list())
        .await?
        .into_iter()
        .map(|i| i.id)
        .collect();
    let mut pending = PendingCreate {
        provider: p.name().into(),
        boot_id: co.boot_id.clone(),
        before,
        instance_id: None,
    };
    check_cancel(cancel)?;
    ownership.record(pending.clone())?;
    if cancel.is_cancelled() {
        ownership.clear()?;
        return Err(Error::Cancelled);
    }
    // This await owns a submitted create through cancellation.
    let mut pod = match p
        .rent(&co, cancel.clone(), &|note| events.phase("create", note))
        .await
    {
        Ok(pod) => pod,
        Err(Error::UnresolvedCreate { .. }) => {
            let reconcile = async {
                if scope.owned() {
                    cleanup::adopt_owned(d, &pending).await
                } else {
                    cleanup::adopt(d, &pending).await
                }
            };
            tokio::time::timeout(cleanup::CLEANUP_TIMEOUT, reconcile)
                .await
                .map_err(|_| pending.unresolved("reconciliation timed out"))??
        }
        Err(e) => {
            if !matches!(e, Error::Multi(_)) {
                ownership.clear()?;
            }
            if matches!(e, Error::Cancelled) {
                return Err(e);
            }
            return Err(Error::Other(format!("rent on {}: {e}", p.name())));
        }
    };
    if pod.id.is_empty() {
        return Err(pending.unresolved("create response has no instance ID"));
    }
    pending.instance_id = Some(pod.id.clone());
    if let Err(error) = ownership.record(pending.clone()) {
        // A failed journal update must not abandon the returned instance.
        if scope.owned() {
            cleanup::delete_owned_verified(p, &pod.id, &co.boot_id, None, d.poll).await
        } else {
            cleanup::delete_verified(p, &pod.id, d.poll).await
        }
        .map_err(|e| pending.unresolved(format!("{error}; cleanup: {e}")))?;
        ownership.clear()?;
        return Err(error);
    }
    let mut app_target = RuntimeTarget {
        provider: p.name().into(),
        boot_id: co.boot_id.clone(),
        instance_id: Some(pod.id.clone()),
        agent_url: (!pod.agent_url.is_empty()).then(|| pod.agent_url.clone()),
        api_url: (!pod.api_url.is_empty()).then(|| pod.api_url.clone()),
        local_pid: None,
        local_start_id: None,
    };
    // Persist the returned instance before any further fallible setup.
    scope.record_owner(app_target.clone())?;
    if scope.owned() && p.name() == "local" {
        let (boot, pid, start) = p.runtime_identity(&pod.id).await?.ok_or_else(|| {
            Error::Other("created local runtime identity cannot be verified".into())
        })?;
        if boot != co.boot_id {
            return Err(Error::Other("created local runtime boot changed".into()));
        }
        app_target.local_pid = Some(pid);
        app_target.local_start_id = Some(start);
        scope.record_owner(app_target.clone())?;
    }
    check_cancel(cancel)?;
    if p.name() != "local"
        && let Some(connection) = &d.connection
        && let Err(e) = if scope.owned() {
            connection.start_owned(&mut pod, &co).await
        } else {
            connection.start(&mut pod, &co).await
        }
    {
        scope.cleanup(d, ownership).await?;
        return Err(e);
    }
    app_target.agent_url = (!pod.agent_url.is_empty()).then(|| pod.agent_url.clone());
    app_target.api_url = (!pod.api_url.is_empty()).then(|| pod.api_url.clone());
    scope.record_owner(app_target)?;
    let agent = (d.new_agent)(&pod.agent_url);
    if let Some(prepared) = &mut prepared {
        prepared.keep = true;
    }
    events.phase(
        "create",
        format!(
            "{} {}, {}, ${:.2}/h, {} ctx {}",
            p.name(),
            pod.id,
            pod.detail,
            pod.cost_per_hr,
            co.model,
            o.ctx
        ),
    );
    let poll = if d.poll.is_zero() {
        Duration::from_secs(3)
    } else {
        d.poll
    };
    let mut last_phase = String::new();
    let mut last_bytes = -1;
    let mut progress = d.clock.now();
    let mut seen = false;
    let created = d.clock.now();
    let mut last_check = created;
    loop {
        check_cancel(cancel)?;
        if d.clock.now() >= co.expires_at {
            scope.cleanup(d, ownership).await?;
            return Err(events.fail(
                "terminated",
                "maximum lifetime reached during startup".into(),
                Error::Other("maximum lifetime reached during startup; instance deleted".into()),
            ));
        }
        if p.name() != "local"
            && let Some(connection) = &d.connection
            && let Err(e) = if scope.owned() {
                connection.attach_owned(&mut pod, &co.boot_id).await
            } else {
                connection.attach(&mut pod).await
            }
        {
            scope.cleanup(d, ownership).await?;
            return Err(e);
        }
        let mut status = match read(cancel, agent.status()).await {
            Ok(s) => Some(s),
            Err(Error::Cancelled) => return Err(Error::Cancelled),
            Err(_) => None,
        };
        if status.is_none()
            && (p.name() != "local" || !scope.owned())
            && let Some(connection) = &d.connection
            && let Some(detail) = connection.detail()
        {
            events.phase(if seen { &last_phase } else { "image" }, detail);
        }
        if status
            .as_ref()
            .is_some_and(|s| (scope.owned() || !s.boot_id.is_empty()) && s.boot_id != co.boot_id)
        {
            status = None;
        }
        if status.as_ref().is_some_and(|s| {
            s.boot_id.is_empty()
                && attempt > 1
                && !seen
                && s.uptime_s as f64
                    > (d.clock.now() - created).num_milliseconds() as f64 / 1000.0
                        + STALE_SLACK.as_secs_f64()
        }) {
            status = None;
        }
        if status.is_none()
            && seen
            && (d.clock.now() - last_check).to_std().unwrap_or_default() >= POD_CHECK_EVERY
        {
            last_check = d.clock.now();
            if matches!(read(cancel, p.get(&pod.id)).await, Err(Error::NotFound)) {
                let logs = read(cancel, agent.logs(20)).await.unwrap_or_default();
                check_cancel(cancel)?;
                ownership.clear()?;
                return Err(events.fail("failed","pod is gone".into(),Error::Other(format!("pod {} is gone (deleted) while the agent was unreachable in phase {last_phase}\n{}",pod.id,logs.trim()))));
            }
        }
        let phase = if let Some(s) = &status {
            seen = true;
            s.stage.as_str()
        } else if seen {
            &last_phase
        } else {
            "image"
        }
        .to_owned();
        let download = status
            .as_ref()
            .filter(|s| matches!(s.stage, Stage::Download | Stage::Verify))
            .map(|s| s.download.clone());
        let bytes = download
            .as_ref()
            .map(|d| d.bytes)
            .unwrap_or(if status.is_none() && seen {
                last_bytes
            } else {
                0
            });
        if phase != last_phase || bytes != last_bytes {
            progress = d.clock.now();
            let detail = status
                .as_ref()
                .map(|s| s.stage_detail.clone())
                .unwrap_or_default();
            match phase.as_str() {
                "ready" => {
                    let version = read(cancel, agent.version()).await.ok();
                    check_cancel(cancel)?;
                    let timings = status.as_ref().unwrap().timings.clone();
                    let rent_s = match (pod.started_at.0, timings.container_started_at.0) {
                        (Some(a), Some(b)) => (b - a).num_milliseconds() as f64 / 1000.0,
                        _ => 0.0,
                    };
                    let ready = ReadyInfo {
                        pod_id: pod.id.clone(),
                        provider: p.name().into(),
                        detail: pod.detail.clone(),
                        attempts: attempt.into(),
                        rent_s,
                        host_download_mbps: pod.host_download_mbps,
                        timings: Some(timings),
                        url: pod.api_url.clone(),
                        version: version
                            .as_ref()
                            .map(|v| v.version.clone())
                            .unwrap_or_else(|| release_version.into()),
                        git_sha: version.map(|v| v.git_sha).unwrap_or_default(),
                        cost_per_hr: pod.cost_per_hr,
                        elapsed_ns: (d.clock.now() - start)
                            .num_nanoseconds()
                            .unwrap_or(i64::MAX),
                    };
                    ownership.clear()?;
                    events.emit(UpEvent {
                        phase,
                        detail,
                        ready: Some(ready),
                        done: true,
                        ..Default::default()
                    });
                    return Ok(false);
                }
                "failed" | "terminating" => {
                    let s = status.as_ref().unwrap();
                    if retriable(&s.stage_detail) && p.replaceable() {
                        events.phase("image", s.stage_detail.clone());
                        scope.cleanup(d, ownership).await?;
                        return Ok(true);
                    }
                    let logs = read(cancel, agent.logs(20)).await.unwrap_or_default();
                    check_cancel(cancel)?;
                    let why = if s.stage == Stage::Terminating && s.kill_reason != "failed" {
                        format!("watchdog: {}", s.kill_reason)
                    } else {
                        s.stage_detail.clone()
                    };
                    let error = if !p.replaceable() {
                        scope.cleanup(d, ownership).await?;
                        format!("local run stopped: {why} (err=none)\n{logs}")
                    } else {
                        ownership.clear()?;
                        format!("pod stopped: {why} (it deletes itself)\n{logs}")
                    };
                    return Err(events.fail("failed", why, Error::Other(error)));
                }
                _ => events.emit(UpEvent {
                    phase: phase.clone(),
                    detail,
                    download,
                    ..Default::default()
                }),
            }
            last_phase = phase.clone();
            last_bytes = bytes;
        }
        if !seen
            && (d.clock.now() - created).to_std().unwrap_or_default() > CONTAINER_TIMEOUT
            && p.replaceable()
        {
            events.phase("image", "host: container not started after 30m0s".into());
            scope.cleanup(d, ownership).await?;
            return Ok(true);
        }
        if (d.clock.now() - progress).to_std().unwrap_or_default() > o.timeout
            || (d.clock.now() - start).to_std().unwrap_or_default() > o.timeout
        {
            let logs = read(cancel, agent.logs(20)).await.unwrap_or_default();
            check_cancel(cancel)?;
            scope.cleanup(d, ownership).await?;
            return Err(events.fail(
                "terminated",
                String::new(),
                Error::Other(format!(
                    "startup exceeded {:?} in phase {phase}; pod {} deleted (err=none)\n{}",
                    o.timeout,
                    pod.id,
                    logs.trim()
                )),
            ));
        }
        tokio::select! {biased; _=cancel.cancelled()=>return Err(Error::Cancelled),_=tokio::time::sleep(poll)=>{}}
    }
}
fn retriable(detail: &str) -> bool {
    detail.starts_with("gpu: ") || detail.contains("host: ")
}
