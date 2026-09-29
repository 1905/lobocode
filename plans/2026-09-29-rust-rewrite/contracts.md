# Rust rewrite — cross-phase contracts v1.2

**Date:** 2026-09-29
**Status:** approved for implementation after plan correction (user: full auto, 2026-09-29). Earlier review covered v1.0 only.
**Spec:** ./spec.md
**Replaces:** ./contracts-v1.1.md
**Folds in:** "Contract changes and additions" of plan-p2-v1.0, "Contract additions" of plan-p3-v1.1, plan-p4-v1.0, plan-p5-v1.1 (incl. its "Review fixes v1.1") and plan-p6-v1.1.

Public API one phase exposes and a later phase calls. Plans use these names and signatures exactly. Where a current plan still says something else, this file wins, and "Plan edits still needed" at the end names the place.

Tag `(P5)` on an item = added by that later phase, in that phase. No tag = the crate's own phase ships it.

Conventions (all crates):
- Async: tokio. Cancellation: `tokio_util::sync::CancellationToken`. Logging: `tracing`.
- Errors: each crate has `pub enum Error` (thiserror) + `pub type Result<T> = std::result::Result<T, Error>`, both re-exported at the crate root. No `anyhow` in libraries. `lobo-cli` may use `anyhow` at the top level.
- Wire types come from `lobo-proto`. No crate redeclares one. P1 wire rules apply to every serde type added later.
- Clock and HTTP are injected for tests: `Arc<dyn lobo_core::clock::Clock>`, base URLs as fields, `reqwest::Client` passed in.
- Every outgoing HTTP request carries a User-Agent: `lobo-agent/<ver>` from `lobo_agent::http::client()`, `lobo/<ver>` from `lobo_core::http::client(timeout)`. reqwest sends none by default; RunPod refuses that.
- Config is read only from the config file. No crate reads keys from the OS env.
- A function that reads `HOME`, `XDG_*` or the process env has a pure twin that takes the value as an argument (`default_path_from`, `StateFile::at`, `CleanEnv::filter`).

## lobo-proto (P1) — used by every crate and, via ts-rs, by the app UI

```rust
// Wire format = the Go build's, proven by fixtures/. Unknown JSON fields ignored. i64/u64 → TS number.
pub struct GoTime(pub Option<chrono::DateTime<chrono::FixedOffset>>);   // None = Go zero time "0001-01-01T00:00:00Z"
impl GoTime { pub const ZERO: GoTime; pub fn is_zero(&self) -> bool; pub fn from_utc(t: DateTime<Utc>) -> GoTime; }
pub enum Stage { Boot, Tunnel, Gpu, Verify, Download, Load, Ready, Failed, Terminating, #[serde(other)] Unknown }
impl Stage { pub fn as_str(&self) -> &'static str; }                     // (P2) "boot", "gpu", …; used in "<stage>: <cause>"
pub struct DownloadProgress; pub struct Gpu; pub struct Host; pub struct Llama; pub struct Timings; pub struct Status;
pub struct ModelRef; pub struct Defaults; pub struct Manifest; pub struct Resolved;
pub const DEFAULT_LLAMA_IMAGE: &str;                                     // exact line form `pub const DEFAULT_LLAMA_IMAGE: &str = "…";` (pod-image.yml seds it)
pub const DEFAULT_MODEL: &str = "q8";
pub const DEFAULT_DEFAULTS: Defaults;                                    // ctx 65536, idle_min 30, max_hours 12
pub struct Instance { /* wire fields */ #[serde(skip)] pub api_url: String, #[serde(skip)] pub agent_url: String }
pub struct ReadyInfo;
pub struct UpEvent { pub phase: String, pub detail: String, pub download: Option<DownloadProgress>,
                     pub ready: Option<ReadyInfo>, pub done: bool, pub err: Option<String> }   // one `lobo up --json` line
pub struct Snap { pub pod: Option<Instance>, pub version: Option<Manifest>, pub status: Option<Status>, pub down: bool, pub at: GoTime }
pub struct ModelState; pub struct RuntimeInfo; pub struct Listing; pub struct LocalState;
pub struct ConfigShow { pub path: String, pub exists: bool, pub values: BTreeMap<String,String>, pub set: BTreeMap<String,bool> }
pub mod catalog {
    pub const CHUNK_SIZE: i64 = 256 << 20;
    pub struct Model { pub id: String, pub file: String, pub sha256: String, pub alias: String, pub size: i64, pub chunk_sha: Vec<String> }
    pub fn all() -> &'static [Model]; pub fn get(id: &str) -> Result<&'static Model, UnknownModel>;
    pub fn min_free_mib(model_bytes: i64) -> i64;                        // (bytes >> 20) + 2560
    impl Model { pub fn url(&self, bucket_url: &str) -> String; }
}
// (P5) — control.rs and config.rs, both exported to app/ui/src/proto/
pub struct UpRequest { pub provider: Option<String>, pub model: Option<String>, pub ctx: Option<i64>,
                       pub source: Option<String>, pub cloud: Option<String> }  // None = config default (CLI "flag not set")
pub struct Readiness { pub exists: bool, pub cloud_ready: bool, pub ready: bool, pub local_supported: bool,
                       pub providers: Vec<String>, pub default_provider: String, pub default_model: String,
                       pub local_port: u16, pub error: Option<String> }
```

`UpEvent.err` stays `Option<String>`. No `err_kind` field. Terminal events of `control::up`: failure `{phase: "failed", err: Some(msg), done: true}`; cancel `{phase: "cancelled", err: Some("cancelled"), done: true}`.

## lobo-agent (P2) — used by the pod binary (P2) and the local supervisor (P3)

