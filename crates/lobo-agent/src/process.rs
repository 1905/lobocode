use crate::{Error, LogSource, Result};
use std::{path::Path, process::Stdio};
use tokio::{
    io::{AsyncBufReadExt, AsyncRead, BufReader},
    process::Command,
    sync::oneshot,
};
use tokio_util::sync::CancellationToken;

pub struct CleanEnv;
impl CleanEnv {
    pub fn is_stripped(key: &str) -> bool {
        key.starts_with("LLAMA_ARG_")
            || key.starts_with("LOBO_")
            || key == "LD_LIBRARY_PATH"
            || key.ends_with("_API_KEY")
            || key.ends_with("_TOKEN")
    }
    pub fn filter<I: IntoIterator<Item = (String, String)>>(env: I) -> Vec<(String, String)> {
        env.into_iter()
            .filter(|(key, _)| !Self::is_stripped(key))
            .collect()
    }
}
pub fn clean_env() -> Vec<(String, String)> {
    CleanEnv::filter(std::env::vars())
}

pub struct LlamaArgs {
    pub model_path: String,
    pub alias: String,
    pub host: String,
    pub port: u16,
    pub ctx: i64,
}
impl LlamaArgs {
    pub fn to_args(&self) -> Vec<String> {
        let port = self.port.to_string();
        let ctx = self.ctx.to_string();
        [
            "-m",
            &self.model_path,
            "--alias",
            &self.alias,
            "--host",
            &self.host,
            "--port",
            &port,
            "-ngl",
            "99",
            "-c",
            &ctx,
            "--parallel",
            "1",
            "-fa",
            "on",
            "--cache-type-k",
            "q8_0",
            "--cache-type-v",
            "q8_0",
            "--jinja",
            "--reasoning",
            "off",
            "--metrics",
            "--no-webui",
        ]
        .into_iter()
        .map(String::from)
        .collect()
    }
}
pub fn last_line(s: &str) -> String {
    let lines = s.trim().lines().collect::<Vec<_>>();
    lines
        .iter()
        .rev()
        .find(|l| l.contains("error") || l.contains("fail"))
        .or_else(|| lines.last())
        .copied()
        .unwrap_or_default()
        .trim()
        .to_owned()
}

pub struct Proc {
    pub pid: u32,
    pub exited: oneshot::Receiver<Result<()>>,
}

async fn drain(
    r: impl AsyncRead + Unpin,
    logs: LogSource,
    tag: Option<&'static str>,
) -> std::io::Result<()> {
    let mut lines = BufReader::new(r).lines();
    while let Some(line) = lines.next_line().await? {
        let line = if let Some(tag) = tag {
            format!("[{tag}] {line}")
        } else {
            line
        };
        logs.write(format!("{line}\n").as_bytes());
        if tag.is_some() {
            println!("{line}");
        }
    }
    Ok(())
}

