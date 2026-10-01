mod common;

use localsend_rs::Protocol;
use localsend_rs::server::{LocalSendServer, ServerEvent};
use localsend_rs::sha256_from_bytes;
use serde_json::json;

fn assert_rejected_upload_only_emits_rolled_back_progress(
    events: &mut tokio::sync::mpsc::UnboundedReceiver<ServerEvent>,
) {
    let mut samples = Vec::new();
    let mut saw_failure = false;
    while let Ok(event) = events.try_recv() {
        match event {
            ServerEvent::FileReceiveProgress { bytes_received, .. } => samples.push(bytes_received),
            ServerEvent::FileReceived { .. } | ServerEvent::SessionCompleted { .. } => {
                panic!("rejected upload must not emit completion events")
            }
            ServerEvent::SessionFailed { .. } => saw_failure = true,
            other => panic!("unexpected event for rejected upload: {other:?}"),
        }
    }
    assert!(
        !samples.is_empty(),
        "upload should have reported raw progress"
    );
    assert_eq!(samples.last(), Some(&0), "rejected bytes must roll back");
    assert!(saw_failure, "receiver must publish a terminal failure");
}

/// Drive `prepare-upload` over raw reqwest and return `(session_id, token)`
/// for the single offered file id `f1`.
async fn prepare_single(port: u16, size: u64, sha256: Option<String>) -> (String, String) {
    let mut file: serde_json::Value = json!({
        "id": "f1",
        "fileName": "big.bin",
        "size": size,
        "fileType": "application/octet-stream",
    });
    if let Some(sha) = sha256 {
        file["sha256"] = json!(sha);
    }
    let body = json!({
        "info": { "alias": "raw", "version": "2.1", "deviceType": "headless",
                  "fingerprint": "fp", "port": 53317, "protocol": "http", "download": false },
        "files": { "f1": file }
    });
    let resp: serde_json::Value = reqwest::Client::new()
        .post(format!(
            "http://127.0.0.1:{port}/api/localsend/v2/prepare-upload"
        ))
        .json(&body)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let session_id = resp["sessionId"].as_str().unwrap().to_string();
    let token = resp["files"]["f1"].as_str().unwrap().to_string();
    (session_id, token)
}

/// A body shorter than the declared size (truncated transfer / misbehaving
/// client) must be rejected: 500, the partial file is discarded, and the
/// session is NOT completed (no FileReceived / SessionDone).
#[tokio::test]
async fn short_body_is_rejected_and_partial_discarded() {
    let save = tempfile::tempdir().unwrap();
    let (server, mut events) = LocalSendServer::builder()
        .alias("R")
        .port(0)
        .save_dir(save.path())
        .protocol(Protocol::Http)
        .auto_accept(true)
        .build()
        .await
        .unwrap();
    let port = server.port();
    common::wait_for_http_info(port).await;

    // Declare 200 bytes but send only 10.
    let (session_id, token) = prepare_single(port, 200, None).await;

    let r = reqwest::Client::new()
        .post(format!(
            "http://127.0.0.1:{port}/api/localsend/v2/upload?sessionId={session_id}&fileId=f1&token={token}"
        ))
        .body(vec![0u8; 10])
        .send()
        .await
        .unwrap();

    // Wire: the receiver reports failure, not success.
    assert_eq!(r.status(), 500);

    // The partial file must not linger on disk.
    assert!(
        !save.path().join("big.bin").exists(),
        "partial upload must be deleted"
    );

    assert_eq!(
        std::fs::read_dir(save.path()).unwrap().count(),
        0,
        "rejected upload must leave no partial or final files"
    );
    assert_rejected_upload_only_emits_rolled_back_progress(&mut events);
}

