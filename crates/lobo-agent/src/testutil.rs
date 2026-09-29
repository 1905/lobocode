use axum::{
    Router,
    body::Body,
    extract::State,
    http::{HeaderMap, Response, StatusCode},
    routing::get,
};
use bytes::Bytes;
use futures_util::stream;
use sha2::{Digest, Sha256};
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

#[derive(Clone, Copy)]
pub enum Fault {
    Normal,
    HalfThenClose,
    Silent,
    StallAfter(usize),
}
struct Data {
    bytes: Arc<Vec<u8>>,
    fault: Fault,
    calls: AtomicUsize,
    ranges: Mutex<Vec<String>>,
}
pub struct RangeServer {
    pub url: String,
    data: Arc<Data>,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for RangeServer {
    fn drop(&mut self) {
        self.task.abort();
    }
}
impl RangeServer {
    pub async fn new(bytes: Arc<Vec<u8>>, fault: Fault) -> Self {
        let data = Arc::new(Data {
            bytes,
            fault,
            calls: AtomicUsize::new(0),
            ranges: Mutex::new(Vec::new()),
        });
        let app = Router::new()
            .route("/file", get(serve))
            .with_state(data.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/file", listener.local_addr().unwrap());
        let task = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        Self { url, data, task }
    }
    pub fn calls(&self) -> usize {
        self.data.calls.load(Ordering::Relaxed)
    }
    pub fn ranges(&self) -> Vec<String> {
        self.data.ranges.lock().unwrap().clone()
    }
}
async fn serve(State(data): State<Arc<Data>>, headers: HeaderMap) -> Response<Body> {
    let call = data.calls.fetch_add(1, Ordering::Relaxed);
    let range = headers
        .get("range")
        .and_then(|s| s.to_str().ok())
        .unwrap_or_default();
    data.ranges.lock().unwrap().push(range.into());
    let fault = if call == 0 { data.fault } else { Fault::Normal };
    if matches!(fault, Fault::Silent) {
        return std::future::pending().await;
    }
    let total = data.bytes.len();
    let mut start = 0;
    let mut end = total;
    if let Some((a, b)) = range.strip_prefix("bytes=").and_then(|s| s.split_once('-')) {
        start = a.parse().unwrap();
        end = b
            .parse::<usize>()
            .map(|x| x + 1)
            .unwrap_or(total)
            .min(total);
    }
    if start > end {
        return Response::builder().status(416).body(Body::empty()).unwrap();
    }
    let bytes = Bytes::copy_from_slice(&data.bytes[start..end]);
    let body = match fault {
        Fault::Normal => Body::from(bytes),
        Fault::HalfThenClose | Fault::StallAfter(_) => {
            let n = match fault {
                Fault::StallAfter(n) => n.min(bytes.len()),
                _ => bytes.len() / 2,
            };
            let first = bytes.slice(..n);
            Body::from_stream(stream::unfold(
                (0, first),
                move |(step, first)| async move {
                    match step {
                        0 => Some((Ok::<_, std::io::Error>(first.clone()), (1, first))),
                        1 if matches!(fault, Fault::StallAfter(_)) => std::future::pending().await,
                        1 => {
                            tokio::time::sleep(Duration::from_millis(10)).await;
                            Some((
                                Err(std::io::Error::other("test connection drop")),
                                (2, first),
                            ))
                        }
                        _ => None,
                    }
                },
            ))
        }
        Fault::Silent => unreachable!(),
    };
    let mut response = Response::builder()
        .status(if range.is_empty() {
            StatusCode::OK
        } else {
            StatusCode::PARTIAL_CONTENT
        })
        .header("Content-Length", end - start);
    if !range.is_empty() {
        response = response.header(
            "Content-Range",
            format!("bytes {start}-{}/{total}", end.saturating_sub(1)),
        );
    }
    response.body(body).unwrap()
}
pub fn data(n: usize) -> Arc<Vec<u8>> {
    Arc::new((0..n).map(|i| ((i * 137 + i / 257) % 251) as u8).collect())
}
pub fn chunk_table(data: &[u8], chunk: i64) -> Vec<String> {
    data.chunks(chunk as usize)
        .map(|b| hex::encode(Sha256::digest(b)))
        .collect()
}
pub async fn within<T>(d: Duration, f: impl std::future::Future<Output = T>) -> T {
    tokio::time::timeout(d, f).await.expect("hung")
}

#[tokio::test]
async fn range_server_serves_ranges() {
    let bytes = data(100);
    let server = RangeServer::new(bytes.clone(), Fault::Normal).await;
    let r = crate::http::client()
        .get(&server.url)
        .header("Range", "bytes=5-9")
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 206);
    assert_eq!(r.headers()["Content-Range"], "bytes 5-9/100");
    assert_eq!(r.bytes().await.unwrap().as_ref(), &bytes[5..10]);
}
#[tokio::test]
async fn range_server_half_then_close() {
    let server = RangeServer::new(data(100), Fault::HalfThenClose).await;
    assert!(
        crate::http::client()
            .get(&server.url)
            .send()
            .await
            .unwrap()
            .bytes()
            .await
            .is_err()
    );
}

// Each test's Tokio runtime owns the listener and connection tasks.
#[derive(Clone)]
struct Feesh {
    data: Arc<Vec<u8>>,
    client_pub: russh::keys::PublicKey,
    calls: Arc<AtomicUsize>,
}
impl russh::server::Server for Feesh {
    type Handler = Self;
    fn new_client(&mut self, _: Option<std::net::SocketAddr>) -> Self {
        self.clone()
    }
}
impl russh::server::Handler for Feesh {
    type Error = russh::Error;
    async fn auth_publickey(
        &mut self,
        _: &str,
        key: &russh::keys::PublicKey,
    ) -> std::result::Result<russh::server::Auth, Self::Error> {
        Ok(if key.key_data() == self.client_pub.key_data() {
            russh::server::Auth::Accept
        } else {
            russh::server::Auth::Reject {
                proceed_with_methods: None,
                partial_success: false,
            }
        })
    }
    async fn channel_open_session(
        &mut self,
        _: russh::Channel<russh::server::Msg>,
        reply: russh::server::ChannelOpenHandle,
        _: &mut russh::server::Session,
    ) -> std::result::Result<(), Self::Error> {
        reply.accept().await;
        Ok(())
    }
    async fn exec_request(
        &mut self,
        channel: russh::ChannelId,
        command: &[u8],
        session: &mut russh::server::Session,
    ) -> std::result::Result<(), Self::Error> {
        let text = std::str::from_utf8(command).unwrap();
        let args: Vec<_> = text.split_whitespace().collect();
        let start: usize = args[1].parse().unwrap();
        let end = args
            .get(2)
            .map(|v| start + v.parse::<usize>().unwrap())
            .unwrap_or(self.data.len())
            .min(self.data.len());
        let first = self.calls.fetch_add(1, Ordering::SeqCst) == 0;
        let end = if first {
            start + (end - start) / 2
        } else {
            end
        };
        session.channel_success(channel)?;
        let handle = session.handle();
        let data = self.data.clone();
        tokio::spawn(async move {
            // Keep large output below the peer's window and let russh apply backpressure.
            for chunk in data[start..end].chunks(16 * 1024) {
                if handle.data(channel, chunk.to_vec()).await.is_err() {
                    return;
                }
            }
            if first {
                tokio::time::sleep(Duration::from_millis(20)).await;
                let _ = handle
                    .disconnect(
                        russh::Disconnect::ByApplication,
                        "test drop".into(),
                        "en".into(),
                    )
                    .await;
            } else {
                let _ = handle.exit_status_request(channel, 0).await;
                let _ = handle.eof(channel).await;
                let _ = handle.close(channel).await;
            }
        });
        Ok(())
    }
}
pub fn ssh_key() -> Arc<russh::keys::PrivateKey> {
    Arc::new(
        russh::keys::PrivateKey::random(&mut rand::rng(), russh::keys::Algorithm::Ed25519).unwrap(),
    )
}
pub async fn fake_feesh(
    data: Arc<Vec<u8>>,
    client_pub: russh::keys::PublicKey,
) -> (String, russh::keys::PublicKey, Arc<AtomicUsize>) {
    use russh::server::Server;
    let ed = ssh_key();
    let ec = russh::keys::PrivateKey::random(
        &mut rand::rng(),
        russh::keys::Algorithm::Ecdsa {
            curve: russh::keys::EcdsaCurve::NistP256,
        },
    )
    .unwrap();
    let config = russh::server::Config {
        keys: vec![ec, (*ed).clone()],
        auth_rejection_time: Duration::ZERO,
        auth_rejection_time_initial: Some(Duration::ZERO),
        ..Default::default()
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap().to_string();
    let calls = Arc::new(AtomicUsize::new(0));
    let mut server = Feesh {
        data,
        client_pub,
        calls: calls.clone(),
    };
    tokio::spawn(async move {
        let _ = server.run_on_socket(Arc::new(config), &listener).await;
    });
    (addr, ed.public_key().clone(), calls)
}
pub async fn silent_tcp() -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap().to_string();
    tokio::spawn(async move {
        let mut sockets = Vec::new();
        loop {
            sockets.push(listener.accept().await.unwrap().0);
        }
    });
    addr
}

use crate::{Error, Result, runner::*};
use async_trait::async_trait;
use futures_util::future::BoxFuture;
use tokio_util::sync::CancellationToken;
type AsyncHook<T> = Box<dyn Fn(CancellationToken) -> BoxFuture<'static, Result<T>> + Send + Sync>;
pub struct FakeTunnel {
    pub start: Box<
        dyn Fn(CancellationToken, CancellationToken) -> BoxFuture<'static, Result<Exit>>
            + Send
            + Sync,
    >,
}
#[async_trait]
impl Tunnel for FakeTunnel {
    async fn start(&self, b: CancellationToken, l: CancellationToken) -> Result<Exit> {
        (self.start)(b, l).await
    }
}
pub struct FakeGpu {
    pub check: AsyncHook<()>,
}
#[async_trait]
impl GpuCheck for FakeGpu {
    async fn check(&self, c: CancellationToken) -> Result<()> {
        (self.check)(c).await
    }
}
type DownloadHook = Box<
    dyn for<'a> Fn(
            CancellationToken,
            &'a (dyn Fn(lobo_proto::DownloadProgress) + Sync),
        ) -> BoxFuture<'a, Result<()>>
        + Send
        + Sync,
