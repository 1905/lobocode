pub mod run;
pub mod status;
pub mod styles;
pub mod up;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flow {
    Continue,
    Quit,
}
pub fn quit_key(key: crossterm::event::KeyEvent, escape: bool) -> bool {
    use crossterm::event::{KeyCode, KeyEventKind, KeyModifiers};
    key.kind != KeyEventKind::Release
        && (key.code == KeyCode::Char('q')
            || escape && key.code == KeyCode::Esc
            || key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL))
}
