//! A LAN peer answers our client's requests. Whatever it sends, or does not
//! send, must not hold a request open forever or make us buffer an unbounded
//! response.

use localsend_rs::client::{ClientLimits, LocalSendClient};
use localsend_rs::protocol::{FileId, FileMetadata, SessionId};
use localsend_rs::{DeviceInfo, HttpDiscovery, Protocol};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::timeout;

/// Stop a flooding peer after this many bytes so a client without a limit
/// fails the test instead of buffering forever.
const FLOOD_CAP: u64 = 256 * 1024 * 1024;
/// Far above any response limit plus what loopback socket buffers can absorb,
/// far below what an unbounded reader takes before the flood ends.
const FLOOD_ALLOWANCE: u64 = 16 * 1024 * 1024;
const OUTER: Duration = Duration::from_secs(20);

#[derive(Clone, Copy)]
enum Peer {
    /// Reads the request and never answers.
    Silent,
    /// Answers 200 with an endless chunked JSON body.
    Flood,
}

async fn read_request_head(stream: &mut TcpStream) {
    let mut head = Vec::new();
    let mut byte = [0u8; 1];
    while !head.ends_with(b"\r\n\r\n") && head.len() < 64 * 1024 {
        match stream.read(&mut byte).await {
            Ok(1) => head.push(byte[0]),
            _ => return,
        }
    }
}

async fn flood(stream: &mut TcpStream, sent: &AtomicU64) {
    let head =
        b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nTransfer-Encoding: chunked\r\n\r\n";
    if stream.write_all(head).await.is_err() {
        return;
    }
    let payload = vec![b' '; 64 * 1024];
    let mut chunk = format!("{:x}\r\n", payload.len()).into_bytes();
    chunk.extend_from_slice(&payload);
    chunk.extend_from_slice(b"\r\n");
    let mut opening = b"1\r\n{\r\n".to_vec();
    opening.extend_from_slice(&chunk);
    if stream.write_all(&opening).await.is_err() {
        return;
    }
    sent.fetch_add(payload.len() as u64 + 1, Ordering::Relaxed);
    while sent.load(Ordering::Relaxed) < FLOOD_CAP {
        if stream.write_all(&chunk).await.is_err() {
            return;
        }
        sent.fetch_add(payload.len() as u64, Ordering::Relaxed);
    }
}

/// Start a fake peer and return its address and the response bytes it sent.
async fn fake_peer(mode: Peer) -> (SocketAddr, Arc<AtomicU64>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let sent = Arc::new(AtomicU64::new(0));
    let counter = sent.clone();
    tokio::spawn(async move {
        while let Ok((mut stream, _)) = listener.accept().await {
            let counter = counter.clone();
            tokio::spawn(async move {
                read_request_head(&mut stream).await;
                match mode {
                    Peer::Silent => {
                        tokio::time::sleep(Duration::from_secs(3600)).await;
                    }
                    Peer::Flood => flood(&mut stream, &counter).await,
                }
            });
        }
    });
    (address, sent)
}

fn target(address: SocketAddr) -> DeviceInfo {
    let mut device = DeviceInfo::new("Peer".into(), address.port(), Protocol::Http);
    device.fingerprint = "peer".into();
    device.ip = Some(address.ip().to_string());
    device
}

fn client() -> LocalSendClient {
    LocalSendClient::new(DeviceInfo::new("Nearby".into(), 53317, Protocol::Http)).with_limits(
        ClientLimits {
            control_timeout: Duration::from_secs(1),
            ..ClientLimits::default()
        },
    )
}

fn offer() -> HashMap<FileId, FileMetadata> {
    let id = FileId::new();
    HashMap::from([(
        id.clone(),
        FileMetadata {
            id,
            file_name: "a.txt".into(),
            size: 1,
            file_type: "text/plain".into(),
            sha256: None,
            preview: None,
            metadata: None,
        },
    )])
}

#[tokio::test]
async fn register_gives_up_on_a_peer_that_never_answers() {
    let (address, _) = fake_peer(Peer::Silent).await;
    let result = timeout(OUTER, client().register(&target(address)))
        .await
        .expect("register must not wait for a silent peer indefinitely");
    assert!(result.is_err());
}

#[tokio::test]
async fn cancel_gives_up_on_a_peer_that_never_answers() {
    let (address, _) = fake_peer(Peer::Silent).await;
    let session = SessionId::from_string("session".into());
    let result = timeout(OUTER, client().cancel(&target(address), &session))
        .await
        .expect("cancel must not wait for a silent peer indefinitely");
    assert!(result.is_err());
}

#[tokio::test]
async fn register_stops_reading_an_oversized_response() {
    let (address, sent) = fake_peer(Peer::Flood).await;
    let result = timeout(OUTER, client().register(&target(address)))
        .await
        .expect("register must stop reading an endless response");
    assert!(
        result.is_err(),
        "an oversized registration response must be rejected"
    );
    let sent = sent.load(Ordering::Relaxed);
    assert!(
        sent < FLOOD_ALLOWANCE,
        "register kept reading {sent} response bytes"
    );
}

#[tokio::test]
async fn prepare_upload_stops_reading_an_oversized_response() {
    let (address, sent) = fake_peer(Peer::Flood).await;
    let result = timeout(
        OUTER,
        client().prepare_upload(&target(address), offer(), None),
    )
    .await
    .expect("prepare-upload must stop reading an endless response");
    assert!(
        result.is_err(),
        "an oversized prepare-upload response must be rejected"
    );
    let sent = sent.load(Ordering::Relaxed);
    assert!(
        sent < FLOOD_ALLOWANCE,
        "prepare-upload kept reading {sent} response bytes"
    );
}

#[tokio::test]
async fn discovery_probe_stops_reading_an_oversized_response() {
    let (address, sent) = fake_peer(Peer::Flood).await;
    let scanner = HttpDiscovery::new("Nearby".into(), 53317, Protocol::Http).unwrap();
    let mut found = Vec::new();
    timeout(
        OUTER,
        scanner.scan_devices_incremental_with_limit(vec![target(address)], 1, |device| {
            found.push(device)
        }),
    )
    .await
    .expect("a probe must finish")
    .unwrap();
    assert!(
        found.is_empty(),
        "an oversized response is not a LocalSend peer"
    );
    let sent = sent.load(Ordering::Relaxed);
    assert!(
        sent < FLOOD_ALLOWANCE,
        "the discovery probe kept reading {sent} response bytes"
    );
}
