use clap::{Args, CommandFactory, Parser, Subcommand};
use std::{collections::BTreeSet, ffi::OsString, path::PathBuf, time::Duration};

#[derive(Debug, Parser)]
#[command(
    name = "lobo",
    bin_name = "lobo",
    disable_version_flag = true,
    args_override_self = true,
    about = "Rent an RTX 5090 and serve Qwen3.5-27B"
)]
pub struct Cli {
    #[arg(
        long,
        alias = "env",
        global = true,
        value_name = "string",
        help_heading = "Global Flags",
        help = "config file (dotenv; the OS env is never read)"
    )]
    pub config: Option<PathBuf>,
    #[command(subcommand)]
    pub cmd: Option<Cmd>,
}

#[derive(Debug, Subcommand)]
pub enum Cmd {
    #[command(about = "Print version, commit and build date")]
    Version(IgnoredArgs),
    #[command(
        about = "Set API keys and defaults for `lobo up` (form in a terminal)",
        long_about = "Opens a form for the provider keys, access keys and the defaults `lobo up` uses.\nEverything is stored as plain KEY=value lines in one file (see `lobo config path`).\nYou can edit that file by hand; the form keeps keys it does not show."
    )]
    Config(ConfigArgs),
    #[command(
        about = "Create LOBO_API_KEY in the config once (kept across ups) and write opencode.lobo.json with it"
    )]
    GenApiKey(GenKeyArgs),
    #[command(
        hide = true,
        about = "Build lobo-agent, zip it with release.json, scan for secrets, upload to bucket lobo"
    )]
    Release(ReleaseArgs),
    #[command(about = "Rent a 5090 and boot lobo; shows progress until the API is ready")]
    Up(UpArgs),
    #[command(about = "Delete every lobo pod on every provider")]
    Down(DownArgs),
    #[command(about = "Live dashboard of the running pod")]
    Status(StatusArgs),
    #[command(about = "Print the pod's recent agent, llama-server and cloudflared logs")]
    Logs(LogsArgs),
    #[command(about = "Smoke test the live API: streamed chat + tool call")]
    Test(IgnoredArgs),
    #[command(about = "Catalog models and their state in the local weights folder")]
    Models(ModelsArgs),
    #[command(
        hide = true,
        about = "Local-mode internals (spawned by `lobo up --provider local`)"
    )]
    Local(LocalArgs),
    #[command(
        hide = true,
        about = "Generate the autocompletion script for the specified shell",
        long_about = "Generate the autocompletion script for lobo for the specified shell.\nSee each sub-command's help for details on how to use the generated script."
    )]
    Completion(CompletionArgs),
}

#[derive(Debug, Args, Default)]
pub struct IgnoredArgs {
    #[arg(hide = true)]
    pub extra: Vec<OsString>,
}

#[derive(Debug, Args)]
pub struct ConfigArgs {
    #[command(subcommand)]
    pub cmd: Option<ConfigCmd>,
    #[command(flatten)]
    pub ignored: IgnoredArgs,
}

#[derive(Debug, Subcommand)]
pub enum ConfigCmd {
    #[command(about = "Print the config with API keys masked")]
    Show(ConfigShowArgs),
    #[command(
        about = "Set keys in the config file (KEY= removes it); keeps every other line",
        long_about = "Set keys in the config file. KEY= removes a key. With --stdin, reads one JSON object\n{\"KEY\": \"value\"} from stdin instead, so secrets never show up in the process list."
    )]
    Set(ConfigSetArgs),
    #[command(about = "Print one value in clear (e.g. LOBO_API_KEY for a client config)")]
    Get {
        #[arg(value_name = "KEY")]
        key: String,
    },
    #[command(about = "Print the config file path")]
    Path(IgnoredArgs),
}

