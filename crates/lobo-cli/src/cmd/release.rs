use crate::{
    app::{App, Io, load_cfg},
    cli::ReleaseArgs,
};
use anyhow::Context;
use chrono::{DateTime, Timelike, Utc};
use lobo_core::{
    control,
    release::{self, Manifest, ModelRef, Resolved, Store},
};
use std::{
    io::Write,
    path::Path,
    process::{Command, Stdio},
};

fn git_output(dir: &Path, args: &[&str], label: &str) -> anyhow::Result<String> {
    let output = Command::new("git")
        .current_dir(dir)
        .args(args)
        .output()
        .with_context(|| label.to_owned())?;
    if !output.status.success() {
        anyhow::bail!(
            "{label}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8(output.stdout)?.trim().into())
}
pub fn git_info(dir: &Path) -> anyhow::Result<(String, bool)> {
    let sha = git_output(dir, &["rev-parse", "--short", "HEAD"], "git rev-parse")?;
    let dirty = !git_output(dir, &["status", "--porcelain"], "git status")?.is_empty();
    Ok((sha, dirty))
}
pub fn whoami() -> String {
    let get = |program: &str, args: &[&str]| {
        Command::new(program)
            .args(args)
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
            .unwrap_or_default()
    };
    let user = get("id", &["-un"]);
    let host = get("hostname", &[]);
    if user.is_empty() {
        host
    } else {
        format!("{user}@{host}")
    }
}
pub fn agent_build_command(top: &Path, version: &str) -> Command {
    let mut command = Command::new("cargo");
    command
        .current_dir(top)
        .args([
            "zigbuild",
            "--release",
            "--locked",
            "-p",
            "lobo-agent",
            "--target",
            "x86_64-unknown-linux-musl",
            "--target-dir",
        ])
        .arg(top.join("target"))
        .env("LOBO_VERSION", version)
        .stdout(Stdio::from(std::io::stderr()))
        .stderr(Stdio::inherit());
    command
}
pub fn release_manifest(
    version: &str,
    sha: &str,
    dirty: bool,
    at: DateTime<Utc>,
    by: &str,
) -> Manifest {
    let model =
        lobo_proto::catalog::get(release::DEFAULT_MODEL).expect("default model belongs to catalog");
    Manifest {
        version: version.into(),
        git_sha: sha.into(),
        git_dirty: dirty,
        built_at: lobo_proto::GoTime::from_utc(at.with_nanosecond(0).unwrap()),
        built_by: by.into(),
        llama_image: release::DEFAULT_LLAMA_IMAGE.into(),
        model: ModelRef {
            id: model.id.clone(),
            file: model.file.clone(),
            sha256: model.sha256.clone(),
        },
        defaults: release::DEFAULT_DEFAULTS,
    }
}
pub async fn run(app: &App, args: &ReleaseArgs, path: &Path, io: &mut Io) -> anyhow::Result<()> {
    let cfg = load_cfg(path)?;
    control::check_release(&cfg)?;
    let store = Store::new(&cfg.r2)?;
    let keys = tokio::select! {
        biased;
        _ = app.cancel.cancelled() => return Err(lobo_core::Error::Cancelled.into()),
        keys = store.list_release_keys() => keys?,
    };
    let version = release::next_version(&keys, app.clock.now());
    let cwd = std::env::current_dir()?;
    let (sha, dirty) = git_info(&cwd)?;
    let top = git_output(&cwd, &["rev-parse", "--show-toplevel"], "git rev-parse")?;
    let top = Path::new(&top);
    if app.cancel.is_cancelled() {
        return Err(lobo_core::Error::Cancelled.into());
    }
    let mut build = tokio::process::Command::from(agent_build_command(top, &version));
    // Await the build child. Ctrl-C also reaches cargo through its process group.
    let status = build.status().await.context("build agent")?;
    if !status.success() {
        anyhow::bail!("build agent: {status}");
    }
    if app.cancel.is_cancelled() {
        return Err(lobo_core::Error::Cancelled.into());
    }
    let manifest = release_manifest(&version, &sha, dirty, app.clock.now(), &whoami());
    let tmp = tempfile::Builder::new().prefix("lobo-release-").tempdir()?;
    let zip = tmp.path().join(format!("lobo-{version}.zip"));
    let hash = release::build_zip(
        &top.join("target/x86_64-unknown-linux-musl/release/lobo-agent"),
        &manifest,
        &zip,
    )?;
    release::scan_for_secrets(&zip, &cfg.secret_values())?;
    if app.cancel.is_cancelled() {
        return Err(lobo_core::Error::Cancelled.into());
    }
    let resolved = Resolved {
        manifest,
        zip_key: release::zip_key(&version),
        zip_sha256: hash,
    };
    // Once publication starts, await its result. Dropping a PUT future cannot
    // establish whether the object was written. Candidate mode never moves latest.
    if args.no_promote {
        store.publish_version(&zip, &resolved).await?;
    } else {
        store.publish(&zip, &resolved).await?;
    }
    tracing::info!(
        version = version.as_str(),
        git_sha = sha.as_str(),
        dirty,
        zip = release::zip_url(&resolved, &cfg.bucket_url).as_str(),
        "released"
    );
    if dirty {
        tracing::warn!("working tree is dirty: /api/version will say git_dirty=true");
    }
    writeln!(io.out, "{version}")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn git_info_clean_dirty_and_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let git = |args: &[&str]| {
            let out = Command::new("git")
                .current_dir(tmp.path())
                .args(args)
                .output()
                .unwrap();
            assert!(
                out.status.success(),
                "{}",
                String::from_utf8_lossy(&out.stderr)
            );
        };
        assert!(
            git_info(tmp.path())
                .unwrap_err()
                .to_string()
                .starts_with("git rev-parse:")
        );
        git(&["init", "-q"]);
        std::fs::write(tmp.path().join("a"), "a").unwrap();
        git(&["add", "a"]);
        git(&[
            "-c",
            "user.name=fixture",
            "-c",
            "user.email=fixture@example.test",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-qm",
            "fixture",
        ]);
        let (sha, dirty) = git_info(tmp.path()).unwrap();
        assert_eq!(sha.len(), 7);
        assert!(!dirty);
        std::fs::write(tmp.path().join("b"), "b").unwrap();
        assert!(git_info(tmp.path()).unwrap().1);
    }
    #[test]
    fn build_command_and_manifest() {
        let command = agent_build_command(Path::new("/repo"), "2026.09.30-1");
        assert_eq!(command.get_program(), "cargo");
        assert_eq!(command.get_current_dir(), Some(Path::new("/repo")));
        assert_eq!(
            command.get_args().collect::<Vec<_>>(),
            [
                "zigbuild",
                "--release",
                "--locked",
                "-p",
                "lobo-agent",
                "--target",
                "x86_64-unknown-linux-musl",
                "--target-dir",
                "/repo/target"
            ]
        );
        assert_eq!(
            command.get_envs().collect::<Vec<_>>(),
            [(
                std::ffi::OsStr::new("LOBO_VERSION"),
                Some(std::ffi::OsStr::new("2026.09.30-1"))
            )]
        );
        let manifest = release_manifest(
            "v",
            "abc1234",
            true,
            "2026-09-30T10:11:12.123456Z".parse().unwrap(),
            "user@host",
        );
        assert_eq!(manifest.built_at.0.unwrap().nanosecond(), 0);
        assert_eq!(manifest.llama_image, release::DEFAULT_LLAMA_IMAGE);
        assert_eq!(manifest.model.id, release::DEFAULT_MODEL);
        assert_eq!(manifest.defaults, release::DEFAULT_DEFAULTS);
        assert!(manifest.git_dirty);
    }
}
