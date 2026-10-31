use axum::{
    Router,
    body::Body,
    extract::{Path, State},
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
/// Behaviour of `RangeServer::chunked`; the default serves every chunk normally.
#[derive(Default)]
pub struct ChunkOpts {
    /// `cf-cache-status` per chunk index; empty string or no entry sends no header.
    pub cache: Vec<&'static str>,
    pub missing: Option<usize>,
    pub ignore_range: bool,
    /// Chunk index and body delay per KiB, applied to that chunk's first request only.
    pub slow_first: Option<(usize, Duration)>,
    /// Body delay per KiB on every request.
    pub pace: Duration,
}
struct Chunked {
    bytes: Arc<Vec<u8>>,
    chunk: usize,
    opts: ChunkOpts,
    ranges: Mutex<Vec<Vec<String>>>,
    inflight: AtomicUsize,
    max_inflight: AtomicUsize,
}
struct Inflight(Arc<Chunked>);
impl Drop for Inflight {
    fn drop(&mut self) {
        self.0.inflight.fetch_sub(1, Ordering::SeqCst);
    }
}
pub struct RangeServer {
    pub url: String,
    pub prefix: String,
    data: Arc<Data>,
    chunked: Option<Arc<Chunked>>,
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
        Self {
            url,
            prefix: String::new(),
            data,
            chunked: None,
            task,
        }
    }
    /// Serves `bytes` as `chunk`-sized objects on `GET /m/{nnn}`; the last one is shorter.
    pub async fn chunked(bytes: Arc<Vec<u8>>, chunk: usize, opts: ChunkOpts) -> Self {
        let count = bytes.len().div_ceil(chunk);
        let state = Arc::new(Chunked {
            bytes: bytes.clone(),
            chunk,
            opts,
            ranges: Mutex::new(vec![Vec::new(); count]),
            inflight: AtomicUsize::new(0),
            max_inflight: AtomicUsize::new(0),
        });
        let app = Router::new()
            .route("/m/{nnn}", get(serve_chunk))
            .with_state(state.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let prefix = format!("http://{}/m", listener.local_addr().unwrap());
        let task = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        Self {
            url: String::new(),
            prefix,
            data: Arc::new(Data {
                bytes,
                fault: Fault::Normal,
                calls: AtomicUsize::new(0),
                ranges: Mutex::new(Vec::new()),
            }),
            chunked: Some(state),
            task,
        }
    }
    pub fn calls_for(&self, n: usize) -> usize {
        self.ranges_for(n).len()
    }
    pub fn ranges_for(&self, n: usize) -> Vec<String> {
        self.chunked.as_ref().unwrap().ranges.lock().unwrap()[n].clone()
    }
    /// Highest number of simultaneously open chunk responses.
    pub fn max_inflight(&self) -> usize {
        self.chunked
            .as_ref()
            .unwrap()
            .max_inflight
            .load(Ordering::SeqCst)
    }
    pub fn calls(&self) -> usize {
        self.data.calls.load(Ordering::Relaxed)
    }
    pub fn ranges(&self) -> Vec<String> {
        self.data.ranges.lock().unwrap().clone()
    }
}
fn parse_range(range: &str, total: usize) -> (usize, usize) {
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
    (start, end)
}
async fn serve_chunk(
    State(state): State<Arc<Chunked>>,
    Path(nnn): Path<String>,
    headers: HeaderMap,
) -> Response<Body> {
    let n: usize = nnn.parse().unwrap();
    let range = headers
        .get("range")
        .and_then(|s| s.to_str().ok())
        .unwrap_or_default();
    let first = {
        let mut ranges = state.ranges.lock().unwrap();
        ranges[n].push(range.into());
        ranges[n].len() == 1
    };
    if state.opts.missing == Some(n) {
        return Response::builder().status(404).body(Body::empty()).unwrap();
    }
    let lo = n * state.chunk;
    let object = &state.bytes[lo..(lo + state.chunk).min(state.bytes.len())];
    let (mut start, mut end) = parse_range(range, object.len());
    let ranged = !range.is_empty() && !state.opts.ignore_range;
    if !ranged {
        (start, end) = (0, object.len());
    }
    if start > end {
        return Response::builder().status(416).body(Body::empty()).unwrap();
    }
    let mut delay = state.opts.pace;
    if let Some((slow, d)) = state.opts.slow_first
        && slow == n
        && first
    {
        delay += d;
    }
    let now = state.inflight.fetch_add(1, Ordering::SeqCst) + 1;
    state.max_inflight.fetch_max(now, Ordering::SeqCst);
    let guard = Inflight(state.clone());
    let bytes = Bytes::copy_from_slice(&object[start..end]);
    let body = Body::from_stream(stream::unfold(
        (0, bytes, guard),
        move |(at, bytes, guard)| async move {
            if at >= bytes.len() {
                return None;
            }
            if !delay.is_zero() {
                tokio::time::sleep(delay).await;
            }
            let piece = bytes.slice(at..(at + 1024).min(bytes.len()));
            Some((Ok::<_, std::io::Error>(piece), (at + 1024, bytes, guard)))
        },
    ));
    let mut response = Response::builder()
        .status(if ranged {
            StatusCode::PARTIAL_CONTENT
        } else {
            StatusCode::OK
        })
        .header("Content-Length", end - start);
    if ranged {
        response = response.header(
            "Content-Range",
            format!("bytes {start}-{}/{}", end.saturating_sub(1), object.len()),
        );
    }
    if let Some(status) = state.opts.cache.get(n).filter(|s| !s.is_empty()) {
        response = response.header("cf-cache-status", *status);
    }
    response.body(body).unwrap()
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
    let (start, end) = parse_range(range, total);
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
async fn range_server_chunked_tracks_requests() {
    let bytes = data(3000);
    let opts = ChunkOpts {
        slow_first: Some((1, Duration::from_millis(30))),
        pace: Duration::from_millis(1),
        ..Default::default()
    };
    let server = RangeServer::chunked(bytes.clone(), 1024, opts).await;
    let get = |n: usize| {
        let url = format!("{}/{n:03}", server.prefix);
        async move { crate::http::client().get(url).send().await.unwrap() }
    };
    let started = std::time::Instant::now();
    let (a, b) = tokio::join!(get(0), get(1));
    assert_eq!(a.bytes().await.unwrap().as_ref(), &bytes[..1024]);
    assert!(started.elapsed() < Duration::from_secs(5));
    assert_eq!(b.bytes().await.unwrap().as_ref(), &bytes[1024..2048]);
    assert_eq!(get(2).await.bytes().await.unwrap().len(), 3000 - 2048);
    assert_eq!((server.calls_for(0), server.calls_for(1)), (1, 1));
    assert_eq!(server.ranges_for(2), [""]);
    assert_eq!(server.max_inflight(), 2);
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
        model: "qwen38-q6".into(),
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
