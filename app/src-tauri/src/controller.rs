#![deny(clippy::await_holding_lock)]
use crate::{
    backend::{Backend, Result},
    notify::Notifier,
    opencode,
    prefs::Prefs,
    store::Store,
    types::*,
};
use lobo_core::{
    clock::Clock,
    control::{OwnerSink, RuntimeTarget},
};
use std::{
    cell::RefCell,
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
    setup_id: u64,
    setup: Option<u64>,
}

struct SetupReservation {
    controller: Arc<Controller>,
    id: u64,
}
impl Drop for SetupReservation {
    fn drop(&mut self) {
        let mut active = self.controller.active.lock().unwrap();
        if active.setup == Some(self.id) {
            active.setup = None;
        }
    }
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
    runtime: tokio::runtime::Handle,
}
impl Controller {
    pub fn new(
        backend: Arc<dyn Backend>,
        notifier: Arc<dyn Notifier>,
        clock: Arc<dyn Clock>,
        _prefs: Prefs,
        _prefs_dir: Option<PathBuf>,
        runtime: tokio::runtime::Handle,
        emit: Arc<dyn Fn(PanelState) + Send + Sync>,
    ) -> Arc<Self> {
        let mut store = Store::new();
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
    fn ready_cloud_owner(&self) -> Result<RuntimeTarget> {
        let store = self.store.lock().unwrap();
        let view = store.view(self.clock.now());
        let owner = store.runtime().ok_or_else(|| AppError {
            kind: "ownership".into(),
            message: "Start a cloud runtime and wait for Ready before copying its API key.".into(),
        })?;
        crate::backend::require_cloud_provider(&owner.provider)?;
        let matches = view.snap.as_ref().is_some_and(|snap| {
            !snap.down
                && snap.pod.as_ref().is_some_and(|pod| {
                    pod.provider == owner.provider && Some(&pod.id) == owner.instance_id.as_ref()
                })
                && snap.status.as_ref().is_some_and(|status| {
                    status.stage == lobo_proto::Stage::Ready
                        && status.boot_id == owner.boot_id
                        && !owner.boot_id.is_empty()
                })
        });
        if view.phase != Phase::Ready || store.up_running || store.stop_running || !matches {
            return Err(AppError {
                kind: "ownership".into(),
                message: "Wait for the owned cloud runtime to be Ready before copying its API key."
                    .into(),
            });
        }
        Ok(owner)
    }
    pub async fn copy_api_key(&self) -> Result<String> {
        let owner = self.ready_cloud_owner()?;
        let key = self.backend.api_key().await?;
        if self.ready_cloud_owner()? != owner {
            return Err(AppError {
                kind: "ownership".into(),
                message: "Runtime changed while reading its API key. Retry the copy.".into(),
            });
        }
        Ok(key)
    }
    pub fn opencode_info(&self, selected: Option<String>) -> Result<OpenCodeInfo> {
        // Discover the path even when there is no runtime. No provider call here.
        let path = match selected {
            Some(path) => {
                let path = PathBuf::from(path);
                opencode::config_path(&path, true)?;
                path
            }
            None => self.backend.opencode_path()?,
        };
        let mut info = OpenCodeInfo {
            path: path.to_string_lossy().into_owned(),
            endpoint: None,
            provider: None,
            model_alias: None,
            context: None,
            can_configure: false,
            reason: Some("Start Lobocode and wait for Ready before configuring OpenCode.".into()),
            warnings: vec![],
        };
        let binding = {
            let active = self.active.lock().unwrap();
            let store = self.store.lock().unwrap();
            let view = store.view(self.clock.now());
            if active.quitting
                || self.shutdown.is_cancelled()
                || store.stop_running
                || store.up_running
                || view.phase != Phase::Ready
            {
                return Ok(info);
            }
            match (store.runtime(), view.snap.as_ref()) {
                (Some(owner), Some(snap)) => {
                    opencode::binding_from_snap(&owner, snap, String::new())
                }
                _ => Err(opencode::error(
                    "Ready runtime metadata is unavailable. Refresh and retry.",
                )),
            }
        };
        match binding {
            Ok(binding) => {
                info.endpoint = Some(binding.endpoint);
                info.provider = Some(binding.provider.clone());
                info.model_alias = Some(binding.model_alias);
                info.context = Some(binding.context);
                info.warnings = match opencode::warnings(&path, &binding.provider) {
                    Ok(warnings) => warnings,
                    Err(e) => {
                        info.reason = Some(e.message);
                        return Ok(info);
                    }
                };
                info.can_configure = true;
                info.reason = None;
            }
            Err(e) => info.reason = Some(e.message),
        }
        Ok(info)
    }
    pub async fn configure_opencode(
        self: &Arc<Self>,
        path: String,
        make_default: bool,
    ) -> Result<OpenCodeResult> {
        let path = PathBuf::from(path);
        opencode::config_path(&path, false)?;
        let (reservation, owner, generations, expected) = {
            let mut active = self.active.lock().unwrap();
            let store = self.store.lock().unwrap();
            let view = store.view(self.clock.now());
            if active.quitting
                || self.shutdown.is_cancelled()
                || active.config_writes > 0
                || active.setup.is_some()
                || store.stop_running
                || store.up_running
                || view.phase != Phase::Ready
            {
                return Err(opencode::error(
                    "OpenCode setup requires an idle Ready runtime. Retry after the current operation.",
                ));
            }
            let owner = store.runtime().ok_or_else(|| {
                opencode::error("Runtime ownership is missing. Refresh and retry.")
            })?;
            let expected = opencode::binding_from_snap(
                &owner,
                view.snap.as_ref().ok_or_else(|| {
                    opencode::error("Ready runtime metadata is unavailable. Refresh and retry.")
                })?,
                String::new(),
            )?;
            active.setup_id = active.setup_id.wrapping_add(1);
            let id = active.setup_id;
            active.setup = Some(id);
            (
                SetupReservation {
                    controller: self.clone(),
                    id,
                },
                owner,
                store.generations(),
                expected,
            )
        };
        // Network work holds neither gate. Stop and settings changes remain responsive.
        let prepared = self.backend.prepare_opencode(owner.clone()).await?;
        if !opencode::same_runtime(&expected, &prepared.binding) {
            return Err(opencode::error(
                "The running model or endpoint changed. Refresh and retry.",
            ));
        }
        let c = self.clone();
        tokio::task::spawn_blocking(move || {
            // Core invokes this after preparing files, immediately before final checks.
            // Keep both guards through replacement and rollback, including no-op returns.
            let retained = RefCell::new(None);
            let validate = || -> Result<()> {
                if retained.borrow().is_some() {
                    return Ok(());
                }
                let active = c.active.lock().unwrap();
                let store = c.store.lock().unwrap();
                let view = store.view(c.clock.now());
                if active.setup != Some(reservation.id)
                    || active.quitting
                    || c.shutdown.is_cancelled()
                    || active.config_writes > 0
                    || store.generations() != generations
                    || store.stop_running
                    || store.up_running
                    || view.phase != Phase::Ready
                    || store.runtime().as_ref() != Some(&owner)
                {
                    return Err(opencode::error(
                        "Runtime or settings changed. Retry OpenCode setup.",
                    ));
                }
                let current = opencode::binding_from_snap(
                    &owner,
                    view.snap.as_ref().ok_or_else(|| {
                        opencode::error("Ready runtime metadata is unavailable. Refresh and retry.")
                    })?,
                    String::new(),
                )?;
                if !opencode::same_runtime(&current, &prepared.binding) {
                    return Err(opencode::error(
                        "The running model or endpoint changed. Refresh and retry.",
                    ));
                }
                *retained.borrow_mut() = Some((active, store));
                Ok(())
            };
            // Catch while retained lives outside the unwinding stack. Drop the gates
            // normally so a failed backend cannot poison Active or Store.
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                c.backend
                    .configure_opencode(&path, &prepared, make_default, &validate)
            }))
            .unwrap_or_else(|_| {
                Err(opencode::error(
                    "OpenCode setup worker failed. Retry setup.",
                ))
            });
            drop(retained);
            drop(reservation);
            result
        })
        .await
        .map_err(|_| opencode::error("OpenCode setup worker failed. Retry setup."))?
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
            c.load_config().await;
            loop {
                tokio::select! { _=c.shutdown.cancelled()=>break, _=c.refresh()=>{} }
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
    pub async fn load_config(&self) {
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
    }
    pub async fn refresh(&self) {
        if !self.state().readiness.is_some_and(|r| r.cloud_ready) {
            self.load_config().await;
        }
        if !self.state().readiness.is_some_and(|r| r.cloud_ready) {
            self.change(|s| {
                s.needs_setup();
                vec![]
            });
            return;
        }
        let (provider, owner, generations) = {
            let s = self.store.lock().unwrap();
            if s.up_running || s.stop_running {
                return;
            }
            let view = s.view(self.clock.now());
            let provider = s.runtime().map_or(view.provider, |owner| owner.provider);
            (provider, s.runtime(), s.generations())
        };
        match self.backend.snapshot_owned(&provider, owner.clone()).await {
            Ok((candidate, snap)) => {
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
            Err(e) => self.change(|s| {
                if s.generations() != generations || s.up_running || s.stop_running {
                    return vec![];
                }
                s.poll_failed(e.message);
                vec![]
            }),
        }
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
                c.refresh().await;
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
                    tokio::time::sleep(Duration::from_secs(10)).await;
                    c.backend.down(target.clone()).await?;
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
            c.refresh().await;
        });
    }
    pub fn set_provider(&self, p: String) -> Result<()> {
        crate::backend::require_cloud_provider(&p)?;
        let _active = self.active.lock().unwrap();
        self.change(|s| {
            s.set_provider(p);
            vec![]
        });
        Ok(())
    }
    pub fn set_model(self: &Arc<Self>, m: String) {
        let _active = self.active.lock().unwrap();
        self.change(|s| {
            s.set_model(m);
            vec![]
        });
    }
    pub fn panel_shown(self: &Arc<Self>, open: bool) {
        self.change(|s| {
            s.set_panel_open(open);
            vec![]
        });
        if open {
            let c = self.clone();
            self.runtime.spawn(async move {
                c.refresh().await;
            });
        }
    }
}