#[derive(Debug, Args)]
pub struct ConfigShowArgs {
    #[arg(long, action = clap::ArgAction::Set, num_args = 0..=1, require_equals = true, default_missing_value = "true", default_value = "false", value_parser = parse_bool_flag,
        help = "machine-readable: path, exists, masked values, which keys are set")]
    pub json: bool,
    #[command(flatten)]
    pub ignored: IgnoredArgs,
}
#[derive(Debug, Args)]
pub struct ConfigSetArgs {
    #[arg(long, action = clap::ArgAction::Set, num_args = 0..=1, require_equals = true, default_missing_value = "true", default_value = "false", value_parser = parse_bool_flag,
        help = "read {\"KEY\": \"value\"} JSON from stdin (keeps secrets out of argv)")]
    pub stdin: bool,
    #[arg(value_name = "KEY=value")]
    pub values: Vec<String>,
}
#[derive(Debug, Args)]
pub struct GenKeyArgs {
    #[arg(long, action = clap::ArgAction::Set, num_args = 0..=1, require_equals = true, default_missing_value = "true", default_value = "false", value_parser = parse_bool_flag, help = "replace the existing key")]
    pub rotate: bool,
    #[command(flatten)]
    pub ignored: IgnoredArgs,
}
#[derive(Debug, Args)]
pub struct ReleaseArgs {
    #[arg(long, hide = true, action = clap::ArgAction::Set, num_args = 0..=1, require_equals = true, default_missing_value = "true", default_value = "false", value_parser = parse_bool_flag)]
    pub no_promote: bool,
    #[command(flatten)]
    pub ignored: IgnoredArgs,
}

