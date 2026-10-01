mod common;

use localsend_rs::Protocol;
use localsend_rs::server::LocalSendServer;
use serde_json::json;

#[tokio::test]
async fn empty_files_map_returns_204() {
    let save = tempfile::tempdir().unwrap();
    let (server, _events) = LocalSendServer::builder()
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

    let body = json!({
        "info": { "alias": "raw", "version": "2.1", "deviceType": "headless",
                  "fingerprint": "fp", "port": 53317, "protocol": "http", "download": false },
        "files": {}
    });
    let r = reqwest::Client::new()
        .post(format!(
            "http://127.0.0.1:{port}/api/localsend/v2/prepare-upload"
        ))
        .json(&body)
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 204);
}

/// Offer `count` tiny files, keeping the JSON well below the 2 MB body limit
/// so only the file-count bound can reject it.
async fn prepare_many(port: u16, count: usize) -> reqwest::Response {
    let files: serde_json::Map<String, serde_json::Value> = (0..count)
        .map(|i| {
            let id = i.to_string();
            let file = json!({ "id": id, "fileName": "a", "size": 1, "fileType": "x" });
            (id, file)
        })
        .collect();
    let body = json!({
        "info": { "alias": "raw", "version": "2.1", "deviceType": "headless",
                  "fingerprint": "fp", "port": 53317, "protocol": "http", "download": false },
        "files": files
    });
    assert!(serde_json::to_vec(&body).unwrap().len() < 1024 * 1024);
    reqwest::Client::new()
        .post(format!(
            "http://127.0.0.1:{port}/api/localsend/v2/prepare-upload"
        ))
        .json(&body)
        .send()
        .await
        .unwrap()
}

/// An oversized offer is rejected before a session is reserved or the user is
/// asked, so an unauthenticated peer cannot push it to the approval UI.
#[tokio::test]
async fn too_many_offered_files_are_rejected_before_asking() {
    use tokio::time::{Duration, timeout};

    let save = tempfile::tempdir().unwrap();
    let (server, mut events) = LocalSendServer::builder()
        .alias("R")
        .port(0)
        .save_dir(save.path())
        .protocol(Protocol::Http)
        .auto_accept(false)
        .build()
        .await
        .unwrap();
    let port = server.port();
    common::wait_for_http_info(port).await;

    let r = timeout(Duration::from_secs(5), prepare_many(port, 10_001))
        .await
        .expect("an oversized offer must not wait for a decision");
    assert_eq!(r.status(), 413);
    assert!(
        events.try_recv().is_err(),
        "an oversized offer must not reach the event consumer"
    );
}

#[tokio::test]
async fn offer_at_the_file_count_limit_is_accepted() {
    let save = tempfile::tempdir().unwrap();
    let (server, _events) = LocalSendServer::builder()
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

    let r = prepare_many(port, 10_000).await;
    assert_eq!(r.status(), 200);
    let response: serde_json::Value = r.json().await.unwrap();
    assert_eq!(response["files"].as_object().unwrap().len(), 10_000);
}
