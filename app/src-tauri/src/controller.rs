#![deny(clippy::await_holding_lock)]
use crate::{
    backend::{Backend, Result},
    notify::Notifier,
    prefs::Prefs,
    store::Store,
    types::*,
};
use lobo_core::clock::Clock;
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
        Arc::new(Self {
            store: Mutex::new(Store::new(
                prefs.target,
                lobo_core::local::supported().is_ok(),
            )),
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
    pub fn invalidate_memory(&self) {
        self.change(|s| {
            s.invalidate_memory();
            vec![]
        });
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
            Ok((cfg, r)) => self.change(|s| {
                s.apply_config(cfg, r);
                vec![]
            }),
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
            Ok(m) => self.change(|s| {
                s.apply_models(m);
                vec![]
            }),
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
        match self.backend.snapshot().await {
            Ok(snap) => {
                let down = snap.down;
                self.change(|s| {
                    if !s.stop_running {
                        s.set_warning(None);
                    }
                    s.apply_snap(snap, self.clock.now())
                });
                if models && down {
                    self.load_models().await;
                }
            }
            Err(e) => self.change(|s| {
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
    pub fn start(self: &Arc<Self>) -> Result<()> {
        let mut active = self.active.lock().unwrap();
        let req = {
            let mut s = self.store.lock().unwrap();
            let v = s.view(self.clock.now());
            if active.quitting
                || self.shutdown.is_cancelled()
                || s.up_running
                || s.stop_running
                || !matches!(v.phase, Phase::Off | Phase::Failed { .. })
            {
                return Ok(());
            }
            s.begin_up(self.clock.now())
        };
        let cancel = CancellationToken::new();
        let _runtime = self.runtime.enter();
        let operation = self.backend.up(req, cancel.clone());
        let mut operation = match operation {
            Ok(o) => o,
            Err(e) => {
                drop(active);
                self.change(|s| {
                    s.invalidate_memory();
                    s.up_ended(Some(&e.message))
                });
                return Err(e);
            }
        };
        let mut events = operation.take_events().expect("new up has events");
        let c = self.clone();
        active.cancel = Some(cancel);
        active.up = Some(self.runtime.spawn(async move {
            while let Some(ev) = events.recv().await {
                c.change(|s| s.handle_event(&ev, c.clock.now()));
            }
            let result = operation.wait().await.map_err(AppError::from);
            c.change(|s| {
                let notes = s.up_ended(result.as_ref().err().map(|e| e.message.as_str()));
                if let Err(e) = &result
                    && e.kind != "cancelled"
                    && !s.stop_running
                {
                    // A ready event must not conceal a failed owned completion.
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
        Ok(())
    }
    pub fn stop(self: &Arc<Self>) {
        let mut active = self.active.lock().unwrap();
        let (local, was_up) = {
            let mut s = self.store.lock().unwrap();
            if s.stop_running {
                return;
            }
            let up = s.up_running;
            (s.begin_stop(), up)
        };
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
                    Ok(Err(e)) if was_up && e.kind != "cancelled" => Some(e.message),
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
            let result = async {
                c.backend.down().await?;
                if !local {
                    tokio::time::sleep(Duration::from_secs(10)).await;
                    c.backend.down().await?;
                }
                Ok::<_, AppError>(())
            }
            .await;
            c.change(|s| {
                match result {
                    Ok(()) => s.stop_done(),
                    Err(e) => s.stop_failed(format!("down: {}", e.message)),
                }
                vec![]
            });
            c.refresh(true).await;
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
        self.change(|s| {
            s.set_provider(p);
            vec![]
        });
    }
    pub fn set_model(self: &Arc<Self>, m: String) {
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