/// When the metadata carries a sha256, a full-length body whose contents
/// hash to a different digest must be rejected the same way.
#[tokio::test]
async fn sha256_mismatch_is_rejected_and_partial_discarded() {
    let save = tempfile::tempdir().unwrap();
    let (server, mut events) = LocalSendServer::builder()
        .alias("R")
        .port(0)
        .save_dir(save.path())
        .protocol(Protocol::Http)
        .auto_accept(true)
        .build()
        .await
        .unwrap();
    let port = server.port();
    common::wait_for_http_info(port).await;

    // Declare the sha256 of all-zero bytes, but send all-one bytes of the
    // same length: size matches, digest does not.
    let declared = sha256_from_bytes(&[0u8; 64]);
    let (session_id, token) = prepare_single(port, 64, Some(declared)).await;

    let r = reqwest::Client::new()
        .post(format!(
            "http://127.0.0.1:{port}/api/localsend/v2/upload?sessionId={session_id}&fileId=f1&token={token}"
        ))
        .body(vec![1u8; 64])
        .send()
        .await
        .unwrap();

    assert_eq!(r.status(), 422);
    assert!(
        !save.path().join("big.bin").exists(),
        "partial upload must be deleted"
    );
    assert_eq!(
        std::fs::read_dir(save.path()).unwrap().count(),
        0,
        "rejected upload must leave no partial or final files"
    );
    assert_rejected_upload_only_emits_rolled_back_progress(&mut events);
}

/// Reject excess bytes before EOF, including a sender that keeps a much larger
/// HTTP body open. Previously the receiver waited for the entire body and kept
/// writing it to disk before checking the negotiated size.
#[tokio::test]
async fn oversized_body_is_rejected_before_eof_and_partial_discarded() {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    use tokio::time::{Duration, timeout};

    let save = tempfile::tempdir().unwrap();
    let (server, mut events) = LocalSendServer::builder()
        .alias("R")
        .port(0)
        .save_dir(save.path())
        .protocol(Protocol::Http)
        .auto_accept(true)
        .build()
        .await
        .unwrap();
    let port = server.port();
    let (session_id, token) = prepare_single(port, 3, None).await;
    let mut socket = tokio::net::TcpStream::connect(("127.0.0.1", port))
        .await
        .unwrap();
    let headers = format!(
        "POST /api/localsend/v2/upload?sessionId={session_id}&fileId=f1&token={token} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Length: 1048576\r\nConnection: close\r\n\r\n"
    );
    socket.write_all(headers.as_bytes()).await.unwrap();
    socket.write_all(b"abc").await.unwrap();

    // Wait for the accepted chunk to be written so the excess arrives in a
    // later chunk and exercises progress rollback as well as early rejection.
    let progress = timeout(Duration::from_secs(5), events.recv())
        .await
        .expect("accepted chunk must report progress")
        .unwrap();
    assert!(matches!(
        progress,
        ServerEvent::FileReceiveProgress {
            bytes_received: 3,
            ..
        }
    ));
    socket.write_all(b"d").await.unwrap();
    // Do not finish the advertised HTTP body or close the sending half.
    let mut response = BufReader::new(socket);
    let mut status_line = String::new();
    timeout(Duration::from_secs(5), response.read_line(&mut status_line))
        .await
        .expect("oversized upload must be rejected before EOF")
        .unwrap();
    assert!(status_line.starts_with("HTTP/1.1 500"), "{status_line:?}");

    assert_eq!(
        std::fs::read_dir(save.path()).unwrap().count(),
        0,
        "neither the final file nor a .part may remain"
    );
    assert_rejected_upload_only_emits_rolled_back_progress(&mut events);

    // A rejected upload terminates its session; its old token cannot retry.
    let rejected_retry = reqwest::Client::new()
        .post(format!(
            "http://127.0.0.1:{port}/api/localsend/v2/upload?sessionId={session_id}&fileId=f1&token={token}"
        ))
        .body("abc")
        .send()
        .await
        .unwrap();
    assert_eq!(rejected_retry.status(), 403);

    // The receiver remains available for a fresh, exact-size transfer.
    let (next_session, next_token) = prepare_single(port, 3, None).await;
    let accepted = reqwest::Client::new()
        .post(format!(
            "http://127.0.0.1:{port}/api/localsend/v2/upload?sessionId={next_session}&fileId=f1&token={next_token}"
        ))
        .body("abc")
        .send()
        .await
        .unwrap();
    assert_eq!(accepted.status(), 200);
    assert_eq!(std::fs::read(save.path().join("big.bin")).unwrap(), b"abc");
}

