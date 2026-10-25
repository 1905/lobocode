use crate::{
    Error, Result,
    source::{BoxRead, Source},
};
use futures_util::{StreamExt, TryStreamExt, stream};
use lobo_proto::DownloadProgress;
use sha2::{Digest, Sha256};
use std::{collections::HashMap, future::Future, time::Duration};
use std::{
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicI64, Ordering},
    },
};
use tokio::{
    fs::{File, OpenOptions},
    io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt},
    sync::Semaphore,
    time::Instant,
};
use tokio_util::sync::CancellationToken;

pub const MAX_RESUMES: usize = 8;
type Progress<'a> = Option<&'a (dyn Fn(DownloadProgress) + Sync)>;

#[derive(Clone, Copy)]
pub struct Tuning {
    pub stall: Duration,
    pub chunk: i64,
    /// Throughput window that drives the stream ramp.
    pub window: Duration,
    /// Minimum stream age and span of its recent rate before a slow stream is dropped.
    pub slow_after: Duration,
}
impl Default for Tuning {
    fn default() -> Self {
        Self {
            stall: Duration::from_secs(30),
            chunk: lobo_proto::catalog::CHUNK_SIZE,
            window: Duration::from_secs(5),
            slow_after: Duration::from_secs(10),
        }
    }
}
tokio::task_local! { static TUNING: Tuning; }

/// Override tuning for one operation. Scoped futures keep parallel tests independent.
#[doc(hidden)]
pub async fn with_tuning<F: Future>(t: Tuning, f: F) -> F::Output {
    TUNING.scope(t, f).await
}
pub(crate) fn tuning() -> Tuning {
    TUNING.try_with(|t| *t).unwrap_or_default()
}

struct Reporter<'a> {
    n: i64,
    total: i64,
    base: i64,
    start: Instant,
    last: Option<Instant>,
    verifying: bool,
    callback: Progress<'a>,
}
impl<'a> Reporter<'a> {
    fn new(total: i64, callback: Progress<'a>) -> Self {
        Self {
            n: 0,
            total,
            base: 0,
            start: Instant::now(),
            last: None,
            verifying: false,
            callback,
        }
    }
    fn report(&mut self, force: bool) {
        if !force
            && self
                .last
                .is_some_and(|t| t.elapsed() < Duration::from_millis(500))
        {
            return;
        }
        self.last = Some(Instant::now());
        if let Some(callback) = self.callback {
            callback(DownloadProgress {
                bytes: self.n,
                total: self.total,
                mbps: (self.n - self.base) as f64
                    / self.start.elapsed().as_secs_f64().max(1e-9)
                    / 1e6,
                verifying: self.verifying,
                source: String::new(),
            });
        }
    }
}

async fn cancelled<T>(cancel: &CancellationToken, f: impl Future<Output = Result<T>>) -> Result<T> {
    tokio::select! { biased; () = cancel.cancelled() => Err(Error::Cancelled), result = f => result }
}
async fn sleep(cancel: &CancellationToken, duration: Duration) -> Result<()> {
    cancelled(cancel, async {
        tokio::time::sleep(duration).await;
        Ok(())
    })
    .await
}
async fn read(cancel: &CancellationToken, r: &mut BoxRead, buf: &mut [u8]) -> Result<usize> {
    cancelled(cancel, async {
        tokio::time::timeout(tuning().stall, r.read(buf))
            .await
            .map_err(|_| Error::msg("download stream stalled"))?
            .map_err(Error::from)
    })
    .await
}

pub async fn hash_file(
    cancel: CancellationToken,
    path: &Path,
    total: i64,
    on_progress: Progress<'_>,
) -> Result<String> {
    let mut file = File::open(path).await?;
    let mut hash = Sha256::new();
    let mut report = Reporter::new(total, on_progress);
    report.verifying = true;
    report.report(true);
    let mut buf = vec![0; 4 << 20];
    loop {
        let n = cancelled(&cancel, async {
            file.read(&mut buf).await.map_err(Error::from)
        })
        .await?;
        if n == 0 {
            break;
        }
        hash.update(&buf[..n]);
        report.n += n as i64;
        report.report(false);
    }
    report.report(true);
    Ok(hex::encode(hash.finalize()))
}

