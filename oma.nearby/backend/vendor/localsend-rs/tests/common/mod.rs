#![allow(dead_code)] // each integration-test binary uses a subset of these helpers

use localsend_rs::{DeviceInfo, Protocol, sha256_from_bytes};
use std::path::{Path, PathBuf};

/// Bind port 0, read the assigned port, release it.
pub fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .expect("bind ephemeral port")
        .local_addr()
        .expect("local addr")
        .port()
}

/// Deterministic pseudo-random file (xorshift, no extra deps).
/// Returns (path, sha256-hex-of-contents).
pub fn make_random_file(dir: &Path, name: &str, size: usize) -> (PathBuf, String) {
    let mut buf = vec![0u8; size];
    let mut x: u64 = 0x9E3779B97F4A7C15 ^ (size as u64);
    for b in buf.iter_mut() {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        *b = (x & 0xFF) as u8;
    }
    let sha = sha256_from_bytes(&buf);
    let path = dir.join(name);
    std::fs::write(&path, &buf).expect("write random file");
    (path, sha)
}

/// Poll GET /info until the server answers (or panic after ~5 s).
pub async fn wait_for_http_info(port: u16) {
    let url = format!("http://127.0.0.1:{port}/api/localsend/v2/info");
    for _ in 0..50 {
        if reqwest::get(&url).await.is_ok() {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    panic!("server on port {port} never became ready");
}

/// A DeviceInfo pointing at a local HTTP server, usable as a client target.
pub fn target_device(port: u16) -> DeviceInfo {
    let mut d = DeviceInfo::new("test-target".to_string(), port, Protocol::Http);
    d.ip = Some("127.0.0.1".to_string());
    d.fingerprint = "test-target-fp".to_string();
    d
}
