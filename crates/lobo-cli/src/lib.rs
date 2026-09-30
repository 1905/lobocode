pub mod app;
pub mod bootlog;
pub mod build_info;
pub mod cli;
pub mod cmd;
pub mod duration;
#[cfg(feature = "test-fakes")]
pub mod fakes;
pub mod help;
pub mod logfmt;
pub mod tui;
pub mod wizard;

use app::{App, Io};
use clap::FromArgMatches;
use cli::{Cli, Cmd};
use std::{
    ffi::OsString,
    io::Write,
    path::PathBuf,
    sync::{Arc, Mutex},
};
use tracing::instrument::WithSubscriber;

pub async fn run(app: &App, argv: Vec<OsString>, io: &mut Io) -> i32 {
    let original = std::mem::replace(&mut io.err, Box::new(std::io::sink()));
    let shared = logfmt::SharedWriter(Arc::new(Mutex::new(original)));
    io.err = Box::new(shared.clone());
    let subscriber = tracing_subscriber::fmt()
        .event_format(logfmt::ZerologConsole {
            color: app.term.stderr_tty && !app.term.no_color,
            tz: *chrono::Local::now().offset(),
        })
        .with_writer(shared.clone())
        .finish();
    let result = execute(app, argv, io).with_subscriber(subscriber).await;
    let code = match result {
        Ok(()) => 0,
        Err(e) => {
            let _ = writeln!(io.err, "error: {e:#}");
            1
        }
    };
    let code = if io.out.flush().is_err() { 1 } else { code };
    let _ = io.err.flush();
    io.err = std::mem::replace(&mut *shared.0.lock().unwrap(), Box::new(std::io::sink()));
    code
}

fn root_help_request(args: &[OsString]) -> Option<PathBuf> {
    let mut path = lobo_core::config::default_path();
    let mut it = args.iter().skip(1);
    while let Some(arg) = it.next() {
        if arg == "--config" || arg == "--env" {
            path = it.next()?.into();
            continue;
        }
        let s = arg.to_str()?;
        if let Some(value) = s
            .strip_prefix("--config=")
            .or_else(|| s.strip_prefix("--env="))
        {
            path = value.into();
            continue;
        }
        if !["help", "-h", "--help"].contains(&s) {
            return None;
        }
    }
    Some(path)
}
async fn execute(app: &App, argv: Vec<OsString>, io: &mut Io) -> anyhow::Result<()> {
    let matches = match cli::command().try_get_matches_from(&argv) {
        Ok(m) => m,
        Err(e) if e.kind() == clap::error::ErrorKind::DisplayHelp => {
            if let Some(path) = root_help_request(&argv) {
                help::root_help(
                    &mut io.out,
                    app.term.stdout_tty && !app.term.no_color,
                    path.exists(),
                    &path,
                )?;
            } else {
                write!(io.out, "{e}")?;
            }
            return Ok(());
        }
        Err(e) => {
            let message = e.to_string();
            anyhow::bail!(
                "{}",
                message
                    .lines()
                    .next()
                    .unwrap_or("invalid command")
                    .trim_start_matches("error: ")
            );
        }
    };
    let changed = cli::changed(&matches);
    let cli = Cli::from_arg_matches(&matches)?;
    let path = cli.config.unwrap_or_else(lobo_core::config::default_path);
    match cli.cmd {
        None => help::root_help(
            &mut io.out,
            app.term.stdout_tty && !app.term.no_color,
            path.exists(),
            &path,
        )?,
        Some(Cmd::Version(_)) => writeln!(
            io.out,
            "lobo {} ({}, {})",
            build_info::VERSION,
            build_info::COMMIT,
            build_info::DATE
        )?,
        Some(Cmd::Config(a)) => cmd::config::run(app, a, &path, io)?,
        Some(Cmd::GenApiKey(a)) => cmd::genkey::run(&path, a.rotate, io)?,
        Some(Cmd::Models(a)) => {
            cmd::models::write_models(&mut io.out, &app::load_cfg(&path)?.weights(), a.json)?
        }
        Some(Cmd::Local(a)) => match a.cmd {
            Some(cli::LocalCmd::Run(a)) => {
                (app.supervise)(a.to_run_config(&path)?, app.cancel.clone()).await?;
            }
            None => {
                let mut tree = cli::command();
                let sub = tree.find_subcommand_mut("local").unwrap();
                sub.write_long_help(&mut io.out)?;
                writeln!(io.out)?;
            }
        },
        Some(Cmd::Up(a)) => {
            cmd::up::run(app, &a, &changed, &path, io).await?;
        }
        Some(Cmd::Down(a)) => cmd::down::run(app, &a, &path, io).await?,
        Some(Cmd::Status(a)) => cmd::status::run(app, &a, &path, io).await?,
        Some(Cmd::Logs(_)) | Some(Cmd::Test(_)) => {
            app::load_cfg(&path)?;
            anyhow::bail!("agent command implementation pending");
        }
        Some(Cmd::Release(_)) => {
            let cfg = app::load_cfg(&path)?;
            lobo_core::control::check_release(&cfg)?;
            anyhow::bail!("release command implementation pending");
        }
        Some(Cmd::Completion(a)) => {
            if let Some(shell) = a.shell {
                clap_complete::generate(
                    clap_complete::Shell::from(shell),
                    &mut cli::command(),
                    "lobo",
                    &mut io.out,
                );
            } else {
                let mut tree = cli::command();
                tree.find_subcommand_mut("completion")
                    .unwrap()
                    .write_long_help(&mut io.out)?;
                writeln!(io.out)?;
            }
        }
    }
    Ok(())
}