pub fn start_process(
    program: &Path,
    args: &[String],
    env: &[(String, String)],
    logs: LogSource,
    tag: Option<&'static str>,
    kill: CancellationToken,
) -> Result<Proc> {
    if kill.is_cancelled() {
        return Err(Error::Cancelled);
    }
    let mut child = Command::new(program)
        .args(args)
        .env_clear()
        .envs(env.iter().cloned())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()?;
    let pid = child.id().expect("new child has a PID");
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let (tx, exited) = oneshot::channel();
    tokio::spawn(async move {
        let out = tokio::spawn(drain(stdout, logs.clone(), tag));
        let err = tokio::spawn(drain(stderr, logs, tag));
        let status = tokio::select! {
            status = child.wait() => status,
            () = kill.cancelled() => { let _ = child.start_kill(); child.wait().await }
        };
        // Drain before publishing completion, so the last error line is available to the runner.
        let _ = tokio::join!(out, err);
        let result = status.map_err(Error::from).and_then(|status| {
            if status.success() {
                Ok(())
            } else if let Some(code) = status.code() {
                Err(Error::msg(format!("exit status {code}")))
            } else {
                use std::os::unix::process::ExitStatusExt;
                Err(Error::msg(format!(
                    "signal: {}",
                    status.signal().unwrap_or_default()
                )))
            }
        });
        let _ = tx.send(result);
    });
    Ok(Proc { pid, exited })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::LogRing;
    use std::{sync::Arc, time::Duration};
    #[test]
    fn clean_env() {
        let keys = [
            "LLAMA_ARG_HOST",
            "LOBO_MODEL_SSH_KEY",
            "LOBO_API_KEY",
            "LLAMA_API_KEY",
            "RUNPOD_API_KEY",
            "CONTAINER_API_KEY",
            "CF_TUNNEL_TOKEN",
            "LD_LIBRARY_PATH",
            "PATH",
            "HOME",
        ];
        let input = keys.map(|k| {
            (
                k.to_owned(),
                if k == "PATH" { "/bin" } else { "/root" }.to_owned(),
            )
        });
        assert_eq!(
            CleanEnv::filter(input),
            [
                ("PATH".into(), "/bin".into()),
                ("HOME".into(), "/root".into())
            ]
        );
    }
    #[test]
    fn llama_args() {
        let m = lobo_proto::catalog::get("q8").unwrap();
        let path = format!("/models/{}", m.file);
        assert_eq!(
            LlamaArgs {
                model_path: path.clone(),
                alias: m.alias.clone(),
                host: "127.0.0.1".into(),
                port: 8080,
                ctx: 65536
            }
            .to_args(),
            [
                "-m",
                &path,
                "--alias",
                &m.alias,
                "--host",
                "127.0.0.1",
                "--port",
                "8080",
                "-ngl",
                "99",
                "-c",
                "65536",
                "--parallel",
                "1",
                "-fa",
                "on",
                "--cache-type-k",
                "q8_0",
                "--cache-type-v",
                "q8_0",
                "--jinja",
                "--reasoning",
                "off",
                "--metrics",
                "--no-webui"
            ]
        );
    }
    #[test]
    fn last_line() {
        for (s, want) in [
            (
                "Available devices:\n  ggml_metal_init: error: failed\n  done\n",
                "ggml_metal_init: error: failed",
            ),
            ("a\n b \n", "b"),
            ("one line", "one line"),
            ("", ""),
        ] {
            assert_eq!(super::last_line(s), want);
        }
    }
    #[tokio::test]
    async fn start_process() {
        let logs = Arc::new(LogRing::new(10));
        let p = super::start_process(
            Path::new("/bin/sh"),
            &["-c".into(), "echo hi; exit 3".into()],
            &[],
            logs.clone(),
            None,
            CancellationToken::new(),
        )
        .unwrap();
        assert_eq!(
            p.exited.await.unwrap().unwrap_err().to_string(),
            "exit status 3"
        );
        assert_eq!(logs.tail(1), ["hi"]);
    }
    #[tokio::test]
    async fn start_process_tags_lines() {
        let logs = Arc::new(LogRing::new(10));
        let p = super::start_process(
            Path::new("/bin/sh"),
            &["-c".into(), "echo hi".into()],
            &[],
            logs.clone(),
            Some("llama"),
            CancellationToken::new(),
        )
        .unwrap();
        p.exited.await.unwrap().unwrap();
        assert_eq!(logs.tail(1), ["[llama] hi"]);
    }
    #[tokio::test]
    async fn start_process_does_not_inherit_parent_env() {
        let logs = Arc::new(LogRing::new(100));
        let p = super::start_process(
            Path::new("/usr/bin/env"),
            &[],
            &[("A".into(), "1".into())],
            logs.clone(),
            None,
            CancellationToken::new(),
        )
        .unwrap();
        p.exited.await.unwrap().unwrap();
        assert_eq!(logs.tail(100), ["A=1"]);
    }
    #[tokio::test]
    async fn start_process_kill_token_kills() {
        let token = CancellationToken::new();
        let p = super::start_process(
            Path::new("/bin/sleep"),
            &["30".into()],
            &[],
            Arc::new(LogRing::new(10)),
            None,
            token.clone(),
        )
        .unwrap();
        token.cancel();
        let error = tokio::time::timeout(Duration::from_secs(2), p.exited)
            .await
            .unwrap()
            .unwrap()
            .unwrap_err();
        assert_eq!(error.to_string(), "signal: 9");
    }
}
