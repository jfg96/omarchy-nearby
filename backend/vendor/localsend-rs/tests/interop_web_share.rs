mod common;

use localsend_rs::Protocol;
use localsend_rs::server::{LocalSendServer, ServerEvent, WebShareFile};

#[tokio::test(flavor = "multi_thread")]
async fn approved_browser_session_downloads_exact_bytes() {
    let fixture = b"crosscopy-web-share\nsecond line";
    let (mut server, mut events) = LocalSendServer::builder()
        .alias("Browser sender")
        .port(0)
        .protocol(Protocol::Http)
        .auto_accept(true)
        .build()
        .await
        .expect("server starts");
    common::wait_for_http_info(server.port()).await;

    server
        .start_web_share(
            vec![WebShareFile::inline("hello.txt", fixture.to_vec())],
            None,
            true,
        )
        .await
        .expect("web share starts");
    assert!(server.device().download);

    let client = reqwest::Client::new();
    let base = format!("http://127.0.0.1:{}", server.port());
    let response = client
        .post(format!("{base}/api/localsend/v2/prepare-download"))
        .send()
        .await
        .expect("prepare request")
        .error_for_status()
        .expect("prepare accepted");
    let body: serde_json::Value = response.json().await.expect("prepare JSON");
    let session_id = body["sessionId"].as_str().expect("session id");
    let file_id = body["files"]
        .as_object()
        .and_then(|files| files.keys().next())
        .expect("file id");

    let bytes = client
        .get(format!(
            "{base}/api/localsend/v2/download?sessionId={session_id}&fileId={file_id}"
        ))
        .send()
        .await
        .expect("download request")
        .error_for_status()
        .expect("download accepted")
        .bytes()
        .await
        .expect("download bytes");
    assert_eq!(bytes.as_ref(), fixture);

    let mut last_progress = 0;
    loop {
        let event = tokio::time::timeout(std::time::Duration::from_secs(2), events.recv())
            .await
            .expect("web share event timeout")
            .expect("web share event channel");
        match event {
            ServerEvent::WebShareRequest(request) => {
                assert_eq!(request.session_id().as_str(), session_id);
            }
            ServerEvent::WebShareDownloadProgress {
                bytes_sent,
                total_bytes,
                ..
            } => {
                assert!(bytes_sent >= last_progress);
                assert!(bytes_sent <= total_bytes);
                last_progress = bytes_sent;
            }
            ServerEvent::WebShareSessionDone { .. } => break,
            other => panic!("unexpected event: {other:?}"),
        }
    }
    assert_eq!(last_progress, fixture.len() as u64);

    server.stop_web_share().await.expect("web share stops");
    assert!(!server.device().download);
    assert_eq!(
        client
            .post(format!("{base}/api/localsend/v2/prepare-download"))
            .send()
            .await
            .expect("inactive prepare")
            .status(),
        reqwest::StatusCode::FORBIDDEN
    );
    server.stop();
}

#[tokio::test(flavor = "multi_thread")]
async fn browser_prepare_waits_for_explicit_approval() {
    let (mut server, mut events) = LocalSendServer::builder()
        .alias("Approval sender")
        .port(0)
        .protocol(Protocol::Http)
        .auto_accept(false)
        .build()
        .await
        .expect("server starts");
    server
        .start_web_share(
            vec![WebShareFile::inline("hello.txt", b"approved".to_vec())],
            None,
            false,
        )
        .await
        .expect("web share starts");

    let url = format!(
        "http://127.0.0.1:{}/api/localsend/v2/prepare-download",
        server.port()
    );
    let request = tokio::spawn(async move { reqwest::Client::new().post(url).send().await });
    let pending = match tokio::time::timeout(std::time::Duration::from_secs(5), events.recv())
        .await
        .expect("approval event timeout")
        .expect("event channel")
    {
        ServerEvent::WebShareRequest(pending) => pending,
        other => panic!("unexpected event: {other:?}"),
    };
    server
        .respond_web_share(pending.session_id(), true)
        .await
        .expect("approve request");
    assert!(request.await.unwrap().unwrap().status().is_success());
    server.stop();
}

#[tokio::test(flavor = "multi_thread")]
async fn browser_share_pin_uses_receiver_rate_limit() {
    let (mut server, _events) = LocalSendServer::builder()
        .port(0)
        .protocol(Protocol::Http)
        .auto_accept(true)
        .build()
        .await
        .expect("server starts");
    server
        .start_web_share(
            vec![WebShareFile::inline("secret.txt", b"secret".to_vec())],
            Some("123456".to_string()),
            true,
        )
        .await
        .expect("web share starts");
    let client = reqwest::Client::new();
    let base = format!(
        "http://127.0.0.1:{}/api/localsend/v2/prepare-download",
        server.port()
    );

    for _ in 0..3 {
        assert_eq!(
            client
                .post(format!("{base}?pin=wrong"))
                .send()
                .await
                .unwrap()
                .status(),
            reqwest::StatusCode::UNAUTHORIZED
        );
    }
    assert_eq!(
        client
            .post(format!("{base}?pin=123456"))
            .send()
            .await
            .unwrap()
            .status(),
        reqwest::StatusCode::TOO_MANY_REQUESTS
    );
    server.stop();
}

#[tokio::test(flavor = "multi_thread")]
async fn browser_share_streams_disk_files_and_rejects_unknown_file_ids() {
    let fixture = b"disk-backed browser download\n";
    let directory = tempfile::tempdir().expect("temporary directory");
    let path = directory.path().join("report.txt");
    tokio::fs::write(&path, fixture)
        .await
        .expect("write fixture");

    let (mut server, _events) = LocalSendServer::builder()
        .port(0)
        .protocol(Protocol::Http)
        .auto_accept(true)
        .build()
        .await
        .expect("server starts");
    let file = WebShareFile::path(path, "report.txt")
        .await
        .expect("disk-backed web share file");
    server
        .start_web_share(vec![file], None, true)
        .await
        .expect("web share starts");

    let client = reqwest::Client::new();
    let base = format!("http://127.0.0.1:{}", server.port());
    let body: serde_json::Value = client
        .post(format!("{base}/api/localsend/v2/prepare-download"))
        .send()
        .await
        .expect("prepare request")
        .error_for_status()
        .expect("prepare accepted")
        .json()
        .await
        .expect("prepare JSON");
    let session_id = body["sessionId"].as_str().expect("session id");
    let file_id = body["files"]
        .as_object()
        .and_then(|files| files.keys().next())
        .expect("file id");

    let unknown_file = client
        .get(format!(
            "{base}/api/localsend/v2/download?sessionId={session_id}&fileId={}",
            uuid::Uuid::new_v4()
        ))
        .send()
        .await
        .expect("unknown file request");
    assert_eq!(unknown_file.status(), reqwest::StatusCode::FORBIDDEN);

    let response = client
        .get(format!(
            "{base}/api/localsend/v2/download?sessionId={session_id}&fileId={file_id}"
        ))
        .send()
        .await
        .expect("download request")
        .error_for_status()
        .expect("download accepted");
    assert_eq!(
        response.headers()[reqwest::header::CONTENT_DISPOSITION],
        "attachment; filename=\"report.txt\""
    );
    assert_eq!(
        response.bytes().await.expect("download bytes").as_ref(),
        fixture
    );
    server.stop();
}
