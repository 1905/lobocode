#![deny(clippy::await_holding_lock)]
use crate::{
    backend::{Backend, Result},
    notify::Notifier,
    prefs::Prefs,
    store::Store,
    types::*,
};
use lobo_core::{
    clock::Clock,
    control::{OwnerSink, RuntimeTarget},
};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

#[derive(Default)]
struct Active {
    quitting: bool,
    up: Option<JoinHandle<Result<()>>>,
    cancel: Option<CancellationToken>,
    stop: Option<JoinHandle<()>>,
    submission_id: u64,
    config_writes: usize,
}

#[cfg(test)]
mod tests;
pub struct Controller {
    store: Mutex<Store>,
    pub backend: Arc<dyn Backend>,
    notifier: Arc<dyn Notifier>,
    clock: Arc<dyn Clock>,
    emit: Arc<dyn Fn(PanelState) + Send + Sync>,
    active: Mutex<Active>,
    shutdown: CancellationToken,
    quit_lock: tokio::sync::Mutex<()>,
    prefs_dir: Option<PathBuf>,
    runtime: tokio::runtime::Handle,
}
impl Controller {
    pub fn new(
        backend: Arc<dyn Backend>,
        notifier: Arc<dyn Notifier>,
        clock: Arc<dyn Clock>,
        prefs: Prefs,
        prefs_dir: Option<PathBuf>,
        runtime: tokio::runtime::Handle,
        emit: Arc<dyn Fn(PanelState) + Send + Sync>,
    ) -> Arc<Self> {
        let mut store = Store::new(prefs.target, lobo_core::local::supported().is_ok());
        match backend.load_owner() {
            Ok(owner) => store.set_runtime(owner),
            Err(e) => store.poll_failed(e.message),
        }
        Arc::new(Self {
            store: Mutex::new(store),
            backend,
            notifier,
            clock,
            emit,
            active: Mutex::new(Active::default()),
            shutdown: CancellationToken::new(),
            quit_lock: tokio::sync::Mutex::new(()),
            prefs_dir,
            runtime,
        })
    }
    fn change(&self, f: impl FnOnce(&mut Store) -> Vec<Note>) {
        let (s, notes) = {
            let mut s = self.store.lock().unwrap();
            let n = f(&mut s);
            (s.view(self.clock.now()), n)
        };
        (self.emit)(s);
        for n in notes {
            self.notifier.send(&n);
        }
    }
    pub fn state(&self) -> PanelState {
        self.store.lock().unwrap().view(self.clock.now())
    }
    pub fn begin_config_write(&self) {
        let mut active = self.active.lock().unwrap();
        active.config_writes += 1;
        self.store.lock().unwrap().invalidate_config();
    }
    pub fn end_config_write(&self) {
        let mut active = self.active.lock().unwrap();
        active.config_writes = active.config_writes.saturating_sub(1);
    }
    fn runtime_target(&self) -> Result<RuntimeTarget> {
        self.store
            .lock()
            .unwrap()
            .runtime()
            .filter(|target| !target.boot_id.is_empty())
            .ok_or_else(|| AppError {
                kind: "ownership".into(),
                message: "Runtime ownership is missing. Refresh before retrying.".into(),
            })
    }
    pub fn spawn_loops(self: &Arc<Self>) {
        let c = self.clone();
        self.runtime.spawn(async move {
            c.load_config(true).await;
            loop {
                tokio::select! { _=c.shutdown.cancelled()=>break, _=c.refresh(false)=>{} }
                let wait = c.store.lock().unwrap().poll_interval();
                tokio::select! {_=c.shutdown.cancelled()=>break,_=tokio::time::sleep(wait)=>{}}
            }
        });
        let c = self.clone();
        self.runtime.spawn(async move {
            let mut prev = String::new();
            let mut tick = tokio::time::interval(Duration::from_secs(1));
            loop {
                tokio::select! {_=c.shutdown.cancelled()=>break,_=tick.tick()=>{
                    let s=c.state();if s.menu_text!=prev {prev=s.menu_text.clone();(c.emit)(s);}
                }}
            }
        });
    }
    pub async fn load_config(&self, models: bool) {
        match self.backend.config().await {
            Ok((cfg, r)) => {
                let _active = self.active.lock().unwrap();
                self.change(|s| {
                    s.apply_config(cfg, r);
                    vec![]
                });
            }
            Err(e) => self.change(|s| {
                s.poll_failed(e.message);
                vec![]
            }),
        }
        if models {
            self.load_models().await;
        }
    }
    async fn load_models(&self) {
        if !self
            .state()
            .readiness
            .as_ref()
            .is_some_and(|r| r.local_supported)
        {
            return;
        }
        match self.backend.models().await {
            Ok(m) => {
                let _active = self.active.lock().unwrap();
                self.change(|s| {
                    s.apply_models(m);
                    vec![]
                });
            }
            Err(e) => self.change(|s| {
                s.set_warning(Some(e.message));
                vec![]
            }),
        }
    }
    pub async fn refresh(&self, models: bool) {
        if !self.state().readiness.is_some_and(|r| r.ready) {
            self.load_config(false).await;
        }
        if !self.state().readiness.is_some_and(|r| r.ready) {
            self.change(|s| {
                s.needs_setup();
                vec![]
            });
            self.refresh_memory().await;
            return;
        }
        let (provider, owner, generations) = {
            let s = self.store.lock().unwrap();
            if s.up_running || s.stop_running {
                return;
            }
            let view = s.view(self.clock.now());
            let provider = s.runtime().map_or_else(
                || {
                    if view.target == Target::Local {
                        "local".into()
                    } else {
                        view.provider
                    }
                },
                |owner| owner.provider,
            );
            (provider, s.runtime(), s.generations())
        };
        match self.backend.snapshot_owned(&provider, owner.clone()).await {
            Ok((candidate, snap)) => {
                let down = snap.down;
                {
                    let _active = self.active.lock().unwrap();
                    {
                        let s = self.store.lock().unwrap();
                        if s.generations() != generations || s.up_running || s.stop_running {
                            return;
                        }
                    }
                    if let Some(candidate) = &candidate
                        && let Err(e) = self.backend.adopt_owner(owner, candidate.clone())
                    {
                        self.change(|s| {
                            s.poll_failed(e.message);
                            vec![]
                        });
                        return;
                    }
                    self.change(|s| {
                        if s.generations() != generations || s.up_running || s.stop_running {
                            return vec![];
                        }
                        s.set_runtime(candidate);
                        if !s.stop_running {
                            s.set_warning(None);
                        }
                        s.apply_snap(snap, self.clock.now())
                    });
                }
                if models && down {
                    self.load_models().await;
                }
            }
            Err(e) => self.change(|s| {
                if s.generations() != generations || s.up_running || s.stop_running {
                    return vec![];
                }
                s.poll_failed(e.message);
                vec![]
            }),
        }
        self.refresh_memory().await;
    }
    pub async fn refresh_memory(&self) {
        let request = self.store.lock().unwrap().begin_memory();
        let Some((generation, request, model)) = request else {
            return;
        };
        let result = self.backend.local_memory(&model).await;
        let memory = match result {
            Ok(a) => LocalMemory::from(a),
            Err(e) => {
                let ctx = self
                    .state()
                    .config
                    .as_ref()
                    .and_then(|c| c.values.get("LOBO_CTX"))
                    .and_then(|v| v.parse().ok())
                    .filter(|ctx| *ctx > 0)
                    .unwrap_or(lobo_core::release::DEFAULT_DEFAULTS.ctx);
                LocalMemory::unavailable(model, ctx, e.message)
            }
        };
        self.change(|s| {
            s.apply_memory(generation, request, memory);
            vec![]
        });
    }
    fn schedule_memory(self: &Arc<Self>) {
        let c = self.clone();
        self.runtime.spawn(async move {
            c.refresh_memory().await;
        });
    }
    // Reserve synchronously. The returned receiver reports admission, not completion.
    pub fn submit_start(self: &Arc<Self>) -> Result<tokio::sync::oneshot::Receiver<Result<()>>> {
        let mut active = self.active.lock().unwrap();
        let (reply, response) = tokio::sync::oneshot::channel();
        let (submission, previous) = {
            let mut s = self.store.lock().unwrap();
            let v = s.view(self.clock.now());
            if active.quitting
                || active.config_writes > 0
                || self.shutdown.is_cancelled()
                || s.up_running
                || s.stop_running
                || !matches!(v.phase, Phase::Off | Phase::Failed { .. })
            {
                let _ = reply.send(Ok(()));
                return Ok(response);
            }
            (s.submit(self.clock.now()), s.runtime())
        };
        active.submission_id = active.submission_id.wrapping_add(1);
        let id = active.submission_id;
        self.store.lock().unwrap().set_submission_id(id);
        let cancel = CancellationToken::new();
        active.cancel = Some(cancel.clone());
        let c = self.clone();
        active.up = Some(self.runtime.spawn(async move {
            let result = c.run_start(id, submission, previous, cancel, reply).await;
            c.change(|s| {
                if !s.owns_submission(id) {
                    return vec![];
                }
                let notes = s.up_ended(result.as_ref().err().map(|e| e.message.as_str()));
                if let Err(e) = &result
                    && e.kind != "cancelled"
                    && !s.stop_running
                    && s.operation_launched()
                {
                    s.stop_failed(e.message.clone());
                }
                notes
            });
            if !c.store.lock().unwrap().stop_running {
                c.refresh(true).await;
            }
            result
        }));
        drop(active);
        (self.emit)(self.state());
        Ok(response)
    }
    fn check_submission(
        &self,
        active: &Active,
        id: u64,
        submission: &StartSubmission,
        cancel: &CancellationToken,
    ) -> Result<()> {
        if cancel.is_cancelled() || active.quitting || self.shutdown.is_cancelled() {
            return Err(AppError::from(lobo_core::Error::Cancelled));
        }
        if active.submission_id != id
            || active.config_writes > 0
            || !self.store.lock().unwrap().submission_valid(submission)
        {
            return Err(AppError {
                kind: "stale".into(),
                message: "Start settings changed before admission. Retry Start.".into(),
            });
        }
        Ok(())
    }
    async fn run_start(
        self: &Arc<Self>,
        id: u64,
        submission: StartSubmission,
        previous: Option<RuntimeTarget>,
        cancel: CancellationToken,
        reply: tokio::sync::oneshot::Sender<Result<()>>,
    ) -> Result<()> {
        let admitted: Result<lobo_core::control::UpOperation> = async {
            {
                let active = self.active.lock().unwrap();
                self.check_submission(&active, id, &submission, &cancel)?;
            }
            let backend = self.backend.clone();
            let request = submission.request.clone();
            let prepared = tokio::task::spawn_blocking(move || backend.prepare_up(request))
                .await
                .map_err(|e| AppError {
                    kind: "app".into(),
                    message: format!("Start admission worker: {e}"),
                })??;
            let active = self.active.lock().unwrap();
            self.check_submission(&active, id, &submission, &cancel)?;
            let c = self.clone();
            let owner: OwnerSink = Arc::new(move |target| {
                // No Active lock: core invokes this callback from its owned worker.
                c.change(|s| {
                    if s.owns_submission(id) {
                        s.set_runtime(Some(target));
                    }
                    vec![]
                });
                Ok(())
            });
            let operation = self.backend.up(prepared, previous, cancel, owner)?;
            self.store.lock().unwrap().mark_launched();
            Ok(operation)
        }
        .await;
        let mut operation = match admitted {
            Ok(operation) => {
                let _ = reply.send(Ok(()));
                operation
            }
            Err(e) => {
                let _ = reply.send(Err(AppError {
                    kind: e.kind.clone(),
                    message: e.message.clone(),
                }));
                self.change(|s| {
                    s.invalidate_memory();
                    vec![]
                });
                return Err(e);
            }
        };
        let mut events = operation.take_events().expect("new up has events");
        while let Some(ev) = events.recv().await {
            self.change(|s| {
                if s.owns_submission(id) {
                    s.handle_event(&ev, self.clock.now())
                } else {
                    vec![]
                }
            });
        }
        operation.wait().await.map_err(AppError::from)
    }
    pub fn stop(self: &Arc<Self>) {
        let mut active = self.active.lock().unwrap();
        let captured = self.runtime_target().ok();
        let (owner, was_up) = {
            let mut s = self.store.lock().unwrap();
            if s.stop_running {
                return;
            }
            let up = s.up_running;
            let owner = captured;
            s.begin_stop();
            (owner, up)
        };
        let id = active.submission_id;
        if let Some(cancel) = active.cancel.take() {
            cancel.cancel();
        }
        let up = active.up.take();
        let c = self.clone();
        active.stop = Some(self.runtime.spawn(async move {
            if let Some(mut task) = up {
                let result = match tokio::time::timeout(Duration::from_secs(120), &mut task).await {
                    Ok(r) => r,
                    Err(_) => {
                        c.change(|s| {
                            s.set_warning(Some("stop: cleanup is still running".into()));
                            vec![]
                        });
                        task.await
                    }
                };
                let error = match result {
                    Ok(Err(e))
                        if was_up
                            && e.kind != "cancelled"
                            && c.store.lock().unwrap().operation_launched() =>
                    {
                        Some(e.message)
                    }
                    Err(e) => Some(format!("up worker: {e}")),
                    _ => None,
                };
                if let Some(e) = error {
                    c.change(|s| {
                        s.stop_failed(e);
                        vec![]
                    });
                    return;
                }
            }
            // A cancelled create can publish ownership after Stop captured it.
            // Startup remains reserved until this task completes, so this is the same job.
            let target = if c.active.lock().unwrap().submission_id == id {
                c.store.lock().unwrap().runtime().or(owner)
            } else {
                owner
            };
            let result = async {
                let mut final_snap = lobo_proto::Snap {
                    down: true,
                    ..Default::default()
                };
                if let Some(target) = &target {
                    c.backend.down(target.clone()).await?;
                    if target.provider != "local" {
                        tokio::time::sleep(Duration::from_secs(10)).await;
                        c.backend.down(target.clone()).await?;
                    }
                    let (_, snap) = c
                        .backend
                        .snapshot_owned(&target.provider, Some(target.clone()))
                        .await?;
                    if !snap.down {
                        return Err(AppError {
                            kind: "cleanup".into(),
                            message: "Runtime is still running after Stop.".into(),
                        });
                    }
                    final_snap = snap;
                }
                Ok::<_, AppError>(final_snap)
            }
            .await;
            c.change(|s| {
                match result {
                    Ok(snap) => {
                        if s.runtime() == target {
                            s.set_runtime(None);
                        }
                        s.apply_snap(snap, c.clock.now());
                        s.stop_done();
                    }
                    Err(e) => s.stop_failed(format!("down: {}", e.message)),
                }
                vec![]
            });
            // Do not discover the UI-selected provider during Stop's final refresh.
        }));
        drop(active);
        (self.emit)(self.state());
    }
    /// Quit cancels pending startup and retains cleanup ownership. A ready runtime stays up.
    pub async fn quit(self: &Arc<Self>) -> Result<()> {
        let _quit = self.quit_lock.lock().await;
        self.active.lock().unwrap().quitting = true;
        let result = self.finish_quit().await;
        if result.is_ok() {
            self.shutdown.cancel();
        } else {
            // Keep polling and allow a cleanup retry after a failed Quit.
            self.active.lock().unwrap().quitting = false;
        }
        result
    }
    async fn finish_quit(self: &Arc<Self>) -> Result<()> {
        let (pending, ready) = {
            let s = self.store.lock().unwrap();
            (
                s.needs_cleanup(),
                s.view(self.clock.now()).phase == Phase::Ready && !s.stop_running,
            )
        };
        if ready {
            let task = self.active.lock().unwrap().up.take();
            if let Some(task) = task {
                task.await.map_err(|e| AppError {
                    kind: "other".into(),
                    message: format!("up worker: {e}"),
                })??;
            }
            return Ok(());
        }
        if pending {
            self.stop();
        }
        let task = self.active.lock().unwrap().stop.take();
        if let Some(task) = task {
            task.await.map_err(|e| AppError {
                kind: "other".into(),
                message: format!("stop worker: {e}"),
            })?;
        }
        if pending && let Phase::Failed { message } = self.state().phase {
            return Err(AppError {
                kind: "cleanup".into(),
                message,
            });
        }
        Ok(())
    }
    pub fn dismiss(self: &Arc<Self>) {
        let busy = {
            let s = self.store.lock().unwrap();
            s.up_running || s.stop_running
        };
        if busy {
            return;
        }
        self.change(|s| {
            s.dismiss();
            vec![]
        });
        let c = self.clone();
        self.runtime.spawn(async move {
            c.refresh(false).await;
        });
    }
    pub fn choose(self: &Arc<Self>, t: Target) {
        let _active = self.active.lock().unwrap();
        self.change(|s| {
            s.choose(t);
            vec![]
        });
        self.schedule_memory();
        if let Some(dir) = &self.prefs_dir
            && let Err(e) = (Prefs {
                target: Some(self.state().target),
            })
            .save(dir)
        {
            self.change(|s| {
                s.set_warning(Some(e.to_string()));
                vec![]
            });
        }
    }
    pub fn set_provider(&self, p: String) {
        let _active = self.active.lock().unwrap();
        self.change(|s| {
            s.set_provider(p);
            vec![]
        });
    }
    pub fn set_model(self: &Arc<Self>, m: String) {
        let _active = self.active.lock().unwrap();
        self.change(|s| {
            s.set_model(m);
            vec![]
        });
        self.schedule_memory();
    }
    pub fn panel_shown(self: &Arc<Self>, open: bool) {
        self.change(|s| {
            s.set_panel_open(open);
            vec![]
        });
        if open {
            let c = self.clone();
            self.runtime.spawn(async move {
                c.refresh(true).await;
            });
        }
    }
}