/// Drive `prepare-upload` for `(id, file name, size)` entries and return the
/// session id and per-file tokens.
async fn prepare_files(
    port: u16,
    files: &[(&str, &str, u64)],
) -> (String, std::collections::HashMap<String, String>) {
    let offered: serde_json::Map<String, serde_json::Value> = files
        .iter()
        .map(|(id, name, size)| {
            (
                id.to_string(),
                json!({ "id": id, "fileName": name, "size": size,
                        "fileType": "application/octet-stream" }),
            )
        })
        .collect();
    let body = json!({
        "info": { "alias": "raw", "version": "2.1", "deviceType": "headless",
                  "fingerprint": "fp", "port": 53317, "protocol": "http", "download": false },
        "files": offered
    });
    let resp: serde_json::Value = reqwest::Client::new()
        .post(format!(
            "http://127.0.0.1:{port}/api/localsend/v2/prepare-upload"
        ))
        .json(&body)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let tokens = resp["files"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(id, token)| (id.clone(), token.as_str().unwrap().to_string()))
        .collect();
    (resp["sessionId"].as_str().unwrap().to_string(), tokens)
}

async fn upload(
    port: u16,
    session_id: &str,
    file_id: &str,
    token: &str,
    body: &'static [u8],
) -> reqwest::StatusCode {
    // File ids in these tests only need `/` escaped for the query string.
    let file_id = file_id.replace('/', "%2F");
    reqwest::Client::new()
        .post(format!(
            "http://127.0.0.1:{port}/api/localsend/v2/upload?sessionId={session_id}&fileId={file_id}&token={token}"
        ))
        .body(body)
        .send()
        .await
        .unwrap()
        .status()
}

async fn auto_accept_server(
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
        .auto_accept(true)
        .build()
        .await
        .unwrap();
    let port = server.port();
    common::wait_for_http_info(port).await;
    (server, events, port)
}

