use crate::{Error, Result, download::tuning};
use async_trait::async_trait;
use futures_util::TryStreamExt;
use std::fmt;
use tokio_util::io::StreamReader;

pub type BoxRead = Box<dyn tokio::io::AsyncRead + Send + Unpin>;

#[async_trait]
pub trait Source: Send + Sync + fmt::Display {
    /// Negative length reads to EOF. The returned size is the full object size, or -1 if unknown.
    async fn open(&self, offset: i64, len: i64) -> Result<(BoxRead, i64)>;
}

pub struct HttpSource {
    pub url: String,
    http: reqwest::Client,
}
impl HttpSource {
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            http: crate::http::client(),
        }
    }
}

pub fn redact_url(raw: &str) -> String {
    let Ok(url) = url::Url::parse(raw) else {
        return "?".into();
    };
    if url.host_str().is_none() {
        return "?".into();
    }
    format!(
        "{}://{}",
        url.scheme(),
        &url[url::Position::BeforeHost..url::Position::AfterPort]
    )
}

impl fmt::Display for HttpSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&redact_url(&self.url))
    }
}

pub fn range_total(content_range: &str) -> i64 {
    content_range
        .rsplit_once('/')
        .and_then(|(_, n)| n.parse().ok())
        .filter(|n: &i64| *n >= 0)
        .unwrap_or(-1)
}

#[async_trait]
impl Source for HttpSource {
    async fn open(&self, offset: i64, len: i64) -> Result<(BoxRead, i64)> {
        let mut req = self.http.get(&self.url);
        let ranged = offset > 0 || len >= 0;
        if ranged {
            let end = if len >= 0 {
                offset
                    .checked_add(len)
                    .and_then(|n| n.checked_sub(1))
                    .ok_or_else(|| Error::Permanent("invalid Range".into()))?
                    .to_string()
            } else {
                String::new()
            };
            req = req.header(reqwest::header::RANGE, format!("bytes={offset}-{end}"));
        }
        let response = tokio::time::timeout(tuning().stall, req.send())
            .await
            .map_err(|_| Error::msg(format!("no response headers after {:?}", tuning().stall)))?
            .map_err(|e| Error::msg(e.without_url().to_string()))?;
        let status = response.status().as_u16();
        let total = match (ranged, status) {
            (false, 200) => response
                .content_length()
                .and_then(|n| i64::try_from(n).ok())
                .unwrap_or(-1),
            (true, 206) => range_total(
                response
                    .headers()
                    .get(reqwest::header::CONTENT_RANGE)
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or_default(),
            ),
            (true, 200) => {
                return Err(Error::Permanent(format!(
                    "server ignored Range at offset {offset}, can't resume"
                )));
            }
            (_, 429 | 500..=599) => return Err(Error::msg(format!("HTTP {status}"))),
            _ => return Err(Error::Permanent(format!("download {self}: HTTP {status}"))),
        };
        let stream = response
            .bytes_stream()
            .map_err(|e| std::io::Error::other(e.without_url()));
        Ok((Box::new(StreamReader::new(stream)), total))
    }
}

