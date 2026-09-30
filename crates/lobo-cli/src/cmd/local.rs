use crate::{build_info, cli::LocalRunArgs};
use lobo_core::local::RunConfig;
use lobo_proto::Manifest;
use std::path::Path;
impl LocalRunArgs {
    pub fn to_run_config(&self, path: &Path) -> anyhow::Result<RunConfig> {
        // Shared parser validates in Go order before narrowing the port integers.
        let mut args = Vec::new();
        for (flag, value) in [
            ("--model", self.model.clone()),
            ("--ctx", self.ctx.to_string()),
            ("--idle-min", self.idle_min.to_string()),
            ("--boot-id", self.boot_id.clone()),
            ("--port", self.port.to_string()),
            ("--api-port", self.api_port.to_string()),
        ] {
            args.extend([flag.to_owned(), value]);
        }
        let mut cfg = RunConfig::from_args(
            &args,
            Manifest {
                version: build_info::VERSION.into(),
                git_sha: build_info::COMMIT.into(),
                ..Default::default()
            },
        )?;
        cfg.config_path = Some(path.into());
        Ok(cfg)
    }
}
