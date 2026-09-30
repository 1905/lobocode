use super::*;
use std::{
    fs,
    os::unix::fs::{PermissionsExt, symlink},
    sync::atomic::{AtomicUsize, Ordering},
};

fn binding() -> Binding {
    Binding {
        provider: "lobo-local".into(),
        model_alias: "qwen3.5-27b-q6".into(),
        context: 32768,
        endpoint: "http://127.0.0.1:8931/v1".into(),
        api_key: "test-secret-never-in-errors".into(),
    }
}

fn fixture() -> &'static str {
    r#"// prefix
{
  "small_model" : "other/small", // keep this exact
  "model" : "other/large",
  "default_agent": "custom",
  "provider": {
    "other": {"options": {"apiKey": "user-secret"}},
    "lobo-local": {
      "npm": "old-package",
      "options": {"baseURL": "old", "apiKey": "old", "headers" : { "X-User": "é漢" }},
      "models": {"untouched": { "limit" : {"context": 12} }},
    },
    "lobo": { "options" : { "baseURL" : "https://untouched/v1" } },
  },
  "agent": {
    "lobo": {"prompt" : "custom prompt", "tools": {"skill": true}, "permission": { "bash": "ask" }, "model": "old"},
    "custom": { "model" : "other/large", "temperature" : 0.5 },
  },
  "policy" : [ { "a": "b" } ],
}
// suffix
"#
}

#[test]
fn patch_preserves_unrelated_bytes_and_custom_agent() {
    let b = binding();
    for make_default in [false, true] {
        let result = patch_config(fixture(), &b, "{file:/private/test-key}", make_default).unwrap();
        for exact in [
            "// prefix\n",
            "// suffix\n",
            "\"small_model\" : \"other/small\", // keep this exact",
            "\"other\": {\"options\": {\"apiKey\": \"user-secret\"}}",
            "\"headers\" : { \"X-User\": \"é漢\" }",
            "\"untouched\": { \"limit\" : {\"context\": 12} }",
            "\"lobo\": { \"options\" : { \"baseURL\" : \"https://untouched/v1\" } }",
            "\"prompt\" : \"custom prompt\", \"tools\": {\"skill\": true}, \"permission\": { \"bash\": \"ask\" }",
            "\"custom\": { \"model\" : \"other/large\", \"temperature\" : 0.5 }",
            "\"policy\" : [ { \"a\": \"b\" } ]",
        ] {
            assert!(result.contains(exact), "lost unrelated bytes: {exact}");
        }
        assert!(result.contains("{file:/private/test-key}"));
        assert!(result.contains("32768"));
        assert!(!result.contains(&b.api_key));
        if !make_default {
            assert!(result.contains("\"model\" : \"other/large\""));
            assert!(result.contains("\"default_agent\": \"custom\""));
        }
        assert_eq!(
            patch_config(&result, &b, "{file:/private/test-key}", make_default).unwrap(),
            result
        );
    }
}

#[test]
fn patch_handles_comments_crlf_unicode_and_empty_objects() {
    for src in [
        "{}",
        "{/* keep */}",
        "{\r\n// keep\r\n}",
        "{\"unrelated\":\"é,}漢\" // comma, brace }\n}",
        "{\"unrelated\":1,/* ,} */}",
    ] {
        let result = patch_config(src, &binding(), "{file:/private/key}", false).unwrap();
        assert!(!result.contains("default_agent"));
        assert!(result.contains("\"*\":false"));
        assert!(result.contains("\"mcp:*\":\"deny\""));
        assert_eq!(
            patch_config(&result, &binding(), "{file:/private/key}", false).unwrap(),
            result
        );
    }
}

