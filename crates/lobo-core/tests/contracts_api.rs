//! Compile the P3 interfaces consumed by CLI, app and E2E tests.
#![allow(clippy::type_complexity)]
use lobo_core::{Result, config as c, control as ctl, provider as p, release as r};
use lobo_proto::{Instance, Manifest, Resolved};
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
    let _: fn(&str) -> Result<u16> = c::parse_cloud_port;
    let _: fn(&Path) -> Result<BTreeMap<String, String>> = c::values;
    let _: fn(&Path, &BTreeMap<String, String>) -> Result<()> = c::save;
    let _: fn(&Path, &str, &str) -> Result<()> = c::set_env_value;
    let _: fn(&str) -> String = c::mask;
    let _: fn(&str, &str) -> String = c::masked;
    let _: fn(&Path) -> Result<lobo_proto::ConfigShow> = c::show;
    let _: (&[c::LayoutGroup], &str, &[&str], u16) =
        (c::LAYOUT, c::HEADER, c::PLAIN_KEYS, c::DEFAULT_CLOUD_PORT);
    let _: fn(&BTreeMap<String, String>) -> c::Laptop = c::Laptop::from_values;
    let _: fn(&c::Laptop) -> Result<()> = c::Laptop::require_cloud;
    let _: fn(&c::Laptop) -> Result<()> = c::Laptop::require_provider_key;
    let _: fn(&c::Laptop) -> Result<()> = c::Laptop::require_bucket;
    let _: fn(&c::Laptop) -> Result<()> = c::Laptop::require_r2;
    let _: fn(&c::Laptop) -> BTreeMap<String, String> = c::Laptop::secret_values;
    let _: fn(&c::Laptop) -> Result<c::Defaults> = c::Laptop::defaults;
    let _: fn(&c::Laptop) -> (c::Defaults, BTreeMap<String, String>) = c::defaults_partial;
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
    let _: fn(&c::Laptop, &str) -> Result<()> = ctl::check_target;
    let _: fn(&c::Laptop) -> Result<()> = ctl::check_providers;
    let _: fn(&c::Laptop) -> Result<()> = ctl::check_release;
    let _: fn(&mut ctl::UpOpts, &c::Laptop, &dyn Fn(&str) -> bool, &Path) -> Result<()> =
        ctl::apply_defaults;
    let _: fn(PathBuf, PathBuf) -> ctl::Wiring = ctl::Wiring::new;
    let _: fn(&c::Laptop, &ctl::Wiring) -> BTreeMap<String, Arc<dyn p::Provider>> =
        ctl::providers_from_config;
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
    let _: fn(&c::Laptop, &str) -> Result<String> = lobo_core::genkey::opencode_for_laptop;
    let _: fn(&Path, &c::Laptop, &str) -> Result<()> = lobo_core::genkey::write_opencode_for_laptop;
    let _: fn() -> PathBuf = lobo_core::state::state_dir;
    let _: fn(Duration) -> reqwest::Client = lobo_core::http::client;
    let _: fn(chrono::DateTime<chrono::Utc>, Duration) -> lobo_core::clock::StepClock =
        lobo_core::clock::StepClock::new;
    let cfg = c::Laptop::default();
    let wiring = ctl::Wiring::new("/unused".into(), "/unused".into());
    let deps = ctl::deps_from_config(cfg, &wiring).unwrap();
    output::<lobo_proto::Snap, _>(ctl::snapshot(&deps));
    output::<f64, _>(ctl::down(&deps));
    output::<(Arc<dyn ctl::AgentApi>, String), _>(ctl::target(&deps));
    let http = lobo_core::http::client(Duration::from_secs(1));
    output::<Resolved, _>(r::resolve(&http, "https://unused", ""));
    output::<String, _>(lobo_core::checks::read_stream(&b""[..]));
    output::<String, _>(lobo_core::checks::chat(
        &http,
        "https://unused",
        "k",
        "qwen38-q6",
    ));
    output::<Vec<u8>, _>(lobo_core::checks::tool_call(
        &http,
        "https://unused",
        "k",
        "qwen38-q6",
    ));
}