pub async fn download(
    cancel: CancellationToken,
    src: &dyn Source,
    dst: &Path,
    sha: &str,
    on_progress: Progress<'_>,
) -> Result<()> {
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(dst)
        .await?;
    let mut hash = Sha256::new();
    let mut report = Reporter::new(-1, on_progress);
    let existing = file.metadata().await?.len() as i64;
    let mut done = false;
    let mut buf = vec![0; 4 << 20];
    if existing > 0 {
        let (probe, total) = cancelled(&cancel, src.open(0, 1)).await?;
        drop(probe);
        if total < 0 || existing > total {
            file.set_len(0).await?;
            file.rewind().await?;
        } else {
            loop {
                let n = cancelled(&cancel, async {
                    file.read(&mut buf).await.map_err(Error::from)
                })
                .await?;
                if n == 0 {
                    break;
                }
                hash.update(&buf[..n]);
            }
            report.n = existing;
            report.base = existing;
            report.total = total;
            done = existing == total;
        }
    }
    let mut last = None;
    for attempt in 0..=MAX_RESUMES {
        if done {
            break;
        }
        if attempt > 0 {
            sleep(&cancel, Duration::from_secs(attempt as u64 * 2)).await?;
        }
        let result: Result<()> = async {
            let (mut body, total) = cancelled(&cancel, src.open(report.n, -1)).await?;
            if report.total < 0 && total >= 0 {
                report.total = total;
            }
            loop {
                let n = read(&cancel, &mut body, &mut buf).await?;
                if n == 0 {
                    break;
                }
                // Finish the write before honoring cancellation, so a partial file can be resumed safely.
                file.write_all(&buf[..n]).await?;
                hash.update(&buf[..n]);
                report.n += n as i64;
                report.report(false);
            }
            if report.total < 0 {
                return Err(Error::Permanent(format!("unknown total size from {src}")));
            }
            if report.n != report.total {
                return Err(Error::msg(format!(
                    "short body: {} of {} bytes",
                    report.n, report.total
                )));
            }
            Ok(())
        }
        .await;
        match result {
            Ok(()) => {
                done = true;
                last = None;
            }
            Err(e) if e.is_permanent() || matches!(e, Error::Cancelled) => return Err(e),
            Err(e) => last = Some(e),
        }
    }
    file.flush().await?;
    drop(file);
    if !done {
        return Err(Error::msg(format!(
            "download {src}: {} (after {MAX_RESUMES} resumes)",
            last.map(|e| e.to_string()).unwrap_or_default()
        )));
    }
    report.report(true);
    let got = hex::encode(hash.finalize());
    if got != sha {
        tokio::fs::rename(dst, format!("{}.bad", dst.display())).await?;
        return Err(Error::msg(format!(
            "download {src}: sha256 {got}, want {sha}"
        )));
    }
    Ok(())
}

/// Stream count: starts at `min(4, max)`, adds 4 while each window beats the last
/// growth window by at least 10 %, then holds for good.
pub(crate) struct Ramp {
    max: usize,
    permits: usize,
    baseline: Option<f64>,
    holding: bool,
}
impl Ramp {
    pub(crate) fn new(max: usize) -> Self {
        Self {
            max,
            permits: max.min(4),
            baseline: None,
            holding: false,
        }
    }
    pub(crate) fn permits(&self) -> usize {
        self.permits
    }
    /// Takes one window's aggregate MB/s and returns the new stream count.
    pub(crate) fn on_window(&mut self, mbps: f64) -> usize {
        if !self.holding {
            if self.baseline.is_none_or(|b| mbps >= b * 1.1) {
                self.baseline = Some(mbps);
                self.permits = (self.permits + 4).min(self.max);
            } else {
                self.holding = true;
            }
        }
        self.permits
    }
}

/// A stream is dropped when it is old enough, not alone, and under 25 % of the mean per-stream rate.
pub(crate) fn too_slow(
    age: Duration,
    recent_mbps: f64,
    aggregate_mbps: f64,
    active: usize,
    slow_after: Duration,
) -> bool {
    age >= slow_after && active >= 2 && recent_mbps < 0.25 * aggregate_mbps / active as f64
}