#[derive(Debug, Args)]
pub struct UpArgs {
    #[arg(
        long,
        value_name = "string",
        default_value = "community",
        help = "community ($0.69/h, default, community hosts only) or secure (datacenter $0.99/h first, community fallback)"
    )]
    pub cloud: String,
    #[arg(
        long,
        value_name = "int",
        default_value_t = 0,
        allow_hyphen_values = true,
        help = "parallel download streams on the pod (0 = agent default)"
    )]
    pub conns: i64,
    #[arg(
        long,
        value_name = "int",
        default_value_t = 0,
        allow_hyphen_values = true,
        help = "context size (0 = release default)"
    )]
    pub ctx: i64,
    #[arg(
        long,
        value_name = "int",
        default_value_t = 0,
        allow_hyphen_values = true,
        help = "minutes without requests before the pod deletes itself (0 = release default)"
    )]
    pub idle_min: i64,
    #[arg(
        long,
        value_name = "string",
        default_value = "",
        help = "pod image with lobo-agent baked in, e.g. ghcr.io/1905/lobocode@sha256:… (default: LOBO_POD_IMAGE, else the release zip)"
    )]
    pub image: String,
    #[arg(long, action = clap::ArgAction::Set, num_args = 0..=1, require_equals = true, default_missing_value = "true", default_value = "false", value_parser = parse_bool_flag, help = "one JSON object per event on stdout (for scripts and tests)")]
    pub json: bool,
    #[arg(long, value_name = "duration", default_value = "0", value_parser = crate::duration::parse_go_duration, help = "hard pod lifetime, e.g. 12h (0 = release default)")]
    pub max_life: Duration,
    #[arg(
        long,
        value_name = "int",
        default_value_t = 0,
        allow_hyphen_values = true,
        help = "drop the pod if the model downloads slower than this after 20 s (0 = LOBO_MIN_MBPS or 100)"
    )]
    pub min_mbps: i64,
    #[arg(long, action = clap::ArgAction::Set, num_args = 0..=1, require_equals = true, default_missing_value = "true", default_value = "false", value_parser = parse_bool_flag, help = "log lines instead of the TUI")]
    pub plain: bool,
    #[arg(
        long,
        value_name = "string",
        default_value = "",
        help = "runpod, vast or local (this Mac) (default: LOBO_PROVIDER, else the one with a key, runpod first)"
    )]
    pub provider: String,
    #[arg(long, action = clap::ArgAction::Set, num_args = 0..=1, require_equals = true, default_missing_value = "true", default_value = "false", value_parser = parse_bool_flag, help = "serve Q6_K instead of the release default")]
    pub q6: bool,
    #[arg(
        long,
        value_name = "string",
        default_value = "",
        help = "release version (default latest)"
    )]
    pub release: String,
    #[arg(
        long,
        value_name = "string",
        default_value = "",
        help = "model source: r2 (presigned, default) | feesh (the model server HTTP) | ssh (the model server SSH) | public (r2.dev)"
    )]
    pub source: String,
    #[arg(
        long,
        value_name = "string",
        default_value = "",
        help = "debug: path to a public key; opens 22/tcp and runs sshd on the pod"
    )]
    pub ssh: String,
    #[command(flatten)]
    pub ignored: IgnoredArgs,
}
#[derive(Debug, Args)]
pub struct DownArgs {
    #[arg(long, action = clap::ArgAction::Set, num_args = 0..=1, require_equals = true, default_missing_value = "true", default_value = "false", value_parser = parse_bool_flag, help = "print {\"spent_usd\": …} on stdout")]
    pub json: bool,
    #[command(flatten)]
    pub ignored: IgnoredArgs,
}
#[derive(Debug, Args)]
pub struct StatusArgs {
    #[arg(long, action = clap::ArgAction::Set, num_args = 0..=1, require_equals = true, default_missing_value = "true", default_value = "false", value_parser = parse_bool_flag, help = "print one snapshot as JSON (pod, release, agent status) and exit")]
    pub json: bool,
    #[arg(long, action = clap::ArgAction::Set, num_args = 0..=1, require_equals = true, default_missing_value = "true", default_value = "false", value_parser = parse_bool_flag, help = "print one snapshot and exit")]
    pub once: bool,
    #[command(flatten)]
    pub ignored: IgnoredArgs,
}
#[derive(Debug, Args)]
pub struct LogsArgs {
    #[arg(
        short = 'n',
        long,
        value_name = "int",
        default_value_t = 200,
        allow_hyphen_values = true,
        help = "number of lines (max 1000)"
    )]
    pub n: i64,
    #[command(flatten)]
    pub ignored: IgnoredArgs,
}
#[derive(Debug, Args)]
pub struct ModelsArgs {
    #[arg(long, action = clap::ArgAction::Set, num_args = 0..=1, require_equals = true, default_missing_value = "true", default_value = "false", value_parser = parse_bool_flag, help = "print the listing as JSON (weights, free_bytes, models, runtime)")]
    pub json: bool,
}
#[derive(Debug, Args)]
pub struct LocalArgs {
    #[command(subcommand)]
    pub cmd: Option<LocalCmd>,
}
#[derive(Debug, Subcommand)]
pub enum LocalCmd {
    #[command(
        about = "Supervise llama-server on this Mac and serve the agent API on 127.0.0.1:--api-port"
    )]
    Run(LocalRunArgs),
}
#[derive(Debug, Args)]
pub struct LocalRunArgs {
    #[arg(
        long,
        value_name = "int",
        default_value_t = 8932,
        allow_hyphen_values = true,
        help = "agent API port"
    )]
    pub api_port: i64,
    #[arg(
        long,
        value_name = "string",
        default_value = "",
        help = "echoed in /api/status"
    )]
    pub boot_id: String,
    #[arg(
        long,
        value_name = "int",
        default_value_t = 0,
        allow_hyphen_values = true,
        help = "context size"
    )]
    pub ctx: i64,
    #[arg(
        long,
        value_name = "int",
        default_value_t = 0,
        allow_hyphen_values = true,
        help = "minutes without requests before it stops"
    )]
    pub idle_min: i64,
    #[arg(
        long,
        value_name = "string",
        default_value = "q6",
        help = "catalog model id"
    )]
    pub model: String,
    #[arg(
        long,
        value_name = "int",
        default_value_t = 8931,
        allow_hyphen_values = true,
        help = "llama-server port"
    )]
    pub port: i64,
}
#[derive(Debug, Args)]
pub struct CompletionArgs {
    #[command(subcommand)]
    pub shell: Option<CompletionShell>,
}
#[derive(Debug, Subcommand)]
pub enum CompletionShell {
    #[command(about = "Generate the autocompletion script for bash")]
    Bash,
    #[command(about = "Generate the autocompletion script for fish")]
    Fish,
    #[command(about = "Generate the autocompletion script for powershell")]
    Powershell,
    #[command(about = "Generate the autocompletion script for zsh")]
    Zsh,
}
impl From<CompletionShell> for clap_complete::Shell {
    fn from(s: CompletionShell) -> Self {
        match s {
            CompletionShell::Bash => Self::Bash,
            CompletionShell::Fish => Self::Fish,
            CompletionShell::Powershell => Self::PowerShell,
            CompletionShell::Zsh => Self::Zsh,
        }
    }
}

