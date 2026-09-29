//! Compile the P3 interfaces consumed by CLI, app and E2E tests.
#![allow(clippy::type_complexity)]
use lobo_core::{Result, config as c, control as ctl, local as l, provider as p, release as r};
use lobo_proto::{Instance, Listing, LocalState, Manifest, Resolved};
use std::{
    collections::BTreeMap,
    future::Future,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use tokio_util::sync::CancellationToken;
fn output<T, F: Future<Output = Result<T>>>(_: F) {}
#[test]
fn contracts_p3_signatures() {
    let _: fn(&Path) -> Result<c::Laptop> = c::load_laptop;
    let _: fn() -> PathBuf = c::default_path;
    let _: fn(Option<&str>, Option<&Path>) -> PathBuf = c::default_path_from;
    let _: fn(&Path) -> bool = c::loose_mode;
    let _: fn(&str) -> Result<u16> = c::parse_local_port;
    let _: fn(&Path) -> Result<BTreeMap<String, String>> = c::values;
    let _: fn(&Path, &BTreeMap<String, String>) -> Result<()> = c::save;
    let _: fn(&Path, &str, &str) -> Result<()> = c::set_env_value;
    let _: fn(&str) -> String = c::mask;
    let _: fn(&str, &str) -> String = c::masked;
    let _: fn(&Path) -> Result<lobo_proto::ConfigShow> = c::show;
    let _: (&[c::LayoutGroup], &str, &[&str], u16) =
        (c::LAYOUT, c::HEADER, c::PLAIN_KEYS, c::DEFAULT_LOCAL_PORT);
    let _: fn(&BTreeMap<String, String>) -> c::Laptop = c::Laptop::from_values;
    let _: fn(&c::Laptop) -> Result<()> = c::Laptop::require_cloud;
    let _: fn(&c::Laptop) -> Result<()> = c::Laptop::require_provider_key;
    let _: fn(&c::Laptop) -> Result<()> = c::Laptop::require_bucket;
    let _: fn(&c::Laptop) -> Result<()> = c::Laptop::require_r2;
    let _: fn(&c::Laptop) -> BTreeMap<String, String> = c::Laptop::secret_values;
    let _: fn(&c::Laptop) -> Result<c::Defaults> = c::Laptop::defaults;
    let _: fn(&c::Laptop) -> (c::Defaults, BTreeMap<String, String>) = c::defaults_partial;
    let _: fn(&c::Laptop) -> PathBuf = c::Laptop::weights;
    let _: fn(&c::Laptop, &Path) -> PathBuf = c::Laptop::weights_with_home;
    let _: fn(&c::Laptop) -> u16 = c::Laptop::port;
    let _: fn(&c::Laptop) -> Vec<String> = c::Laptop::providers;
    let _: fn(&c::Laptop) -> String = c::Laptop::default_provider;
    let _: fn(Instance, &str) -> Instance = p::on_domain;
    let _: fn(&str) -> String = lobo_core::bootstrap::script;
    let _: fn(&p::CreateOpts, &str) -> BTreeMap<String, String> = lobo_core::bootstrap::env;
    let _: fn(&p::CreateOpts, &str, f64) -> serde_json::Value = p::runpod::build_create_payload;
    let _: fn(&p::CreateOpts) -> serde_json::Value = p::vast::create_body;
    let _: fn(f64, i64) -> serde_json::Value = p::vast::search_query;
    let _: fn(&str) -> p::runpod::Client = p::runpod::Client::new;
    let _: fn(&str, &str) -> p::runpod::Client = p::runpod::Client::with_base;
    let _: fn(&str) -> p::vast::Client = p::vast::Client::new;
    let _: fn(&str, &str) -> p::vast::Client = p::vast::Client::with_base;
    let _: fn(p::vast::Client, f64, &str) -> p::vast::VastProvider = p::vast::VastProvider::new;
    let _: fn(ctl::Deps, ctl::UpOpts, CancellationToken) -> ctl::UpOperation = ctl::up;
    let _: fn(&mut ctl::UpOperation) -> Option<tokio::sync::mpsc::Receiver<lobo_proto::UpEvent>> =
        ctl::UpOperation::take_events;
    let _: fn(&ctl::UpOperation) = ctl::UpOperation::cancel;
    let _: fn(&str, &str) -> ctl::HttpAgent = ctl::HttpAgent::new;
    let _: fn(&str, &str) -> ctl::HttpAgent = ctl::HttpAgent::on_domain;
    let _: fn(&c::Laptop, &str, fn() -> Result<()>) -> Result<()> = ctl::check_target;
    let _: fn(&c::Laptop, fn() -> Result<()>) -> Result<()> = ctl::check_providers;
    let _: fn(&c::Laptop) -> Result<()> = ctl::check_release;
    let _: fn(&mut ctl::UpOpts, &c::Laptop, &dyn Fn(&str) -> bool, &Path) -> Result<()> =
        ctl::apply_defaults;
    let _: fn(PathBuf, l::Spawner) -> ctl::Wiring = ctl::Wiring::new;
    let _: fn(&c::Laptop, &ctl::Wiring) -> BTreeMap<String, Arc<dyn p::Provider>> =
        ctl::providers_from_config;
    let _: fn(&c::Laptop, &ctl::Wiring) -> Option<l::LocalProvider> =
        ctl::local_provider_from_config;
    let _: fn(c::Laptop, &ctl::Wiring) -> Result<ctl::Deps> = ctl::deps_from_config;
    let _: fn(&[u8]) -> Result<()> = lobo_core::checks::validate_tool_call;
    let _: fn(&[String], chrono::DateTime<chrono::Utc>) -> String = r::next_version;
    let _: fn(&str) -> String = r::zip_key;
    let _: fn(&str) -> String = r::meta_key;
    let _: fn(&Resolved, &str) -> String = r::zip_url;
    let _: fn(&Path, &Manifest, &Path) -> Result<String> = r::build_zip;
    let _: fn(&Path, &BTreeMap<String, String>) -> Result<()> = r::scan_for_secrets;
    let _: fn(&c::R2Creds) -> Result<r::Store> = r::Store::new;
    let _: fn(&c::R2Creds, reqwest::Client) -> Result<r::Store> = r::Store::with_endpoint_for_test;
    let _: fn(&r::Store, &str, Duration) -> String = r::Store::presign_get;
    let _: fn() -> String = lobo_core::genkey::new_api_key;
    let _: fn(&Path, bool) -> Result<(String, bool)> = lobo_core::genkey::ensure_api_key;
    let _: fn(&str, &str, u16) -> Result<String> = lobo_core::genkey::opencode_config;
    let _: fn(&Path, &str, &str, u16) -> Result<()> = lobo_core::genkey::write_opencode;
    let _: fn() -> Result<()> = l::supported;
    let _: fn() -> Result<i64> = l::usable_mib;
    let _: fn(&Path, &str) -> PathBuf = l::marker_path;
    let _: fn(&Path) -> Result<Listing> = l::list;
    let _: fn(&Path) -> PathBuf = l::runtime_dir;
    let _: fn() -> PathBuf = l::state_path;
    let _: fn() -> Result<Option<LocalState>> = l::read_state;
    let _: fn(&LocalState) -> Result<()> = l::claim_state;
    let _: fn(i32, &str) -> Result<()> = l::remove_state_if;
    let _: fn(i32) -> bool = l::alive;
    let _: fn() -> PathBuf = l::StateFile::default_path;
    let _: fn(Option<&str>, Option<&Path>) -> PathBuf = l::StateFile::path_from;
    let _: fn(PathBuf) -> l::StateFile = l::StateFile::at;
    let _: fn(&l::StateFile) -> Result<Option<LocalState>> = l::StateFile::read;
    let _: fn(&l::StateFile, &LocalState, &dyn Fn(i32, &str) -> bool) -> Result<()> =
        l::StateFile::claim;
    let _: fn(&l::StateFile, i32, &str) -> Result<()> = l::StateFile::remove_if;
    let _: fn(&[String], Manifest) -> Result<l::RunConfig> = l::RunConfig::from_args;
    let _: fn(&l::RunConfig) -> Vec<String> = l::RunConfig::to_args;
    let _: fn(&l::RunConfig) -> Result<()> = l::RunConfig::validate;
    let _: fn(PathBuf) -> l::Spawner = l::Spawner::cli;
    let _: fn(PathBuf) -> l::Spawner = l::Spawner::app;
    let _: fn(i32, &str, &dyn Fn(i32) -> Result<String>) -> bool = l::is_supervisor;
    let _: fn(i32) -> Result<String> = l::command_of;
    let _: fn(&LocalState) -> Instance = l::instance;
    let _: fn(&l::StateFile) -> PathBuf = l::log_path;
    let _: Option<Arc<dyn l::EnsureRuntime>> = None;
    let _: Option<p::local::LocalProvider> = None;
    let _: fn(Duration) -> reqwest::Client = lobo_core::http::client;
    let _: fn(chrono::DateTime<chrono::Utc>, Duration) -> lobo_core::clock::StepClock =
        lobo_core::clock::StepClock::new;
    let cfg = c::Laptop::default();
    let wiring = ctl::Wiring::new("/unused".into(), l::Spawner::cli("/unused".into()));
    let deps = ctl::deps_from_config(cfg, &wiring).unwrap();
    output::<lobo_proto::Snap, _>(ctl::snapshot(&deps));
    output::<f64, _>(ctl::down(&deps));
    output::<(Arc<dyn ctl::AgentApi>, String), _>(ctl::target(&deps));
    let http = lobo_core::http::client(Duration::from_secs(1));
    output::<Resolved, _>(r::resolve(&http, "https://unused", ""));
    output::<String, _>(lobo_core::checks::read_stream(&b""[..]));
    output::<String, _>(lobo_core::checks::chat(&http, "https://unused", "k", "q6"));
    output::<Vec<u8>, _>(lobo_core::checks::tool_call(
        &http,
        "https://unused",
        "k",
        "q6",
    ));
    output::<PathBuf, _>(l::ensure_runtime(
        Path::new("/unused"),
        CancellationToken::new(),
        &|_| {},
    ));
    let run = l::RunConfig::from_args(
        &[
            "--ctx".into(),
            "512".into(),
            "--idle-min".into(),
            "1".into(),
        ],
        Manifest::default(),
    )
    .unwrap();
    output::<(), _>(l::supervise(run, CancellationToken::new()));
}
