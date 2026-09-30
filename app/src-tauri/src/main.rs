fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args
        .get(1)
        .is_some_and(|s| s == lobo_core::local::SUPERVISOR_ARG)
    {
        if args.get(2).map(String::as_str) != Some("local")
            || args.get(3).map(String::as_str) != Some("run")
        {
            eprintln!("lobocode --lobo-local-run: expected local run");
            std::process::exit(2);
        }
        std::process::exit(lobocode_app::supervisor::main(&args[4..]));
    }
    lobocode_app::run();
}