```rust
pub const VERSION: &str;                                   // option_env!("LOBO_VERSION") or "dev"
pub const LLAMA_ADDR: &str = "127.0.0.1:8080"; pub const AGENT_ADDR: &str = "127.0.0.1:8081";
pub use error::{Error, Result};
pub use runner::{Deps, Runner, RunnerConfig, Tunnel, GpuCheck, Download, Llama, Metrics, Killer};
pub use logring::{LogRing, LogSource};

pub mod error {
    pub enum Error { #[error("{0}")] Permanent(String), #[error("context canceled")] Cancelled,
                     #[error("{0}")] Msg(String), #[error(transparent)] Io(std::io::Error) }
    impl Error { pub fn is_permanent(&self) -> bool; pub fn msg(s: impl Into<String>) -> Self; }
}
pub mod http { pub const USER_AGENT: &str; /* "lobo-agent/<VERSION>" */ pub fn client() -> reqwest::Client; }
pub mod runner {                                           // all traits #[async_trait]
    pub trait Tunnel: Send + Sync { async fn start(&self, boot: CancellationToken, life: CancellationToken) -> Result<oneshot::Receiver<Result<()>>>; }
    pub trait GpuCheck: Send + Sync { async fn check(&self, cancel: CancellationToken) -> Result<()>; }
    pub trait Download: Send + Sync { async fn run(&self, cancel: CancellationToken, on_progress: &(dyn Fn(DownloadProgress) + Sync)) -> Result<()>; }
    pub trait Llama: Send + Sync { async fn start(&self) -> Result<oneshot::Receiver<Result<()>>>; async fn wait_healthy(&self, cancel: CancellationToken) -> Result<()>; }
    pub trait Metrics: Send + Sync { async fn llama(&self) -> Result<lobo_proto::Llama>; async fn gpu(&self) -> Result<Gpu>; async fn host(&self) -> Result<Host>; }
    pub trait Killer: Send + Sync { async fn kill_self(&self, cancel: CancellationToken) -> Result<()>; }
    pub struct Deps { pub tunnel: Arc<dyn Tunnel>, pub gpu_check: Arc<dyn GpuCheck>, pub download: Arc<dyn Download>,
                      pub llama: Arc<dyn Llama>, pub metrics: Arc<dyn Metrics>, pub killer: Arc<dyn Killer> }
    pub struct RunnerConfig { pub boot_id: String, pub timings: Timings, pub model: String, pub ctx: i64, pub idle: Duration,
        pub expires_at: DateTime<Utc>, pub boot_timeout: Duration, pub tick: Duration, pub fail_grace: Duration }
    pub struct Runner;
    impl Runner { pub fn new(d: Deps, cfg: RunnerConfig) -> Arc<Runner>; pub fn status(&self) -> lobo_proto::Status;
                  pub async fn run(self: Arc<Self>, cancel: CancellationToken) -> Result<()>; }   // Ok = killed, Err(Cancelled) = cancel
}
pub mod api {
    pub fn router(runner: Arc<Runner>, api_key: String, logs: LogSource, version: bytes::Bytes) -> axum::Router;
    // version = the exact /api/version body. Pod: raw /lobo/release.json bytes (fallback {"version":"unknown"}).
}
pub mod logring {
    pub struct LogRing; impl LogRing { pub fn new(max: usize) -> Self; pub fn write(&self, b: &[u8]); pub fn tail(&self, n: usize) -> Vec<String>; }
    impl std::io::Write for &LogRing;  pub type LogSource = Arc<LogRing>;
}
pub mod process {
    pub struct CleanEnv; impl CleanEnv { pub fn is_stripped(key: &str) -> bool;
        pub fn filter<I: IntoIterator<Item = (String, String)>>(env: I) -> Vec<(String, String)>; }
    pub fn clean_env() -> Vec<(String, String)>;           // = CleanEnv::filter(std::env::vars()); takes no argument
    pub struct LlamaArgs { pub model_path: String, pub alias: String, pub host: String, pub port: u16, pub ctx: i64 }
    impl LlamaArgs { pub fn to_args(&self) -> Vec<String>; }
    pub struct Proc { pub pid: u32, pub exited: oneshot::Receiver<Result<()>> }
    pub fn start_process(program: &Path, args: &[String], env: &[(String, String)], logs: LogSource,
                         tag: Option<&'static str>, kill: CancellationToken) -> Result<Proc>;   // always env_clear() first
    pub fn last_line(s: &str) -> String;
}
pub mod health { pub async fn wait_healthy(base: &str, poll: Duration, cancel: CancellationToken) -> Result<()>; }
pub mod fetch  { pub async fn fetch_file(url: &str, dst: &Path, mode: u32, size: i64, sha: &str) -> Result<()>; }
pub mod source {
    pub type BoxRead = Box<dyn tokio::io::AsyncRead + Send + Unpin>;
    pub trait Source: Send + Sync + std::fmt::Display { async fn open(&self, offset: i64, len: i64) -> Result<(BoxRead, i64)>; }  // Display = redacted
    pub struct HttpSource { pub url: String }
    pub struct SshSource { pub user: String, pub addr: String, pub file: String, pub size: i64,
                           pub key: Arc<russh::keys::PrivateKey>, pub host_key: russh::keys::PublicKey }
    pub fn redact_url(raw: &str) -> String; pub fn range_total(content_range: &str) -> i64;
    pub fn model_source(url: &str, ssh_key_b64: &str, host_key_line: &str, file: &str, size: i64) -> Result<Arc<dyn Source>>;
}
pub mod download {                                         // on_progress None = no reports; empty chunk_sha = no table
    pub const MAX_RESUMES: usize = 8;
    #[derive(Clone, Copy)] pub struct Tuning { pub stall: Duration, pub chunk: i64 }
    #[doc(hidden)] pub async fn with_tuning<F: Future>(t: Tuning, f: F) -> F::Output;   // tests only
    pub async fn download(cancel: CancellationToken, src: &dyn Source, dst: &Path, sha: &str,
                          on_progress: Option<&(dyn Fn(DownloadProgress) + Sync)>) -> Result<()>;   // sha mismatch → <dst>.bad
    pub async fn download_parallel(cancel: CancellationToken, src: &dyn Source, dst: &Path, size: i64, sha: &str,
                                   chunk_sha: &[String], conns: usize, on_progress: Option<&(dyn Fn(DownloadProgress) + Sync)>) -> Result<()>;
    pub async fn hash_file(cancel: CancellationToken, path: &Path, total: i64, on_progress: Option<&(dyn Fn(DownloadProgress) + Sync)>) -> Result<String>;
    pub async fn bench(cancel: CancellationToken, src: &dyn Source, size: i64, conns: usize, dur: Duration) -> Result<(i64, f64)>;
}
pub mod watchdog {
    pub struct Sample { pub at: DateTime<Utc>, pub ok: bool, pub processing: i64, pub deferred: i64, pub prompt_tokens: i64, pub gen_tokens: i64 }
    pub struct Config { pub idle: Duration, pub expires_at: DateTime<Utc> }
    pub enum Reason { Idle, Expired }
    pub enum Decision { Kill { reason: Reason }, Wait { reason: Reason, kill_in: Duration } }
    impl Decision { pub fn reason(&self) -> Reason; pub fn kill_in(&self) -> Duration; pub fn is_kill(&self) -> bool; }
    pub struct State; impl State { pub fn new(start: DateTime<Utc>) -> Self; pub fn set_ready(&mut self, at: DateTime<Utc>);
        pub fn observe(&mut self, s: Sample); pub fn idle_for(&self, now: DateTime<Utc>) -> Duration;
        pub fn failed_samples(&self) -> i64; pub fn decide(&self, now: DateTime<Utc>, cfg: &Config) -> Decision; }
}
pub mod metrics {                                          // Linux/pod only: llama /metrics, nvidia-smi, /proc. No macOS code.
    pub const SMI_ARGS: [&str; 2];
    pub fn parse_llama(text: &str) -> Result<lobo_proto::Llama>; pub fn parse_nvidia_smi(csv: &str) -> Result<Gpu>;
    pub fn parse_host(loadavg: &str, meminfo: &str) -> Result<Host>;
    pub struct Collector { pub llama_url: String, pub api_key: String, pub http: reqwest::Client, pub nvidia_smi: PathBuf, pub proc_dir: PathBuf }
    impl runner::Metrics for Collector;
}
pub mod selfkill {                                         // pod self-terminate lives here, not in lobo-core
    pub trait PodApi: Send + Sync { async fn terminate(&self) -> Result<()>; async fn gone(&self) -> Result<bool>; }
    pub struct RetryKiller { pub api: Arc<dyn PodApi> }   // impl runner::Killer
    pub struct RunPodSelf { pub pod_id: String, pub key: String, pub url: String }   // GraphQL podTerminate, pod-scoped key
    pub struct VastSelf { pub id: i64, pub key: String, pub base_url: String }        // REST DELETE, CONTAINER_API_KEY
    impl RunPodSelf { pub fn new(pod_id: &str, key: &str) -> Self; }  impl VastSelf { pub fn new(id: &str, key: &str) -> Result<Self>; }
    pub fn self_api(provider: &str, rp_id: &str, rp_key: &str, vast_id: &str, vast_key: &str) -> Option<Arc<dyn PodApi>>;
}
pub mod config {                                           // pod env config (Go config.LoadAgent)
    pub struct AgentConfig { /* plan-p2 Task 18 fields */ }
    impl AgentConfig { pub fn from_vars(get: &dyn Fn(&str) -> Option<String>) -> Result<Self>; }
}
pub mod pod { /* bin wiring (PodTunnel, PodGpuCheck, ModelDownload, main_flow, …); public for the bin only, no other crate calls it */ }
```

