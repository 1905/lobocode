pub mod backend;
pub mod commands;
pub mod controller;
#[cfg(feature = "e2e")]
mod e2e_memory;
pub mod fmt;
pub mod icons;
pub mod notify;
pub mod prefs;
pub mod store;
pub mod supervisor;
pub mod tray;
pub mod types;
pub mod windows;

pub fn run() {
    use std::sync::Arc;
    use tauri::{Emitter, Manager};
    let builder = tauri::Builder::default();
    #[cfg(feature = "e2e")]
    let builder = builder
        .plugin(tauri_plugin_wdio::init())
        .plugin(tauri_plugin_wdio_webdriver::init());
    builder
        .plugin(tauri_plugin_positioner::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            commands::get_state,
            commands::start,
            commands::stop,
            commands::dismiss,
            commands::choose_target,
            commands::set_provider,
            commands::set_model,
            commands::refresh,
            commands::copy_api_key,
            commands::copy_text,
            commands::config_show,
            commands::config_save,
            commands::local_models,
            commands::catalog,
            commands::free_bytes,
            commands::gen_api_key,
            commands::choose_weights,
            commands::open_settings,
            commands::reveal_config,
            commands::open_config,
            commands::quit
        ])
        .setup(|app| {
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            let handle = app.handle().clone();
            tray::build(&handle)?;
            let prefs_dir = app.path().app_config_dir()?;
            let prefs = prefs::Prefs::load(&prefs_dir);
            let backend = Arc::new(backend::CoreBackend::new(
                backend::CoreBackend::configured_path(),
            )?);
            let emit_app = handle.clone();
            let controller = controller::Controller::new(
                backend,
                Arc::new(notify::NativeNotifier(handle.clone())),
                Arc::new(lobo_core::clock::SystemClock),
                prefs,
                Some(prefs_dir),
                tauri::async_runtime::handle().inner().clone(),
                Arc::new(move |s| {
                    let _ = emit_app.emit("lobo://state", &s);
                    tray::update(&emit_app, &s);
                }),
            );
            app.manage(controller.clone());
            windows::prepare(&handle)?;
            controller.spawn_loops();
            let flags = windows::launch_flags(&std::env::args().collect::<Vec<_>>());
            if !flags.background {
                windows::show(&handle, "main")?;
            }
            if flags.settings {
                let h = handle.clone();
                tauri::async_runtime::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                    let _ = windows::show(&h, "settings");
                });
            }
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("failed to build lobocode")
        .run(|app, event| match event {
            tauri::RunEvent::Reopen { .. } => {
                let _ = windows::show(app, "main");
            }
            tauri::RunEvent::ExitRequested {
                api, code: None, ..
            } => {
                api.prevent_exit();
                commands::request_quit(app);
            }
            _ => {}
        });
}
