use clap::Command;
use regex_lite::Regex;
use std::{collections::BTreeMap, path::Path};

#[derive(Debug, PartialEq)]
struct Flag {
    short: Option<char>,
    kind: String,
    default: Option<String>,
    help: String,
}
#[derive(Debug, PartialEq)]
struct Facts {
    about: String,
    flags: BTreeMap<String, Flag>,
    children: BTreeMap<String, String>,
}
fn cobra(text: &str) -> Facts {
    let mut facts = Facts {
        about: text.split("\n\nUsage:").next().unwrap().into(),
        flags: BTreeMap::new(),
        children: BTreeMap::new(),
    };
    let flag = Regex::new(
        r"^  (?:-([a-zA-Z]), )?\s*--([a-zA-Z0-9-]+)(?: (string|int|duration))?\s{2,}(.*)$",
    )
    .unwrap();
    let default = Regex::new(r#"^(.*) \(default (".*"|[0-9].*|true|false)\)$"#).unwrap();
    let mut child = false;
    for line in text.lines() {
        if line == "Available Commands:" {
            child = true;
            continue;
        }
        if child {
            if line.is_empty() {
                child = false;
                continue;
            }
            let mut parts = line.trim().splitn(2, char::is_whitespace);
            facts.children.insert(
                parts.next().unwrap().into(),
                parts.next().unwrap_or("").trim().into(),
            );
        }
        if let Some(c) = flag.captures(line) {
            if &c[2] == "help" {
                continue;
            }
            let (help, def) = if let Some(d) = default.captures(&c[4]) {
                (d[1].to_owned(), Some(d[2].trim_matches('"').to_owned()))
            } else {
                (c[4].into(), None)
            };
            facts.flags.insert(
                c[2].into(),
                Flag {
                    short: c.get(1).map(|v| v.as_str().chars().next().unwrap()),
                    kind: c.get(3).map_or("", |v| v.as_str()).into(),
                    default: def,
                    help,
                },
            );
        }
    }
    facts
}
fn clap(cmd: &Command) -> Facts {
    let mut facts = Facts {
        about: cmd
            .get_long_about()
            .or_else(|| cmd.get_about())
            .unwrap()
            .to_string(),
        flags: BTreeMap::new(),
        children: BTreeMap::new(),
    };
    for a in cmd.get_arguments().filter(|a| !a.is_hide_set()) {
        let Some(long) = a.get_long().filter(|v| *v != "help") else {
            continue;
        };
        let raw = a
            .get_default_values()
            .first()
            .map(|v| v.to_string_lossy().into_owned());
        let def = raw.filter(|v| !["", "0", "false"].contains(&v.as_str()));
        facts.flags.insert(
            long.into(),
            Flag {
                short: a.get_short(),
                kind: if a.get_value_parser().type_id() == std::any::TypeId::of::<bool>() {
                    String::new()
                } else {
                    a.get_value_names().unwrap()[0].to_string()
                },
                default: def,
                help: a.get_help().unwrap().to_string(),
            },
        );
    }
    for sub in cmd
        .get_subcommands()
        .filter(|s| !s.is_hide_set() && s.get_name() != "help")
    {
        facts
            .children
            .insert(sub.get_name().into(), sub.get_about().unwrap().to_string());
    }
    facts
}
#[test]
fn help_facts_match_go() {
    let mut root = lobo_cli::cli::command().mut_arg("config", |a| {
        a.default_value("/home/u/.config/lobo/config.env")
    });
    root.build();
    for file in
        std::fs::read_dir(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/go/help"))
            .unwrap()
    {
        let file = file.unwrap().path();
        let name = file.file_stem().unwrap().to_str().unwrap();
        if name.starts_with("root") {
            continue;
        }
        let mut cmd = &root;
        for part in name.split('_') {
            cmd = cmd.find_subcommand(part).unwrap();
        }
        assert_eq!(
            clap(cmd),
            cobra(&std::fs::read_to_string(&file).unwrap()),
            "{name}"
        );
    }
}
#[test]
fn help_snapshots() {
    for name in [
        "up",
        "status",
        "logs",
        "test",
        "down",
        "config",
        "config_show",
        "config_set",
        "config_get",
        "config_path",
        "models",
        "gen-api-key",
        "version",
        "release",
        "local",
        "local_run",
        "completion",
    ] {
        let output = assert_cmd::cargo::cargo_bin_cmd!("lobo")
            .env("XDG_CONFIG_HOME", "/home/u/.config")
            .env("NO_COLOR", "1")
            .args(name.split('_'))
            .arg("--help")
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        insta::assert_snapshot!(name, String::from_utf8(output).unwrap());
    }
}
