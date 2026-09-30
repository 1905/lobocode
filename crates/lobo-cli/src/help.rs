use std::{
    io::{self, Write},
    path::Path,
};
use unicode_width::UnicodeWidthStr;

pub fn root_help(w: &mut dyn Write, color: bool, have_config: bool, path: &Path) -> io::Result<()> {
    let style = |text: &str, ansi: &str| {
        if color {
            format!("\x1b[{ansi}m{text}\x1b[0m")
        } else {
            text.into()
        }
    };
    let pad = |s: &str, n: usize| format!("{s}{}", " ".repeat(n.saturating_sub(s.width())));
    writeln!(
        w,
        "{} {}",
        style("lobo", "1;38;2;125;86;244"),
        style(crate::build_info::VERSION, "38;2;139;148;158")
    )?;
    writeln!(
        w,
        "Rent an RTX 5090 on RunPod or Vast.ai and serve Qwen3.5-27B behind an OpenAI-compatible API.\nPods delete themselves when idle or expired."
    )?;
    if !have_config {
        writeln!(
            w,
            "\n{} run {} first (no config at {})",
            style("⚠", "38;2;210;153;34"),
            style("lobo config", "38;2;63;185;80"),
            path.display()
        )?;
    }
    let tree = crate::cli::command();
    for (group, names) in [
        ("Run", &["up", "status", "logs", "test", "down"][..]),
        ("Setup", &["config", "models", "gen-api-key", "version"][..]),
    ] {
        writeln!(w, "\n{}", style(group, "1"))?;
        for name in names {
            let cmd = tree.find_subcommand(name).expect("help command exists");
            writeln!(
                w,
                "  {} {}",
                style(&pad(name, 12), "38;2;63;185;80"),
                cmd.get_about().unwrap()
            )?;
        }
    }
    writeln!(w, "\n{}", style("Examples", "1"))?;
    for (cmd, description) in [
        ("lobo config", "set API keys and defaults (first run)"),
        ("lobo up", "rent a 5090 and boot the model; live progress"),
        (
            "lobo up --provider vast",
            "rent on Vast.ai instead of the default provider",
        ),
        (
            "lobo up --provider local",
            "run on this Mac (Apple Silicon, llama.cpp); no cloud keys",
        ),
        (
            "lobo up --q6 --min-mbps 200",
            "smaller model, drop hosts slower than 200 MB/s",
        ),
        (
            "lobo status",
            "live dashboard: GPU, tokens/s, idle timer, cost",
        ),
        ("lobo down", "delete every lobo pod on every provider"),
    ] {
        writeln!(
            w,
            "  {} {}",
            style(&pad(cmd, 29), "38;2;63;185;80"),
            style(description, "38;2;139;148;158")
        )?;
    }
    writeln!(
        w,
        "\n{}\n  {}\n  {}\n\n{}",
        style("Config", "1"),
        path.display(),
        style(
            "plain KEY=value lines: edit by hand or with `lobo config`; flags override its defaults",
            "38;2;139;148;158"
        ),
        style("`lobo <command> --help` for flags.", "38;2;139;148;158")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn root_help_golden() {
        for (have, expected) in [
            (true, include_str!("../tests/goldens/help_config.golden")),
            (false, include_str!("../tests/goldens/help_noconfig.golden")),
        ] {
            let mut out = Vec::new();
            root_help(
                &mut out,
                false,
                have,
                Path::new("/home/u/.config/lobo/config.env"),
            )
            .unwrap();
            assert_eq!(
                String::from_utf8(out).unwrap(),
                expected.replace(
                    "lobo dev\n",
                    &format!("lobo {}\n", crate::build_info::VERSION)
                )
            );
        }
    }
}
