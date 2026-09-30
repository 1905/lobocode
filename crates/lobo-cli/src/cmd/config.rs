use crate::{
    app::{App, Io},
    cli::{ConfigArgs, ConfigCmd},
};
use anyhow::{Context, Result, bail};
use lobo_core::config;
use serde::Deserialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{Read, Write},
    path::Path,
};

fn valid_key(s: &str) -> bool {
    s.as_bytes().first().is_some_and(u8::is_ascii_uppercase)
        && s.bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
}
pub fn parse_set_args(args: &[String]) -> Result<BTreeMap<String, String>> {
    let mut out = BTreeMap::new();
    for a in args {
        let (k, v) = a
            .split_once('=')
            .filter(|(k, _)| valid_key(k))
            .ok_or_else(|| {
                anyhow::anyhow!("want KEY=value with KEY like LOBO_MIN_MBPS, got {a:?}")
            })?;
        out.insert(k.into(), v.into());
    }
    Ok(out)
}
pub fn parse_set_json(r: impl Read) -> Result<BTreeMap<String, String>> {
    // Match Go's first-value-only decoder, including null strings as empty values.
    let mut de = serde_json::Deserializer::from_reader(r.take(1 << 20));
    let values = Option::<BTreeMap<String, Option<String>>>::deserialize(&mut de)
        .context("config set --stdin: want a JSON object of strings")?
        .unwrap_or_default();
    if values.is_empty() {
        bail!("config set --stdin: empty object");
    }
    for k in values.keys() {
        if !valid_key(k) {
            bail!("config set --stdin: bad key {k:?} (want like LOBO_MIN_MBPS)");
        }
    }
    Ok(values
        .into_iter()
        .map(|(k, v)| (k, v.unwrap_or_default()))
        .collect())
}
pub fn show_config_text(
    w: &mut dyn Write,
    path: &Path,
    cur: &BTreeMap<String, String>,
) -> std::io::Result<()> {
    writeln!(w, "# {}", path.display())?;
    if cur.is_empty() {
        return writeln!(w, "# no config yet: run `lobo config` in a terminal");
    }
    let mut seen = BTreeSet::new();
    for group in config::LAYOUT {
        writeln!(w, "\n# {}", group.title)?;
        for &key in group.keys {
            seen.insert(key);
            if let Some(v) = cur.get(key) {
                writeln!(w, "{key}={}", config::masked(key, v))?;
            }
        }
    }
    let rest: Vec<_> = cur
        .iter()
        .filter(|(k, _)| !seen.contains(k.as_str()))
        .collect();
    if !rest.is_empty() {
        writeln!(w, "\n# other")?;
    }
    for (k, v) in rest {
        writeln!(w, "{k}={}", config::masked(k, v))?;
    }
    Ok(())
}
pub fn run(app: &App, args: ConfigArgs, path: &Path, io: &mut Io) -> Result<()> {
    match args.cmd {
        Some(ConfigCmd::Path(_)) => writeln!(io.out, "{}", path.display())?,
        Some(ConfigCmd::Get { key }) => {
            let values = config::values(path)?;
            let v = values
                .get(&key)
                .ok_or_else(|| anyhow::anyhow!("{key} is not set in {}", path.display()))?;
            writeln!(io.out, "{v}")?;
        }
        Some(ConfigCmd::Set(a)) => {
            let values = match (a.stdin, a.values.is_empty()) {
                (true, false) => bail!("use KEY=value arguments or --stdin, not both"),
                (true, true) => parse_set_json(&mut io.input)?,
                (false, true) => bail!("want KEY=value arguments or --stdin"),
                (false, false) => parse_set_args(&a.values)?,
            };
            config::save(path, &values)?;
        }
        Some(ConfigCmd::Show(a)) if a.json => {
            serde_json::to_writer(&mut io.out, &config::show(path)?)?;
            writeln!(io.out)?;
        }
        Some(ConfigCmd::Show(_)) => show_config_text(&mut io.out, path, &config::values(path)?)?,
        None => {
            let values = config::values(path)?;
            if app.term.stdout_tty && app.term.stdin_tty {
                let set = crate::wizard::run_wizard(
                    &mut *(app.prompter)(),
                    path,
                    values.clone(),
                    (app.local_supported)().is_ok(),
                )?;
                if let Some(set) = set {
                    config::save(path, &set)?;
                    writeln!(io.out, "saved {}", path.display())?;
                    if set.get("LOBO_API_KEY") != values.get("LOBO_API_KEY")
                        && values.get("LOBO_API_KEY").is_some_and(|s| !s.is_empty())
                    {
                        writeln!(
                            io.out,
                            "new LOBO_API_KEY: a running pod keeps the old one until the next `lobo up`; `lobo gen-api-key` rewrites opencode.lobo.json"
                        )?;
                    }
                    if let Err(e) = config::load_laptop(path) {
                        writeln!(io.out, "still missing before `lobo up` works: {e}")?;
                    }
                } else {
                    writeln!(io.out, "nothing saved")?;
                }
            } else {
                writeln!(
                    io.err,
                    "not a terminal: showing {} instead of the form",
                    path.display()
                )?;
                show_config_text(&mut io.out, path, &values)?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parse_set() {
        let m = parse_set_args(&[
            "LOBO_CTX=8192".into(),
            "CF_TUNNEL_TOKEN=a=b".into(),
            "LOBO_CTX=".into(),
        ])
        .unwrap();
        assert_eq!(m["LOBO_CTX"], "");
        assert_eq!(m["CF_TUNNEL_TOKEN"], "a=b");
        for s in ["x=1", "1KEY=1", "A", "=x", "A B=x"] {
            assert!(parse_set_args(&[s.into()]).is_err());
        }
        assert_eq!(
            parse_set_json(&b"{\"A\":\"x\"} ignored"[..]).unwrap()["A"],
            "x"
        );
        assert_eq!(parse_set_json(&b"{\"A\":null}"[..]).unwrap()["A"], "");
        for s in ["{}", "null", "[]", "{\"A\":1}", "{\"lower\":\"x\"}"] {
            assert!(parse_set_json(s.as_bytes()).is_err());
        }
        let large = format!("{{\"A\":\"{}\"}}", "a".repeat(1 << 20));
        assert!(parse_set_json(large.as_bytes()).is_err());
    }
}
