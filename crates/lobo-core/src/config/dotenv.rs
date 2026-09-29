//! godotenv v1.5.1 parser port. Expansion reads only this file's earlier keys.
//! Copyright (c) 2013 John Barton; MIT license in licenses/godotenv-MIT.txt.
use crate::{Error, Result};
use regex_lite::Regex;
use std::{collections::BTreeMap, sync::LazyLock};
fn space(c: char) -> bool {
    matches!(
        c,
        '\t' | '\u{b}' | '\u{c}' | '\r' | ' ' | '\u{85}' | '\u{a0}'
    )
}
pub fn parse(src: &str) -> Result<BTreeMap<String, String>> {
    let src = src.replace("\r\n", "\n");
    let mut rest = src.as_str();
    let mut out = BTreeMap::new();
    loop {
        rest = rest.trim_start_matches(char::is_whitespace);
        if rest.is_empty() {
            break;
        }
        if rest.starts_with('#') {
            rest = rest.find('\n').map(|n| &rest[n..]).unwrap_or("");
            continue;
        }
        rest = rest.trim_start_matches(space);
        if let Some(left) = rest.strip_prefix("export")
            && left.chars().next().is_some_and(space)
        {
            rest = left.trim_start_matches(space);
        }
        let mut key = "";
        let mut offset = 0;
        for (i, b) in rest.bytes().enumerate() {
            if space(b as char) {
                continue;
            }
            if b == b'=' || b == b':' {
                key = &rest[..i];
                offset = i + 1;
                break;
            }
            if b != b'_' && b != b'.' && !(b as char).is_alphanumeric() {
                return Err(Error::Config(format!(
                    "unexpected character {:?} in variable name near {:?}",
                    (b as char).to_string(),
                    rest
                )));
            }
        }
        let key = key.trim_end_matches(char::is_whitespace).to_owned();
        rest = rest[offset..].trim_start_matches(space);
        let (value, left) = extract(rest, &out)?;
        out.insert(key, value);
        rest = left;
    }
    Ok(out)
}
fn extract<'a>(src: &'a str, vars: &BTreeMap<String, String>) -> Result<(String, &'a str)> {
    let quote = src.as_bytes().first().copied();
    if !matches!(quote, Some(b'\'' | b'"')) {
        let end = src.find(['\n', '\r']).unwrap_or(src.len());
        let line = &src[..end];
        let chars: Vec<_> = line.char_indices().collect();
        let mut end_var = line.len();
        for i in (1..chars.len()).rev() {
            if chars[i].1 == '#' && space(chars[i - 1].1) {
                end_var = chars[i].0;
                break;
            }
        }
        return Ok((
            expand(line[..end_var].trim_matches(space), vars),
            &src[end..],
        ));
    }
    let quote = quote.unwrap();
    for i in 1..src.len() {
        if src.as_bytes()[i] != quote || src.as_bytes()[i - 1] == b'\\' {
            continue;
        }
        let mut value = src[..i].trim_matches(quote as char).to_owned();
        if quote == b'"' {
            value = expand(&unescape(&value), vars);
        }
        return Ok((value, &src[i + 1..]));
    }
    let end = src.find('\n').unwrap_or(src.len());
    Err(Error::Config(format!(
        "unterminated quoted value {}",
        &src[..end]
    )))
}
fn unescape(value: &str) -> String {
    static ESCAPE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\\.").unwrap());
    static UNESCAPE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\\([^$])").unwrap());
    let expanded = ESCAPE.replace_all(value, |c: &regex_lite::Captures<'_>| match &c[0] {
        r"\n" => "\n".to_owned(),
        r"\r" => "\r".to_owned(),
        other => other.to_owned(),
    });
    UNESCAPE.replace_all(&expanded, "$1").into_owned()
}
fn expand(value: &str, vars: &BTreeMap<String, String>) -> String {
    static VAR: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(\\)?(\$)(\()?\{?([A-Z0-9_]+)?\}?").unwrap());
    VAR.replace_all(value, |c: &regex_lite::Captures<'_>| {
        if c.get(1).is_some() {
            return c[0][1..].to_owned();
        }
        if let Some(key) = c.get(4) {
            return vars.get(key.as_str()).cloned().unwrap_or_default();
        }
        c[0].to_owned()
    })
    .into_owned()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn golden_cases() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/dotenv");
        let mut count = 0;
        for entry in std::fs::read_dir(dir).unwrap() {
            let p = entry.unwrap().path();
            if p.extension().and_then(|x| x.to_str()) != Some("env") {
                continue;
            }
            count += 1;
            let expected: serde_json::Value =
                serde_json::from_slice(&std::fs::read(p.with_extension("json")).unwrap()).unwrap();
            let actual = parse(&std::fs::read_to_string(&p).unwrap());
            if let Some(error) = expected.get("error").and_then(|v| v.as_str()) {
                assert!(
                    actual
                        .unwrap_err()
                        .to_string()
                        .contains(&error[..error.len().min(20)]),
                    "{}",
                    p.display()
                );
            } else {
                assert_eq!(
                    serde_json::to_value(actual.unwrap()).unwrap(),
                    expected,
                    "{}",
                    p.display()
                );
            }
        }
        assert_eq!(count, 15);
    }
    #[test]
    fn never_reads_process_env() {
        assert_eq!(
            parse("H=$HOME\nP=${PATH}\n").unwrap(),
            [("H".into(), "".into()), ("P".into(), "".into())].into()
        );
    }
}
