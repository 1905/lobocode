//! Pod agent and local supervisor building blocks.
pub mod api;
pub mod config;
pub mod download;
pub mod error;
pub mod fetch;
pub mod health;
pub mod http;
pub mod logring;
pub mod metrics;
pub mod pod;
pub mod process;
pub mod runner;
pub mod selfkill;
pub mod source;
pub mod watchdog;

pub use error::{Error, Result};
pub use logring::{LogRing, LogSource};
pub use runner::{Deps, Download, GpuCheck, Killer, Llama, Metrics, Runner, RunnerConfig, Tunnel};

pub const VERSION: &str = match option_env!("LOBO_VERSION") {
    Some(v) => v,
    None => "dev",
};
pub const LLAMA_ADDR: &str = "127.0.0.1:8080";
pub const AGENT_ADDR: &str = "127.0.0.1:8081";

#[cfg(test)]
mod testutil;

#[cfg(test)]
mod docker_tests {
    /// Bootstrap execs /lobo/lobo-agent for both zip and baked-image boots.
    #[test]
    fn dockerfile_keeps_lobo_layout() {
        let docker = include_str!("../../../docker/pod/Dockerfile");
        for text in [
            "COPY --from=build /out/lobo-agent /lobo/lobo-agent",
            "COPY --from=build /out/release.json /lobo/release.json",
            "FROM ${LLAMA_IMAGE}",
            "--target x86_64-unknown-linux-musl",
        ] {
            assert!(docker.contains(text), "missing {text}");
        }
    }
}