## lobo-core (P3) — used by lobo-cli (P4), the Tauri app (P5) and lobo-e2e (P6)

```rust
pub use error::{Error, Result};
pub mod error {
    pub enum Error { Config(String), Defaults(BTreeMap<String, String>), NoCapacity(String), NotFound, NoCredit, Rejected(String),
                     AlreadyRunning(String), Api(String), Http(reqwest::Error), Io(std::io::Error), Json(serde_json::Error),
                     Agent(lobo_agent::Error), Local(String), Release(String), Cancelled, Multi(Vec<Error>), Other(String) }
    impl Error { pub fn kind(&self) -> &'static str; pub fn is_not_found(&self) -> bool; pub fn is_no_capacity(&self) -> bool; }
    // kind(): config, no_capacity, not_found, no_credit, rejected, already_running, provider_api, network, io, json,
    //         agent, local, release, cancelled, multi, other. Stable: the app shows it.
    // Display: byte-identical to Go wherever the Go CLI printed it (require_*, apply_defaults, provider, control,
    //          ensure_api_key "read <path>: …"). P4's Go replay compares stderr bytes.
}
pub mod clock { pub trait Clock: Send + Sync { fn now(&self) -> DateTime<Utc>; }
                pub struct SystemClock; pub struct FixedClock(pub DateTime<Utc>);
                pub struct StepClock; impl StepClock { pub fn new(t0: DateTime<Utc>, step: Duration) -> Self; } }
pub mod http  { pub const USER_AGENT: &str = concat!("lobo/", env!("CARGO_PKG_VERSION")); pub fn client(timeout: Duration) -> reqwest::Client; }

pub mod config {
    pub struct R2Creds { pub account_id: String, pub access_key: String, pub secret_key: String, pub endpoint: String }
    pub struct Laptop { pub runpod_api_key: String, pub lobo_api_key: String, pub cf_tunnel_token: String, pub domain: String,
        pub bucket_url: String, pub model_source: String, pub model_ssh_key_file: String, pub model_ssh_host_key: String,
        pub min_mbps: String, pub feesh_http_url: String, pub vast_api_key: String, pub vast_max_dph: String,
        pub pod_image: String, pub provider: String, pub model: String, pub cloud: String, pub ctx: String,
        pub idle_min: String, pub max_hours: String, pub weights_dir: String, pub local_port: String, pub r2: R2Creds }
    impl Laptop { pub fn from_values(m: &BTreeMap<String, String>) -> Laptop;
        pub fn require_cloud(&self) -> Result<()>; pub fn require_provider_key(&self) -> Result<()>;
        pub fn require_bucket(&self) -> Result<()>; pub fn require_r2(&self) -> Result<()>;
        pub fn secret_values(&self) -> BTreeMap<String, String>; pub fn defaults(&self) -> Result<Defaults>;
        pub fn weights(&self) -> PathBuf; pub fn weights_with_home(&self, home: &Path) -> PathBuf; pub fn port(&self) -> u16;
        pub fn providers(&self) -> Vec<String>; pub fn default_provider(&self) -> String; }
    pub fn load_laptop(path: &Path) -> Result<Laptop>;                          // file only, never the OS env
    pub struct Defaults { pub provider: String, pub model: String, pub cloud: String, pub ctx: i64, pub idle_min: i64,
                          pub max_hours: i64, pub min_mbps: i64, pub vast_max_dph: f64 }
    pub fn defaults_partial(l: &Laptop) -> (Defaults, BTreeMap<String, String>);
    pub const DEFAULT_LOCAL_PORT: u16 = 8931;
    pub fn parse_local_port(v: &str) -> Result<u16>;                          // "" or "0" → 8931; else 1024..=65534
    pub fn default_path() -> PathBuf; pub fn default_path_from(xdg_config_home: Option<&str>, home: Option<&Path>) -> PathBuf;
    pub fn loose_mode(path: &Path) -> bool;
    pub struct LayoutGroup { pub title: &'static str, pub keys: &'static [&'static str] }
    pub const LAYOUT: &[LayoutGroup]; pub const HEADER: &str;
    pub fn save(path: &Path, set: &BTreeMap<String, String>) -> Result<()>;   // keeps comments + layout, atomic, 0600
    pub fn set_env_value(path: &Path, key: &str, value: &str) -> Result<()>;
    pub fn values(path: &Path) -> Result<BTreeMap<String, String>>;           // missing file = empty map
    pub const PLAIN_KEYS: &[&str];
    pub fn mask(s: &str) -> String;                                           // "" → "(not set)", < 12 bytes → "••••", else first4…last4
    pub fn masked(k: &str, v: &str) -> String;
    pub fn show(path: &Path) -> Result<lobo_proto::ConfigShow>;
    // (P5)
    pub fn readiness(path: &Path) -> lobo_proto::Readiness;                   // CLI rules: require_cloud, providers(), default_provider()
    pub fn validate_set(set: &BTreeMap<String, String>) -> std::result::Result<(), String>;   // SettingsView.swift texts
}

pub mod provider {
    pub const POD_NAME: &str = "lobo";
    pub struct CreateOpts { pub image: String, pub release_url: String, pub release_sha256: String, pub model_url: String,
        pub model_fallback: String, pub lobo_api_key: String, pub cf_tunnel_token: String, pub model: String, pub ctx: i64,
        pub idle_min: i64, pub dl_conns: i64, pub min_mbps: i64, pub expires_at: DateTime<Utc>, pub cloud: String,
        pub ssh_pub_key: String, pub model_ssh_key: String, pub model_host_key: String, pub boot_id: String }
    #[async_trait] pub trait Provider: Send + Sync {
        fn name(&self) -> &'static str; fn replaceable(&self) -> bool;
        async fn rent(&self, o: &CreateOpts, cancel: CancellationToken, note: &(dyn Fn(String) + Sync)) -> Result<Instance>;
        async fn list(&self) -> Result<Vec<Instance>>; async fn get(&self, id: &str) -> Result<Instance>;
        async fn delete(&self, id: &str) -> Result<()>; }
    pub fn on_domain(i: Instance, domain: &str) -> Instance;
    pub mod runpod {                                        // REST only; no GraphQL here
        pub struct RunPodTime(pub Option<DateTime<Utc>>); pub struct Machine; pub struct Pod;
        #[async_trait] pub trait RunPodApi: Send + Sync {
            async fn create(&self, o: &CreateOpts, cloud: &str, min_download_mbps: f64) -> Result<Pod>;
            async fn list(&self) -> Result<Vec<Pod>>; async fn get(&self, id: &str) -> Result<Pod>; async fn delete(&self, id: &str) -> Result<()>; }
        pub struct Client; impl Client { pub fn new(key: &str) -> Self; pub fn with_base(key: &str, base_url: &str) -> Self; }
        pub struct RunPodProvider { pub api: Arc<dyn RunPodApi>, pub domain: String }
        pub fn build_create_payload(o: &CreateOpts, cloud: &str, min_download_mbps: f64) -> serde_json::Value;
        pub const NET_TIERS: [f64; 5]; pub const GPU_TYPE: &str;
    }
    pub mod vast {
        pub struct Offer; pub struct Inst;
        pub struct Client; impl Client { pub fn new(key: &str) -> Self; pub fn with_base(key: &str, base: &str) -> Self; /* search_offers, create, list, get, destroy */ }
        pub struct VastProvider; impl VastProvider { pub fn new(client: Client, max_dph: f64, domain: &str) -> Self; }
        pub fn create_body(o: &CreateOpts) -> serde_json::Value; pub fn search_query(max_dph: f64, min_mbps: i64) -> serde_json::Value;
        pub const DISK_GB: i64 = 80;
    }
    pub mod local { pub use crate::local::provider::*; }   // LocalProvider
}

pub mod bootstrap { pub fn script(provider: &str) -> String; pub fn env(o: &CreateOpts, provider: &str) -> BTreeMap<String, String>; }

pub mod control {
    #[async_trait] pub trait AgentApi: Send + Sync { async fn status(&self) -> Result<Status>;
        async fn version(&self) -> Result<Manifest>; async fn logs(&self, n: usize) -> Result<String>; }
    #[async_trait] pub trait ReleaseResolver: Send + Sync { async fn resolve(&self, version: &str) -> Result<Resolved>; }
    #[async_trait] pub trait Presigner: Send + Sync { async fn presign_get(&self, key: &str, ttl: Duration) -> Result<String>; }
    #[derive(Debug, Clone, Default, PartialEq)]
    pub struct UpOpts { pub model: String, pub ctx: i64, pub release: String, pub idle_min: i64, pub max_life: Duration, pub timeout: Duration,
                        pub source: String, pub conns: i64, pub cloud: String, pub provider: String, pub min_mbps: i64, pub ssh_key: String, pub image: String }
    #[derive(Clone)]
    pub struct Deps { pub providers: BTreeMap<String, Arc<dyn Provider>>, pub releases: Arc<dyn ReleaseResolver>,
                      pub presign: Option<Arc<dyn Presigner>>, pub new_agent: Arc<dyn Fn(&str) -> Arc<dyn AgentApi> + Send + Sync>,
                      pub cfg: Laptop, pub clock: Arc<dyn Clock>, pub poll: Duration }
    pub const MAX_GPU_RETRIES: u32 = 4; pub const CONTAINER_TIMEOUT: Duration; /* 6 min */
    pub const STALE_SLACK: Duration; /* 15 s */ pub const POD_CHECK_EVERY: Duration; /* 30 s */
    pub struct HttpAgent; impl HttpAgent { pub fn new(base: &str, key: &str) -> Self;       // base URL, e.g. "https://lobo.example.com"
                                           pub fn on_domain(domain: &str, key: &str) -> Self; }  // = new("https://<domain>", key)

    pub fn up(d: Deps, o: UpOpts, cancel: CancellationToken) -> UpOperation;
    pub struct UpOperation { /* owns cancellation token, event receiver and worker JoinHandle */ }
    impl UpOperation {
        pub fn take_events(&mut self) -> Option<tokio::sync::mpsc::Receiver<lobo_proto::UpEvent>>;
        pub fn cancel(&self);
        pub async fn wait(&mut self) -> Result<()>; // cancellation-safe wait; retains handle on timeout
    }
    // Drop requests cancellation. CLI/controller MUST keep the operation and runtime alive through wait().
    // EOF is event transport completion only. wait() reports cleanup errors and worker panics separately.
    // Events never block cleanup: if the consumer closes, cancel and discard pending progress while joining.
    // Cancellation reaches provider retries and local runtime preparation. Check before every create/spawn.
    // Await a submitted create request; cancel never abandons its response. Delete any returned instance.
    // Cleanup has its own timeout, independent of cancellation. Any uncertainty/failure remains an error.
    // Last cancel event: {phase: "cancelled", err: Some("cancelled"), done: true} only after successful cleanup.
    pub async fn snapshot(d: &Deps) -> Result<lobo_proto::Snap>;
    pub async fn down(d: &Deps) -> Result<f64>;                                 // spent USD
    pub async fn target(d: &Deps) -> Result<(Arc<dyn AgentApi>, String)>;

    // Pre-checks and defaults: the one shape both CLI and app use.
    pub fn check_target(cfg: &Laptop, provider: &str, supported: fn() -> Result<()>) -> Result<()>;
    pub fn check_providers(cfg: &Laptop, supported: fn() -> Result<()>) -> Result<()>;
    pub fn check_release(cfg: &Laptop) -> Result<()>;
    pub fn apply_defaults(o: &mut UpOpts, cfg: &Laptop, set: &dyn Fn(&str) -> bool, cfg_path: &Path) -> Result<()>;
      // set(flag) uses Go flag names: provider, q6, cloud, ctx, idle-min, max-life, min-mbps
    // (P5) thin composition of the pieces above, app only:
    pub fn resolve_up(cfg: &Laptop, cfg_path: &Path, req: &UpRequest, supported: fn() -> Result<()>) -> Result<UpOpts>;
      // 1. UpOpts from req; unset cloud = "community" (the cobra default). 2. cloud not secure/community → the CLI's U-02 text.
      // 3. apply_defaults with set = the req field is Some (provider→"provider", model→"q6", cloud→"cloud", ctx→"ctx").
      // 4. check_target(cfg, &o.provider, supported). source copied as given. No other logic.

    pub struct Wiring { pub config_path: PathBuf, pub spawner: Spawner, pub supported: fn() -> Result<()> }
    impl Wiring { pub fn new(config_path: PathBuf, spawner: Spawner) -> Self; }   // supported = local::supported
    pub fn providers_from_config(cfg: &Laptop, w: &Wiring) -> BTreeMap<String, Arc<dyn Provider>>;
    pub fn local_provider_from_config(cfg: &Laptop, w: &Wiring) -> Option<LocalProvider>;
    pub fn deps_from_config(cfg: Laptop, w: &Wiring) -> Result<Deps>;        // the real wiring; CLI and app both call it
      // CLI: Wiring{abs(--config), Spawner::cli(current exe), App.local_supported}
      // app: Wiring::new(abs(LOBO_APP_CONFIG or default_path()), Spawner::app(current_exe()))

    #[cfg(any(test, feature = "testkit"))] pub mod testkit {
        // port of internal/control/controltest/fakes.go, names kept
        pub struct Created; pub struct FakeRunPod; /* impl RunPodApi; its Mutex'd state struct and fields (pods, no_cap, …) are pub */
        pub struct FakeReleases(pub Resolved); pub struct FakeAgent; pub struct FakeLocal;
        pub fn deps(rp: Arc<FakeRunPod>, ag: Arc<FakeAgent>, clock: Arc<dyn Clock>) -> Deps;
        pub async fn events(script: Vec<Option<Status>>, no_cap: &[&str]) -> Vec<lobo_proto::UpEvent>;   // async: drives control::up
        pub fn boot_script() -> Vec<Option<Status>>; pub fn local_boot_script() -> Vec<Option<Status>>; pub fn release() -> Resolved;
    }
}

pub mod checks {                                            // hc is an async reqwest::Client
    pub fn validate_tool_call(body: &[u8]) -> Result<()>;
    pub async fn read_stream<R: tokio::io::AsyncBufRead + Unpin>(r: R) -> Result<String>;
    pub async fn chat(hc: &reqwest::Client, base_url: &str, key: &str, model: &str) -> Result<String>;       // base_url ends in /v1
    pub async fn tool_call(hc: &reqwest::Client, base_url: &str, key: &str, model: &str) -> Result<Vec<u8>>;
}

pub mod release {
    pub fn next_version(existing: &[String], today: DateTime<Utc>) -> String;
    pub fn zip_key(v: &str) -> String; pub fn meta_key(v: &str) -> String;
    pub const LATEST_KEY: &str = "releases/latest.json"; pub const BUCKET: &str = "lobo";
    pub fn zip_url(r: &Resolved, bucket_url: &str) -> String;                 // "" when zip_key is empty (baked image)
    pub fn build_zip(agent_bin: &Path, m: &Manifest, out: &Path) -> Result<String>;   // sha256 hex
    pub fn scan_for_secrets(zip: &Path, secrets: &BTreeMap<String, String>) -> Result<()>;
    pub async fn resolve(hc: &reqwest::Client, bucket_url: &str, version: &str) -> Result<Resolved>;
    pub struct BucketReleases { pub bucket_url: String, pub hc: reqwest::Client }   // impl control::ReleaseResolver
    pub struct Store;                                                        // impl control::Presigner
    impl Store { pub fn new(r2: &R2Creds) -> Result<Self>; pub fn with_endpoint_for_test(r2: &R2Creds, hc: reqwest::Client) -> Result<Self>;
                 pub fn presign_get(&self, key: &str, ttl: Duration) -> String;          // sync; the trait impl wraps it
                 pub async fn list_release_keys(&self) -> Result<Vec<String>>;
                 pub async fn publish_version(&self, zip: &Path, r: &Resolved) -> Result<()>; // immutable zip + version JSON, never latest
                 pub async fn publish(&self, zip: &Path, r: &Resolved) -> Result<()>; // publish_version then promote latest }
}

pub mod genkey {
    pub const OPENCODE_OUT: &str = "opencode.lobo.json";
    pub fn new_api_key() -> String;                                          // "sk-" + 48 lowercase hex; the ONLY key generator (CLI wizard + app)
    pub fn ensure_api_key(config_path: &Path, rotate: bool) -> Result<(String, bool)>;   // (key, written)
    pub fn opencode_config(domain: &str, key: &str, port: u16) -> Result<String>;
    pub fn write_opencode(path: &Path, domain: &str, key: &str, port: u16) -> Result<()>;   // 0600
}

pub mod local {
    // platform — all macOS host code lives in lobo-core::local (sysctl via libc::sysctlbyname, vm_stat, ps)
    pub fn supported() -> Result<()>; pub fn usable_mib() -> Result<i64>;
    // models + runtime
    pub const HF_BASE: &str; pub fn marker_path(weights: &Path, file: &str) -> PathBuf;
    pub fn list(weights: &Path) -> Result<lobo_proto::Listing>;
    pub const RUNTIME_VERSION: &str = "b11118"; pub fn runtime_dir(weights: &Path) -> PathBuf;
    pub async fn ensure_runtime(weights: &Path, cancel: CancellationToken, note: &(dyn Fn(String) + Sync)) -> Result<PathBuf>;
    // state file (Go-compatible bytes and flock)
    pub struct StateFile { pub path: PathBuf }
    impl StateFile { pub fn default_path() -> PathBuf; pub fn path_from(xdg_state_home: Option<&str>, home: Option<&Path>) -> PathBuf;
        pub fn at(path: PathBuf) -> Self; pub fn read(&self) -> Result<Option<LocalState>>;
        pub fn claim(&self, s: &LocalState, is_supervisor: &dyn Fn(i32, &str) -> bool) -> Result<()>;
        pub fn remove_if(&self, pid: i32, boot_id: &str) -> Result<()>; }
    pub fn alive(pid: i32) -> bool;
    pub fn state_path() -> PathBuf; pub fn read_state() -> Result<Option<LocalState>>;
    pub fn claim_state(s: &LocalState) -> Result<()>; pub fn remove_state_if(pid: i32, boot_id: &str) -> Result<()>;
    // supervisor
    pub const SUPERVISOR_ARG: &str = "--lobo-local-run";
    pub struct RunConfig { pub model: String, pub ctx: i64, pub idle_min: i64, pub boot_id: String, pub port: u16, pub api_port: u16,
                           pub config_path: Option<PathBuf>, pub version: lobo_proto::Manifest }
    impl RunConfig {
        pub fn from_args(args: &[String], version: Manifest) -> Result<RunConfig>;   // args AFTER the prefix; --boot-id optional (Go parity)
        pub fn to_args(&self) -> Vec<String>;                                       // [--config p] --model … --ctx … --idle-min … --boot-id … --port … --api-port …
        pub fn validate(&self) -> Result<()>; }                                     // cmd/lobo/local.go:43-58 texts
    pub async fn supervise(cfg: RunConfig, cancel: CancellationToken) -> Result<()>;
      // uses lobo_agent::Runner + api::router. /api/version body = {"git_sha":…,"version":…} from cfg.version
      // (sorted keys, no newline; Go local.go:127). Owns its logging (stdout + LogRing); installs no global subscriber.
    // spawn + identity
    pub struct Spawner { pub exe: PathBuf, pub args_prefix: Vec<String> }
    impl Spawner { pub fn cli(exe: PathBuf) -> Self;   // prefix ["local", "run"]
                   pub fn app(exe: PathBuf) -> Self; } // prefix [SUPERVISOR_ARG, "local", "run"]
    pub fn is_supervisor(pid: i32, boot_id: &str, ps: &dyn Fn(i32) -> Result<String>) -> bool;
      // argv has "local" "run" as whole args AND "--boot-id <boot_id>". SUPERVISOR_ARG alone does NOT match.
    pub fn command_of(pid: i32) -> Result<String>;                              // ps -ww -o command= -p <pid>
    pub fn instance(s: &LocalState) -> Instance;                                // URLs from the state's ports
    pub fn log_path(state: &StateFile) -> PathBuf;
    #[async_trait] pub trait EnsureRuntime: Send + Sync { async fn ensure(&self, weights: &Path, cancel: CancellationToken, note: &(dyn Fn(String) + Sync)) -> Result<PathBuf>; }
    pub struct LocalHooks { pub supported: fn() -> Result<()>, pub ensure_runtime: Arc<dyn EnsureRuntime>, pub free_bytes: fn(&Path) -> Result<u64>,
                            pub ps: fn(i32) -> Result<String>, pub state_wait: Duration, pub stop_wait: Duration }   // impl Default
    pub struct LocalProvider { pub spawner: Spawner, pub config_path: Option<PathBuf>, pub weights: PathBuf, pub port: u16,
                               pub state: StateFile, pub hooks: LocalHooks }    // impl Provider, name "local", replaceable false
    // (P5)
    pub fn free_bytes_nearest(path: &Path) -> Option<u64>;                      // statfs of path or its nearest existing parent; "~/" expanded
}
```

