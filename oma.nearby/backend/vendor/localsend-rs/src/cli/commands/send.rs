use crate::DeviceInfo;
use crate::client::{LocalSendClient, TlsTrustPolicy};
use crate::core::file::build_file_metadata;
use crate::crypto::generate_fingerprint;
use crate::discovery::{Discovery, MulticastDiscovery};
use crate::protocol::types::FileMetadataDetails;
use crate::protocol::{DeviceType, FileMetadata, Protocol};
use clap::Parser;
use reqwest::Client;
use std::collections::HashMap;
use std::net::IpAddr;
use std::path::PathBuf;
use std::time::Duration;

#[derive(Parser, Debug)]
#[command(name = "send", about = "Send files to a LocalSend device")]
pub struct SendCommand {
    /// Target device: an IP or hostname (default port 53317), `host:port`,
    /// `[ipv6]:port`, or a discovered device alias.
    target: String,

    #[arg(required = true)]
    files: Vec<String>, // Changed from PathBuf to String to support text

    #[arg(short, long)]
    pin: Option<String>,

    /// Test-only: throttle sender stream production to the given KiB/s.
    #[arg(long, hide = true, value_parser = clap::value_parser!(u64).range(1..))]
    send_rate_limit_kib: Option<u64>,
}

pub async fn execute(command: SendCommand) -> anyhow::Result<()> {
    let target = resolve_target(&command.target).await?;
    println!("Sending to: {} ({:?})", target.alias, target.ip);

    let sender = DeviceInfo {
        alias: "LocalSend-Rust".to_string(),
        version: "2.1".to_string(),
        device_model: Some(std::env::consts::OS.to_string()),
        device_type: Some(DeviceType::Desktop),
        fingerprint: generate_fingerprint(),
        port: 53318,
        protocol: Protocol::Https, // Default to HTTPS
        download: false,
        ip: None,
    };
    let client = build_client_for_target(sender, &target)?;
    let send_rate_limit = command
        .send_rate_limit_kib
        .map(|kib_per_second| kib_per_second.saturating_mul(1_024));

    // Register first to ensure connection
    let _ = client.register(&target).await;

    let mut file_metadata_map: HashMap<String, FileSource> = HashMap::new();
    let mut files_metadata = HashMap::new();

    enum FileSource {
        Path(PathBuf),
        Text(String),
    }

    for input in &command.files {
        let path = PathBuf::from(input);
        if path.exists() {
            let file_meta = build_file_metadata(&path).await?;
            file_metadata_map.insert(file_meta.id.as_str().to_string(), FileSource::Path(path));
            files_metadata.insert(file_meta.id.clone(), file_meta);
        } else {
            // Treat as text
            let text = input.clone();
            let id = crate::protocol::FileId::new();
            let file_meta = FileMetadata {
                id: id.clone(),
                file_name: format!("{}.txt", id), // Random name or "message.txt"
                size: text.len() as u64,
                file_type: "text/plain".to_string(),
                sha256: None,
                preview: Some(text.clone()), // Preview is the text itself
                metadata: Some(FileMetadataDetails {
                    modified: None,
                    accessed: None,
                }),
            };
            file_metadata_map.insert(id.as_str().to_string(), FileSource::Text(text));
            files_metadata.insert(id.clone(), file_meta);
        }
    }

    let upload_response = client
        .prepare_upload(&target, files_metadata, command.pin.as_deref())
        .await?;

    println!("Session ID: {}", upload_response.session_id);

    if upload_response.session_id.as_str().is_empty() {
        // 204 No Content - likely text message sent successfully
        println!("Transfer completed successfully (No files needed to be transferred).");
        return Ok(());
    }

    for (file_id, token) in &upload_response.files {
        let source = file_metadata_map
            .get(file_id.as_str())
            .ok_or_else(|| anyhow::anyhow!("File not found for ID: {}", file_id))?;

        match source {
            FileSource::Path(path) => {
                let file_size = tokio::fs::metadata(path).await?.len();
                println!("Uploading: {} ({} bytes)", path.display(), file_size);

                client
                    .upload_file_with_rate_limit(
                        &target,
                        &upload_response.session_id,
                        file_id,
                        token,
                        path,
                        None,
                        send_rate_limit,
                    )
                    .await?;

                println!(
                    "Success: {}",
                    path.file_name().unwrap_or_default().to_string_lossy()
                );
            }
            FileSource::Text(text) => {
                println!("Sending text message: \"{}\"", text);
                // We need to write text to a temp file or modify client to accept bytes.
                // For now, let's just write to a temp file.
                let temp_dir = std::env::temp_dir();
                let temp_file = temp_dir.join(format!("localsend_text_{}.txt", file_id));
                tokio::fs::write(&temp_file, text.as_bytes()).await?;

                client
                    .upload_file_with_rate_limit(
                        &target,
                        &upload_response.session_id,
                        file_id,
                        token,
                        &temp_file,
                        None,
                        send_rate_limit,
                    )
                    .await?;

                let _ = tokio::fs::remove_file(temp_file).await;
                println!("Success: Text message sent");
            }
        }
    }

    Ok(())
}