#[test]
fn patch_rejects_duplicate_decoded_keys_and_wrong_containers_without_secrets() {
    for src in [
        r#"{"agent":{},"\u0061gent":{}}"#,
        r#"{"untouched":[{"a":1,"a":2}]}"#,
        r#"{"provider":[]}"#,
        r#"{"provider":{"lobo-local":null}}"#,
        r#"{"provider":{"lobo-local":{"options":false}}}"#,
        r#"{"provider":{"lobo-local":{"models":{"qwen3.5-27b-q6":[]}}}}"#,
        r#"{"agent":{"lobo":"bad"}}"#,
        "[]",
        "null",
        "",
        "// only a comment",
        "{unquoted:1}",
        "{'single':1}",
        "{\"a\":1 \"b\":2}",
        "{\"a\":+1}",
        "{\"a\":0xff}",
        "{\"test-secret-never-in-errors\": }",
    ] {
        let err = patch_config(src, &binding(), "{file:/private/key}", true).unwrap_err();
        assert!(!err.to_string().contains("test-secret"));
        assert!(!err.to_string().contains("unquoted"));
    }
}

#[test]
fn contexts_and_output_limits_follow_binding() {
    for context in [4096, 32768, 65536, 131072] {
        let mut b = binding();
        b.context = context;
        b.provider = "lobo".into();
        b.model_alias = "qwen3.5-27b-q8".into();
        let result = patch_config("{}", &b, "{file:/private/key}", true).unwrap();
        assert!(result.contains(&format!("\"context\":{context}")));
        assert!(result.contains(&format!("\"output\":{}", context.min(8192))));
        assert!(result.contains("lobo/qwen3.5-27b-q8"));
    }
}

#[test]
fn discovery_precedence_and_missing_config() {
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join("opencode");
    assert_eq!(
        discover_config(dir.path()).unwrap().path,
        folder.join("opencode.jsonc")
    );
    fs::create_dir(&folder).unwrap();
    for file in ["config.json", "opencode.json", "opencode.jsonc"] {
        fs::write(folder.join(file), "{}").unwrap();
        assert_eq!(discover_config(dir.path()).unwrap().path, folder.join(file));
    }
}

#[test]
fn private_write_noop_and_rotation_keep_old_references() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("opencode.jsonc");
    let keys = dir.path().join("private/keys");
    fs::write(&path, fixture()).unwrap();
    let first = configure(&path, &keys, &binding(), false, &|| Ok(())).unwrap();
    assert!(first.changed);
    let first_bytes = fs::read(&path).unwrap();
    let old_keys: Vec<_> = fs::read_dir(&keys)
        .unwrap()
        .map(|p| p.unwrap().path())
        .collect();
    assert_eq!(old_keys.len(), 1);
    assert_eq!(fs::read_to_string(&old_keys[0]).unwrap(), binding().api_key);
    assert_eq!(
        fs::metadata(&keys).unwrap().permissions().mode() & 0o777,
        0o700
    );
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert_eq!(
        fs::metadata(&old_keys[0]).unwrap().permissions().mode() & 0o777,
        0o600
    );
    let entries_before = fs::read_dir(dir.path()).unwrap().count();
    let validations = AtomicUsize::new(0);
    let noop = configure(&path, &keys, &binding(), false, &|| {
        validations.fetch_add(1, Ordering::SeqCst);
        Ok(())
    })
    .unwrap();
    assert!(!noop.changed);
    assert_eq!(validations.load(Ordering::SeqCst), 1);
    assert_eq!(fs::read(&path).unwrap(), first_bytes);
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), entries_before);
    assert_eq!(fs::read_dir(&keys).unwrap().count(), 1);
    let mut rotated = binding();
    rotated.api_key = "rotated-test-secret".into();
    configure(&path, &keys, &rotated, false, &|| Ok(())).unwrap();
    assert_eq!(fs::read_dir(&keys).unwrap().count(), 2);
    assert_eq!(fs::read_to_string(&old_keys[0]).unwrap(), binding().api_key);
    let backups: Vec<_> = fs::read_dir(dir.path())
        .unwrap()
        .map(|p| p.unwrap().path())
        .filter(|p| p.file_name().unwrap().to_string_lossy().contains("backup-"))
        .collect();
    assert_eq!(backups.len(), 2);
    assert!(backups.iter().any(|p| fs::read(p).unwrap() == first_bytes));
    for p in backups {
        assert_eq!(fs::metadata(p).unwrap().permissions().mode() & 0o777, 0o600);
    }
}

