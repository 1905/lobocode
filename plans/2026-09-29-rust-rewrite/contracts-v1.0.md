# Rust rewrite — cross-phase contracts v1.0

**Date:** 2026-09-29
**Status:** superseded by v1.1 (contracts.md)
**Spec:** ./spec.md

Public API one phase exposes and a later phase calls. Every phase plan uses these names and signatures exactly. A plan that needs more public API lists it under "Contract additions" at its end; the orchestrator folds it in here as a new version.

Conventions (all crates):
- Async: tokio. Cancellation: `tokio_util::sync::CancellationToken`. Logging: `tracing`.
- Errors: each crate has `pub enum Error` (thiserror) + `pub type Result<T> = std::result::Result<T, Error>`. No `anyhow` in libraries. `lobo-cli` may use `anyhow` at the top level.
- Wire types come from `lobo-proto` (P1). No crate redeclares one.
- Clock and HTTP are injected for tests: `Arc<dyn Clock>` (`fn now(&self) -> DateTime<Utc>`), base URLs as fields, `reqwest::Client` passed in.

## lobo-agent (P2) — used by the pod binary (P2) and the local supervisor (P3)

```rust
pub struct Deps {                                   // mirrors internal/agent/runner.go Deps
    pub tunnel:  Arc<dyn Tunnel>,                   // start(boot, life) -> Result<oneshot::Receiver<Result<()>>>
    pub gpu_check: Arc<dyn GpuCheck>,               // check(cancel) -> Result<()>
    pub download: Arc<dyn Download>,                // run(cancel, on_progress: &(dyn Fn(DownloadProgress) + Sync)) -> Result<()>
    pub llama:   Arc<dyn Llama>,                    // start() -> Result<oneshot::Receiver<Result<()>>>; wait_healthy(cancel) -> Result<()>
    pub metrics: Arc<dyn Metrics>,                  // llama(), gpu(), host() -> Result<lobo_proto::{Llama,Gpu,Host}>
    pub killer:  Arc<dyn Killer>,                   // kill_self(cancel) -> Result<()>
}
pub struct RunnerConfig { pub boot_id: String, pub timings: Timings, pub model: String, pub ctx: i64,
    pub idle: Duration, pub expires_at: DateTime<Utc>, pub boot_timeout: Duration, pub tick: Duration, pub fail_grace: Duration }
pub struct Runner;                                  // Arc-shared
impl Runner { pub fn new(d: Deps, cfg: RunnerConfig) -> Arc<Runner>;
              pub fn status(&self) -> lobo_proto::Status;
              pub async fn run(self: Arc<Self>, cancel: CancellationToken) -> Result<()>; }
pub mod api    { pub fn router(runner: Arc<Runner>, api_key: String, logs: LogSource, version: Manifest) -> axum::Router; }
pub mod source { pub trait Source: Send + Sync { async fn open(&self, offset: i64, len: i64) -> Result<(BoxRead, i64)>; }
                 pub struct HttpSource { pub url: String }  pub struct SshSource { user, addr, file, key, host_key }
                 pub fn redact_url(raw: &str) -> String; }
pub mod download { pub async fn download(cancel, src: &dyn Source, dst: &Path, sha: &str, on_progress) -> Result<()>;
                   pub async fn download_parallel(cancel, src, dst, size: i64, sha: &str, chunk_sha: &[String], conns: usize, on_progress) -> Result<()>;
                   pub async fn hash_file(cancel, path, total: i64, on_progress) -> Result<String>;
                   pub async fn bench(cancel, src, size: i64, conns: usize, dur: Duration) -> Result<(i64, f64)>; }
pub mod watchdog { pub struct State; pub struct Config; pub enum Decision; }   // port of internal/watchdog
pub mod metrics  { pub struct Collector { pub llama_url: String, .. } }        // nvidia-smi, /proc, macOS sysctl/ps
pub mod process  { pub struct CleanEnv; pub fn clean_env() -> Vec<(String,String)>;   // secret-stripping env for children
                   pub struct LlamaArgs { .. } impl LlamaArgs { pub fn to_args(&self) -> Vec<String>; } }
```

## lobo-core (P3) — used by lobo-cli (P4) and the Tauri app (P5)

