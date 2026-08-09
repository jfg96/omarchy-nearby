use localsend_rs::{
    DeviceInfo, LocalSendClient, LocalSendServer, Protocol, TlsTrustPolicy,
    generate_tls_certificate,
};
use std::time::Duration;

#[tokio::test]
async fn builder_uses_the_supplied_certificate_fingerprint_for_https() {
    let output = tempfile::tempdir().expect("output directory");
    let certificate = generate_tls_certificate().expect("certificate");
    let expected_fingerprint = certificate.fingerprint.clone();
    let (mut server, _events) = LocalSendServer::builder()
        .alias("Pinned receiver")
        .port(0)
        .save_dir(output.path())
        .protocol(Protocol::Https)
        .tls_certificate(certificate)
        .build()
        .await
        .expect("start HTTPS receiver");

    let client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .build()
        .expect("test client");
    let info_url = format!("https://127.0.0.1:{}/api/localsend/v2/info", server.port());
    let mut response = None;
    for _ in 0..50 {
        if let Ok(candidate) = client.get(&info_url).send().await {
            response = Some(candidate);
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    let info: DeviceInfo = response
        .expect("HTTPS /info should become available")
        .json()
        .await
        .expect("LocalSend device info");
    assert_eq!(server.device().fingerprint, expected_fingerprint);
    assert_eq!(info.fingerprint, expected_fingerprint);

    server.stop();
}

#[tokio::test]
async fn builder_generates_a_nonempty_https_fingerprint_when_none_is_supplied() {
    let output = tempfile::tempdir().expect("output directory");
    let (mut server, _events) = LocalSendServer::builder()
        .alias("Generated certificate receiver")
        .port(0)
        .save_dir(output.path())
        .protocol(Protocol::Https)
        .build()
        .await
        .expect("start HTTPS receiver");

    let client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .build()
        .expect("test client");
    let info_url = format!("https://127.0.0.1:{}/api/localsend/v2/info", server.port());
    let mut response = None;
    for _ in 0..50 {
        if let Ok(candidate) = client.get(&info_url).send().await {
            response = Some(candidate);
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    let info: DeviceInfo = response
        .expect("HTTPS /info should become available")
        .json()
        .await
        .expect("LocalSend device info");
    assert!(!server.device().fingerprint.is_empty());
    assert_eq!(info.fingerprint, server.device().fingerprint);

    server.stop();
}

#[tokio::test]
async fn pinned_client_accepts_the_matching_self_signed_leaf() {
    let output = tempfile::tempdir().expect("output directory");
    let (mut server, _events) = LocalSendServer::builder()
        .alias("Pinned receiver")
        .port(0)
        .save_dir(output.path())
        .protocol(Protocol::Https)
        .build()
        .await
        .expect("start HTTPS receiver");

    let mut target = server.device().clone();
    target.ip = Some("127.0.0.1".into());
    let sender = DeviceInfo::new("Pinned sender".into(), 0, Protocol::Https);
    let client = LocalSendClient::with_trust_policy(
        sender,
        TlsTrustPolicy::new([target.fingerprint.clone()]),
    )
    .expect("pinned client");

    client
        .register(&target)
        .await
        .expect("matching fingerprint should be accepted");

    server.stop();
}

#[tokio::test]
async fn pinned_client_rejects_a_non_matching_self_signed_leaf() {
    let output = tempfile::tempdir().expect("output directory");
    let (mut server, _events) = LocalSendServer::builder()
        .alias("Pinned receiver")
        .port(0)
        .save_dir(output.path())
        .protocol(Protocol::Https)
        .build()
        .await
        .expect("start HTTPS receiver");

    let mut target = server.device().clone();
    target.ip = Some("127.0.0.1".into());
    let sender = DeviceInfo::new("Pinned sender".into(), 0, Protocol::Https);
    let client = LocalSendClient::with_trust_policy(sender, TlsTrustPolicy::new(["f".repeat(64)]))
        .expect("pinned client");

    assert!(client.register(&target).await.is_err());
    server.stop();
}

#[test]
fn pinned_client_rejects_an_empty_or_malformed_discovered_fingerprint() {
    let device = DeviceInfo::new("Pinned sender".into(), 0, Protocol::Https);
    assert!(LocalSendClient::with_trust_policy(device.clone(), TlsTrustPolicy::new([""])).is_err());
    assert!(
        LocalSendClient::with_trust_policy(device, TlsTrustPolicy::new(["not-a-sha256"])).is_err()
    );
}

#[tokio::test]
async fn http_requests_bypass_the_tls_verifier() {
    let output = tempfile::tempdir().expect("output directory");
    let (mut server, _events) = LocalSendServer::builder()
        .alias("HTTP receiver")
        .port(0)
        .save_dir(output.path())
        .protocol(Protocol::Http)
        .build()
        .await
        .expect("start HTTP receiver");

    let mut target = server.device().clone();
    target.ip = Some("127.0.0.1".into());
    let sender = DeviceInfo::new("Pinned sender".into(), 0, Protocol::Http);
    let client = LocalSendClient::with_trust_policy(sender, TlsTrustPolicy::new(["f".repeat(64)]))
        .expect("pinned client configuration");

    client
        .register(&target)
        .await
        .expect("HTTP should not invoke TLS verification");
    server.stop();
}
