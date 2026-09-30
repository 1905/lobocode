//! Offline terminal smoke: `cargo run -p lobo-cli --example tui_demo -- up`.
use lobo_core::{
    clock::SystemClock,
    control::{
        self, UpOpts,
        testkit::{self, FakeAgent, FakeRunPod},
    },
};
use std::{sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let clock = Arc::new(SystemClock);
    let mut deps = testkit::deps(
        Arc::new(FakeRunPod::default()),
        Arc::new(FakeAgent::new(testkit::boot_script())),
        clock.clone(),
    );
    deps.poll = Duration::from_millis(300);
    let cancel = CancellationToken::new();
    if std::env::args().nth(1).as_deref() == Some("status") {
        return lobo_cli::tui::run::run_status(&deps, &cancel, false).await;
    }
    let mut operation = control::up(deps, UpOpts::default(), cancel.clone());
    let mut events = operation.take_events().unwrap();
    let output = lobo_cli::tui::run::run_up(&mut events, &mut None, clock, &cancel, false).await;
    if output.is_err() {
        operation.cancel();
    }
    while events.recv().await.is_some() {}
    let result = operation.wait().await;
    println!(
        "terminal restored: {}",
        !crossterm::terminal::is_raw_mode_enabled()?
    );
    match result {
        Err(lobo_core::Error::Cancelled) => println!("cancelled after cleanup"),
        other => other?,
    }
    Ok(())
}
