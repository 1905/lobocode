use super::state::StateFile;
use crate::{Error, Result};
use lobo_proto::{Instance, LocalState};
use std::{path::PathBuf, process::Command};

#[derive(Debug, Clone)]
pub struct Spawner {
    pub exe: PathBuf,
    pub args_prefix: Vec<String>,
}
impl Spawner {
    pub fn cli(exe: PathBuf) -> Self {
        Self {
            exe,
            args_prefix: vec!["local".into(), "run".into()],
        }
    }
    pub fn app(exe: PathBuf) -> Self {
        Self {
            exe,
            args_prefix: vec![
                super::supervise::SUPERVISOR_ARG.into(),
                "local".into(),
                "run".into(),
            ],
        }
    }
}

pub fn command_of(pid: i32) -> Result<String> {
    let out = Command::new("ps")
        .args(["-ww", "-o", "command=", "-p", &pid.to_string()])
        .output()?;
    if !out.status.success() {
        return Err(Error::Local(format!("ps {pid}: {}", out.status)));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
}
pub fn is_supervisor(pid: i32, boot_id: &str, ps: &dyn Fn(i32) -> Result<String>) -> bool {
    if pid <= 0 || boot_id.is_empty() {
        return false;
    }
    let Ok(command) = ps(pid) else { return false };
    let fields: Vec<_> = command.split_whitespace().collect();
    fields.windows(2).any(|f| f == ["local", "run"])
        && fields.windows(2).any(|f| f == ["--boot-id", boot_id])
}
pub fn instance(s: &LocalState) -> Instance {
    Instance {
        provider: "local".into(),
        id: s.pid.to_string(),
        status: "running".into(),
        started_at: s.started_at.clone(),
        detail: format!("this Mac, {}", s.model),
        api_url: format!("http://127.0.0.1:{}/v1", s.port),
        agent_url: format!("http://127.0.0.1:{}", s.api_port),
        ..Default::default()
    }
}
pub fn log_path(state: &StateFile) -> PathBuf {
    state.path.with_file_name("local.log")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn is_supervisor_table() {
        for (command, boot, want) in [
            ("lobo local run --boot-id b1", "b1", true),
            (
                "/Applications/lobocode.app/Contents/MacOS/lobocode --lobo-supervisor local run --boot-id b1",
                "b1",
                true,
            ),
            ("lobo local run --boot-id b11", "b1", false),
            ("lobo local run --boot-id b1x", "b1", false),
            ("lobo local run --boot-id", "b1", false),
            ("lobo local run --boot-id=b1", "b1", false),
            ("lobo local status --boot-id b1", "b1", false),
            ("vim notes.txt", "b1", false),
            ("lobo local run --boot-id b1", "", false),
        ] {
            assert_eq!(
                is_supervisor(1, boot, &|_| Ok(command.into())),
                want,
                "{command}"
            );
        }
        assert!(!is_supervisor(0, "b1", &|_| panic!(
            "invalid pid must not run ps"
        )));
        assert!(!is_supervisor(1, "b1", &|_| Err(Error::NotFound)));
    }
    #[test]
    fn instance_urls_from_state_ports() {
        let i = instance(&LocalState {
            pid: 123,
            port: 8931,
            api_port: 8932,
            model: "q6".into(),
            ..Default::default()
        });
        assert_eq!(i.api_url, "http://127.0.0.1:8931/v1");
        assert_eq!(i.agent_url, "http://127.0.0.1:8932");
        assert_eq!(i.id, "123");
        assert_eq!(i.detail, "this Mac, q6");
    }
}
