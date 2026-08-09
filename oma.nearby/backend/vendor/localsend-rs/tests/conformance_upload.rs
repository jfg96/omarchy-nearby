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
    assert_rejected_upload_only_emits_rolled_back_progress(&mut events);
}
