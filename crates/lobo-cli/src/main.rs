#[tokio::main]
async fn main() {
    let app = lobo_cli::app::App::real();
    #[cfg(feature = "test-fakes")]
    let app = if let Some(name) = std::env::var_os("LOBO_TEST_SCENARIO") {
        match lobo_cli::fakes::scenario(&name.to_string_lossy()) {
            Some(app) => app,
            None => {
                eprintln!("error: unknown test scenario");
                std::process::exit(1);
            }
        }
    } else {
        app
    };
    let cancel = app.cancel.clone();
    let signals = tokio::spawn(async move {
        while tokio::signal::ctrl_c().await.is_ok() {
            cancel.cancel();
        }
    });
    let code = lobo_cli::run(
        &app,
        std::env::args_os().collect(),
        &mut lobo_cli::app::Io::real(),
    )
    .await;
    signals.abort();
    let _ = signals.await;
    std::process::exit(code);
}
