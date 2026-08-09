mod common;

use localsend_rs::server::{LocalSendServer, ServerEvent};
use localsend_rs::{DeviceInfo, LocalSendClient, LocalSendError, Protocol, build_file_metadata};
use std::collections::HashMap;
use std::time::Duration;

async fn start_receiver(
    save_dir: std::path::PathBuf,
) -> (
    LocalSendServer,
    tokio::sync::mpsc::UnboundedReceiver<ServerEvent>,
) {
    LocalSendServer::builder()
        .alias("Receiver")
        .port(0)
        .save_dir(save_dir)
        .protocol(Protocol::Http)
        .auto_accept(false)
        .build()
        .await
        .expect("build")
}

fn one_file(
    dir: &std::path::Path,
) -> (
    HashMap<localsend_rs::FileId, localsend_rs::FileMetadata>,
    localsend_rs::FileId,
    std::path::PathBuf,
) {
    let (path, _) = common::make_random_file(dir, "a.bin", 512);
    let meta = futures_blocking(build_file_metadata(&path));
    let id = meta.id.clone();
    let mut m = HashMap::new();
    m.insert(id.clone(), meta);
    (m, id, path)
}

// tiny helper: run an async fn to completion inside a test-owned runtime piece
fn futures_blocking<T>(fut: impl std::future::Future<Output = localsend_rs::Result<T>>) -> T {
    tokio::task::block_in_place(|| tokio::runtime::Handle::current().block_on(fut)).unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn event_consumer_can_accept_a_transfer() {
    let save = tempfile::tempdir().unwrap();
    let src = tempfile::tempdir().unwrap();
    let (mut server, mut events) = start_receiver(save.path().to_path_buf()).await;
    let port = server.port();
    common::wait_for_http_info(port).await;

    tokio::spawn(async move {
        while let Some(ev) = events.recv().await {
            if let ServerEvent::TransferRequest(req) = ev {
                assert_eq!(req.sender().alias, "Sender");
                req.accept();
            }
        }
    });

    let (files, id, path) = one_file(src.path());
    let mut dev = DeviceInfo::new("Sender".to_string(), 0, Protocol::Http);
    dev.fingerprint = "sender-fp".to_string();
    let client = LocalSendClient::new(dev);
    let target = common::target_device(port);
    let prep = client
        .prepare_upload(&target, files, None)
        .await
        .expect("accepted");
    let token = prep.files.get(&id).unwrap().clone();
    client
        .upload_file(&target, &prep.session_id, &id, &token, &path, None)
        .await
        .unwrap();
    assert!(save.path().join("a.bin").exists());
    server.stop();
}

#[tokio::test(flavor = "multi_thread")]
async fn event_consumer_can_decline_a_transfer() {
    let save = tempfile::tempdir().unwrap();
    let src = tempfile::tempdir().unwrap();
    let (mut server, mut events) = start_receiver(save.path().to_path_buf()).await;
    let port = server.port();
    common::wait_for_http_info(port).await;

    tokio::spawn(async move {
        while let Some(ev) = events.recv().await {
            if let ServerEvent::TransferRequest(req) = ev {
                req.decline();
            }
        }
    });

    let (files, _id, _path) = one_file(src.path());
    let mut dev = DeviceInfo::new("Sender".to_string(), 0, Protocol::Http);
    dev.fingerprint = "sender-fp".to_string();
    let client = LocalSendClient::new(dev);
    let target = common::target_device(port);
    let err = client
        .prepare_upload(&target, files, None)
        .await
        .expect_err("declined");
    assert!(matches!(err, LocalSendError::Rejected { status: 403 }));
    server.stop();
}

#[tokio::test(flavor = "multi_thread")]
async fn unanswered_request_expires_with_same_identity_and_cannot_late_accept() {
    let save = tempfile::tempdir().unwrap();
    let (mut server, mut events) = LocalSendServer::builder()
        .alias("Receiver")
        .port(0)
        .save_dir(save.path())
        .protocol(Protocol::Http)
        .auto_accept(false)
        .accept_timeout(Duration::from_millis(80))
        .build()
        .await
        .unwrap();
    let port = server.port();
    common::wait_for_http_info(port).await;
    let request = tokio::spawn(async move {
        reqwest::Client::new().post(format!("http://127.0.0.1:{port}/api/localsend/v2/prepare-upload"))
            .json(&serde_json::json!({"info":{"alias":"Late","version":"2.1","fingerprint":"late","port":53317,"protocol":"http","download":false},"files":{"f":{"id":"f","fileName":"late.bin","size":1,"fileType":"application/octet-stream"}}}))
            .send().await.unwrap().status()
    });
    let pending = match events.recv().await.unwrap() {
        ServerEvent::TransferRequest(req) => req,
        other => panic!("unexpected {other:?}"),
    };
    let request_id = pending.request_id().to_string();
    let expired = loop {
        match events.recv().await.unwrap() {
            ServerEvent::TransferRequestExpired { request_id: id } => break id,
            _ => {}
        }
    };
    assert_eq!(expired, request_id);
    assert!(!pending.accept(), "late accept must not report success");
    assert_eq!(request.await.unwrap(), reqwest::StatusCode::FORBIDDEN);
    server.stop();
}

/// Per-file accept: the consumer answers `accept_files` with a subset, and the
/// server issues tokens only for the accepted ids (this is the exact library
/// path the TUI's interactive confirm popup drives). The skipped file gets no
/// token and never lands on disk.
#[tokio::test(flavor = "multi_thread")]
async fn event_consumer_can_accept_a_subset_of_files() {
    let save = tempfile::tempdir().unwrap();
    let src = tempfile::tempdir().unwrap();
    let (mut server, mut events) = start_receiver(save.path().to_path_buf()).await;
    let port = server.port();
    common::wait_for_http_info(port).await;

    let (keep_path, _) = common::make_random_file(src.path(), "keep.bin", 256);
    let (skip_path, _) = common::make_random_file(src.path(), "skip.bin", 256);
    let keep_meta = build_file_metadata(&keep_path).await.unwrap();
    let skip_meta = build_file_metadata(&skip_path).await.unwrap();
    let keep_id = keep_meta.id.clone();
    let skip_id = skip_meta.id.clone();
    let mut files = HashMap::new();
    files.insert(keep_meta.id.clone(), keep_meta);
    files.insert(skip_meta.id.clone(), skip_meta);

    let keep_for_task = keep_id.clone();
    tokio::spawn(async move {
        while let Some(ev) = events.recv().await {
            if let ServerEvent::TransferRequest(req) = ev {
                req.accept_files(vec![keep_for_task.clone()]);
            }
        }
    });

    let mut dev = DeviceInfo::new("Sender".to_string(), 0, Protocol::Http);
    dev.fingerprint = "sender-fp".to_string();
    let client = LocalSendClient::new(dev);
    let target = common::target_device(port);
    let prep = client
        .prepare_upload(&target, files, None)
        .await
        .expect("subset accepted");

    assert_eq!(prep.files.len(), 1, "only the kept file should get a token");
    assert!(prep.files.contains_key(&keep_id));
    assert!(!prep.files.contains_key(&skip_id));

    let token = prep.files.get(&keep_id).unwrap().clone();
    client
        .upload_file(
            &target,
            &prep.session_id,
            &keep_id,
            &token,
            &keep_path,
            None,
        )
        .await
        .unwrap();
    assert!(save.path().join("keep.bin").exists());
    assert!(!save.path().join("skip.bin").exists());
    server.stop();
}

/// The auto-accept flag is live: flipping it on a already-running server via
/// `set_auto_accept(true)` makes the handler auto-accept, so a consumer that
/// would otherwise decline never even sees the request. This is what the TUI's
/// Settings toggle relies on.
#[tokio::test(flavor = "multi_thread")]
async fn set_auto_accept_true_takes_effect_live() {
    let save = tempfile::tempdir().unwrap();
    let src = tempfile::tempdir().unwrap();
    // Starts with auto_accept(false).
    let (mut server, mut events) = start_receiver(save.path().to_path_buf()).await;
    let port = server.port();
    common::wait_for_http_info(port).await;

    // A consumer that declines everything. If the live toggle works, no
    // TransferRequest is ever emitted, so this never fires.
    tokio::spawn(async move {
        while let Some(ev) = events.recv().await {
            if let ServerEvent::TransferRequest(req) = ev {
                req.decline();
            }
        }
    });

    // Flip the running server on.
    server.set_auto_accept(true);
    assert!(server.auto_accept());

    let (files, id, path) = one_file(src.path());
    let mut dev = DeviceInfo::new("Sender".to_string(), 0, Protocol::Http);
    dev.fingerprint = "sender-fp".to_string();
    let client = LocalSendClient::new(dev);
    let target = common::target_device(port);
    let prep = client
        .prepare_upload(&target, files, None)
        .await
        .expect("auto-accept toggled on → accepted despite the declining consumer");
    let token = prep.files.get(&id).unwrap().clone();
    client
        .upload_file(&target, &prep.session_id, &id, &token, &path, None)
        .await
        .unwrap();
    assert!(save.path().join("a.bin").exists());
    server.stop();
}