## lobo-cli (P4)

Binary `lobo`. Commands, flags, output and exit codes = Go `cmd/lobo` at master HEAD when P4 starts (`6a72092` or later). No public API for other crates.
- Uses the P3 pre-check pieces directly (`check_target`, `check_providers`, `check_release`, `apply_defaults`). It does not call `resolve_up`.
- `Wiring { config_path: abs(--config), spawner: Spawner::cli(exe), supported }`. `up --provider local` sets `deps.presign = None`.
- `local run` builds `RunConfig` from clap args (`version: Manifest { version: VERSION, git_sha: COMMIT, .. }`), calls `RunConfig::validate`, then `local::supervise`. No global tracing subscriber for `local run`.
- Feature `test-fakes` (never in release builds) turns on `lobo-core/testkit` and reads `LOBO_TEST_SCENARIO`.

## app (P5)

`app/src-tauri` is its own cargo workspace (package `lobocode-app`, bin `lobocode`), path deps on `crates/lobo-core` and `crates/lobo-proto`. The root workspace and the pod Dockerfile never see it.

Supervisor re-exec: `main.rs` checks `args[1] == SUPERVISOR_ARG` and `args[2..4] == ["local", "run"]`, then `RunConfig::from_args(&args[4..], Manifest { version: app version, git_sha, .. })` → `local::supervise`. No window, no tray. Bad argv → stderr + exit 2.