pub fn command() -> clap::Command {
    layout(
        Cli::command()
            .mut_arg("config", |a| {
                a.default_value(lobo_core::config::default_path().into_os_string())
            })
            .color(clap::ColorChoice::Never),
    )
}
fn layout(command: clap::Command) -> clap::Command {
    let help = format!("help for {}", command.get_name());
    let root = command.get_name() == "lobo";
    command
        .disable_help_flag(true)
        .disable_help_subcommand(!root)
        .arg(
            clap::Arg::new("help")
                .short('h')
                .long("help")
                .action(clap::ArgAction::Help)
                .help(help),
        )
        .help_template("{about-with-newline}\nUsage:\n  {usage}\n\n{all-args}{after-help}")
        .mut_args(|a| {
            let hide_default = a
                .get_default_values()
                .first()
                .is_some_and(|s| ["", "0", "false"].iter().any(|v| s == v));
            let a = a.hide_default_value(hide_default);
            if a.get_long().is_some() && a.get_id() != "config" {
                a.help_heading("Flags")
            } else {
                a
            }
        })
        .subcommand_help_heading("Available Commands")
        .mut_subcommands(|sub| layout(sub).display_order(0))
}
pub fn changed(m: &clap::ArgMatches) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for id in m.ids() {
        if m.value_source(id.as_str()) == Some(clap::parser::ValueSource::CommandLine) {
            out.insert(id.as_str().replace('_', "-"));
        }
    }
    if let Some((_, sub)) = m.subcommand() {
        out.extend(changed(sub));
    }
    out
}
pub fn parse_bool_flag(s: &str) -> Result<bool, String> {
    match s {
        "true" | "1" | "t" | "T" | "TRUE" | "True" => Ok(true),
        "false" | "0" | "f" | "F" | "FALSE" | "False" => Ok(false),
        _ => Err(format!("invalid boolean {s:?}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::FromArgMatches;
    #[test]
    fn tree_and_booleans() {
        command().debug_assert();
        for flag in ["--q6", "--q6=true", "--q6=false", "--q6=1", "--q6=F"] {
            let matches = command()
                .try_get_matches_from(["lobo", "up", flag])
                .unwrap();
            assert!(changed(&matches).contains("q6"));
            let cli = Cli::from_arg_matches(&matches).unwrap();
            let Some(Cmd::Up(a)) = cli.cmd else { panic!() };
            assert_eq!(a.q6, !matches!(flag, "--q6=false" | "--q6=F"));
        }
        assert!(!changed(&command().try_get_matches_from(["lobo", "up"]).unwrap()).contains("q6"));
        for s in ["yes", "no", "", "Truee"] {
            assert!(parse_bool_flag(s).is_err());
        }
    }
    #[test]
    fn explicit_flag_names_and_globals() {
        for (flag, val) in [
            ("provider", "x"),
            ("cloud", "secure"),
            ("ctx", "1"),
            ("idle-min", "1"),
            ("max-life", "1h"),
            ("min-mbps", "1"),
        ] {
            let m = command()
                .try_get_matches_from(["lobo", "up", &format!("--{flag}"), val])
                .unwrap();
            assert_eq!(changed(&m), BTreeSet::from([flag.to_owned()]));
        }
        let parsed = Cli::try_parse_from(["lobo", "version", "--env", "/x"]).unwrap();
        assert_eq!(parsed.config, Some("/x".into()));
    }
    #[test]
    fn positional_parity() {
        for args in [
            vec!["up", "extra"],
            vec!["up", "extra", "--q6=false"],
            vec!["up", "--", "extra"],
            vec!["version", "extra"],
            vec!["config", "path", "extra"],
        ] {
            assert!(Cli::try_parse_from(std::iter::once("lobo").chain(args)).is_ok());
        }
        for args in [
            vec!["up", "--bogus"],
            vec!["models", "extra"],
            vec!["config", "get"],
            vec!["config", "get", "X", "Y"],
            vec!["local", "run", "--ctx", "1", "--idle-min", "1", "x"],
        ] {
            assert!(Cli::try_parse_from(std::iter::once("lobo").chain(args)).is_err());
        }
        for flag in ["-n", "--n"] {
            let Some(Cmd::Logs(a)) = Cli::try_parse_from(["lobo", "logs", flag, "5"])
                .unwrap()
                .cmd
            else {
                panic!()
            };
            assert_eq!(a.n, 5);
        }
    }
}
