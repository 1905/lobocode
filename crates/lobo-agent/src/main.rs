use clap::{Parser, Subcommand};
use lobo_agent::{Error, Result, pod, selfkill, source};
use std::{process::ExitCode, sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;
#[derive(Parser)]
#[command(
    name = "lobo-agent",
    about = "Pod agent: tunnel, model download, llama-server, watchdog, /api"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}
#[derive(Subcommand)]
enum Command {
    Version {
        #[arg(hide = true)]
        extra: Vec<String>,
    },
    Bench {
        #[arg(long, default_value = "1,4,8")]
        conns: String,
        #[arg(long, default_value_t = 45)]
        seconds: u64,
        #[arg(long)]
        url: Option<String>,
        #[arg(long, default_value = "q8")]
        model: String,
        #[arg(hide = true)]
        extra: Vec<String>,
    },
}
fn env(key: &str) -> Option<String> {
    std::env::var(key).ok()
}
#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Some(Command::Version { .. }) => {
            println!("{}", lobo_agent::VERSION);
            ExitCode::SUCCESS
        }
        Some(Command::Bench {
            conns,
            seconds,
            url,
            model,
            ..
        }) => match bench(conns, seconds, url, model).await {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("Error: {e}");
                ExitCode::FAILURE
            }
        },
        None => {
            let logs = Arc::new(lobo_agent::LogRing::new(5000));
            pod::init_logging(logs.clone());
            let value = |key| env(key).unwrap_or_default();
            let api = selfkill::self_api(
                &value("LOBO_PROVIDER"),
                &value("RUNPOD_POD_ID"),
                &value("RUNPOD_API_KEY"),
                &value("CONTAINER_ID"),
                &value("CONTAINER_API_KEY"),
            );
            let mut term =
                tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                    .expect("SIGTERM handler");
            tokio::select! {
                code=pod::main_flow(async move{pod::run(&env,logs).await},api,Duration::from_secs(600))=>code,
                _=term.recv()=>ExitCode::from(143),
                _=tokio::signal::ctrl_c()=>ExitCode::from(143),
            }
        }
    }
}
async fn bench(conns: String, seconds: u64, url: Option<String>, model: String) -> Result<()> {
    let model = lobo_proto::catalog::get(&model).map_err(|e| Error::msg(e.to_string()))?;
    let src = if let Some(url) = url {
        Arc::new(source::HttpSource { url }) as Arc<dyn source::Source>
    } else {
        source::model_source(
            &env("LOBO_MODEL_URL").unwrap_or_default(),
            &env("LOBO_MODEL_SSH_KEY").unwrap_or_default(),
            &env("LOBO_MODEL_SSH_HOSTKEY").unwrap_or_default(),
            &model.file,
            model.size,
        )?
    };
    for n in conns.split(',') {
        let n = n
            .trim()
            .parse::<usize>()
            .map_err(|_| Error::msg("invalid connection count"))?;
        let result = lobo_agent::download::bench(
            CancellationToken::new(),
            src.as_ref(),
            model.size,
            n,
            Duration::from_secs(seconds),
        )
        .await;
        match result {
            Ok((bytes, mbps)) => println!(
                "{}",
                pod::bench_line(&src.to_string(), n, bytes, mbps, None)
            ),
            Err(e) => println!(
                "{}",
                pod::bench_line(&src.to_string(), n, 0, 0.0, Some(&e.to_string()))
            ),
        }
    }
    Ok(())
}