fn mbps(bytes: i64, elapsed: Duration) -> f64 {
    bytes as f64 / elapsed.as_secs_f64().max(1e-9) / 1e6
}

/// One open chunk attempt, keyed by chunk index. Its read loop only bumps `bytes`.
struct Stream {
    started: Instant,
    bytes: Arc<AtomicI64>,
    drop: CancellationToken,
    droppable: bool,
    /// Snapshot `(at, bytes)` the recent rate is measured from, refreshed every `slow_after`.
    seen: (Instant, i64),
}
type Streams = Mutex<HashMap<usize, Stream>>;
struct Registered<'a>(&'a Streams, usize);
impl Drop for Registered<'_> {
    fn drop(&mut self) {
        self.0.lock().unwrap().remove(&self.1);
    }
}

// Each range opens its own file description. try_clone shares seek offsets on Unix.
#[allow(clippy::too_many_arguments)]
async fn fetch_chunk(
    cancel: &CancellationToken,
    src: &dyn Source,
    dst: Option<&Path>,
    index: usize,
    start: i64,
    end: i64,
    sha: Option<&str>,
    count: &AtomicI64,
    streams: &Streams,
) -> Result<()> {
    let mut file = match dst {
        Some(path) => Some(OpenOptions::new().write(true).open(path).await?),
        None => None,
    };
    let mut pos = start;
    let mut buf = vec![0; 1 << 20];
    let mut attempt = 0;
    let mut droppable = true;
    while pos < end {
        if sha.is_some() {
            pos = start;
        }
        let mut hash = Sha256::new();
        let from = pos;
        // The monitor cancels `abort` to drop this attempt; `cancel` still means the user.
        let abort = cancel.child_token();
        let streamed = Arc::new(AtomicI64::new(0));
        let now = Instant::now();
        streams.lock().unwrap().insert(
            index,
            Stream {
                started: now,
                bytes: streamed.clone(),
                drop: abort.clone(),
                droppable,
                seen: (now, 0),
            },
        );
        let registered = Registered(streams, index);
        let mut dropped = false;
        match cancelled(&abort, src.open(pos, end - pos)).await {
            Ok((mut body, _)) => {
                if let Some(file) = &mut file {
                    file.seek(std::io::SeekFrom::Start(pos as u64)).await?;
                }
                while pos < end {
                    let limit = buf.len().min((end - pos) as usize);
                    let n = match read(&abort, &mut body, &mut buf[..limit]).await {
                        Ok(0) => break,
                        Ok(n) => n,
                        Err(Error::Cancelled) if cancel.is_cancelled() => {
                            return Err(Error::Cancelled);
                        }
                        Err(Error::Cancelled) => {
                            dropped = true;
                            break;
                        }
                        Err(_) => break,
                    };
                    if let Some(file) = &mut file {
                        file.write_all(&buf[..n]).await?;
                    }
                    hash.update(&buf[..n]);
                    pos += n as i64;
                    count.fetch_add(n as i64, Ordering::Relaxed);
                    streamed.fetch_add(n as i64, Ordering::Relaxed);
                }
                if let Some(sha) = sha
                    && (pos < end || hex::encode(hash.finalize()) != sha)
                {
                    count.fetch_sub(pos - from, Ordering::Relaxed);
                    pos = start;
                }
                if pos == end {
                    if let Some(file) = &mut file {
                        file.flush().await?;
                    }
                    return Ok(());
                }
            }
            Err(Error::Cancelled) if !cancel.is_cancelled() => dropped = true,
            Err(e) if e.is_permanent() || matches!(e, Error::Cancelled) => return Err(e),
            Err(_) => {}
        }
        drop(registered);
        // A drop restarts the chunk at once, from its first byte, and costs no resume.
        if dropped {
            count.fetch_sub(pos - start, Ordering::Relaxed);
            pos = start;
            droppable = false;
            continue;
        }
        attempt += 1;
        if attempt > MAX_RESUMES {
            return Err(Error::msg(format!(
                "chunk {start}-{end}: gave up after {MAX_RESUMES} resumes"
            )));
        }
        sleep(cancel, Duration::from_secs(attempt as u64)).await?;
    }
    Ok(())
}

