//! Generate UI data without running backend or lifecycle tests.
#[path = "../tests/fixtures.rs"]
mod fixtures;
use lobocode_app::types::{AppError, PanelState};
use ts_rs::TS;
fn main() {
    fixtures::write_render_fixtures();
    let ui = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../ui/src");
    let config = ts_rs::Config::default().with_out_dir(ui.join("proto"));
    PanelState::export_all(&config).unwrap();
    AppError::export_all(&config).unwrap();
}
