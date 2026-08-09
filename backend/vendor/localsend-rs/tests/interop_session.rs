mod common;

use localsend_rs::server::LocalSendServer;
use localsend_rs::{DeviceInfo, LocalSendClient, Protocol, build_file_metadata};
use std::collections::HashMap;

async fn receiver(save: &std::path::Path) -> (LocalSendServer, u16) {
    let (server, _events) = LocalSendServer::builder()
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
    (server, port)
}

fn client() -> LocalSendClient {
    let mut d = DeviceInfo::new("S".to_string(), 0, Protocol::Http);
    d.fingerprint = "s-fp".to_string();
    LocalSendClient::new(d)
}

#[tokio::test]
async fn multi_file_session_completes_and_frees_the_slot() {
    let save = tempfile::tempdir().unwrap();
    let src = tempfile::tempdir().unwrap();
    let (_server, port) = receiver(save.path()).await;
    let c = client();
    let target = common::target_device(port);

    // 3 files in ONE session
    let mut files = HashMap::new();
    let mut paths = HashMap::new();
    let mut shas = HashMap::new();
    for name in ["one.bin", "two.bin", "three.bin"] {
        let (p, sha) = common::make_random_file(src.path(), name, 2048);
        let m = build_file_metadata(&p).await.unwrap();
        paths.insert(m.id.clone(), p);
        shas.insert(name.to_string(), sha);
        files.insert(m.id.clone(), m);
    }
    let prep = c.prepare_upload(&target, files, None).await.unwrap();
    assert_eq!(prep.files.len(), 3);
    for (id, token) in &prep.files {
        c.upload_file(&target, &prep.session_id, id, token, &paths[id], None)
            .await
            .unwrap();
    }
    for name in ["one.bin", "two.bin", "three.bin"] {
        let got = localsend_rs::sha256_from_file(&save.path().join(name))
            .await
            .unwrap();
        assert_eq!(&got, shas.get(name).unwrap());
    }

    // Session must be CLOSED now: a new prepare-upload succeeds (no 409). (R5)
    let (p2, _) = common::make_random_file(src.path(), "again.bin", 128);
    let m2 = build_file_metadata(&p2).await.unwrap();
    let mut f2 = HashMap::new();
    f2.insert(m2.id.clone(), m2);
    let second = c.prepare_upload(&target, f2, None).await;
    assert!(
        second.is_ok(),
        "expected new session after completion, got {second:?}"
    );
}

#[tokio::test]
async fn concurrent_second_session_is_blocked_with_409() {
    let save = tempfile::tempdir().unwrap();
    let src = tempfile::tempdir().unwrap();
    let (_server, port) = receiver(save.path()).await;
    let c = client();
    let target = common::target_device(port);

    let (p, _) = common::make_random_file(src.path(), "held.bin", 128);
    let m = build_file_metadata(&p).await.unwrap();
    let mut f = HashMap::new();
    f.insert(m.id.clone(), m);
    let _prep = c.prepare_upload(&target, f, None).await.unwrap(); // session open, file NOT uploaded

    let (p2, _) = common::make_random_file(src.path(), "blocked.bin", 128);
    let m2 = build_file_metadata(&p2).await.unwrap();
    let mut f2 = HashMap::new();
    f2.insert(m2.id.clone(), m2);
    let err = c
        .prepare_upload(&target, f2, None)
        .await
        .expect_err("blocked");
    assert!(matches!(err, localsend_rs::LocalSendError::SessionBlocked));
}

#[tokio::test]
async fn same_filename_twice_keeps_both_copies() {
    let save = tempfile::tempdir().unwrap();
    let src = tempfile::tempdir().unwrap();
    let (_server, port) = receiver(save.path()).await;
    let c = client();
    let target = common::target_device(port);

    for round in 0..2 {
        let sub = src.path().join(format!("r{round}"));
        std::fs::create_dir_all(&sub).unwrap();
        let (p, _) = common::make_random_file(&sub, "dup.bin", 64 + round);
        let m = build_file_metadata(&p).await.unwrap();
        let id = m.id.clone();
        let mut f = HashMap::new();
        f.insert(id.clone(), m);
        let prep = c.prepare_upload(&target, f, None).await.unwrap();
        let token = prep.files.get(&id).unwrap().clone();
        c.upload_file(&target, &prep.session_id, &id, &token, &p, None)
            .await
            .unwrap();
    }
    assert!(save.path().join("dup.bin").exists());
    assert!(save.path().join("dup (1).bin").exists());
}

#[tokio::test]
async fn cancel_frees_the_session_slot() {
    let save = tempfile::tempdir().unwrap();
    let src = tempfile::tempdir().unwrap();
    let (_server, port) = receiver(save.path()).await;
    let c = client();
    let target = common::target_device(port);

    let (p, _) = common::make_random_file(src.path(), "c.bin", 128);
    let m = build_file_metadata(&p).await.unwrap();
    let mut f = HashMap::new();
    f.insert(m.id.clone(), m);
    let prep = c.prepare_upload(&target, f, None).await.unwrap();

    c.cancel(&target, &prep.session_id)
        .await
        .expect("cancel ok");

    // Slot is free again
    let (p2, _) = common::make_random_file(src.path(), "c2.bin", 128);
    let m2 = build_file_metadata(&p2).await.unwrap();
    let mut f2 = HashMap::new();
    f2.insert(m2.id.clone(), m2);
    assert!(c.prepare_upload(&target, f2, None).await.is_ok());
}

#[tokio::test]
async fn upload_reports_monotonic_progress_up_to_total() {
    let save = tempfile::tempdir().unwrap();
    let src = tempfile::tempdir().unwrap();
    let (_server, port) = receiver(save.path()).await;
    let c = client();
    let target = common::target_device(port);

    const SIZE: usize = 4 * 1024 * 1024; // several chunks
    let (p, _) = common::make_random_file(src.path(), "p.bin", SIZE);
    let m = build_file_metadata(&p).await.unwrap();
    let id = m.id.clone();
    let mut f = HashMap::new();
    f.insert(id.clone(), m);
    let prep = c.prepare_upload(&target, f, None).await.unwrap();
    let token = prep.files.get(&id).unwrap().clone();

    let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::<(u64, u64)>::new()));
    let seen_cb = seen.clone();
    c.upload_file(
        &target,
        &prep.session_id,
        &id,
        &token,
        &p,
        Some(Box::new(move |sent, total, _elapsed| {
            seen_cb.lock().unwrap().push((sent, total));
        })),
    )
    .await
    .unwrap();

    let seen = seen.lock().unwrap();
    assert!(
        seen.len() >= 2,
        "expected multiple progress callbacks, got {}",
        seen.len()
    );
    assert!(
        seen.windows(2).all(|w| w[0].0 <= w[1].0),
        "progress must be monotonic"
    );
    assert_eq!(seen.last().unwrap().0, SIZE as u64);
    assert!(seen.iter().all(|(_, t)| *t == SIZE as u64));
}