fn build_client_for_target(
    sender: DeviceInfo,
    target: &DeviceInfo,
) -> anyhow::Result<LocalSendClient> {
    match target_trust_policy(target) {
        Some(policy) => Ok(LocalSendClient::with_trust_policy(sender, policy)?),
        None => Ok(LocalSendClient::new(sender)),
    }
}

fn target_trust_policy(target: &DeviceInfo) -> Option<TlsTrustPolicy> {
    (target.protocol == Protocol::Https).then(|| TlsTrustPolicy::new([target.fingerprint.clone()]))
}

/// Split a `host` / `host:port` target into its host and optional explicit port.
///
/// Bracketed IPv6 (`[::1]:53317`) is supported. A bare, unbracketed string with
/// more than one `:` is treated as a host with no port (so raw IPv6 literals like
/// `fe80::1` aren't mis-split on their internal colons).
fn split_host_port(target: &str) -> (String, Option<u16>) {
    // Bracketed IPv6: [host] or [host]:port
    if let Some(rest) = target.strip_prefix('[') {
        if let Some((host, tail)) = rest.split_once(']') {
            let port = tail.strip_prefix(':').and_then(|p| p.parse::<u16>().ok());
            return (host.to_string(), port);
        }
        return (target.to_string(), None);
    }

    match target.rsplit_once(':') {
        Some((host, port)) if !host.contains(':') => {
            if port.is_empty() {
                // Trailing colon ("host:") → host with no port.
                (host.to_string(), None)
            } else {
                match port.parse::<u16>() {
                    // Exactly one colon and a numeric tail → host:port.
                    Ok(p) => (host.to_string(), Some(p)),
                    // Non-numeric tail: keep the whole string as the host (it may
                    // be an alias that legitimately contains a colon).
                    Err(_) => (target.to_string(), None),
                }
            }
        }
        // Zero colons, or a bare IPv6 literal (multiple colons) → host only.
        _ => (target.to_string(), None),
    }
}

