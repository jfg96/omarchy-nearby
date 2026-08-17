mod common;

use localsend_rs::Protocol;
use localsend_rs::server::LocalSendServer;
use serde_json::json;
use std::time::Duration;

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
    let (server, mut events) = LocalSendServer::builder()
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

    // A missing PIN is rejected without surfacing a transfer request and does
    // not consume one of the three incorrect-PIN attempts.
    let r = http
        .post(&base)
        .json(&minimal_prepare_body())
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 401);
    assert!(
        tokio::time::timeout(Duration::from_millis(50), events.recv())
            .await
            .is_err()
    );

    // wrong pin -> 401, three times
    for _ in 0..3 {
        let r = http
            .post(format!("{base}?pin=000000"))
            .json(&minimal_prepare_body())
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), 401);
        assert!(
            tokio::time::timeout(Duration::from_millis(50), events.recv())
                .await
                .is_err()
        );
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
async fn pin_gate_protects_message_shaped_requests_before_events() {
    let save = tempfile::tempdir().unwrap();
    let (server, mut events) = LocalSendServer::builder()
        .alias("Pinned Message")
        .port(0)
        .save_dir(save.path())
        .protocol(Protocol::Http)
        .pin("Text-1")
        .auto_accept(true)
        .build()
        .await
        .unwrap();
    common::wait_for_http_info(server.port()).await;
    let base = format!(
        "http://127.0.0.1:{}/api/localsend/v2/prepare-upload",
        server.port()
    );
    let mut body = minimal_prepare_body();
    body["files"]["f1"]["fileType"] = json!("text/plain");
    body["files"]["f1"]["preview"] = json!("protected message");
    body["files"]["f1"]["size"] = json!(17);
    let http = reqwest::Client::new();

    let status = http
        .post(format!("{base}?pin=wrong"))
        .json(&body)
        .send()
        .await
        .unwrap()
        .status();
    assert_eq!(status, 401);
    assert!(
        tokio::time::timeout(Duration::from_millis(50), events.recv())
            .await
            .is_err()
    );

    let status = http
        .post(format!("{base}?pin=Text-1"))
        .json(&body)
        .send()
        .await
        .unwrap()
        .status();
    assert_eq!(status, 204);
    let event = tokio::time::timeout(Duration::from_secs(1), events.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(
        matches!(event, localsend_rs::server::ServerEvent::TextReceived { text, .. } if text == "protected message")
    );
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

#[tokio::test]
async fn live_pin_changes_apply_without_restart_and_clear_lockout() {
    let save = tempfile::tempdir().unwrap();
    let (mut server, _events) = LocalSendServer::builder()
        .alias("Live PIN")
        .port(0)
        .save_dir(save.path())
        .protocol(Protocol::Http)
        .auto_accept(true)
        .build()
        .await
        .unwrap();
    let port = server.port();
    common::wait_for_http_info(port).await;
    let base = format!("http://127.0.0.1:{port}/api/localsend/v2/prepare-upload");
    let http = reqwest::Client::new();
    let empty = json!({
        "info": minimal_prepare_body()["info"].clone(),
        "files": {}
    });

    let status = http.post(&base).json(&empty).send().await.unwrap().status();
    assert_eq!(status, 204);

    server.set_pin(Some("old".to_string())).await.unwrap();
    let status = http.post(&base).json(&empty).send().await.unwrap().status();
    assert_eq!(status, 401);
    for _ in 0..3 {
        let status = http
            .post(format!("{base}?pin=bad"))
            .json(&empty)
            .send()
            .await
            .unwrap()
            .status();
        assert_eq!(status, 401);
    }
    let status = http
        .post(format!("{base}?pin=old"))
        .json(&empty)
        .send()
        .await
        .unwrap()
        .status();
    assert_eq!(status, 429);

    server.set_pin(Some("new".to_string())).await.unwrap();
    let status = http
        .post(format!("{base}?pin=old"))
        .json(&empty)
        .send()
        .await
        .unwrap()
        .status();
    assert_eq!(status, 401);
    let status = http
        .post(format!("{base}?pin=new"))
        .json(&empty)
        .send()
        .await
        .unwrap()
        .status();
    assert_eq!(status, 204);

    server.set_pin(None).await.unwrap();
    let status = http.post(&base).json(&empty).send().await.unwrap().status();
    assert_eq!(status, 204);
}
