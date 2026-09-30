use lobo_core::local::{self, RunConfig};
use lobo_proto::Manifest;
use tokio_util::sync::CancellationToken;

pub fn main(args: &[String]) -> i32 {
    let version = Manifest {
        version: env!("CARGO_PKG_VERSION").into(),
        git_sha: option_env!("LOBO_COMMIT").unwrap_or("dev").into(),
        ..Default::default()
    };
    let cfg = match RunConfig::from_args(args, version) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("lobocode --lobo-local-run: {e}");
            return 2;
        }
    };
    let runtime = match tokio::runtime::Runtime::new() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("lobocode --lobo-local-run: {e}");
            return 1;
        }
    };
    runtime.block_on(async move {
        let cancel = CancellationToken::new();
        let signal = cancel.clone();
        let listener = tokio::spawn(async move {
            let mut term =
                tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                    .expect("SIGTERM handler");
            tokio::select! {_=tokio::signal::ctrl_c()=>{},_=term.recv()=>{}}
            signal.cancel();
        });
        let r = local::supervise(cfg, cancel).await;
        listener.abort();
        match r {
            Ok(()) => 0,
            Err(e) => {
                eprintln!("lobocode --lobo-local-run: {e}");
                1
            }
        }
    })
}
#[cfg(test)]
mod tests {
    #[test]
    fn invalid_args_do_not_start_app() {
        assert_eq!(super::main(&["--bogus".into()]), 2);
    }
}