Rust → TS: ts-rs types from `lobo-proto` (`app/ui/src/proto/`) and from the app crate (`app/ui/src/gen/`: `Phase, Target, Step, StepMark, PanelState, AppError`).

```rust
pub struct AppError { pub kind: String, pub message: String }   // app/src-tauri/src/types.rs
// from lobo_core::Error: kind = e.kind(), message = e.to_string(). Settings validation → kind "invalid".
// up stream failures arrive as UpEvent.err (a string) and become Phase::Failed { message }; where an AppError is
// needed for one, kind = "up". Errors before the stream (config, resolve_up, deps) keep Error::kind().
```

Tauri commands (TS calls only these):

| Command | Args → Result |
|---|---|
| `get_state` | → `PanelState` |
| `start` / `stop` / `dismiss` | → `()` |
| `choose_target` | `t: Target` |
| `set_provider` / `set_model` | `v: String` |
| `refresh` | `models: bool` |
| `copy_api_key` | → `Result<(), AppError>` (clear key never enters the webview) |
| `copy_text` | `s: String` |
| `config_show` | → `Result<ConfigShow, AppError>` |
| `config_save` | `set: BTreeMap<String,String>` → `Result<(), AppError>` (runs `validate_set`, then `config::save`) |
| `local_models` | → `Result<Listing, AppError>` |
| `catalog` | → `Vec<Model>` |
| `free_bytes` | `path: String` → `Option<u64>` (`local::free_bytes_nearest`) |
| `gen_api_key` | → `String` (`genkey::new_api_key`) |
| `choose_weights` | `start: String` → `Option<String>` |
| `open_settings` / `reveal_config` / `open_config` / `quit` | → `()` |

