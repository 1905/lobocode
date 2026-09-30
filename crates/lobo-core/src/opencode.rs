//! Private, comment-preserving OpenCode configuration for an authenticated runtime.
use crate::{Error, Result};
use jsonc_parser::{
    CollectOptions, ParseOptions, ParseResult,
    ast::{Object, Value},
    common::Ranged,
};
use std::{
    collections::HashSet,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt},
    path::{Component, Path, PathBuf},
};

// The key must never reach a derived Debug or frontend serialization implementation.
pub struct Binding {
    pub provider: String,
    pub model_alias: String,
    pub context: u64,
    pub endpoint: String,
    pub api_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigChoice {
    pub path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigOutcome {
    pub path: PathBuf,
    pub provider: String,
    pub model_alias: String,
    pub changed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConfigRestrictions {
    pub provider_disabled: bool,
    pub provider_not_enabled: bool,
}

pub fn inspect_restrictions(source: &str, provider: &str) -> Result<ConfigRestrictions> {
    let parsed = parse(source)?;
    let object = root(&parsed);
    let contains = |name: &str| -> Result<Option<bool>> {
        let Some(prop) = object.get(name) else {
            return Ok(None);
        };
        let array = prop
            .value
            .as_array()
            .ok_or_else(|| error("provider policy must be an array of strings"))?;
        let mut matched = false;
        for value in &array.elements {
            let value = value
                .as_string_lit()
                .ok_or_else(|| error("provider policy must be an array of strings"))?;
            matched |= value.value == provider;
        }
        Ok(Some(matched))
    };
    Ok(ConfigRestrictions {
        provider_disabled: contains("disabled_providers")?.unwrap_or(false),
        provider_not_enabled: contains("enabled_providers")?.is_some_and(|matched| !matched),
    })
}

fn error(message: &'static str) -> Error {
    Error::Config(format!("OpenCode: {message}"))
}

fn parse(source: &str) -> Result<ParseResult<'_>> {
    let parsed = jsonc_parser::parse_to_ast(
        source,
        &CollectOptions::default(),
        &ParseOptions {
            allow_comments: true,
            allow_trailing_commas: true,
            allow_loose_object_property_names: false,
            allow_missing_commas: false,
            allow_single_quoted_strings: false,
            allow_hexadecimal_numbers: false,
            allow_unary_plus_numbers: false,
        },
    )
    .map_err(|_| error("invalid JSONC; no files were changed"))?;
    let value = parsed
        .value
        .as_ref()
        .ok_or_else(|| error("config must be an object"))?;
    if value.as_object().is_none() {
        return Err(error("config must be an object"));
    }
    duplicates(value)?;
    Ok(parsed)
}

fn duplicates(value: &Value<'_>) -> Result<()> {
    match value {
        Value::Object(object) => {
            let mut names = HashSet::new();
            for prop in &object.properties {
                if !names.insert(prop.name.as_str()) {
                    return Err(error("duplicate config keys are ambiguous"));
                }
                duplicates(&prop.value)?;
            }
        }
        Value::Array(array) => {
            for item in &array.elements {
                duplicates(item)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn object_at<'a, 'b>(root: &'b Object<'a>, path: &[&str]) -> Result<Option<&'b Object<'a>>> {
    let mut object = root;
    for name in path {
        match object.get(name) {
            Some(prop) => {
                object = prop
                    .value
                    .as_object()
                    .ok_or_else(|| error("target config container must be an object"))?
            }
            None => return Ok(None),
        }
    }
    Ok(Some(object))
}

fn root<'a, 'b>(parsed: &'b ParseResult<'a>) -> &'b Object<'a> {
    parsed.value.as_ref().unwrap().as_object().unwrap()
}

// Inserting directly after the opening brace never scans comment/string punctuation.
// All original bytes, including the closing brace's whitespace, remain untouched.
fn set(source: &mut String, path: &[&str], name: &str, value: &str) -> Result<()> {
    for index in 0..path.len() {
        let parsed = parse(source)?;
        let object = object_at(root(&parsed), &path[..index])?.unwrap();
        let missing = object.get(path[index]).is_none();
        if !missing {
            object_at(root(&parsed), &path[..=index])?;
        }
        drop(parsed);
        if missing {
            set(source, &path[..index], path[index], "{}")?;
        }
    }
    let parsed = parse(source)?;
    let object = object_at(root(&parsed), path)?.unwrap();
    if let Some(prop) = object.get(name) {
        let range = prop.value.range();
        // Compare decoded values too: repairs must not churn equivalent whitespace/escapes.
        let candidate =
            jsonc_parser::parse_to_ast(value, &CollectOptions::default(), &ParseOptions::default())
                .map_err(|_| error("invalid replacement"))?;
        if equivalent(&prop.value, candidate.value.as_ref().unwrap()) {
            return Ok(());
        }
        let range = range.start..range.end;
        drop(parsed);
        source.replace_range(range, value);
    } else {
        let offset = object.start() + 1;
        let comma = if object.properties.is_empty() {
            ""
        } else {
            ","
        };
        let newline = if source.contains("\r\n") {
            "\r\n"
        } else {
            "\n"
        };
        let insertion = format!(
            "{newline}{}{}:{value}{comma}{newline}",
            "  ".repeat(path.len() + 1),
            quoted(name)
        );
        drop(parsed);
        source.insert_str(offset, &insertion);
    }
    Ok(())
}

fn equivalent(a: &Value<'_>, b: &Value<'_>) -> bool {
    match (a, b) {
        (Value::StringLit(a), Value::StringLit(b)) => a.value == b.value,
        (Value::NumberLit(a), Value::NumberLit(b)) => a
            .value
            .parse::<u64>()
            .ok()
            .zip(b.value.parse::<u64>().ok())
            .is_some_and(|(a, b)| a == b),
        (Value::BooleanLit(a), Value::BooleanLit(b)) => a.value == b.value,
        (Value::NullKeyword(_), Value::NullKeyword(_)) => true,
        _ => false,
    }
}

fn quoted(value: &str) -> String {
    serde_json::to_string(value).expect("serializing a string cannot fail")
}

fn validate_binding(binding: &Binding) -> Result<()> {
    if !matches!(binding.provider.as_str(), "lobo-local" | "lobo")
        || binding.model_alias.is_empty()
        || binding.model_alias.len() > 256
        || binding.context == 0
        || binding.api_key.is_empty()
        || binding.api_key.trim() != binding.api_key
    {
        return Err(error("invalid runtime binding"));
    }
    let endpoint =
        url::Url::parse(&binding.endpoint).map_err(|_| error("invalid runtime endpoint"))?;
    if !matches!(endpoint.scheme(), "http" | "https")
        || endpoint.host_str().is_none()
        || !endpoint.username().is_empty()
        || endpoint.password().is_some()
        || endpoint.fragment().is_some()
    {
        return Err(error("invalid runtime endpoint"));
    }
    Ok(())
}

pub fn patch_config(
    source: &str,
    binding: &Binding,
    key_reference: &str,
    make_default: bool,
) -> Result<String> {
    validate_binding(binding)?;
    if !key_reference.starts_with("{file:/") || !key_reference.ends_with('}') {
        return Err(error("a private key-file reference is required"));
    }
    parse(source)?;
    let mut text = source.to_owned();
    let provider = ["provider", binding.provider.as_str()];
    set(
        &mut text,
        &provider,
        "npm",
        &quoted("@ai-sdk/openai-compatible"),
    )?;
    // Preserve an existing display name; it does not affect the runtime binding.
    if object_at(root(&parse(&text)?), &provider)?
        .unwrap()
        .get("name")
        .is_none()
    {
        set(
            &mut text,
            &provider,
            "name",
            &quoted(if binding.provider == "lobo-local" {
                "Lobo (this Mac)"
            } else {
                "Lobo"
            }),
        )?;
    }
    let options = ["provider", binding.provider.as_str(), "options"];
    set(&mut text, &options, "baseURL", &quoted(&binding.endpoint))?;
    set(&mut text, &options, "apiKey", &quoted(key_reference))?;
    let model = [
        "provider",
        binding.provider.as_str(),
        "models",
        binding.model_alias.as_str(),
    ];
    set(&mut text, &model, "name", &quoted(&binding.model_alias))?;
    let limit = [
        "provider",
        binding.provider.as_str(),
        "models",
        binding.model_alias.as_str(),
        "limit",
    ];
    set(&mut text, &limit, "context", &binding.context.to_string())?;
    set(
        &mut text,
        &limit,
        "output",
        &binding.context.min(8192).to_string(),
    )?;
    let agent_exists = object_at(root(&parse(&text)?), &["agent", "lobo"])?.is_some();
    let model_id = format!("{}/{}", binding.provider, binding.model_alias);
    if !agent_exists {
        set(
            &mut text,
            &["agent"],
            "lobo",
            &format!(
                r#"{{"mode":"primary","model":{},"tools":{{"*":false,"read":true,"edit":true,"write":true,"bash":true,"glob":true,"grep":true}},"permission":{{"read":{{"*":"allow","mcp:*":"deny"}}}}}}"#,
                quoted(&model_id)
            ),
        )?;
    } else {
        set(&mut text, &["agent", "lobo"], "model", &quoted(&model_id))?;
    }
    if make_default {
        set(&mut text, &[], "model", &quoted(&model_id))?;
        set(&mut text, &[], "default_agent", &quoted("lobo"))?;
    }
    parse(&text)?;
    Ok(text)
}

pub fn discover_config(config_home: &Path) -> Result<ConfigChoice> {
    if !config_home.is_absolute() {
        return Err(error("config home must be absolute"));
    }
    let dir = config_home.join("opencode");
    for name in ["opencode.jsonc", "opencode.json", "config.json"] {
        let path = dir.join(name);
        match fs::symlink_metadata(&path) {
            Ok(_) => return Ok(ConfigChoice { path }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err(error("cannot inspect config location")),
        }
    }
    Ok(ConfigChoice {
        path: dir.join("opencode.jsonc"),
    })
}

#[derive(PartialEq, Eq)]
struct Snapshot {
    text: String,
    fingerprint: Option<Fingerprint>,
}

#[derive(PartialEq, Eq)]
struct Fingerprint {
    device: u64,
    inode: u64,
    length: u64,
    modified: (i64, i64),
    changed: (i64, i64),
    mode: u32,
}

impl Fingerprint {
    fn of(metadata: &fs::Metadata) -> Self {
        Self {
            device: metadata.dev(),
            inode: metadata.ino(),
            length: metadata.len(),
            modified: (metadata.mtime(), metadata.mtime_nsec()),
            changed: (metadata.ctime(), metadata.ctime_nsec()),
            mode: metadata.mode(),
        }
    }
}

fn read_snapshot(path: &Path) -> Result<Snapshot> {
    let file = match OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
    {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Snapshot {
                text: "{}\n".into(),
                fingerprint: None,
            });
        }
        Err(_) => return Err(error("cannot read config; symlinks are unsupported")),
    };
    let metadata = file
        .metadata()
        .map_err(|_| error("cannot inspect config"))?;
    if !metadata.is_file() || metadata.len() > 8 * 1024 * 1024 {
        return Err(error("config must be a regular file below 8 MiB"));
    }
    let before = Fingerprint::of(&metadata);
    let mut text = String::new();
    file.take(8 * 1024 * 1024 + 1)
        .read_to_string(&mut text)
        .map_err(|_| error("cannot read UTF-8 config"))?;
    let after = fs::symlink_metadata(path).map_err(|_| error("config changed while reading"))?;
    if after.file_type().is_symlink()
        || before != Fingerprint::of(&after)
        || text.len() > 8 * 1024 * 1024
    {
        return Err(error("config changed while reading"));
    }
    Ok(Snapshot {
        text,
        fingerprint: Some(before),
    })
}

fn absolute_clean(path: &Path) -> Result<()> {
    if !path.is_absolute()
        || path
            .components()
            .any(|c| matches!(c, Component::ParentDir | Component::CurDir))
    {
        return Err(error(
            "config paths must be absolute without parent components",
        ));
    }
    Ok(())
}

fn private_dir(path: &Path) -> Result<PathBuf> {
    absolute_clean(path)?;
    // Refuse direct directory links. Existing parent links (for example macOS /var)
    // are resolved once and checked again immediately before the config replacement.
    if fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink()) {
        return Err(error("private directory symlinks are unsupported"));
    }
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(path)
        .map_err(|_| error("cannot create private directory"))?;
    let resolved = fs::canonicalize(path).map_err(|_| error("cannot resolve private directory"))?;
    let metadata =
        fs::metadata(&resolved).map_err(|_| error("cannot inspect private directory"))?;
    if !metadata.is_dir() || metadata.uid() != unsafe { libc::geteuid() } {
        return Err(error("private directory must belong to this user"));
    }
    fs::set_permissions(&resolved, fs::Permissions::from_mode(0o700))
        .map_err(|_| error("cannot secure private directory"))?;
    Ok(resolved)
}

fn reusable_key(source: &str, dir: &Path, binding: &Binding) -> Result<Option<PathBuf>> {
    let parsed = parse(source)?;
    let Some(options) = object_at(root(&parsed), &["provider", &binding.provider, "options"])?
    else {
        return Ok(None);
    };
    let Some(reference) = options.get_string("apiKey").map(|s| s.value.as_ref()) else {
        return Ok(None);
    };
    let Some(path) = reference
        .strip_prefix("{file:")
        .and_then(|s| s.strip_suffix('}'))
        .map(PathBuf::from)
    else {
        return Ok(None);
    };
    if path.parent() != Some(dir) || absolute_clean(&path).is_err() {
        return Ok(None);
    }
    let Some(filename) = path.file_name().and_then(|s| s.to_str()) else {
        return Ok(None);
    };
    let prefix = format!("{}-", binding.provider);
    let Some(id) = filename
        .strip_prefix(&prefix)
        .and_then(|s| s.strip_suffix(".key"))
    else {
        return Ok(None);
    };
    if id.len() < 6 || !id.bytes().all(|b| b.is_ascii_alphanumeric()) {
        return Ok(None);
    }
    let Ok(file) = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(&path)
    else {
        return Ok(None);
    };
    let Ok(metadata) = file.metadata() else {
        return Ok(None);
    };
    if !metadata.is_file()
        || metadata.uid() != unsafe { libc::geteuid() }
        || metadata.nlink() != 1
        || metadata.mode() & 0o777 != 0o600
        || metadata.len() != binding.api_key.len() as u64
    {
        return Ok(None);
    }
    let mut contents = Vec::new();
    if file
        .take(binding.api_key.len() as u64 + 1)
        .read_to_end(&mut contents)
        .is_err()
        || contents != binding.api_key.as_bytes()
    {
        return Ok(None);
    }
    Ok(Some(path))
}

// Only these newly allocated files belong to this transaction. Old references stay valid.
#[derive(Default)]
struct NewFiles {
    paths: Vec<PathBuf>,
    committed: bool,
}
impl Drop for NewFiles {
    fn drop(&mut self) {
        if !self.committed {
            for path in &self.paths {
                let _ = fs::remove_file(path);
            }
        }
    }
}

fn prepare(
    dir: &Path,
    prefix: &str,
    suffix: &str,
    contents: &[u8],
    files: &mut NewFiles,
) -> Result<PathBuf> {
    let name = format!(
        "{prefix}{}{suffix}",
        hex::encode(rand::random::<[u8; 16]>())
    );
    let path = dir.join(name);
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)
        .map_err(|_| error("cannot prepare private file"))?;
    files.paths.push(path.clone());
    file.write_all(contents)
        .and_then(|_| file.sync_all())
        .map_err(|_| error("cannot flush private file"))?;
    Ok(path)
}

pub fn configure(
    path: &Path,
    key_dir: &Path,
    binding: &Binding,
    make_default: bool,
    validate: &dyn Fn() -> Result<()>,
) -> Result<ConfigOutcome> {
    configure_with_replace(
        path,
        key_dir,
        binding,
        make_default,
        validate,
        &|file, destination| {
            file.persist(destination)
                .map_err(|_| error("atomic config replacement failed"))?;
            Ok(())
        },
    )
}

fn configure_with_replace(
    path: &Path,
    key_dir: &Path,
    binding: &Binding,
    make_default: bool,
    validate: &dyn Fn() -> Result<()>,
    replace: &dyn Fn(tempfile::NamedTempFile, &Path) -> Result<()>,
) -> Result<ConfigOutcome> {
    absolute_clean(path)?;
    validate_binding(binding)?;
    let parent = path
        .parent()
        .ok_or_else(|| error("config parent is required"))?;
    // Creating the standard global-config directory is allowed; no managed/project path
    // is selected here. The caller supplies the explicit chosen destination.
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(parent)
        .map_err(|_| error("cannot create config directory"))?;
    let resolved_parent =
        fs::canonicalize(parent).map_err(|_| error("cannot resolve config directory"))?;
    let destination = resolved_parent.join(
        path.file_name()
            .ok_or_else(|| error("config filename is required"))?,
    );
    let original = read_snapshot(&destination)?;
    // Fail on invalid JSONC and wrong containers before allocating a key or backup.
    patch_config(
        &original.text,
        binding,
        "{file:/preflight-only}",
        make_default,
    )?;
    let resolved_keys = private_dir(key_dir)?;
    let mut fresh = NewFiles::default();
    let key_path = match reusable_key(&original.text, &resolved_keys, binding)? {
        Some(path) => path,
        None => prepare(
            &resolved_keys,
            &format!("{}-", binding.provider),
            ".key",
            binding.api_key.as_bytes(),
            &mut fresh,
        )?,
    };
    let reference = format!(
        "{{file:{}}}",
        key_path
            .to_str()
            .ok_or_else(|| error("key path must be UTF-8"))?
    );
    let patched = patch_config(&original.text, binding, &reference, make_default)?;
    let changed = patched != original.text || original.fingerprint.is_none();
    let mut prepared = None;
    if changed {
        let mut file = tempfile::NamedTempFile::new_in(&resolved_parent)
            .map_err(|_| error("cannot prepare config replacement"))?;
        file.as_file()
            .set_permissions(fs::Permissions::from_mode(0o600))
            .map_err(|_| error("cannot secure config replacement"))?;
        file.write_all(patched.as_bytes())
            .and_then(|_| file.as_file().sync_all())
            .map_err(|_| error("cannot flush config replacement"))?;
        if original.fingerprint.is_some() {
            let prefix = format!(
                "{}.backup-{}-",
                path.file_name().unwrap().to_string_lossy(),
                chrono::Utc::now().format("%Y%m%dT%H%M%S%.9fZ")
            );
            prepare(
                &resolved_parent,
                &prefix,
                "",
                original.text.as_bytes(),
                &mut fresh,
            )?;
        }
        File::open(&resolved_keys)
            .and_then(|f| f.sync_all())
            .map_err(|_| error("cannot flush key directory"))?;
        prepared = Some(file);
    }
    // Never include a callback's possibly secret-bearing message in an app error.
    validate()
        .map_err(|_| error("runtime changed or validation failed; no config was replaced"))?;
    // A removed/rotated key must not be committed as a stale reference either.
    if reusable_key(&patched, &resolved_keys, binding)?.as_ref() != Some(&key_path) {
        return Err(error("private key changed; retry repair"));
    }
    if fs::canonicalize(parent).ok().as_ref() != Some(&resolved_parent)
        || fs::canonicalize(key_dir).ok().as_ref() != Some(&resolved_keys)
        || read_snapshot(&destination)? != original
    {
        return Err(error("config location or contents changed; retry repair"));
    }
    if let Some(file) = prepared {
        replace(file, &destination)?;
    }
    fresh.committed = true;
    // The rename has committed. A directory-sync error cannot undo it safely.
    let _ = File::open(&resolved_parent).and_then(|f| f.sync_all());
    Ok(ConfigOutcome {
        path: path.to_owned(),
        provider: binding.provider.clone(),
        model_alias: binding.model_alias.clone(),
        changed,
    })
}

#[cfg(test)]
mod tests;
