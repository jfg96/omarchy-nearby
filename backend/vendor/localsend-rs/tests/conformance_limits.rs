mod common;

use localsend_rs::Protocol;
use localsend_rs::server::{LocalSendServer, ServerEvent, ServerLimits};
use serde_json::json;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpSocket, TcpStream};
use tokio::time::timeout;

const SHORT: Duration = Duration::from_secs(1);
const LONG: Duration = Duration::from_secs(30);

fn limits() -> ServerLimits {
    ServerLimits {
        header_read_timeout: LONG,
        request_body_timeout: LONG,
        upload_idle_timeout: LONG,
        max_connections: 64,
        max_connections_per_ip: 16,
    }
}

async fn http_server(
    limits: ServerLimits,
    auto_accept: bool,
    save: &std::path::Path,
) -> (
    LocalSendServer,
    tokio::sync::mpsc::UnboundedReceiver<ServerEvent>,
    u16,
) {
    let (server, events) = LocalSendServer::builder()
        .alias("R")
        .port(0)
        .save_dir(save)
        .protocol(Protocol::Http)
        .auto_accept(auto_accept)
        .limits(limits)
        .build()
        .await
        .unwrap();
    let port = server.port();
    common::wait_for_http_info(port).await;
    (server, events, port)
}

/// Connect from a chosen loopback address so tests can act as distinct peers.
async fn connect_from(local: Ipv4Addr, port: u16) -> TcpStream {
    let socket = TcpSocket::new_v4().unwrap();
    socket.bind(SocketAddr::new(IpAddr::V4(local), 0)).unwrap();
    socket
        .connect(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port))
        .await
        .unwrap()
}

/// True when the server ends the connection within `wait`, reading and
/// discarding anything it sends first.
async fn closed_within<S: AsyncRead + Unpin>(stream: &mut S, wait: Duration) -> bool {
    let mut buffer = [0u8; 1024];
    timeout(wait, async {
        loop {
            match stream.read(&mut buffer).await {
                Ok(0) | Err(_) => return,
                Ok(_) => {}
            }
        }
    })
    .await
    .is_ok()
}

async fn status_line<S: AsyncRead + Unpin>(stream: S, wait: Duration) -> Option<String> {
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    match timeout(wait, reader.read_line(&mut line)).await {
        Ok(Ok(n)) if n > 0 => Some(line),
        _ => None,
    }
}

fn prepare_body(size: u64) -> serde_json::Value {
    json!({
        "info": { "alias": "raw", "version": "2.1", "deviceType": "headless",
                  "fingerprint": "fp", "port": 53317, "protocol": "http", "download": false },
        "files": { "f1": { "id": "f1", "fileName": "a.bin", "size": size,
                           "fileType": "application/octet-stream" } }
    })
}

async fn prepare(port: u16, size: u64) -> (String, String) {
    let response: serde_json::Value = reqwest::Client::new()
        .post(format!(
            "http://127.0.0.1:{port}/api/localsend/v2/prepare-upload"
        ))
        .json(&prepare_body(size))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    (
        response["sessionId"].as_str().unwrap().to_string(),
        response["files"]["f1"].as_str().unwrap().to_string(),
    )
}

#[tokio::test]
async fn idle_connection_is_closed_after_the_header_timeout() {
    let save = tempfile::tempdir().unwrap();
    let (_server, _events, port) = http_server(
        ServerLimits {
            header_read_timeout: SHORT,
            ..limits()
        },
        true,
        save.path(),
    )
    .await;

    let mut idle = connect_from(Ipv4Addr::LOCALHOST, port).await;
    assert!(
        closed_within(&mut idle, Duration::from_secs(4)).await,
        "an idle connection must not be held open"
    );
}

#[tokio::test]
async fn partial_request_head_is_closed_after_the_header_timeout() {
    let save = tempfile::tempdir().unwrap();
    let (_server, _events, port) = http_server(
        ServerLimits {
            header_read_timeout: SHORT,
            ..limits()
        },
        true,
        save.path(),
    )
    .await;

    let mut partial = connect_from(Ipv4Addr::LOCALHOST, port).await;
    partial
        .write_all(b"POST /api/localsend/v2/register HTTP/1.1\r\nHost: x\r\n")
        .await
        .unwrap();
    assert!(
        closed_within(&mut partial, Duration::from_secs(4)).await,
        "an unfinished request head must not be held open"
    );
}