Event `lobo://state` → `PanelState`, after every store change and on the 1 s tick when `menu_text` changed.

Removed from the v1.0 TS surface: `snapshot`, `up` + event `lobo://up`, `cancel_up`, `down`. The Rust controller calls them through its `Backend` trait. Stop owns `UpOperation`: cancel, drain events and await `wait()`, then `down`. At 120 s show a warning and keep waiting. Keep Start disabled and retain ownership. Never report OFF or call down concurrently with an unfinished create. Cleanup failure remains FAILED until a later verified cleanup succeeds.

## lobo-e2e (P6)

No public API. `#[ignore]` live suite. Config path: `LOBO_E2E_CONFIG` (a path) or `<workspace>/.env`, read with `config::load_laptop`, never OS env keys. Calls `control::HttpAgent::new(base URL, key)`, `release::resolve`, `checks::{chat, tool_call, validate_tool_call}` with an async `reqwest::Client`.

---

## Resolved conflicts

1. `api::router` version: P2's `version: bytes::Bytes` wins over v1.0 `Manifest`. Pod = raw `release.json`. Local `supervise` keeps `RunConfig.version: Manifest` and serves `{"git_sha","version"}` bytes like Go `local.go:127`. P3 "Assumed `lobo-agent` API" + Task 46 change.
2. `resolve_up` (P5) vs P3 `check_*`/`apply_defaults`: P3's pieces are the one shape. CLI calls them directly (P4 Tasks 15–16 stand). `resolve_up` is a thin lobo-core composition for the app, with an added `supported` seam. P5 changes.
3. `is_supervisor` accepting the app argv: moot. `Spawner::app` prefix `[SUPERVISOR_ARG, "local", "run"]` already puts `local run` in the app argv. Identity stays P3's (`local run` + exact `--boot-id`, 3-arg signature); `SUPERVISOR_ARG` alone does not match. P5 Task 6 + contract addition 5 change.
4. `RunConfig::parse` (P5) vs `RunConfig::from_args(args, version)` (P3): `from_args` wins. App passes `args[4..]` and a `Manifest`. `--boot-id` stays optional (Go `local.go:77`), so P5's "missing `--boot-id` → Err" row is wrong.
5. `deps_from_config`: P3's `(cfg, &Wiring)` wins over v1.0 `(cfg)`. P5 `deps_with_spawner` is dropped (P5 v1.1 already says so).
6. `Spawner::app` prefix: P3's `[SUPERVISOR_ARG, "local", "run"]` wins over v1.0 `[SUPERVISOR_ARG]`.
7. `new_api_key`: `genkey::new_api_key` (P3) is the only one. No `config::new_api_key`. P5 changes.
8. `mask`: one function, `config::mask` from P3 Task 13 (byte cut with char-boundary fallback). P5 Task 4 only tests it.
9. macOS metrics home: P3's answer. `lobo_core::local` (platform.rs sysctl, deps.rs `vm_stat` + `host_gpu`, provider.rs `ps`). `lobo_agent::metrics` is Linux/pod only; v1.0's "macOS sysctl/ps" note is gone.
10. Self-terminate: `lobo_agent::selfkill` + `lobo_agent::config::AgentConfig` own pod self-kill and pod env config. lobo-core has no GraphQL and no `LoadAgent` (it depends on lobo-agent, not the other way).
11. `control::up` v1.2: owned `UpOperation`, cancellation through provider/local preparation, independent completion. See execution.md. P4 treats `cancelled` like `failed`/`terminated`.
12. `UpEvent.err` stays `Option<String>`. No wire change. App shows stream failures as `Phase::Failed`, kind `up` where a kind is needed.
13. Progress callbacks: free functions take `Option<&(dyn Fn(DownloadProgress) + Sync)>` (P2). The `Download` trait keeps a plain `&(dyn Fn + Sync)`.
14. `clean_env`: P2's `clean_env()` takes no argument. P3 Tasks 42/44 wrote `clean_env(base_env)`; they use `CleanEnv::filter(base_env)`.
15. `testkit::events`: P3 locked it sync; P4 awaits it. Async wins (it drives `control::up`).
16. `FakeRunPod` pods: P4 seeds pods from another crate. P3's Mutex'd state struct and its fields are `pub`.
17. lobo-core file layout: P3's directories (`config/`, `control/`, `local/`) win over P5's `config.rs` / `control.rs`.
18. App command surface: P5's list wins over v1.0 (no `up`/`lobo://up`/`cancel_up`/`snapshot`/`down` for TS).
19. `HttpAgent::new` takes a base URL (P3); `on_domain(domain, key)` for a domain. P6 already passes a base URL.
20. Async vs blocking client: `checks::*` and `release::resolve` take an async `reqwest::Client`. P6's `Env.hc` is blocking, so P6 adds an async one.
21. Crate-root re-exports: P3 writes `lobo_agent::Deps` and `lobo_agent::Error`. P2 re-exports `Deps`, `Runner`, `RunnerConfig`, the 6 traits, `Error`, `Result`, `LogSource` at the root.