async fn fetch_ranges(
    cancel: &CancellationToken,
    src: &dyn Source,
    dst: Option<&Path>,
    size: i64,
    conns: usize,
    chunk_sha: &[String],
    count: &AtomicI64,
) -> Result<()> {
    let Tuning {
        chunk,
        window,
        slow_after,
        ..
    } = tuning();
    if size < 0 || chunk <= 0 {
        return Err(Error::msg("invalid download size or chunk size"));
    }
    let mut ramp = Ramp::new(conns.max(1));
    let permits = Semaphore::new(ramp.permits());
    let streams = Streams::default();
    let ranges = (0..size).step_by(chunk as usize).enumerate();
    let work = stream::iter(ranges)
        .map(|(i, start)| {
            let (permits, streams) = (&permits, &streams);
            async move {
                let _permit = permits.acquire().await.expect("semaphore is never closed");
                fetch_chunk(
                    cancel,
                    src,
                    dst,
                    i,
                    start,
                    (start + chunk).min(size),
                    chunk_sha.get(i).map(String::as_str),
                    count,
                    streams,
                )
                .await
            }
        })
        .buffer_unordered(conns.max(1))
        .try_collect::<Vec<_>>();
    tokio::pin!(work);
    let started = Instant::now();
    let (mut last_at, mut last_n, mut drops) = (started, 0, 0);
    let mut ticker = tokio::time::interval(window);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    ticker.tick().await;
    let result = loop {
        tokio::select! {
            result = &mut work => break result,
            _ = ticker.tick() => {
                let (now, n) = (Instant::now(), count.load(Ordering::Relaxed));
                let window_mbps = mbps(n - last_n, now - last_at);
                (last_at, last_n) = (now, n);
                let old = ramp.permits();
                let new = ramp.on_window(window_mbps);
                if new > old {
                    permits.add_permits(new - old);
                    tracing::info!(service = "lobo-agent", streams = new, mbps = window_mbps, "download streams raised");
                }
                let mut streams = streams.lock().unwrap();
                let active = streams.len();
                for (&i, s) in streams.iter_mut() {
                    if !s.droppable || now - s.seen.0 < slow_after {
                        continue;
                    }
                    let bytes = s.bytes.load(Ordering::Relaxed);
                    let recent = mbps(bytes - s.seen.1, now - s.seen.0);
                    s.seen = (now, bytes);
                    if too_slow(now - s.started, recent, window_mbps, active, slow_after) {
                        s.drop.cancel();
                        s.droppable = false;
                        drops += 1;
                        tracing::info!(
                            service = "lobo-agent",
                            chunk = i,
                            stream_mbps = recent,
                            mean_mbps = window_mbps / active as f64,
                            "slow download stream dropped"
                        );
                    }
                }
            }
        }
    };
    tracing::info!(
        service = "lobo-agent",
        elapsed_s = started.elapsed().as_secs_f64(),
        avg_mbps = mbps(count.load(Ordering::Relaxed), started.elapsed()),
        peak_streams = ramp.permits(),
        drops,
        source = src.summary(),
        "download summary"
    );
    result?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn download_parallel(
    cancel: CancellationToken,
    src: &dyn Source,
    dst: &Path,
    size: i64,
    sha: &str,
    chunk_sha: &[String],
    conns: usize,
    on_progress: Progress<'_>,
) -> Result<()> {
    if conns <= 1 {
        return download(cancel, src, dst, sha, on_progress).await;
    }
    let chunk = tuning().chunk;
    if size < 0 || chunk <= 0 {
        return Err(Error::msg("invalid download size or chunk size"));
    }
    if !chunk_sha.is_empty()
        && chunk_sha.len() as i64 != size / chunk + i64::from(size % chunk != 0)
    {
        return Err(Error::msg(format!(
            "download {src}: chunk table has {} entries for {size} bytes",
            chunk_sha.len()
        )));
    }
    let file = File::create(dst).await?;
    file.set_len(size as u64).await?;
    drop(file);
    let count = AtomicI64::new(0);
    let mut report = Reporter::new(size, on_progress);
    let work = fetch_ranges(&cancel, src, Some(dst), size, conns, chunk_sha, &count);
    tokio::pin!(work);
    let mut ticker = tokio::time::interval(Duration::from_millis(500));
    ticker.tick().await;
    loop {
        tokio::select! {
            result = &mut work => { result?; break; }
            _ = ticker.tick() => { report.n = count.load(Ordering::Relaxed); report.report(true); }
        }
    }
    report.n = size;
    let download_mbps = size as f64 / report.start.elapsed().as_secs_f64().max(1e-9) / 1e6;
    if chunk_sha.is_empty() {
        let got = hash_file(cancel.clone(), dst, size, on_progress).await?;
        if got != sha {
            tokio::fs::rename(dst, format!("{}.bad", dst.display())).await?;
            return Err(Error::msg(format!(
                "download {src}: sha256 {got}, want {sha}"
            )));
        }
    }
    if let Some(callback) = on_progress {
        callback(DownloadProgress {
            bytes: size,
            total: size,
            mbps: download_mbps,
            ..Default::default()
        });
    }
    Ok(())
}

pub async fn bench(
    cancel: CancellationToken,
    src: &dyn Source,
    size: i64,
    conns: usize,
    dur: Duration,
) -> Result<(i64, f64)> {
    let count = AtomicI64::new(0);
    let start = Instant::now();
    if let Ok(result) = tokio::time::timeout(
        dur,
        fetch_ranges(&cancel, src, None, size, conns, &[], &count),
    )
    .await
    {
        result?;
    }
    let n = count.load(Ordering::Relaxed);
    Ok((n, n as f64 / start.elapsed().as_secs_f64().max(1e-9) / 1e6))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        source::{ChunkSource, HttpSource},
        testutil::{self, ChunkOpts, Fault, RangeServer, within},
    };
    use std::sync::{Arc, Mutex};

    fn sha(bytes: &[u8]) -> String {
        hex::encode(Sha256::digest(bytes))
    }
    fn tune() -> Tuning {
        Tuning {
            stall: Duration::from_millis(200),
            chunk: 64 << 10,
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn download_basic() {
        let data = testutil::data(500_000);
        let server = RangeServer::new(data.clone(), Fault::Normal).await;
        let src = HttpSource::new(server.url.clone());
        let temp = tempfile::tempdir().unwrap();
        let dst = temp.path().join("model");
        download(CancellationToken::new(), &src, &dst, &sha(&data), None)
            .await
            .unwrap();
        assert_eq!(tokio::fs::read(&dst).await.unwrap(), *data);
        let bad = temp.path().join("bad-model");
        assert!(
            download(CancellationToken::new(), &src, &bad, "bad", None)
                .await
                .unwrap_err()
                .to_string()
                .contains("sha256")
        );
        assert!(!bad.exists());
        assert!(temp.path().join("bad-model.bad").exists());
        let missing = HttpSource::new(server.url.replace("/file", "/missing"));
        assert!(
            download(
                CancellationToken::new(),
                &missing,
                &temp.path().join("missing"),
                "",
                None
            )
            .await
            .unwrap_err()
            .to_string()
            .contains("404")
        );
    }
    #[tokio::test]
    async fn download_resumes_after_drop() {
        let data = testutil::data(500_000);
        let server = RangeServer::new(data.clone(), Fault::HalfThenClose).await;
        let temp = tempfile::tempdir().unwrap();
        let dst = temp.path().join("model");
        download(
            CancellationToken::new(),
            &HttpSource::new(server.url.clone()),
            &dst,
            &sha(&data),
            None,
        )
        .await
        .unwrap();
        assert_eq!(tokio::fs::read(dst).await.unwrap(), *data);
        assert_eq!(server.calls(), 2);
        assert_eq!(server.ranges()[1], "bytes=250000-");
    }
    #[tokio::test]
    async fn download_resumes_partial_file() {
        let data = testutil::data(500_000);
        for existing in [300_000, 500_000, 600_000] {
            let server = RangeServer::new(data.clone(), Fault::Normal).await;
            let temp = tempfile::tempdir().unwrap();
            let dst = temp.path().join("model");
            let mut prior = data.to_vec();
            prior.resize(existing, 0);
            tokio::fs::write(&dst, prior).await.unwrap();
            let events = Mutex::new(Vec::new());
            let callback = |p| events.lock().unwrap().push(p);
            download(
                CancellationToken::new(),
                &HttpSource::new(server.url.clone()),
                &dst,
                &sha(&data),
                Some(&callback),
            )
            .await
            .unwrap();
            assert_eq!(tokio::fs::read(dst).await.unwrap(), *data);
            assert_eq!(server.ranges()[0], "bytes=0-0");
            if existing == 300_000 {
                assert_eq!(server.ranges()[1], "bytes=300000-");
            }
            if existing == 500_000 {
                assert_eq!(server.calls(), 1);
            }
            let events = events.lock().unwrap();
            let last = events.last().unwrap();
            assert_eq!((last.bytes, last.total), (500_000, 500_000));
        }
    }
    #[tokio::test]
    async fn download_stall_respects_cancel() {
        let data = testutil::data(100);
        let server = RangeServer::new(data.clone(), Fault::StallAfter(3)).await;
        let cancel = CancellationToken::new();
        let timer = cancel.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            timer.cancel();
        });
        let temp = tempfile::tempdir().unwrap();
        let result = within(
            Duration::from_secs(1),
            download(
                cancel,
                &HttpSource::new(server.url.clone()),
                &temp.path().join("model"),
                &sha(&data),
                None,
            ),
        )
        .await;
        assert!(matches!(result, Err(Error::Cancelled)));
    }
    #[tokio::test]
    async fn single_stalled_stream() {
        let data = testutil::data(100);
        let server = RangeServer::new(data.clone(), Fault::StallAfter(3)).await;
        let temp = tempfile::tempdir().unwrap();
        within(
            Duration::from_secs(10),
            with_tuning(
                tune(),
                download_parallel(
                    CancellationToken::new(),
                    &HttpSource::new(server.url.clone()),
                    &temp.path().join("model"),
                    100,
                    &sha(&data),
                    &[],
                    1,
                    None,
                ),
            ),
        )
        .await
        .unwrap();
        assert!(server.calls() >= 2);
    }
    #[tokio::test]
    async fn hash_file_matches_sha256() {
        let data = testutil::data(100);
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("model");
        tokio::fs::write(&path, &*data).await.unwrap();
        let events = Mutex::new(Vec::new());
        let callback = |p| events.lock().unwrap().push(p);
        assert_eq!(
            hash_file(CancellationToken::new(), &path, 100, Some(&callback))
                .await
                .unwrap(),
            sha(&data)
        );
        let events = events.lock().unwrap();
        assert!(events.first().unwrap().verifying);
        assert!(events.last().unwrap().verifying);
        assert_eq!(events.last().unwrap().bytes, 100);
    }
    #[tokio::test]
    async fn hash_file_cancel() {
        let temp = tempfile::NamedTempFile::new().unwrap();
        let cancel = CancellationToken::new();
        cancel.cancel();
        assert!(matches!(
            hash_file(cancel, temp.path(), 0, None).await,
            Err(Error::Cancelled)
        ));
    }
    async fn parallel_case(fault: Fault, table: bool) {
        let data = testutil::data(3 * (64 << 10) + 37);
        let server = RangeServer::new(data.clone(), fault).await;
        let temp = tempfile::tempdir().unwrap();
        let dst = temp.path().join("model");
        let table = if table {
            testutil::chunk_table(&data, tune().chunk)
        } else {
            Vec::new()
        };
        let events = Mutex::new(Vec::new());
        let callback = |p| events.lock().unwrap().push(p);
        within(
            Duration::from_secs(20),
            with_tuning(
                tune(),
                download_parallel(
                    CancellationToken::new(),
                    &HttpSource::new(server.url.clone()),
                    &dst,
                    data.len() as i64,
                    &sha(&data),
                    &table,
                    3,
                    Some(&callback),
                ),
            ),
        )
        .await
        .unwrap();
        assert_eq!(tokio::fs::read(dst).await.unwrap(), *data);
        assert!(server.calls() >= 4);
        let events = events.lock().unwrap();
        let last = events.last().unwrap();
        assert!(!last.verifying);
        assert_eq!(last.bytes, data.len() as i64);
        assert_eq!(events.iter().any(|p| p.verifying), table.is_empty());
    }
    #[tokio::test]
    async fn parallel_http() {
        parallel_case(Fault::HalfThenClose, false).await;
    }
    #[tokio::test]
    async fn parallel_chunk_sha() {
        parallel_case(Fault::HalfThenClose, true).await;
    }
    #[tokio::test]
    async fn parallel_no_headers() {
        parallel_case(Fault::Silent, true).await;
    }
    #[tokio::test]
    async fn parallel_stalled_stream() {
        parallel_case(Fault::StallAfter(100), true).await;
    }
    #[tokio::test]
    async fn parallel_bad_sha() {
        let data = testutil::data(100);
        let server = RangeServer::new(data.clone(), Fault::Normal).await;
        let temp = tempfile::tempdir().unwrap();
        let dst = temp.path().join("model");
        let e = download_parallel(
            CancellationToken::new(),
            &HttpSource::new(server.url.clone()),
            &dst,
            100,
            "bad",
            &[],
            2,
            None,
        )
        .await
        .unwrap_err();
        assert!(e.to_string().contains("sha256"));
        assert!(temp.path().join("model.bad").exists());
        assert!(!dst.exists());
    }
    #[tokio::test]
    async fn parallel_chunk_sha_mismatch() {
        let data = testutil::data(100);
        let server = RangeServer::new(data, Fault::Normal).await;
        let temp = tempfile::tempdir().unwrap();
        let dst = temp.path().join("model");
        let e = within(
            Duration::from_secs(50),
            with_tuning(
                tune(),
                download_parallel(
                    CancellationToken::new(),
                    &HttpSource::new(server.url.clone()),
                    &dst,
                    100,
                    "bad",
                    &["0".repeat(64)],
                    2,
                    None,
                ),
            ),
        )
        .await
        .unwrap_err();
        assert!(e.to_string().contains("gave up"));
        assert_eq!(server.calls(), 9);
    }
    #[tokio::test]
    async fn parallel_bad_table_no_ticker_leak() {
        let server = RangeServer::new(testutil::data(10), Fault::Normal).await;
        let temp = tempfile::tempdir().unwrap();
        let count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let callback = |_: DownloadProgress| {
            count.fetch_add(1, Ordering::Relaxed);
        };
        let e = download_parallel(
            CancellationToken::new(),
            &HttpSource::new(server.url.clone()),
            &temp.path().join("model"),
            10,
            "bad",
            &["a".into(), "b".into()],
            2,
            Some(&callback),
        )
        .await
        .unwrap_err();
        assert!(e.to_string().contains("chunk table"));
        tokio::time::sleep(Duration::from_millis(700)).await;
        assert_eq!(count.load(Ordering::Relaxed), 0);
        assert_eq!(server.calls(), 0);
    }
    #[tokio::test]
    async fn bench() {
        let data = testutil::data(4 << 20);
        let server = RangeServer::new(data, Fault::Normal).await;
        let (n, mbps) = with_tuning(
            tune(),
            super::bench(
                CancellationToken::new(),
                &HttpSource::new(server.url.clone()),
                4 << 20,
                2,
                Duration::from_secs(5),
            ),
        )
        .await
        .unwrap();
        assert_eq!(n, 4 << 20);
        assert!(mbps > 0.0);
    }
    #[test]
    fn default_tuning() {
        assert_eq!(tuning().stall, Duration::from_secs(30));
        assert_eq!(tuning().chunk, lobo_proto::catalog::CHUNK_SIZE);
    }
    #[test]
    fn ramp_starts_at_four_and_respects_small_caps() {
        assert_eq!(Ramp::new(2).permits(), 2);
        assert_eq!(Ramp::new(32).permits(), 4);
    }
    #[test]
    fn ramp_grows_on_ten_percent_gain_then_holds() {
        let mut r = Ramp::new(32);
        assert_eq!(r.on_window(100.0), 8); // first window is the baseline, then grow
        assert_eq!(r.on_window(115.0), 12); // +15 %
        assert_eq!(r.on_window(120.0), 12); // +4.3 %: hold
        assert_eq!(r.on_window(500.0), 12); // holding is final
    }
    #[test]
    fn ramp_never_exceeds_cap() {
        let mut r = Ramp::new(10);
        assert_eq!(r.on_window(10.0), 8);
        assert_eq!(r.on_window(20.0), 10);
        assert_eq!(r.on_window(40.0), 10);
    }
    #[test]
    fn too_slow_rules() {
        let s = Duration::from_secs(10);
        assert!(!too_slow(Duration::from_secs(9), 1.0, 100.0, 4, s)); // too young
        assert!(!too_slow(Duration::from_secs(11), 1.0, 100.0, 1, s)); // only stream
        assert!(too_slow(Duration::from_secs(11), 6.0, 100.0, 4, s)); // 6 < 0.25 * 25
        assert!(!too_slow(Duration::from_secs(11), 7.0, 100.0, 4, s));
    }
    fn saturated() -> Tuning {
        Tuning {
            stall: Duration::from_secs(5),
            chunk: 64 << 10,
            window: Duration::from_millis(50),
            slow_after: Duration::from_millis(200),
        }
    }
    async fn chunked_case(
        chunks: usize,
        opts: ChunkOpts,
        conns: usize,
        cancel: CancellationToken,
    ) -> (RangeServer, Result<()>, Vec<u8>, Arc<Vec<u8>>) {
        let chunk = 64 << 10;
        let data = testutil::data(chunks * chunk);
        let server = RangeServer::chunked(data.clone(), chunk, opts).await;
        let src = ChunkSource::new(server.prefix.clone(), data.len() as i64);
        let temp = tempfile::tempdir().unwrap();
        let dst = temp.path().join("model");
        let result = within(
            Duration::from_secs(20),
            with_tuning(
                saturated(),
                download_parallel(
                    cancel,
                    &src,
                    &dst,
                    data.len() as i64,
                    &sha(&data),
                    &testutil::chunk_table(&data, chunk as i64),
                    conns,
                    None,
                ),
            ),
        )
        .await;
        let got = tokio::fs::read(&dst).await.unwrap_or_default();
        (server, result, got, data)
    }
    #[tokio::test]
    async fn parallel_drops_a_slow_stream_and_finishes() {
        // pace keeps the other streams active while chunk 1 is slow; a drop needs >= 2 active streams.
        let opts = ChunkOpts {
            pace: Duration::from_millis(1),
            slow_first: Some((1, Duration::from_millis(200))),
            ..Default::default()
        };
        let (server, result, got, data) = chunked_case(32, opts, 4, CancellationToken::new()).await;
        result.unwrap();
        assert_eq!(got, *data);
        assert_eq!(server.calls_for(1), 2); // dropped once, never twice
    }
    #[tokio::test]
    async fn parallel_never_exceeds_stream_cap() {
        let opts = ChunkOpts {
            pace: Duration::from_millis(1),
            ..Default::default()
        };
        let (server, result, got, data) = chunked_case(8, opts, 2, CancellationToken::new()).await;
        result.unwrap();
        assert_eq!(got, *data);
        assert_eq!(server.max_inflight(), 2);
    }
    #[tokio::test]
    async fn parallel_ramps_streams_up_to_cap() {
        let opts = ChunkOpts {
            pace: Duration::from_millis(1),
            ..Default::default()
        };
        let (server, result, got, data) = chunked_case(32, opts, 8, CancellationToken::new()).await;
        result.unwrap();
        assert_eq!(got, *data);
        assert_eq!(server.max_inflight(), 8); // starts at 4; the first window adds 4
    }
    #[tokio::test]
    async fn cancel_while_streams_are_slow_returns_cancelled() {
        // conns 4: the single-stream path cannot read chunk objects.
        let opts = ChunkOpts {
            slow_first: Some((0, Duration::from_secs(1))),
            ..Default::default()
        };
        let cancel = CancellationToken::new();
        let timer = cancel.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(100)).await;
            timer.cancel();
        });
        let started = Instant::now();
        let (_server, result, _, _) = chunked_case(1, opts, 4, cancel).await;
        assert!(matches!(result, Err(Error::Cancelled)));
        assert!(started.elapsed() < Duration::from_secs(2));
    }
}