pub struct SshSource {
    pub user: String,
    pub addr: String,
    pub file: String,
    pub size: i64,
    pub key: std::sync::Arc<russh::keys::PrivateKey>,
    pub host_key: russh::keys::PublicKey,
}
impl fmt::Display for SshSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ssh://{}@{}/{}", self.user, self.addr, self.file)
    }
}
struct PinnedKey(russh::keys::PublicKey);
impl russh::client::Handler for PinnedKey {
    type Error = russh::Error;
    async fn check_server_key(
        &mut self,
        offered: &russh::keys::PublicKeyOrCertificate,
    ) -> std::result::Result<bool, Self::Error> {
        Ok(
            matches!(offered, russh::keys::PublicKeyOrCertificate::PublicKey { key, .. }
            if key.key_data() == self.0.key_data()),
        )
    }
}
// russh's Handle drop detaches its task. Own a socket shutdown guard so cancelling
// a handshake or dropping a model body also stops the connection's IO.
struct SocketGuard(std::net::TcpStream);
impl Drop for SocketGuard {
    fn drop(&mut self) {
        let _ = self.0.shutdown(std::net::Shutdown::Both);
    }
}
struct SshBody {
    stream: russh::ChannelStream<russh::client::Msg>,
    _socket: SocketGuard,
    _session: russh::client::Handle<PinnedKey>,
}
impl tokio::io::AsyncRead for SshBody {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.stream).poll_read(cx, buf)
    }
}
#[async_trait]
impl Source for SshSource {
    async fn open(&self, offset: i64, len: i64) -> Result<(BoxRead, i64)> {
        use russh::{
            client,
            keys::{Algorithm, HashAlg, PrivateKeyWithHashAlg},
        };
        use std::{borrow::Cow, sync::Arc};
        let tcp = tokio::time::timeout(tuning().stall, tokio::net::TcpStream::connect(&self.addr))
            .await
            .map_err(|_| Error::msg(format!("ssh {}: TCP connect timeout", self.addr)))??;
        let socket = tcp.into_std()?;
        let guard = SocketGuard(socket.try_clone()?);
        let tcp = tokio::net::TcpStream::from_std(socket)?;
        let keys = match self.host_key.algorithm() {
            Algorithm::Rsa { .. } => vec![
                Algorithm::Rsa {
                    hash: Some(HashAlg::Sha512),
                },
                Algorithm::Rsa {
                    hash: Some(HashAlg::Sha256),
                },
                Algorithm::Rsa { hash: None },
            ],
            algo => vec![algo],
        };
        let cfg = client::Config {
            preferred: russh::Preferred {
                key: Cow::Owned(keys),
                ..Default::default()
            },
            ..Default::default()
        };
        let connect = async {
            let mut session =
                client::connect_stream(Arc::new(cfg), tcp, PinnedKey(self.host_key.clone()))
                    .await
                    .map_err(|e| {
                        Error::Permanent(format!("ssh {}: host key / handshake: {e}", self.addr))
                    })?;
            let hash = session
                .best_supported_rsa_hash()
                .await
                .map_err(|e| Error::Permanent(format!("ssh {}: {e}", self.addr)))?
                .flatten();
            let auth = session
                .authenticate_publickey(
                    &self.user,
                    PrivateKeyWithHashAlg::new(self.key.clone(), hash),
                )
                .await
                .map_err(|e| Error::Permanent(format!("ssh {}: auth: {e}", self.addr)))?;
            if !auth.success() {
                return Err(Error::Permanent(format!(
                    "ssh {}: authentication failed",
                    self.addr
                )));
            }
            let mut channel = session
                .channel_open_session()
                .await
                .map_err(|e| Error::msg(format!("ssh {}: {e}", self.addr)))?;
            let mut command = format!("{} {offset}", self.file);
            if len >= 0 {
                command.push_str(&format!(" {len}"));
            }
            channel
                .exec(true, command)
                .await
                .map_err(|e| Error::msg(format!("ssh {}: {e}", self.addr)))?;
            // Wait for exec acceptance before exposing a body. Failure must not look like EOF.
            loop {
                match channel.wait().await {
                    Some(russh::ChannelMsg::Success) => break,
                    Some(russh::ChannelMsg::WindowAdjusted { .. }) => continue,
                    _ => return Err(Error::msg(format!("ssh {}: exec rejected", self.addr))),
                }
            }
            Ok((session, channel.into_stream()))
        };
        let (session, stream) = tokio::time::timeout(tuning().stall, connect)
            .await
            .map_err(|_| {
                Error::Permanent(format!("ssh {}: handshake/auth/exec timeout", self.addr))
            })??;
        Ok((
            Box::new(SshBody {
                stream,
                _socket: guard,
                _session: session,
            }),
            self.size,
        ))
    }
}
pub fn model_source(
    raw: &str,
    ssh_key_b64: &str,
    host_key_line: &str,
    file: &str,
    size: i64,
) -> Result<std::sync::Arc<dyn Source>> {
    use base64::Engine;
    use std::sync::Arc;
    let url = url::Url::parse(raw).map_err(|_| Error::msg("invalid model URL"))?;
    if url.scheme() != "ssh" {
        return Ok(Arc::new(HttpSource::new(raw)));
    }
    let host = url
        .host_str()
        .ok_or_else(|| Error::msg("ssh model URL: missing host"))?;
    let pem = base64::engine::general_purpose::STANDARD
        .decode(ssh_key_b64)
        .map_err(|e| Error::msg(format!("LOBO_MODEL_SSH_KEY: {e}")))?;
    let key = russh::keys::PrivateKey::from_openssh(&pem)
        .map_err(|e| Error::msg(format!("LOBO_MODEL_SSH_KEY: {e}")))?;
    let host_key = russh::keys::PublicKey::from_openssh(host_key_line)
        .map_err(|e| Error::msg(format!("LOBO_MODEL_SSH_HOSTKEY: {e}")))?;
    Ok(Arc::new(SshSource {
        user: url.username().into(),
        addr: format!("{host}:{}", url.port().unwrap_or(22)),
        file: file.into(),
        size,
        key: Arc::new(key),
        host_key,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::{Mock, MockServer, ResponseTemplate, matchers::any};
    #[test]
    fn range_total() {
        for (s, n) in [
            ("bytes 10-99/100", 100),
            ("", -1),
            ("bytes 10-99/*", -1),
            ("bytes 0-1/-1", -1),
        ] {
            assert_eq!(super::range_total(s), n);
        }
    }
    #[tokio::test]
    async fn http_source_errors_hide_secret_url() {
        let server = MockServer::start().await;
        Mock::given(any())
            .respond_with(ResponseTemplate::new(403))
            .mount(&server)
            .await;
        for base in [server.uri(), "http://127.0.0.1:1".into()] {
            let s = HttpSource::new(format!("{base}/tok3n/file?X-Amz-Signature=s3cret"));
            let e = match s.open(0, -1).await {
                Err(e) => e,
                Ok(_) => panic!("expected error"),
            };
            assert!(!e.to_string().contains("tok3n"));
            assert!(!e.to_string().contains("s3cret"));
            assert_eq!(s.to_string(), base);
        }
        assert_eq!(
            redact_url("https://user:secret@example.com:123/path?key=secret"),
            "https://example.com:123"
        );
    }
    #[tokio::test]
    async fn http_source_status_mapping() {
        for (status, offset, permanent) in [
            (404, 0, true),
            (503, 0, false),
            (429, 0, false),
            (200, 10, true),
        ] {
            let server = MockServer::start().await;
            Mock::given(any())
                .respond_with(ResponseTemplate::new(status))
                .mount(&server)
                .await;
            let e = match HttpSource::new(server.uri()).open(offset, -1).await {
                Err(e) => e,
                Ok(_) => panic!("expected error"),
            };
            assert_eq!(e.is_permanent(), permanent);
        }
    }
}

#[cfg(test)]
mod ssh_tests {
    use super::*;
    use crate::{
        download::{self, Tuning, with_tuning},
        testutil::*,
    };
    use sha2::{Digest, Sha256};
    use std::{sync::atomic::Ordering, time::Duration};
    use tokio::io::AsyncReadExt;
    use tokio_util::sync::CancellationToken;
    async fn source(
        n: usize,
    ) -> (
        SshSource,
        std::sync::Arc<Vec<u8>>,
        std::sync::Arc<std::sync::atomic::AtomicUsize>,
    ) {
        let data = data(n);
        let key = ssh_key();
        let (addr, host_key, calls) = fake_feesh(data.clone(), key.public_key().clone()).await;
        (
            SshSource {
                addr,
                user: "lobo".into(),
                file: "m".into(),
                size: n as i64,
                key,
                host_key,
            },
            data,
            calls,
        )
    }
    #[tokio::test]
    async fn ssh_resume() {
        let (s, bytes, calls) = source(1_000_000).await;
        let dir = tempfile::tempdir().unwrap();
        let dst = dir.path().join("m");
        within(
            Duration::from_secs(15),
            download::download(
                CancellationToken::new(),
                &s,
                &dst,
                &hex::encode(Sha256::digest(&*bytes)),
                None,
            ),
        )
        .await
        .unwrap();
        assert_eq!(std::fs::read(dst).unwrap(), *bytes);
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }
    #[tokio::test]
    async fn ssh_rejects_wrong_host_key() {
        let (mut s, _, calls) = source(100).await;
        s.host_key = ssh_key().public_key().clone();
        let Err(e) = s.open(0, -1).await else {
            panic!("accepted wrong key")
        };
        assert!(e.is_permanent());
        assert!(e.to_string().contains("host key"));
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }
    #[tokio::test]
    async fn ssh_picks_pinned_ed25519_over_ecdsa() {
        let (s, _, _) = source(100).await;
        s.open(0, -1).await.unwrap();
    }
    #[tokio::test]
    async fn ssh_silent_handshake() {
        let key = ssh_key();
        let s = SshSource {
            addr: silent_tcp().await,
            user: "u".into(),
            file: "m".into(),
            size: 1,
            host_key: key.public_key().clone(),
            key,
        };
        with_tuning(
            Tuning {
                stall: Duration::from_millis(200),
                chunk: 65536,
            },
            async {
                let Err(e) = within(Duration::from_secs(2), s.open(0, -1)).await else {
                    panic!("silent server accepted")
                };
                assert!(e.is_permanent());
                assert!(e.to_string().contains("timeout"));
            },
        )
        .await;
    }
    #[tokio::test]
    async fn parallel_ssh() {
        let (s, bytes, calls) = source(200_000).await;
        let dir = tempfile::tempdir().unwrap();
        let dst = dir.path().join("m");
        with_tuning(
            Tuning {
                stall: Duration::from_secs(2),
                chunk: 65536,
            },
            async {
                within(
                    Duration::from_secs(15),
                    download::download_parallel(
                        CancellationToken::new(),
                        &s,
                        &dst,
                        bytes.len() as i64,
                        &hex::encode(Sha256::digest(&*bytes)),
                        &chunk_table(&bytes, 65536),
                        3,
                        None,
                    ),
                )
                .await
                .unwrap();
            },
        )
        .await;
        assert_eq!(std::fs::read(dst).unwrap(), *bytes);
        assert!(calls.load(Ordering::SeqCst) >= 3);
    }
    #[test]
    fn model_source_parses() {
        use base64::Engine;
        assert_eq!(
            model_source("https://host/secret", "", "", "", 0)
                .unwrap()
                .to_string(),
            "https://host"
        );
        let key = ssh_key();
        let pem = key
            .to_openssh(russh::keys::ssh_key::LineEnding::LF)
            .unwrap();
        let encoded = base64::engine::general_purpose::STANDARD.encode(pem.as_bytes());
        let public = key.public_key().to_openssh().unwrap();
        assert_eq!(
            model_source("ssh://lobo@h", &encoded, &public, "m", 1)
                .unwrap()
                .to_string(),
            "ssh://lobo@h:22/m"
        );
        assert_eq!(
            model_source("ssh://lobo@[::1]", &encoded, &public, "m", 1)
                .unwrap()
                .to_string(),
            "ssh://lobo@[::1]:22/m"
        );
        for (key, host, msg) in [
            ("bad", public.as_str(), "LOBO_MODEL_SSH_KEY"),
            (encoded.as_str(), "bad", "LOBO_MODEL_SSH_HOSTKEY"),
        ] {
            let Err(e) = model_source("ssh://lobo@h", key, host, "m", 1) else {
                panic!("accepted invalid key")
            };
            assert!(e.to_string().contains(msg));
        }
    }
    #[tokio::test]
    async fn fake_feesh_serves_range() {
        let (s, bytes, _) = source(100).await;
        let (mut first, _) = s.open(0, -1).await.unwrap();
        let mut discard = Vec::new();
        first.read_to_end(&mut discard).await.unwrap();
        let (mut body, _) = s.open(5, 5).await.unwrap();
        let mut out = Vec::new();
        body.read_to_end(&mut out).await.unwrap();
        assert_eq!(out, bytes[5..10]);
    }
}