#[test]
fn failed_validation_removes_only_new_files_and_preserves_original() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("opencode.jsonc");
    let keys = dir.path().join("keys");
    fs::write(&path, fixture()).unwrap();
    let err = configure(&path, &keys, &binding(), false, &|| {
        Err(Error::Other("test-secret-never-in-errors".into()))
    })
    .unwrap_err();
    assert!(!err.to_string().contains("test-secret"));
    assert_eq!(fs::read_to_string(&path).unwrap(), fixture());
    assert_eq!(fs::read_dir(&keys).unwrap().count(), 0);
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 2);
}

#[test]
fn concurrent_edit_during_validation_is_not_overwritten() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("opencode.jsonc");
    let keys = dir.path().join("keys");
    fs::write(&path, fixture()).unwrap();
    let external = "{\"external\":true}";
    configure(&path, &keys, &binding(), false, &|| {
        fs::write(&path, external).unwrap();
        Ok(())
    })
    .unwrap_err();
    assert_eq!(fs::read_to_string(&path).unwrap(), external);
    assert_eq!(fs::read_dir(&keys).unwrap().count(), 0);
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 2);
}

#[test]
fn config_symlinks_and_swapped_targets_are_refused() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("opencode.jsonc");
    let target = dir.path().join("target");
    let keys = dir.path().join("keys");
    fs::write(&target, fixture()).unwrap();
    symlink(&target, &path).unwrap();
    configure(&path, &keys, &binding(), false, &|| Ok(())).unwrap_err();
    assert_eq!(fs::read_to_string(&target).unwrap(), fixture());
    fs::remove_file(&path).unwrap();
    fs::write(&path, fixture()).unwrap();
    configure(&path, &keys, &binding(), false, &|| {
        fs::remove_file(&path).unwrap();
        symlink(&target, &path).unwrap();
        Ok(())
    })
    .unwrap_err();
    assert!(
        fs::symlink_metadata(&path)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(fs::read_to_string(&target).unwrap(), fixture());
    assert_eq!(fs::read_dir(&keys).unwrap().count(), 0);
}

#[test]
fn restrictions_report_policy_without_rewriting_it() {
    assert_eq!(
        inspect_restrictions("{}", "lobo").unwrap(),
        ConfigRestrictions {
            provider_disabled: false,
            provider_not_enabled: false
        }
    );
    let source =
        r#"{/*keep*/"disabled_providers":["lobo", "other",],"enabled_providers":["lobo-local"]}"#;
    assert_eq!(
        inspect_restrictions(source, "lobo").unwrap(),
        ConfigRestrictions {
            provider_disabled: true,
            provider_not_enabled: true
        }
    );
    assert_eq!(
        inspect_restrictions(source, "lobo-local").unwrap(),
        ConfigRestrictions {
            provider_disabled: false,
            provider_not_enabled: false
        }
    );
    let patched = patch_config(source, &binding(), "{file:/private/key}", false).unwrap();
    assert!(patched.contains(
        r#"/*keep*/"disabled_providers":["lobo", "other",],"enabled_providers":["lobo-local"]"#
    ));
    assert!(
        inspect_restrictions(r#"{"enabled_providers":[]}"#, "lobo")
            .unwrap()
            .provider_not_enabled
    );
}

#[test]
fn restrictions_reject_malformed_policy_and_duplicate_keys() {
    for source in [
        r#"{"disabled_providers":false}"#,
        r#"{"enabled_providers":null}"#,
        r#"{"disabled_providers":["lobo",4]}"#,
        r#"{"enabled_providers":[{}]}"#,
        r#"{"disabled_providers":[],"disabled_providers":[]}"#,
        "{\"test-secret\":}",
    ] {
        let error = inspect_restrictions(source, "lobo").unwrap_err();
        assert!(!error.to_string().contains("test-secret"));
    }
}

#[test]
fn interrupted_atomic_replacement_preserves_original_and_removes_prepared_files() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("opencode.jsonc");
    let keys = dir.path().join("keys");
    fs::write(&path, fixture()).unwrap();
    let replacement_called = AtomicUsize::new(0);
    configure_with_replace(&path, &keys, &binding(), false, &|| Ok(()), &|file, _| {
        replacement_called.fetch_add(1, Ordering::SeqCst);
        assert!(file.path().exists());
        assert_eq!(fs::read_dir(&keys).unwrap().count(), 1);
        Err(error("atomic config replacement failed"))
    })
    .unwrap_err();
    assert_eq!(replacement_called.load(Ordering::SeqCst), 1);
    assert_eq!(fs::read_to_string(&path).unwrap(), fixture());
    assert_eq!(fs::read_dir(&keys).unwrap().count(), 0);
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 2);
}

