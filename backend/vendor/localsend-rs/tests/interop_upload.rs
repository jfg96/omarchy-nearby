mod common;

use localsend_rs::server::{LocalSendServer, ServerEvent};
use localsend_rs::{DeviceInfo, LocalSendClient, Protocol, build_file_metadata, sha256_from_file};
use std::collections::HashMap;

#[tokio::test]
async fn uploads_a_file_byte_for_byte_rs_to_rs() {
    let save_dir = tempfile::tempdir().expect("save dir");
    let src_dir = tempfile::tempdir().expect("src dir");

    // --- receiver ---
    let (mut server, mut events) = LocalSendServer::builder()
        .alias("Test Receiver")
        .port(0)
        .save_dir(save_dir.path())
        .protocol(Protocol::Http)
        .auto_accept(true)
        .build()
        .await
        .expect("build");
    let port = server.port();

    common::wait_for_http_info(port).await;

    // --- sender ---
    let (file_path, want_sha) = common::make_random_file(src_dir.path(), "hello.bin", 1024);
    let meta = build_file_metadata(&file_path).await.expect("metadata");
    let file_id = meta.id.clone();
    let mut files = HashMap::new();
    files.insert(file_id.clone(), meta);

    let mut sender_dev = DeviceInfo::new("Test Sender".to_string(), 0, Protocol::Http);
    sender_dev.fingerprint = "sender-fp".to_string();
    let client = LocalSendClient::new(sender_dev);
    let target = common::target_device(port);

    let prep = client
        .prepare_upload(&target, files, None)
        .await
        .expect("prepare");
    let token = prep.files.get(&file_id).expect("token").clone();
    client
        .upload_file(
            &target,
            &prep.session_id,
            &file_id,
            &token,
            &file_path,
            None,
        )
        .await
        .expect("upload");

    let mut progress_samples = Vec::new();
    let mut saw_file_received = false;
    loop {
        let event = tokio::time::timeout(std::time::Duration::from_secs(2), events.recv())
            .await
            .expect("receiver event should arrive")
            .expect("receiver event stream should stay open");
        match event {
            ServerEvent::FileReceiveProgress {
                session_id,
                file_id: event_file_id,
                file_name,
                sender_alias,
                bytes_received,
                total_bytes,
                file_count,
            } => {
                assert_eq!(session_id, prep.session_id);
                assert_eq!(event_file_id, file_id);
                assert_eq!(file_name, "hello.bin");
                assert_eq!(sender_alias, "Test Sender");
                assert_eq!(total_bytes, 1_024);
                assert_eq!(file_count, 1);
                progress_samples.push(bytes_received);
            }
            ServerEvent::FileReceived { .. } => saw_file_received = true,
            ServerEvent::SessionCompleted { .. } => break,
            _ => {}
        }
    }
    assert!(!progress_samples.is_empty());
    assert!(progress_samples.windows(2).all(|pair| pair[0] <= pair[1]));
    assert_eq!(progress_samples.last(), Some(&1_024));
    assert!(saw_file_received);

    // --- assert byte-identical ---
    let got_sha = sha256_from_file(&save_dir.path().join("hello.bin"))
        .await
        .expect("saved file");
    assert_eq!(got_sha, want_sha);

    server.stop();
}

#[tokio::test]
async fn upload_completes_when_progress_event_channel_is_not_drained() {
    let save_dir = tempfile::tempdir().expect("save dir");
    let src_dir = tempfile::tempdir().expect("src dir");
    let (mut server, _unread_events) = LocalSendServer::builder()
        .alias("Backpressured Receiver")
        .port(0)
        .save_dir(save_dir.path())
        .protocol(Protocol::Http)
        .auto_accept(true)
        .build()
        .await
        .expect("build");
    let port = server.port();
    common::wait_for_http_info(port).await;

    let (file_path, want_sha) =
        common::make_random_file(src_dir.path(), "large.bin", 8 * 1024 * 1024);
    let meta = build_file_metadata(&file_path).await.expect("metadata");
    let file_id = meta.id.clone();
    let mut files = HashMap::new();
    files.insert(file_id.clone(), meta);

    let mut sender = DeviceInfo::new("Sender".into(), 0, Protocol::Http);
    sender.fingerprint = "sender-fp".into();
    let client = LocalSendClient::new(sender);
    let target = common::target_device(port);
    let prep = client
        .prepare_upload(&target, files, None)
        .await
        .expect("prepare");
    let token = prep.files.get(&file_id).expect("token").clone();

    tokio::time::timeout(
        std::time::Duration::from_secs(20),
        client.upload_file(
            &target,
            &prep.session_id,
            &file_id,
            &token,
            &file_path,
            None,
        ),
    )
    .await
    .expect("full progress channel must not block upload")
    .expect("upload");

    let got_sha = sha256_from_file(&save_dir.path().join("large.bin"))
        .await
        .expect("saved file");
    assert_eq!(got_sha, want_sha);
    server.stop();
}

#[tokio::test]
async fn sender_cancel_emits_cancelled_never_completed() {
    let save = tempfile::tempdir().unwrap();
    let (mut server, mut events) = LocalSendServer::builder()
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
    let client = LocalSendClient::new(DeviceInfo::new("S".into(), 0, Protocol::Http));
    let id = localsend_rs::FileId::new();
    let mut files = HashMap::new();
    files.insert(
        id.clone(),
        localsend_rs::FileMetadata {
            id: id.clone(),
            file_name: "cancel.bin".into(),
            size: 100,
            file_type: "application/octet-stream".into(),
            sha256: None,
            preview: None,
            metadata: None,
        },
    );
    let prep = client
        .prepare_upload(&common::target_device(port), files, None)
        .await
        .unwrap();
    client
        .cancel(&common::target_device(port), &prep.session_id)
        .await
        .unwrap();
    let event = tokio::time::timeout(std::time::Duration::from_secs(1), events.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(
        matches!(event,ServerEvent::SessionCancelled{session_id} if session_id==prep.session_id)
    );
    assert!(!save.path().join("cancel.bin").exists());
    server.stop();
}
