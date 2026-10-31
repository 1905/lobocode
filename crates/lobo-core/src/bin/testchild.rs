// Process fixture for cloud connection integration tests.
use std::path::PathBuf;
fn main() {
    let cloud_args: Vec<_> = std::env::args().collect();
    if cloud_args
        .get(1)
        .is_some_and(|s| s == lobo_core::connection::HELPER_ARG)
    {
        let root = PathBuf::from(&cloud_args[2]);
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .unwrap();
        let source = CloudFixture(root.join("address.json"));
        if let Err(e) = runtime.block_on(lobo_core::connection::run_with_source(root, &source)) {
            eprintln!("{e}");
            std::process::exit(1);
        }
        return;
    }
    panic!("test helper requires the cloud connection argument");
}

struct CloudFixture(PathBuf);
#[async_trait::async_trait]
impl lobo_core::connection::AddressSource for CloudFixture {
    async fn get(&self, provider: &str, id: &str) -> lobo_core::Result<lobo_proto::Instance> {
        let v: serde_json::Value = serde_json::from_slice(&std::fs::read(&self.0)?)?;
        if v["gone"] == true {
            return Err(lobo_core::Error::NotFound);
        }
        Ok(lobo_proto::Instance {
            provider: provider.into(),
            id: id.into(),
            ssh_host: v["host"].as_str().unwrap().into(),
            ssh_port: v["port"].as_u64().unwrap() as u16,
            ..Default::default()
        })
    }
}
