use crate::{icons, types::*, windows};
use std::sync::Mutex;
use tauri::{
    image::Image,
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Manager,
};
#[derive(Clone, PartialEq)]
struct Look {
    phase: String,
    progress: u8,
    title: String,
}
#[derive(Default)]
pub struct TrayState(Mutex<Option<Look>>);
fn look(s: &PanelState) -> Look {
    Look {
        phase: s.phase.word().into(),
        progress: (s.boot_progress * 50.0).round().clamp(0.0, 50.0) as u8,
        title: s.menu_text.clone(),
    }
}
fn changes(prev: Option<&Look>, next: &Look) -> (bool, bool) {
    (
        prev.is_none_or(|p| p.phase != next.phase || p.progress != next.progress),
        prev.is_none_or(|p| p.title != next.title),
    )
}
pub fn build(app: &tauri::AppHandle) -> tauri::Result<()> {
    app.manage(TrayState::default());
    let (data, w, h, template) = icons::tray_icon(&Phase::Loading, 0.0);
    TrayIconBuilder::with_id("lobo")
        .icon(Image::new_owned(data, w, h))
        .icon_as_template(template)
        .title("lobo")
        .show_menu_on_left_click(false)
        .on_tray_icon_event(|tray, event| {
            tauri_plugin_positioner::on_tray_event(tray.app_handle(), &event);
            if matches!(
                event,
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                }
            ) {
                let _ = windows::toggle_panel(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}
pub fn update(app: &tauri::AppHandle, s: &PanelState) {
    let a = app.clone();
    let next = look(s);
    let phase = s.phase.clone();
    let progress = s.boot_progress;
    // Native setters dispatch synchronously to the main thread. Keep the cache
    // there too: a worker holding it while waiting for main deadlocks startup.
    let _ = app.run_on_main_thread(move || {
        let Some(tray) = a.tray_by_id("lobo") else {
            return;
        };
        let state = a.state::<TrayState>();
        let mut prev = state.0.lock().unwrap();
        let (icon, title) = changes(prev.as_ref(), &next);
        if icon {
            let (data, w, h, template) = icons::tray_icon(&phase, progress);
            let _ = tray.set_icon_with_as_template(Some(Image::new_owned(data, w, h)), template);
        }
        if title {
            let _ = tray.set_title(Some(&next.title));
        }
        *prev = Some(next);
    });
}
#[cfg(test)]
mod tests {
    #[test]
    fn unchanged_tray_is_not_redrawn() {
        let s = crate::types::PanelState::default();
        let a = super::look(&s);
        assert_eq!(super::changes(None, &a), (true, true));
        assert_eq!(super::changes(Some(&a), &a), (false, false));
        let mut b = a.clone();
        b.title = "off".into();
        assert_eq!(super::changes(Some(&a), &b), (false, true));
    }
}