#[test]
fn arbitrary_or_symlink_key_references_are_never_reused() {
    let dir = tempfile::tempdir().unwrap();
    let keys = dir.path().join("keys");
    let path = dir.path().join("opencode.jsonc");
    let other = dir.path().join("unrelated-file");
    fs::write(&other, binding().api_key).unwrap();
    let src = patch_config(
        "{}",
        &binding(),
        &format!("{{file:{}}}", other.display()),
        false,
    )
    .unwrap();
    fs::write(&path, src).unwrap();
    configure(&path, &keys, &binding(), false, &|| Ok(())).unwrap();
    assert!(
        !fs::read_to_string(&path)
            .unwrap()
            .contains(&format!("{{file:{}}}", other.display()))
    );
    assert_eq!(fs::read_to_string(&other).unwrap(), binding().api_key);
    let link = keys.join("lobo-local-abcdef.key");
    symlink(&other, &link).unwrap();
    let src = patch_config(
        "{}",
        &binding(),
        &format!("{{file:{}}}", link.display()),
        false,
    )
    .unwrap();
    fs::write(&path, src).unwrap();
    configure(&path, &keys, &binding(), false, &|| Ok(())).unwrap();
    assert!(
        fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert!(
        !fs::read_to_string(&path)
            .unwrap()
            .contains(&format!("{{file:{}}}", link.display()))
    );
    assert_eq!(fs::read_to_string(&other).unwrap(), binding().api_key);
}

#[test]
fn invalid_config_leaves_everything_intact_before_key_allocation() {
    let dir = tempfile::tempdir().unwrap();
    let keys = dir.path().join("keys");
    let path = dir.path().join("opencode.jsonc");
    let src = "{\"provider\": [], \"test-secret\": \"private\"}";
    fs::write(&path, src).unwrap();
    configure(&path, &keys, &binding(), false, &|| {
        panic!("invalid file must not validate")
    })
    .unwrap_err();
    assert_eq!(fs::read_to_string(&path).unwrap(), src);
    assert!(!keys.exists());
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
}

#[test]
fn fresh_config_has_no_backup_and_failed_noop_stays_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    let keys = dir.path().join("keys");
    let path = dir.path().join("config/opencode.jsonc");
    configure(&path, &keys, &binding(), true, &|| Ok(())).unwrap();
    assert_eq!(fs::read_dir(path.parent().unwrap()).unwrap().count(), 1);
    let bytes = fs::read(&path).unwrap();
    configure(&path, &keys, &binding(), true, &|| Err(Error::Cancelled)).unwrap_err();
    assert_eq!(fs::read(&path).unwrap(), bytes);
    assert_eq!(fs::read_dir(&keys).unwrap().count(), 1);
    assert_eq!(fs::read_dir(path.parent().unwrap()).unwrap().count(), 1);
}