fn saved_names(save: &std::path::Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(save)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

/// A received file's token must not authorize another upload while the rest
/// of the session is pending. Previously each repeat was saved as a new copy,
/// letting an accepted sender consume disk without bound.
#[tokio::test]
async fn completed_file_cannot_be_uploaded_again() {
    let save = tempfile::tempdir().unwrap();
    let (_server, _events, port) = auto_accept_server(save.path()).await;
    let (session_id, tokens) = prepare_files(port, &[("f1", "a.bin", 3), ("f2", "b.bin", 3)]).await;

    assert_eq!(
        upload(port, &session_id, "f1", &tokens["f1"], b"abc").await,
        200
    );
    assert_eq!(
        upload(port, &session_id, "f1", &tokens["f1"], b"abc").await,
        409,
        "a completed file must not be accepted again"
    );
    assert_eq!(saved_names(save.path()), vec!["a.bin"]);

    // The rejected repeat does not disturb the rest of the session.
    assert_eq!(
        upload(port, &session_id, "f2", &tokens["f2"], b"xyz").await,
        200
    );
    assert_eq!(saved_names(save.path()), vec!["a.bin", "b.bin"]);
}

/// Only one upload may own a file at a time. Previously a second request
/// replaced the first upload's partial file, so the first could validate and
/// publish bytes it had not written.
#[tokio::test]
async fn concurrent_upload_of_the_same_file_is_rejected() {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    use tokio::time::{Duration, timeout};

    let save = tempfile::tempdir().unwrap();
    let (_server, mut events, port) = auto_accept_server(save.path()).await;
    let (session_id, tokens) = prepare_files(port, &[("f1", "a.bin", 3)]).await;
    let token = &tokens["f1"];

    let mut socket = tokio::net::TcpStream::connect(("127.0.0.1", port))
        .await
        .unwrap();
    let headers = format!(
        "POST /api/localsend/v2/upload?sessionId={session_id}&fileId=f1&token={token} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Length: 3\r\nConnection: close\r\n\r\n"
    );
    socket.write_all(headers.as_bytes()).await.unwrap();
    socket.write_all(b"a").await.unwrap();
    let progress = timeout(Duration::from_secs(5), events.recv())
        .await
        .expect("first upload must start writing")
        .unwrap();
    assert!(matches!(
        progress,
        ServerEvent::FileReceiveProgress {
            bytes_received: 1,
            ..
        }
    ));

    assert_eq!(
        upload(port, &session_id, "f1", token, b"xyz").await,
        409,
        "a second upload must not take over a file in progress"
    );

    socket.write_all(b"bc").await.unwrap();
    let mut response = BufReader::new(socket);
    let mut status_line = String::new();
    timeout(Duration::from_secs(5), response.read_line(&mut status_line))
        .await
        .expect("first upload must finish")
        .unwrap();
    assert!(status_line.starts_with("HTTP/1.1 200"), "{status_line:?}");
    assert_eq!(saved_names(save.path()), vec!["a.bin"]);
    assert_eq!(std::fs::read(save.path().join("a.bin")).unwrap(), b"abc");
}

/// Remote file ids are opaque protocol values, not path components. A file id
/// containing a slash must neither shape the partial path nor break receipt.
#[tokio::test]
async fn remote_file_id_does_not_shape_the_partial_path() {
    let save = tempfile::tempdir().unwrap();
    let (_server, _events, port) = auto_accept_server(save.path()).await;
    let (session_id, tokens) = prepare_files(port, &[("nested/../f1", "a.bin", 3)]).await;

    assert_eq!(
        upload(
            port,
            &session_id,
            "nested/../f1",
            &tokens["nested/../f1"],
            b"abc"
        )
        .await,
        200
    );
    assert_eq!(saved_names(save.path()), vec!["a.bin"]);
}

/// Cancelling a session stops its upload at the next chunk. Previously the
/// writer kept consuming and storing the body until EOF after the cancel.
#[tokio::test]
async fn cancel_stops_an_upload_in_progress() {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    use tokio::time::{Duration, timeout};

    let save = tempfile::tempdir().unwrap();
    let (_server, mut events, port) = auto_accept_server(save.path()).await;
    let (session_id, tokens) = prepare_files(port, &[("f1", "a.bin", 1_048_576)]).await;
    let token = &tokens["f1"];

    let mut socket = tokio::net::TcpStream::connect(("127.0.0.1", port))
        .await
        .unwrap();
    let headers = format!(
        "POST /api/localsend/v2/upload?sessionId={session_id}&fileId=f1&token={token} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Length: 1048576\r\nConnection: close\r\n\r\n"
    );
    socket.write_all(headers.as_bytes()).await.unwrap();
    socket.write_all(b"abc").await.unwrap();
    let progress = timeout(Duration::from_secs(5), events.recv())
        .await
        .expect("upload must start writing")
        .unwrap();
    assert!(matches!(
        progress,
        ServerEvent::FileReceiveProgress {
            bytes_received: 3,
            ..
        }
    ));

    let cancelled = reqwest::Client::new()
        .post(format!(
            "http://127.0.0.1:{port}/api/localsend/v2/cancel?sessionId={session_id}"
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(cancelled.status(), 200);

    // The next chunk must end the upload without waiting for the rest of the
    // advertised body.
    socket.write_all(b"d").await.unwrap();
    let mut response = BufReader::new(socket);
    let mut status_line = String::new();
    timeout(Duration::from_secs(5), response.read_line(&mut status_line))
        .await
        .expect("a cancelled upload must stop before EOF")
        .unwrap();
    assert!(!status_line.starts_with("HTTP/1.1 200"), "{status_line:?}");
    assert!(
        saved_names(save.path()).is_empty(),
        "neither the final file nor a .part may remain"
    );
}
