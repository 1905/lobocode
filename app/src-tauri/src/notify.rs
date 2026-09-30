use crate::types::Note;
use tauri_plugin_notification::NotificationExt;
pub trait Notifier: Send + Sync {
    fn send(&self, n: &Note);
}
pub struct NativeNotifier(pub tauri::AppHandle);
impl Notifier for NativeNotifier {
    fn send(&self, n: &Note) {
        if bundled() {
            let _ = self
                .0
                .notification()
                .builder()
                .title(&n.title)
                .body(&n.body)
                .show();
        }
    }
}
pub fn bundled() -> bool {
    std::env::current_exe().ok().is_some_and(|p| {
        p.ancestors()
            .any(|p| p.extension().is_some_and(|s| s == "app"))
    })
}
