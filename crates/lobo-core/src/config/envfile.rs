use super::dotenv;
use crate::{Error, Result};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Write,
    os::unix::fs::{DirBuilderExt, PermissionsExt},
    path::Path,
};
pub struct LayoutGroup {
    pub title: &'static str,
    pub keys: &'static [&'static str],
}
pub const LAYOUT: &[LayoutGroup] = &[
    LayoutGroup {
        title: "providers (at least one)",
        keys: &["RUNPOD_API_KEY", "VASTAI_API_KEY"],
    },
    LayoutGroup {
        title: "access",
        keys: &[
            "LOBO_DOMAIN",
            "LOBO_API_KEY",
            "CF_TUNNEL_TOKEN",
            "LOBO_BUCKET_URL",
        ],
    },
    LayoutGroup {
        title: "defaults for `lobo up` (flags override; empty or 0 = built-in / release default)",
        keys: &[
            "LOBO_PROVIDER",
            "LOBO_MIN_MBPS",
            "LOBO_MODEL",
            "LOBO_CTX",
            "LOBO_IDLE_MIN",
            "LOBO_MAX_HOURS",
            "LOBO_CLOUD",
            "LOBO_VAST_MAX_DPH",
        ],
    },
];
pub const HEADER: &str = "# lobo config. Edit by hand or run `lobo config`.\n# Keys here never go into a pod image or a release. Flags on `lobo up` override the defaults.\n# Advanced keys: LOBO_CONNECTION, LOBO_CLOUD_PORT, LOBO_IMAGE_REPO, LOBO_MODEL_BASE_URL.\n";
pub(crate) fn quote(v: &str) -> String {
    if !v.contains([' ', '\t', '#', '"', '\'', '\\', '$', '\n', '\r']) {
        return v.into();
    }
    let mut out = String::from("\"");
    for c in v.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '$' => out.push_str("\\$"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}
pub(crate) fn sort_by_layout(keys: &mut [String]) {
    let order: Vec<_> = LAYOUT.iter().flat_map(|g| g.keys.iter().copied()).collect();
    keys.sort_by(|a, b| {
        let rank = |key: &str| order.iter().position(|k| *k == key).unwrap_or(order.len());
        rank(a).cmp(&rank(b)).then_with(|| a.cmp(b))
    });
}
pub fn values(path: &Path) -> Result<BTreeMap<String, String>> {
    match std::fs::read_to_string(path) {
        Ok(text) => {
            dotenv::parse(&text).map_err(|e| Error::Config(format!("read {}: {e}", path.display())))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(BTreeMap::new()),
        Err(e) => Err(Error::Config(format!("read {}: {e}", path.display()))),
    }
}
pub fn set_env_value(path: &Path, key: &str, value: &str) -> Result<()> {
    save(path, &[(key.to_owned(), value.to_owned())].into())
}
pub fn save(path: &Path, set: &BTreeMap<String, String>) -> Result<()> {
    super::readiness::validate_selection(set).map_err(Error::Config)?;
    let original = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e.into()),
    };
    let mut set = set.clone();
    // Read migration must not rewrite files. Persist a retired model on normal save.
    if !set.contains_key("LOBO_MODEL") {
        let old = dotenv::parse(&original)
            .map_err(|e| Error::Config(format!("read {}: {e}", path.display())))?;
        if old
            .get("LOBO_MODEL")
            .is_some_and(|v| matches!(v.as_str(), "q6" | "q8"))
        {
            set.insert("LOBO_MODEL".into(), lobo_proto::DEFAULT_MODEL.into());
        }
    }
    let mut lines = Vec::new();
    let mut done = BTreeSet::new();
    if original.is_empty() {
        lines.extend(HEADER.trim_end_matches('\n').split('\n').map(str::to_owned));
        for group in LAYOUT {
            lines.push(String::new());
            lines.push(format!("# {}", group.title));
            for &key in group.keys {
                if let Some(value) = set.get(key).filter(|v| !v.is_empty()) {
                    lines.push(format!("{key}={}", quote(value)));
                }
                done.insert(key.to_owned());
            }
        }
    } else {
        for line in original.trim_end_matches('\n').split('\n') {
            let trimmed = line.trim();
            let assignment = trimmed
                .strip_prefix("export ")
                .unwrap_or(trimmed)
                .split_once('=');
            if !trimmed.starts_with('#')
                && let Some((key, _)) = assignment
                && let Some(value) = set.get(key.trim())
            {
                let key = key.trim();
                if done.insert(key.to_owned()) && !value.is_empty() {
                    lines.push(format!("{key}={}", quote(value)));
                }
                continue;
            }
            lines.push(line.to_owned());
        }
    }
    let mut rest: Vec<_> = set
        .iter()
        .filter(|(k, v)| !done.contains(*k) && !v.is_empty())
        .map(|(k, _)| k.clone())
        .collect();
    sort_by_layout(&mut rest);
    for key in rest {
        lines.push(format!("{key}={}", quote(&set[&key])));
    }
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(parent)?;
    let mut tmp = tempfile::Builder::new()
        .prefix(".lobo-config-")
        .tempfile_in(parent)?;
    tmp.as_file()
        .set_permissions(std::fs::Permissions::from_mode(0o600))?;
    tmp.write_all((lines.join("\n") + "\n").as_bytes())?;
    tmp.flush()?;
    tmp.persist(path).map_err(|e| Error::Io(e.error))?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn quote_table() {
        assert_eq!(quote("plain-123"), "plain-123");
        for (v, want) in [
            ("x y", r#""x y""#),
            ("x#y", r#""x#y""#),
            ("x$A", r#""x\$A""#),
            ("x\ny", r#""x\ny""#),
            ("x\ry", r#""x\ry""#),
            ("a\\b", r#""a\\b""#),
            ("it's", r#""it's""#),
            ("x\ty", "\"x\ty\""),
        ] {
            assert_eq!(quote(v), want);
        }
    }
    #[test]
    fn sort_by_layout_then_alpha() {
        let mut keys = vec![
            "ZZ".into(),
            "LOBO_CTX".into(),
            "RUNPOD_API_KEY".into(),
            "AA".into(),
        ];
        sort_by_layout(&mut keys);
        assert_eq!(keys, ["RUNPOD_API_KEY", "LOBO_CTX", "AA", "ZZ"]);
    }
    #[test]
    fn header_describes_cloud_settings() {
        assert!(HEADER.contains("LOBO_CLOUD_PORT"));
        assert!(HEADER.contains("LOBO_MODEL_BASE_URL"));
        assert!(!HEADER.contains("LOBO_MODEL_SOURCE"));
        assert_eq!(HEADER.lines().count(), 3);
    }
    #[test]
    fn set_env_value_replaces_and_appends() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("config");
        std::fs::write(&p, "# comment\nA=1\nLOBO_API_KEY=old\nB=2\n").unwrap();
        set_env_value(&p, "LOBO_API_KEY", "new").unwrap();
        set_env_value(&p, "C", "3").unwrap();
        assert_eq!(
            std::fs::read_to_string(&p).unwrap(),
            "# comment\nA=1\nLOBO_API_KEY=new\nB=2\nC=3\n"
        );
        assert_eq!(
            std::fs::metadata(p).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    #[test]
    fn save_keeps_hand_edits() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("config");
        std::fs::write(
            &p,
            include_str!("../../fixtures/config_save/keeps_hand_edits.before.env"),
        )
        .unwrap();
        let set = serde_json::from_str(include_str!(
            "../../fixtures/config_save/keeps_hand_edits.set.json"
        ))
        .unwrap();
        save(&p, &set).unwrap();
        assert_eq!(
            std::fs::read_to_string(&p).unwrap(),
            include_str!("../../fixtures/config_save/keeps_hand_edits.after.env")
        );
        assert_eq!(
            values(&p).unwrap()["LOBO_MODEL_SSH_HOSTKEY"],
            "ssh-ed25519 AAAA#x"
        );
    }
    #[test]
    fn save_new_file_layout() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("sub/config");
        save(
            &p,
            &[
                ("VASTAI_API_KEY".into(), "vk".into()),
                ("LOBO_PROVIDER".into(), "vast".into()),
                ("R2_ENDPOINT".into(), "https://e".into()),
            ]
            .into(),
        )
        .unwrap();
        assert_eq!(
            std::fs::read_to_string(&p).unwrap(),
            format!(
                "{HEADER}\n# providers (at least one)\nVASTAI_API_KEY=vk\n\n# access\n\n# defaults for `lobo up` (flags override; empty or 0 = built-in / release default)\nLOBO_PROVIDER=vast\nR2_ENDPOINT=https://e\n"
            )
        );
        assert_eq!(
            std::fs::metadata(p.parent().unwrap())
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        assert!(values(&d.path().join("missing")).unwrap().is_empty());
    }
    #[test]
    fn save_round_trip_special_values() {
        let vals: [(&str, &str); 9] = [
            ("A", "sk-$MISSING"),
            ("B", r"a\b"),
            ("C", "x\ny"),
            ("D", r#"q"uo'te"#),
            ("E", "p #q"),
            ("F", "plain-123"),
            ("G", "a\tb"),
            ("H", "x\ry"),
            ("I", "it's"),
        ];
        let vals = vals
            .into_iter()
            .map(|(k, v)| (k.into(), v.into()))
            .collect();
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("config");
        save(&p, &vals).unwrap();
        assert_eq!(values(&p).unwrap(), vals);
    }
    #[test]
    fn save_existing_files_matches_legacy_go_bytes() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/config_save");
        let mut count = 0;
        for entry in std::fs::read_dir(dir).unwrap() {
            let p = entry.unwrap().path();
            let text = p.to_string_lossy();
            let Some(base) = text.strip_suffix(".set.json") else {
                continue;
            };
            let before = format!("{base}.before.env");
            if !Path::new(&before).exists()
                || std::fs::read_to_string(&before).unwrap().trim().is_empty()
            {
                continue;
            }
            count += 1;
            let d = tempfile::tempdir().unwrap();
            let config = d.path().join("config");
            std::fs::copy(before, &config).unwrap();
            let set = serde_json::from_slice(&std::fs::read(&p).unwrap()).unwrap();
            save(&config, &set).unwrap();
            assert_eq!(
                std::fs::read(config).unwrap(),
                std::fs::read(format!("{base}.after.env")).unwrap(),
                "{base}"
            );
        }
        assert_eq!(count, 8);
    }

    #[test]
    fn empty_and_whitespace_files_preserve_save_behavior() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("config");
        let set = BTreeMap::from([("LOBO_MODEL".into(), lobo_proto::DEFAULT_MODEL.into())]);
        std::fs::write(&p, "").unwrap();
        save(&p, &set).unwrap();
        let text = std::fs::read_to_string(&p).unwrap();
        assert!(text.starts_with(HEADER));
        assert!(text.ends_with("LOBO_MODEL=qwen38-q6\n"));
        assert!(!text.contains("LOBO_LOCAL_PORT"));
        assert!(!text.contains("LOBO_WEIGHTS_DIR"));
        std::fs::write(&p, "\n\n").unwrap();
        save(&p, &set).unwrap();
        assert_eq!(
            std::fs::read_to_string(&p).unwrap(),
            "\nLOBO_MODEL=qwen38-q6\n"
        );
    }

    #[test]
    fn save_migrates_retired_models_and_preserves_legacy_keys() {
        for model in ["q6", "q8"] {
            let d = tempfile::tempdir().unwrap();
            let p = d.path().join("config");
            let preserved = "# keep\nexport CUSTOM_THING='untouched value'\nLOBO_PROVIDER=local\nLOBO_LOCAL_PORT=invalid\nLOBO_WEIGHTS_DIR=/existing/models\n";
            std::fs::write(&p, format!("{preserved}LOBO_MODEL={model}\n")).unwrap();
            save(&p, &BTreeMap::from([("LOBO_CTX".into(), "32768".into())])).unwrap();
            assert_eq!(
                std::fs::read_to_string(&p).unwrap(),
                format!("{preserved}LOBO_MODEL=qwen38-q6\nLOBO_CTX=32768\n")
            );
            assert_eq!(values(&p).unwrap()["LOBO_PROVIDER"], "local");
        }
    }

    #[test]
    fn save_rejects_new_retired_selections_without_rewriting_the_file() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("config");
        let original = "# untouched\nLOBO_MODEL=q6\n";
        std::fs::write(&p, original).unwrap();
        for (key, value) in [
            ("LOBO_MODEL", "q6"),
            ("LOBO_MODEL", "q8"),
            ("LOBO_PROVIDER", "local"),
        ] {
            assert!(save(&p, &BTreeMap::from([(key.into(), value.into())])).is_err());
            assert_eq!(std::fs::read_to_string(&p).unwrap(), original);
        }
    }
}
