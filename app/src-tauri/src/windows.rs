use crate::controller::Controller;
use std::sync::Arc;
use tauri::{Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

#[derive(Default, Debug, PartialEq)]
pub struct LaunchFlags {
    pub background: bool,
    pub settings: bool,
}
pub fn launch_flags(args: &[String]) -> LaunchFlags {
    LaunchFlags {
        background: args.iter().any(|a| a == "--background"),
        settings: args.iter().any(|a| a == "--settings"),
    }
}
fn update_open(app: &tauri::AppHandle) {
    let open = ["main", "panel"].iter().any(|s| {
        app.get_webview_window(s)
            .is_some_and(|w| w.is_visible().unwrap_or(false))
    });
    if let Some(c) = app.try_state::<Arc<Controller>>() {
        c.panel_shown(open);
    }
}
fn create(app: &tauri::AppHandle, label: &str) -> tauri::Result<WebviewWindow> {
    let settings = label == "settings";
    let panel = label == "panel";
    let url = if settings {
        "index.html?view=settings"
    } else {
        "index.html?view=panel"
    };
    let mut b = WebviewWindowBuilder::new(app, label, WebviewUrl::App(url.into()))
        .title(if settings {
            "lobocode · config"
        } else {
            "lobocode"
        })
        .inner_size(
            if settings { 520.0 } else { 340.0 },
            if settings { 540.0 } else { 340.0 },
        )
        .visible(false)
        .resizable(false)
        .theme(Some(tauri::Theme::Dark))
        .background_color(tauri::webview::Color(11, 13, 16, 255));
    if panel {
        b = b
            .decorations(false)
            .transparent(true)
            .background_color(tauri::webview::Color(0, 0, 0, 0))
            .shadow(true)
            .always_on_top(true)
            .skip_taskbar(true);
    } else {
        b = b
            .title_bar_style(tauri::TitleBarStyle::Visible)
            .hidden_title(false)
            .center();
    }
    let w = b.build()?;
    let window = w.clone();
    let a = app.clone();
    w.on_window_event(move |e| match e {
        tauri::WindowEvent::CloseRequested { api, .. } => {
            api.prevent_close();
            let _ = window.hide();
            update_open(&a);
        }
        tauri::WindowEvent::Focused(false) if panel => {
            let _ = window.hide();
            update_open(&a);
        }
        _ => {}
    });
    Ok(w)
}
pub fn prepare(app: &tauri::AppHandle) -> tauri::Result<()> {
    create(app, "panel")?;
    create(app, "main")?;
    Ok(())
}
pub fn show(app: &tauri::AppHandle, label: &str) -> tauri::Result<()> {
    let window = match app.get_webview_window(label) {
        Some(w) => w,
        None => create(app, label)?,
    };
    window.show()?;
    window.set_focus()?;
    if label != "settings" {
        update_open(app);
    }
    Ok(())
}
pub fn toggle_panel(app: &tauri::AppHandle) -> tauri::Result<()> {
    use tauri_plugin_positioner::{Position, WindowExt};
    let w = app.get_webview_window("panel").expect("panel was created");
    if w.is_visible()? {
        w.hide()?;
    } else {
        w.move_window(Position::TrayCenter)?;
        w.show()?;
        w.set_focus()?;
    }
    update_open(app);
    Ok(())
}
#[cfg(test)]
mod tests {
    #[test]
    fn argv_flags() {
        assert_eq!(
            super::launch_flags(&["--background".into(), "--settings".into()]),
            super::LaunchFlags {
                background: true,
                settings: true
            }
        );
        assert_eq!(super::launch_flags(&[]), super::LaunchFlags::default());
    }
}