---

## Alignment record

Original name alignment applied 2026-09-29 (plan-p2-v1.1, p3-v1.1, p4-v1.1, p5-v1.1, p6-v1.1, spec.md).

### P3 local test seams (implementation clarification)

`LocalHooks.ps` is `Arc<dyn Fn(i32) -> Result<String> + Send + Sync>` so identity recheck tests own their counters. `LocalHooks.child_env: Vec<(String, String)>` defaults empty and passes fixture settings directly to the child. Neither seam mutates the parent's process environment. Public Spawner fields and CLI/app argv remain unchanged.

### P3 operation ownership (implementation clarification)

`Deps` adds `operations: Arc<OperationState>`. `OperationState::memory()` isolates tests; `OperationState::persistent(path)` stores pending creates beside local state. Wiring selects `local.json`'s sibling `operation.json`. Both CLI and app use the same file and lock.

`up` and `down` hold the operation lock through create/cleanup. Persist provider, boot ID and the pre-create instance IDs before rent; add the returned ID before polling. Clear ownership only after verified cleanup or successful ready handoff. An ambiguous create remains recorded and blocks later starts, including after process restart. Reconciliation may adopt a single new instance from the before/after list, then delete it. An empty list alone does not prove an ambiguous create failed. Multiple candidates remain unresolved.

This is required by the existing v1.2 unresolved-create rule. The earlier Deps field list lacked storage and cross-process ownership. `new_agent` uses the equivalent `AgentFactory` type alias for lint clarity.
