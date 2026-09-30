use crate::{backend::Result, controller::Controller, types::*, windows};
use std::{collections::BTreeMap, sync::Arc};
use tauri::{Manager, State};
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;
type C<'a> = State<'a, Arc<Controller>>;
fn error(e: impl std::fmt::Display) -> AppError {
    AppError {
        kind: "app".into(),
        message: e.to_string(),
    }
}
#[tauri::command]
pub fn get_state(c: C<'_>) -> PanelState {
    c.state()
}
#[tauri::command]
pub async fn start(c: C<'_>) -> Result<()> {
    let controller = c.inner().clone();
    // Reserve and capture before the first await or blocking-worker queue.
    let reply = controller.submit_start()?;
    let result = reply.await.map_err(error)?;
    controller.refresh_memory().await;
    result
}
#[tauri::command]
pub fn stop(c: C<'_>) {
    c.inner().stop();
}
#[tauri::command]
pub fn dismiss(c: C<'_>) {
    c.inner().dismiss();
}
#[tauri::command]
pub fn choose_target(c: C<'_>, t: Target) {
    c.choose(t);
}
#[tauri::command]
pub fn set_provider(c: C<'_>, v: String) {
    c.set_provider(v);
}
#[tauri::command]
pub fn set_model(c: C<'_>, v: String) {
    c.set_model(v);
}
#[tauri::command]
pub async fn refresh(c: C<'_>, models: bool) -> Result<()> {
    c.refresh(models).await;
    Ok(())
}
#[tauri::command]
pub async fn copy_api_key(c: C<'_>, app: tauri::AppHandle) -> Result<()> {
    let key = c.backend.api_key().await?;
    app.clipboard().write_text(key).map_err(error)
}
#[tauri::command]
pub fn copy_text(app: tauri::AppHandle, s: String) -> Result<()> {
    app.clipboard().write_text(s).map_err(error)
}
#[tauri::command]
pub async fn config_show(c: C<'_>) -> Result<lobo_proto::ConfigShow> {
    Ok(c.backend.config().await?.0)
}
#[tauri::command]
pub async fn config_save(c: C<'_>, set: BTreeMap<String, String>) -> Result<()> {
    // Saving changes the context even while an earlier measurement is pending.
    c.begin_config_write();
    let result = c.backend.save(set).await;
    if result.is_ok() {
        c.load_config(true).await;
    }
    c.end_config_write();
    result?;
    c.refresh(false).await;
    Ok(())
}
#[tauri::command]
pub async fn local_models(c: C<'_>) -> Result<lobo_proto::Listing> {
    c.backend.models().await
}
#[tauri::command]
pub fn catalog() -> Vec<lobo_proto::catalog::Model> {
    lobo_proto::catalog::all().to_vec()
}
#[tauri::command]
pub fn free_bytes(path: String) -> Option<u64> {
    lobo_core::local::free_bytes_nearest(std::path::Path::new(&path))
}
#[tauri::command]
pub fn gen_api_key() -> String {
    lobo_core::genkey::new_api_key()
}
#[tauri::command]
pub async fn choose_weights(app: tauri::AppHandle, start: String) -> Result<Option<String>> {
    tauri::async_runtime::spawn_blocking(move || {
        let cfg = lobo_core::config::Laptop {
            weights_dir: start,
            ..Default::default()
        };
        app.dialog()
            .file()
            .set_directory(cfg.weights())
            .blocking_pick_folder()
            .and_then(|p| p.into_path().ok())
            .map(|p| p.to_string_lossy().into_owned())
    })
    .await
    .map_err(error)
}
#[tauri::command]
pub fn opencode_info(c: C<'_>, path: Option<String>) -> Result<OpenCodeInfo> {
    c.opencode_info(path)
}
#[tauri::command]
pub async fn choose_opencode_config(app: tauri::AppHandle) -> Result<Option<String>> {
    tauri::async_runtime::spawn_blocking(move || {
        let Some(selected) = app
            .dialog()
            .file()
            .add_filter("OpenCode configuration", &["json", "jsonc"])
            .blocking_pick_file()
        else {
            return Ok(None);
        };
        let path = selected.into_path().map_err(|_| {
            crate::opencode::error("Choose an existing JSON or JSONC configuration file.")
        })?;
        crate::opencode::config_path(&path, true)?;
        Ok(Some(path.to_string_lossy().into_owned()))
    })
    .await
    .map_err(|_| crate::opencode::error("OpenCode file selection failed."))?
}
#[tauri::command]
pub async fn configure_opencode(
    c: C<'_>,
    path: String,
    make_default: bool,
) -> Result<OpenCodeResult> {
    c.inner().configure_opencode(path, make_default).await
}
#[tauri::command]
pub fn open_settings(app: tauri::AppHandle, tab: Option<String>) -> Result<()> {
    if tab
        .as_deref()
        .is_some_and(|tab| !matches!(tab, "local" | "cloud" | "defaults" | "clients"))
    {
        return Err(error("Unknown settings tab."));
    }
    windows::show_settings(&app, tab).map_err(error)
}
#[tauri::command]
pub fn consume_settings_tab(app: tauri::AppHandle) -> Option<String> {
    windows::consume_settings_tab(&app)
}
#[tauri::command]
pub fn reveal_config(c: C<'_>, app: tauri::AppHandle) -> Result<()> {
    app.opener()
        .reveal_item_in_dir(c.backend.config_path())
        .map_err(error)
}
#[tauri::command]
pub fn open_config(c: C<'_>, app: tauri::AppHandle) -> Result<()> {
    let p = c.backend.config_path();
    if p.exists() {
        app.opener()
            .open_path(p.to_string_lossy(), None::<&str>)
            .map_err(error)?;
    }
    Ok(())
}
#[tauri::command]
pub async fn quit(c: C<'_>, app: tauri::AppHandle) -> Result<()> {
    c.inner().quit().await?;
    app.exit(0);
    Ok(())
}
pub fn request_quit(app: &tauri::AppHandle) {
    let a = app.clone();
    let c = app.state::<Arc<Controller>>().inner().clone();
    tauri::async_runtime::spawn(async move {
        if let Err(e) = c.quit().await {
            eprintln!("quit: {e}");
        } else {
            a.exit(0);
        }
    });
}
