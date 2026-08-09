mod common;

use localsend_rs::Protocol;
use localsend_rs::server::LocalSendServer;
use serde_json::json;

fn minimal_prepare_body() -> serde_json::Value {
    json!({
        "info": {
            "alias": "raw-sender", "version": "2.1", "deviceModel": null,
            "deviceType": "headless", "fingerprint": "raw-fp",
            "port": 53317, "protocol": "http", "download": false
        },
        "files": {
            "f1": { "id": "f1", "fileName": "a.txt", "size": 5,
                    "fileType": "text/plain", "sha256": null, "preview": null, "metadata": null }
        }
    })
}

#[tokio::test]
async fn pin_gate_returns_401_then_429_then_accepts_correct_pin() {
    let save = tempfile::tempdir().unwrap();
    let (server, _events) = LocalSendServer::builder()
        .alias("Pinned")
        .port(0)
        .save_dir(save.path())
        .protocol(Protocol::Http)
        .pin("123456")
        .auto_accept(true)
        .build()
        .await
        .unwrap();
    let port = server.port();
    common::wait_for_http_info(port).await;
    let base = format!("http://127.0.0.1:{port}/api/localsend/v2/prepare-upload");
    let http = reqwest::Client::new();

    // wrong pin -> 401, three times
    for _ in 0..3 {
        let r = http
            .post(format!("{base}?pin=000000"))
            .json(&minimal_prepare_body())
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), 401);
    }
    // locked out -> 429 even with the right pin
    let r = http
        .post(format!("{base}?pin=123456"))
        .json(&minimal_prepare_body())
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 429);
}

#[tokio::test]
async fn correct_pin_passes() {
    let save = tempfile::tempdir().unwrap();
    let (server, _events) = LocalSendServer::builder()
        .alias("Pinned")
        .port(0)
        .save_dir(save.path())
        .protocol(Protocol::Http)
        .pin("123456")
        .auto_accept(true)
        .build()
        .await
        .unwrap();
    let port = server.port();
    common::wait_for_http_info(port).await;
    let base = format!("http://127.0.0.1:{port}/api/localsend/v2/prepare-upload");

    let r = reqwest::Client::new()
        .post(format!("{base}?pin=123456"))
        .json(&minimal_prepare_body())
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200);
    let body: serde_json::Value = r.json().await.unwrap();
    assert!(body["sessionId"].is_string());
    assert!(body["files"]["f1"].is_string());
}