>;
pub struct FakeDownload {
    pub run: DownloadHook,
}
#[async_trait]
impl Download for FakeDownload {
    async fn run(
        &self,
        c: CancellationToken,
        p: &(dyn Fn(lobo_proto::DownloadProgress) + Sync),
    ) -> Result<()> {
        (self.run)(c, p).await
    }
}
pub struct FakeLlama {
    pub start: Box<dyn Fn() -> BoxFuture<'static, Result<Exit>> + Send + Sync>,
    pub healthy: AsyncHook<()>,
}
#[async_trait]
impl Llama for FakeLlama {
    async fn start(&self) -> Result<Exit> {
        (self.start)().await
    }
    async fn wait_healthy(&self, c: CancellationToken) -> Result<()> {
        (self.healthy)(c).await
    }
}
pub struct FakeMetrics {
    pub llama: Box<dyn Fn() -> Result<lobo_proto::Llama> + Send + Sync>,
}
#[async_trait]
impl Metrics for FakeMetrics {
    async fn llama(&self) -> Result<lobo_proto::Llama> {
        (self.llama)()
    }
    async fn gpu(&self) -> Result<lobo_proto::Gpu> {
        Ok(lobo_proto::Gpu {
            name: "RTX 5090".into(),
            ..Default::default()
        })
    }
    async fn host(&self) -> Result<lobo_proto::Host> {
        Err(Error::msg("no proc"))
    }
}
#[derive(Default)]
pub struct FakeKiller {
    pub calls: AtomicUsize,
}
#[async_trait]
impl Killer for FakeKiller {
    async fn kill_self(&self, _: CancellationToken) -> Result<()> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}