/// A body that keeps trickling still has to finish within the body timeout.
#[tokio::test]
async fn slow_register_body_is_cut_at_the_body_deadline() {
    let save = tempfile::tempdir().unwrap();
    let (_server, _events, port) = http_server(
        ServerLimits {
            request_body_timeout: SHORT,
            ..limits()
        },
        true,
        save.path(),
    )
    .await;

    let stream = connect_from(Ipv4Addr::LOCALHOST, port).await;
    let (read_half, mut write_half) = stream.into_split();
    write_half
        .write_all(b"POST /api/localsend/v2/register HTTP/1.1\r\nHost: x\r\nContent-Type: application/json\r\nContent-Length: 4000\r\n\r\n")
        .await
        .unwrap();
    let trickle = tokio::spawn(async move {
        for _ in 0..40 {
            if write_half.write_all(b" ").await.is_err() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    });

    let status = status_line(read_half, Duration::from_secs(4))
        .await
        .expect("a trickled body must be answered before it completes");
    assert!(!status.starts_with("HTTP/1.1 200"), "{status:?}");
    trickle.abort();
}

/// The body timeout covers receiving the request, not the local user's
/// decision that follows it.
#[tokio::test]
async fn accept_decision_longer_than_the_body_timeout_still_succeeds() {
    let save = tempfile::tempdir().unwrap();
    let (_server, mut events, port) = http_server(
        ServerLimits {
            request_body_timeout: SHORT,
            header_read_timeout: SHORT,
            ..limits()
        },
        false,
        save.path(),
    )
    .await;

    let request = tokio::spawn(async move {
        reqwest::Client::new()
            .post(format!(
                "http://127.0.0.1:{port}/api/localsend/v2/prepare-upload"
            ))
            .json(&prepare_body(3))
            .send()
            .await
            .unwrap()
            .status()
    });
    let pending = loop {
        match timeout(Duration::from_secs(5), events.recv()).await {
            Ok(Some(ServerEvent::TransferRequest(pending))) => break pending,
            Ok(Some(_)) => continue,
            other => panic!("expected a transfer request, got {other:?}"),
        }
    };
    tokio::time::sleep(Duration::from_millis(2_500)).await;
    assert!(pending.accept());
    assert_eq!(request.await.unwrap(), 200);
}

#[tokio::test]
async fn stalled_upload_is_cut_after_the_idle_timeout() {
    let save = tempfile::tempdir().unwrap();
    let (_server, _events, port) = http_server(
        ServerLimits {
            upload_idle_timeout: SHORT,
            ..limits()
        },
        true,
        save.path(),
    )
    .await;
    let (session_id, token) = prepare(port, 10).await;

    let mut stream = connect_from(Ipv4Addr::LOCALHOST, port).await;
    stream
        .write_all(format!(
            "POST /api/localsend/v2/upload?sessionId={session_id}&fileId=f1&token={token} HTTP/1.1\r\nHost: x\r\nContent-Length: 10\r\n\r\nabc"
        ).as_bytes())
        .await
        .unwrap();

    let status = status_line(stream, Duration::from_secs(4))
        .await
        .expect("a stalled upload must be answered");
    assert!(!status.starts_with("HTTP/1.1 200"), "{status:?}");
    assert_eq!(std::fs::read_dir(save.path()).unwrap().count(), 0);
}

/// An upload that keeps making progress is never cut, even when it lasts
/// longer than both the idle and the whole-body timeouts.
#[tokio::test]
async fn slow_but_steady_upload_completes() {
    let save = tempfile::tempdir().unwrap();
    let (_server, _events, port) = http_server(
        ServerLimits {
            upload_idle_timeout: SHORT,
            request_body_timeout: SHORT,
            ..limits()
        },
        true,
        save.path(),
    )
    .await;
    let (session_id, token) = prepare(port, 8).await;

    let stream = connect_from(Ipv4Addr::LOCALHOST, port).await;
    let (read_half, mut write_half) = stream.into_split();
    write_half
        .write_all(format!(
            "POST /api/localsend/v2/upload?sessionId={session_id}&fileId=f1&token={token} HTTP/1.1\r\nHost: x\r\nContent-Length: 8\r\n\r\n"
        ).as_bytes())
        .await
        .unwrap();
    for byte in b"abcdefgh" {
        tokio::time::sleep(Duration::from_millis(400)).await;
        write_half.write_all(&[*byte]).await.unwrap();
    }

    let status = status_line(read_half, Duration::from_secs(5))
        .await
        .expect("a steady upload must complete");
    assert!(status.starts_with("HTTP/1.1 200"), "{status:?}");
    assert_eq!(
        std::fs::read(save.path().join("a.bin")).unwrap(),
        b"abcdefgh"
    );
}

#[tokio::test]
async fn connections_beyond_the_per_ip_limit_are_refused() {
    let save = tempfile::tempdir().unwrap();
    let (_server, _events, port) = http_server(
        ServerLimits {
            max_connections_per_ip: 2,
            ..limits()
        },
        true,
        save.path(),
    )
    .await;
    // Let the readiness probe's connection close before counting.
    tokio::time::sleep(Duration::from_millis(300)).await;

    let first = connect_from(Ipv4Addr::LOCALHOST, port).await;
    let mut second = connect_from(Ipv4Addr::LOCALHOST, port).await;
    let mut third = connect_from(Ipv4Addr::LOCALHOST, port).await;
    assert!(
        closed_within(&mut third, Duration::from_secs(2)).await,
        "a peer over its connection limit must be refused"
    );
    assert!(
        !closed_within(&mut second, Duration::from_millis(500)).await,
        "connections within the limit stay open"
    );

    let mut other_peer = connect_from(Ipv4Addr::new(127, 0, 0, 2), port).await;
    assert!(
        !closed_within(&mut other_peer, Duration::from_millis(500)).await,
        "another peer keeps its own allowance"
    );

    drop(first);
    tokio::time::sleep(Duration::from_millis(300)).await;
    let mut replacement = connect_from(Ipv4Addr::LOCALHOST, port).await;
    assert!(
        !closed_within(&mut replacement, Duration::from_millis(500)).await,
        "a closed connection frees its slot"
    );
}

#[tokio::test]
async fn connections_beyond_the_global_limit_are_refused() {
    let save = tempfile::tempdir().unwrap();
    let (_server, _events, port) = http_server(
        ServerLimits {
            max_connections: 2,
            ..limits()
        },
        true,
        save.path(),
    )
    .await;
    tokio::time::sleep(Duration::from_millis(300)).await;

    let _first = connect_from(Ipv4Addr::new(127, 0, 0, 1), port).await;
    let _second = connect_from(Ipv4Addr::new(127, 0, 0, 2), port).await;
    let mut third = connect_from(Ipv4Addr::new(127, 0, 0, 3), port).await;
    assert!(
        closed_within(&mut third, Duration::from_secs(2)).await,
        "connections over the global limit must be refused"
    );
}

#[cfg(feature = "https")]
mod https {
    use super::*;
    use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
    use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
    use rustls::{DigitallySignedStruct, SignatureScheme};
    use std::sync::Arc;

    /// Accepts any certificate: these tests exercise transport limits, not
    /// trust, which `interop_tls` covers.
    #[derive(Debug)]
    struct AnyCertificate;

    impl ServerCertVerifier for AnyCertificate {
        fn verify_server_cert(
            &self,
            _: &CertificateDer<'_>,
            _: &[CertificateDer<'_>],
            _: &ServerName<'_>,
            _: &[u8],
            _: UnixTime,
        ) -> Result<ServerCertVerified, rustls::Error> {
            Ok(ServerCertVerified::assertion())
        }

        fn verify_tls12_signature(
            &self,
            _: &[u8],
            _: &CertificateDer<'_>,
            _: &DigitallySignedStruct,
        ) -> Result<HandshakeSignatureValid, rustls::Error> {
            Ok(HandshakeSignatureValid::assertion())
        }

        fn verify_tls13_signature(
            &self,
            _: &[u8],
            _: &CertificateDer<'_>,
            _: &DigitallySignedStruct,
        ) -> Result<HandshakeSignatureValid, rustls::Error> {
            Ok(HandshakeSignatureValid::assertion())
        }

        fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
            rustls::crypto::ring::default_provider()
                .signature_verification_algorithms
                .supported_schemes()
        }
    }

    /// HTTPS negotiates HTTP/1.1 even with a client that prefers HTTP/2, and
    /// an idle TLS connection is closed by the header timeout.
    #[tokio::test]
    async fn https_negotiates_http1_and_closes_idle_connections() {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let save = tempfile::tempdir().unwrap();
        let (server, _events) = LocalSendServer::builder()
            .alias("R")
            .port(0)
            .save_dir(save.path())
            .protocol(Protocol::Https)
            .limits(ServerLimits {
                header_read_timeout: SHORT,
                ..limits()
            })
            .build()
            .await
            .unwrap();
        let port = server.port();

        let mut config = rustls::ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(AnyCertificate))
            .with_no_client_auth();
        config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
        let connector = tokio_rustls::TlsConnector::from(Arc::new(config));
        let mut tls = None;
        for _ in 0..50 {
            if let Ok(tcp) = TcpStream::connect(("127.0.0.1", port)).await
                && let Ok(stream) = connector
                    .connect(ServerName::try_from("localhost").unwrap(), tcp)
                    .await
            {
                tls = Some(stream);
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        let mut tls = tls.expect("TLS handshake");
        assert_eq!(
            tls.get_ref().1.alpn_protocol(),
            Some(&b"http/1.1"[..]),
            "the receiver must not negotiate HTTP/2"
        );
        assert!(
            closed_within(&mut tls, Duration::from_secs(4)).await,
            "an idle TLS connection must not be held open"
        );
    }
}
