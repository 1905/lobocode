use crate::{
    Error, Result,
    source::{BoxRead, Source},
};
use futures_util::{StreamExt, TryStreamExt, stream};
use lobo_proto::DownloadProgress;
use sha2::{Digest, Sha256};
use std::{future::Future, time::Duration};
use std::{
    path::Path,
    sync::atomic::{AtomicI64, Ordering},
};
use tokio::{
    fs::{File, OpenOptions},
    io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt},
    time::Instant,
};
use tokio_util::sync::CancellationToken;

pub const MAX_RESUMES: usize = 8;
type Progress<'a> = Option<&'a (dyn Fn(DownloadProgress) + Sync)>;

#[derive(Clone, Copy)]
pub struct Tuning {
    pub stall: Duration,
    pub chunk: i64,
}
impl Default for Tuning {
    fn default() -> Self {
        Self {
            stall: Duration::from_secs(30),
            chunk: lobo_proto::catalog::CHUNK_SIZE,
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

// Each range opens its own file description. try_clone shares seek offsets on Unix.
async fn fetch_chunk(
    cancel: &CancellationToken,
    src: &dyn Source,
    dst: Option<&Path>,
    start: i64,
    end: i64,
    sha: Option<&str>,
    count: &AtomicI64,
) -> Result<()> {
    let mut file = match dst {
        Some(path) => Some(OpenOptions::new().write(true).open(path).await?),
        None => None,
    };
    let mut pos = start;
    let mut buf = vec![0; 1 << 20];
    for attempt in 0..=MAX_RESUMES {
        if pos >= end {
            return Ok(());
        }
        if attempt > 0 {
            sleep(cancel, Duration::from_secs(attempt as u64)).await?;
        }
        if sha.is_some() {
            pos = start;
        }
        let mut hash = Sha256::new();
        let from = pos;
        let (mut body, _) = match cancelled(cancel, src.open(pos, end - pos)).await {
            Ok(v) => v,
            Err(e) if e.is_permanent() || matches!(e, Error::Cancelled) => return Err(e),
            Err(_) => continue,
        };
        if let Some(file) = &mut file {
            file.seek(std::io::SeekFrom::Start(pos as u64)).await?;
        }
        while pos < end {
            let limit = buf.len().min((end - pos) as usize);
            let n = match read(cancel, &mut body, &mut buf[..limit]).await {
                Ok(0) => break,
                Ok(n) => n,
                Err(Error::Cancelled) => return Err(Error::Cancelled),
                Err(_) => break,
            };
            if let Some(file) = &mut file {
                file.write_all(&buf[..n]).await?;
            }
            hash.update(&buf[..n]);
            pos += n as i64;
            count.fetch_add(n as i64, Ordering::Relaxed);
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
    Err(Error::msg(format!(
        "chunk {start}-{end}: gave up after {MAX_RESUMES} resumes"
    )))
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
    let chunk = tuning().chunk;
    if size < 0 || chunk <= 0 {
        return Err(Error::msg("invalid download size or chunk size"));
    }
    let ranges = (0..size).step_by(chunk as usize).enumerate();
    stream::iter(ranges)
        .map(|(i, start)| async move {
            fetch_chunk(
                cancel,
                src,
                dst,
                start,
                (start + chunk).min(size),
                chunk_sha.get(i).map(String::as_str),
                count,
            )
            .await
        })
        .buffer_unordered(conns.max(1))
        .try_collect::<Vec<_>>()
        .await?;
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
        source::HttpSource,
        testutil::{self, Fault, RangeServer, within},
    };
    use std::sync::{Arc, Mutex};

    fn sha(bytes: &[u8]) -> String {
        hex::encode(Sha256::digest(bytes))
    }
    fn tune() -> Tuning {
        Tuning {
            stall: Duration::from_millis(200),
            chunk: 64 << 10,
        }
    }

    #[tokio::test]
    async fn download_basic() {
        let data = testutil::data(500_000);
        let server = RangeServer::new(data.clone(), Fault::Normal).await;
        let src = HttpSource {
            url: server.url.clone(),
        };
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
        let missing = HttpSource {
            url: server.url.replace("/file", "/missing"),
        };
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
            &HttpSource {
                url: server.url.clone(),
            },
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
                &HttpSource {
                    url: server.url.clone(),
                },
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
                &HttpSource {
                    url: server.url.clone(),
                },
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
                    &HttpSource {
                        url: server.url.clone(),
                    },
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
                    &HttpSource {
                        url: server.url.clone(),
                    },
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
            &HttpSource {
                url: server.url.clone(),
            },
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
                    &HttpSource {
                        url: server.url.clone(),
                    },
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
            &HttpSource {
                url: server.url.clone(),
            },
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
                &HttpSource {
                    url: server.url.clone(),
                },
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
}