```rust
pub mod config {
    pub struct Laptop { /* one field per internal/config/laptop.go key, same env names */ pub r2: R2Creds }
    pub fn default_path() -> PathBuf;                              // ~/.config/lobo/config.env
    pub fn load_laptop(path: &Path) -> Result<Laptop>;             // file only, never the OS env
    pub fn values(path: &Path) -> Result<BTreeMap<String,String>>;
    pub fn save(path: &Path, set: &BTreeMap<String,String>) -> Result<()>;   // keeps comments + layout
    pub fn show(path: &Path) -> Result<lobo_proto::ConfigShow>;
    pub fn parse_local_port(v: &str) -> Result<u16>;
    impl Laptop { pub fn require_cloud(&self) -> Result<()>; pub fn require_provider_key(&self) -> Result<()>;
                  pub fn require_bucket(&self) -> Result<()>; pub fn require_r2(&self) -> Result<()>;
                  pub fn secret_values(&self) -> BTreeMap<String,String>; pub fn defaults(&self) -> Result<Defaults>;
                  pub fn weights(&self) -> PathBuf; pub fn port(&self) -> u16;
                  pub fn providers(&self) -> Vec<String>; pub fn default_provider(&self) -> String; }
}
pub mod provider {
    pub struct CreateOpts { /* internal/provider/provider.go CreateOpts, snake_case */ }
    #[async_trait] pub trait Provider: Send + Sync {
        fn name(&self) -> &'static str; fn replaceable(&self) -> bool;
        async fn rent(&self, o: &CreateOpts, note: &(dyn Fn(String) + Sync)) -> Result<Instance>;
        async fn list(&self) -> Result<Vec<Instance>>; async fn get(&self, id: &str) -> Result<Instance>;
        async fn delete(&self, id: &str) -> Result<()>; }
    pub mod runpod; pub mod vast; pub mod local;                   // Error::NoCapacity, Error::NotFound
}
pub mod control {
    pub struct UpOpts { /* internal/control/events.go UpOpts, snake_case, Duration fields */ }
    pub struct Deps { pub providers: BTreeMap<String, Arc<dyn Provider>>, pub releases: Arc<dyn ReleaseResolver>,
                      pub presign: Option<Arc<dyn Presigner>>, pub new_agent: Arc<dyn Fn(&str) -> Arc<dyn AgentApi> + Send + Sync>,
                      pub cfg: Laptop, pub clock: Arc<dyn Clock>, pub poll: Duration }
    pub fn deps_from_config(cfg: Laptop) -> Result<Deps>;           // the real wiring; CLI and app both call it
    pub fn up(d: Deps, o: UpOpts, cancel: CancellationToken) -> tokio::sync::mpsc::Receiver<lobo_proto::UpEvent>;
    pub async fn snapshot(d: &Deps) -> Result<lobo_proto::Snap>;
    pub async fn down(d: &Deps) -> Result<f64>;                     // spent USD
    pub async fn target(d: &Deps) -> Result<(Arc<dyn AgentApi>, String)>;
    #[async_trait] pub trait AgentApi: Send + Sync { async fn status(&self) -> Result<Status>;
        async fn version(&self) -> Result<Manifest>; async fn logs(&self, n: usize) -> Result<String>; }
    pub struct HttpAgent;  impl HttpAgent { pub fn new(base: &str, key: &str) -> Self; }
}
pub mod checks  { pub async fn chat(..) -> Result<String>; pub async fn tool_call(..) -> Result<Vec<u8>>; pub fn validate_tool_call(body: &[u8]) -> Result<()>; }
pub mod release { pub fn next_version(existing: &[String], today: DateTime<Utc>) -> String; pub fn zip_key(v: &str) -> String;
                  pub fn meta_key(v: &str) -> String; pub const LATEST_KEY: &str;
                  pub fn build_zip(agent_bin: &Path, m: &Manifest, out: &Path) -> Result<String>;
                  pub fn scan_for_secrets(zip: &Path, secrets: &BTreeMap<String,String>) -> Result<()>;
                  pub struct Store; impl Store { pub fn new(r2: &R2Creds) -> Result<Self>; publish, list_release_keys, presign_get }
                  pub async fn resolve(hc: &reqwest::Client, bucket_url: &str, version: &str) -> Result<Resolved>; }
pub mod bootstrap { pub fn script(provider: &str) -> String; pub fn env(o: &CreateOpts, provider: &str) -> BTreeMap<String,String>; }
pub mod local {
    pub fn supported() -> Result<()>; pub fn usable_mib() -> Result<i64>;
    pub fn list(weights: &Path) -> Result<lobo_proto::Listing>;
    pub async fn ensure_runtime(weights: &Path, note: &(dyn Fn(String)+Sync)) -> Result<PathBuf>;
    pub fn state_path() -> PathBuf; pub fn read_state() -> Result<Option<LocalState>>;
    pub fn claim_state(s: &LocalState) -> Result<()>; pub fn remove_state_if(pid: i32, boot_id: &str) -> Result<()>;
    pub struct RunConfig { .. }                                    // = today's `lobo local run` flags
    pub async fn supervise(cfg: RunConfig, cancel: CancellationToken) -> Result<()>;   // uses lobo_agent::Runner
    pub const SUPERVISOR_ARG: &str = "--lobo-local-run";           // app re-exec marker (P5)
    pub struct Spawner { pub exe: PathBuf, pub args_prefix: Vec<String> } // how local::Provider starts the supervisor:
                                                                   // CLI = (lobo, ["local","run"]); app = (self, [SUPERVISOR_ARG])
}
pub mod genkey { pub fn opencode_config(..) -> Result<String>; }
pub mod error  { impl Error { pub fn kind(&self) -> &'static str; } }   // app maps to {kind, message}
```

## lobo-cli (P4)

Binary `lobo`. Commands, flags, output and exit codes = Go `cmd/lobo` at `5443667`. No public API.

## app (P5)

Tauri commands (Rust → TS via ts-rs types from lobo-proto + `AppError { kind, message }`):
`config_show() -> ConfigShow`, `config_save(set: BTreeMap<String,String>)`, `snapshot() -> Snap`, `up(opts: UpRequest)` (streams `UpEvent` on event `lobo://up`), `cancel_up()`, `down() -> f64`, `local_models() -> Listing`, `catalog() -> Vec<Model>`.
`UpRequest { provider, model, ctx, source, cloud }` lives in `lobo-proto` (added by P5 via "Contract additions").