async fn resolve_target(target: &str) -> anyhow::Result<DeviceInfo> {
    let (host, explicit_port) = split_host_port(target);
    let port = explicit_port.unwrap_or(crate::protocol::constants::DEFAULT_HTTP_PORT);

    // 1. Try if host is an IP address
    if let Ok(ip) = host.parse::<IpAddr>() {
        println!(
            "Target is an IP address, probing {}:{} directly...",
            ip, port
        );
        if let Ok(device) = probe_device(ip.to_string(), port).await {
            return Ok(device);
        }
        println!("Direct probe failed, falling back to discovery...");
    }

    // 2. Use Multicast Discovery
    let mut discovery = MulticastDiscovery::new(
        "LocalSend-Rust-Finder".to_string(),
        53317,
        crate::protocol::Protocol::Https,
    )?;

    let found_device = std::sync::Arc::new(std::sync::Mutex::new(None as Option<DeviceInfo>));
    let found_device_clone = found_device.clone();
    let host_owned = host.clone();

    discovery.on_discovered(move |device: DeviceInfo| {
        let matches_alias = device.alias == host_owned;
        let matches_ip = device.ip.as_deref() == Some(host_owned.as_str());
        // If the user pinned a port, require it too; otherwise match on alias/ip alone.
        let matches_port = explicit_port.is_none_or(|p| device.port == p);

        if (matches_alias || matches_ip) && matches_port {
            let mut found = found_device_clone.lock().unwrap();
            if found.is_none() {
                *found = Some(device);
            }
        }
    });

    discovery.start().await?;
    // Send announcement to trigger responses
    discovery.announce_presence().await?;

    println!("Searching for device '{}'...", target);
    for _ in 0..50 {
        // Wait up to 5 seconds
        if found_device.lock().unwrap().is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    discovery.stop();

    let found = found_device.lock().unwrap();
    if let Some(device) = found.clone() {
        Ok(device)
    } else {
        anyhow::bail!("Could not resolve target: {}", target);
    }
}

async fn probe_device(ip: String, port: u16) -> anyhow::Result<DeviceInfo> {
    let client = Client::builder()
        .danger_accept_invalid_certs(true)
        .timeout(Duration::from_secs(2))
        .build()?;

    // Try HTTPS first
    let url = format!("https://{}:{}/api/localsend/v2/info", ip, port);
    if let Ok(resp) = client.get(&url).send().await
        && resp.status().is_success()
    {
        let mut device: DeviceInfo = resp.json().await?;
        device.ip = Some(ip.clone());
        device.port = port;
        device.protocol = crate::protocol::Protocol::Https; // Ensure protocol is set matches what we used
        return Ok(device);
    }

    // Try HTTP
    let url = format!("http://{}:{}/api/localsend/v2/info", ip, port);
    if let Ok(resp) = client.get(&url).send().await
        && resp.status().is_success()
    {
        let mut device: DeviceInfo = resp.json().await?;
        device.ip = Some(ip.clone());
        device.port = port;
        device.protocol = crate::protocol::Protocol::Http;
        return Ok(device);
    }

    anyhow::bail!("Failed to probe device at {}:{}", ip, port)
}

#[cfg(test)]
mod tests {
    use super::{SendCommand, build_client_for_target, split_host_port, target_trust_policy};
    use crate::protocol::{DeviceInfo, Protocol};
    use clap::Parser;

    #[test]
    fn bare_ipv4_has_no_port() {
        assert_eq!(split_host_port("127.0.0.1"), ("127.0.0.1".into(), None));
    }

    #[test]
    fn parses_test_sender_rate_limit() {
        let command = SendCommand::try_parse_from([
            "send",
            "--send-rate-limit-kib",
            "512",
            "127.0.0.1",
            "/tmp/large.bin",
        ])
        .expect("positive sender limit should parse");

        assert_eq!(command.send_rate_limit_kib, Some(512));
    }

    #[test]
    fn rejects_zero_sender_rate_limit() {
        SendCommand::try_parse_from([
            "send",
            "--send-rate-limit-kib",
            "0",
            "127.0.0.1",
            "/tmp/large.bin",
        ])
        .expect_err("zero sender limit should be rejected");
    }

    #[test]
    fn ipv4_with_port() {
        assert_eq!(
            split_host_port("127.0.0.1:53666"),
            ("127.0.0.1".into(), Some(53666))
        );
    }

    #[test]
    fn bare_hostname_and_hostname_with_port() {
        assert_eq!(split_host_port("mybox"), ("mybox".into(), None));
        assert_eq!(split_host_port("mybox:1234"), ("mybox".into(), Some(1234)));
    }

    #[test]
    fn non_numeric_port_is_treated_as_host() {
        // Not a valid port → keep the whole string as the host, no port.
        assert_eq!(split_host_port("host:abc"), ("host:abc".into(), None));
    }

    #[test]
    fn trailing_colon_drops_to_host() {
        assert_eq!(split_host_port("host:"), ("host".into(), None));
    }

    #[test]
    fn out_of_range_port_is_treated_as_host() {
        assert_eq!(split_host_port("host:99999"), ("host:99999".into(), None));
    }

    #[test]
    fn bare_ipv6_is_not_split_on_internal_colons() {
        assert_eq!(split_host_port("fe80::1"), ("fe80::1".into(), None));
    }

    #[test]
    fn bracketed_ipv6_with_and_without_port() {
        assert_eq!(split_host_port("[::1]:53317"), ("::1".into(), Some(53317)));
        assert_eq!(split_host_port("[::1]"), ("::1".into(), None));
    }

    #[test]
    fn https_target_requires_a_valid_discovered_fingerprint() {
        let sender = DeviceInfo::new("sender".to_string(), 53318, Protocol::Https);
        let mut target = DeviceInfo::new("receiver".to_string(), 53317, Protocol::Https);
        target.fingerprint = "not-a-certificate-fingerprint".to_string();

        let error = build_client_for_target(sender, &target)
            .err()
            .expect("HTTPS client must reject an invalid peer fingerprint");

        assert!(
            error
                .to_string()
                .contains("Invalid LocalSend TLS fingerprint")
        );
    }

    #[test]
    fn http_target_does_not_require_a_certificate_fingerprint() {
        let target = DeviceInfo::new("receiver".to_string(), 53317, Protocol::Http);

        assert_eq!(target_trust_policy(&target), None);
    }
}
