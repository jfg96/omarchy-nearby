mod common;

use localsend_rs::Protocol;
use localsend_rs::server::LocalSendServer;

#[tokio::test]
async fn builder_starts_on_ephemeral_port_and_reports_it() {
    let save = tempfile::tempdir().unwrap();
    let (server, _events) = LocalSendServer::builder()
        .alias("Builder Test")
        .port(0)
        .save_dir(save.path())
        .protocol(Protocol::Http)
        .auto_accept(true)
        .build()
        .await
        .expect("build");

    let port = server.port();
    assert_ne!(port, 0);
    common::wait_for_http_info(port).await;

    let url = format!("http://127.0.0.1:{port}/api/localsend/v2/info");
    let info: serde_json::Value = reqwest::get(&url).await.unwrap().json().await.unwrap();
    assert_eq!(info["alias"], "Builder Test");
    assert_eq!(info["version"], "2.1");
    assert_eq!(info["port"], serde_json::json!(port));
}