fn held_exit() -> impl Fn() -> BoxFuture<'static, Result<Exit>> + Send + Sync {
    let senders = Mutex::new(Vec::new());
    move || {
        let (tx, rx) = tokio::sync::oneshot::channel();
        senders.lock().unwrap().push(tx);
        Box::pin(async { Ok(rx) })
    }
}
pub fn fake_deps(killer: Arc<FakeKiller>) -> Deps {
    let tunnel = held_exit();
    Deps {
        tunnel: Arc::new(FakeTunnel {
            start: Box::new(move |_, _| tunnel()),
        }),
        gpu_check: Arc::new(FakeGpu {
            check: Box::new(|_| Box::pin(async { Ok(()) })),
        }),
        download: Arc::new(FakeDownload {
            run: Box::new(|_, p| {
                Box::pin(async move {
                    p(lobo_proto::DownloadProgress {
                        bytes: 5,
                        total: 10,
                        ..Default::default()
                    });
                    Ok(())
                })
            }),
        }),
        llama: Arc::new(FakeLlama {
            start: Box::new(held_exit()),
            healthy: Box::new(|_| Box::pin(async { Ok(()) })),
        }),
        metrics: Arc::new(FakeMetrics {
            llama: Box::new(|| Ok(Default::default())),
        }),
        killer,
    }
}
pub fn cfg() -> RunnerConfig {
    RunnerConfig {
        boot_id: "boot".into(),
        timings: Default::default(),
        model: "q8".into(),
        ctx: 8192,
        idle: Duration::from_secs(3600),
        expires_at: chrono::Utc::now() + chrono::TimeDelta::hours(1),
        boot_timeout: Duration::from_secs(60),
        tick: Duration::from_millis(5),
        fail_grace: Duration::from_millis(20),
    }
}
pub async fn wait_stage(r: &Runner, stage: lobo_proto::Stage) -> lobo_proto::Status {
    within(Duration::from_secs(2), async {
        loop {
            let s = r.status();
            if s.stage == stage {
                return s;
            }
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    })
    .await
}
pub async fn wait_done(join: tokio::task::JoinHandle<Result<()>>) -> Result<()> {
    within(Duration::from_secs(3), join).await.unwrap()
}
pub fn timed_exit(after: Duration, error: &'static str) -> Exit {
    let (tx, rx) = tokio::sync::oneshot::channel();
    tokio::spawn(async move {
        tokio::time::sleep(after).await;
        let _ = tx.send(Err(Error::msg(error)));
    });
    rx
}
