fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args
        .get(1)
        .is_some_and(|s| s == lobo_core::connection::HELPER_ARG)
    {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("cloud runtime");
        std::process::exit(runtime.block_on(lobo_core::connection::entry(&args[2..])));
    }
    if args
        .get(1)
        .is_some_and(|s| s == lobo_core::local::SUPERVISOR_ARG)
    {
        eprintln!(
            "Lobocode Mac app supports cloud GPUs only. Use the optional CLI for local execution."
        );
        std::process::exit(2);
    }
    lobocode_app::run();
}
