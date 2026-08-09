use crate::discovery::traits::Discovery;
use clap::Parser;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "receive", about = "Start LocalSend server to receive files")]
pub struct ReceiveCommand {
    #[arg(short, long, default_value = "./downloads")]
    directory: PathBuf,

    #[arg(short, long, default_value = "53317")]
    port: u16,

    #[arg(long)]
    pin: Option<String>,

    #[arg(long)]
    auto_accept: bool,

    /// Wait before automatically accepting each incoming transfer. This is
    /// useful for exercising sender-side waiting UI without an interactive TTY.
    #[arg(long, default_value_t = 0, requires = "auto_accept")]
    accept_delay_ms: u64,

    /// Test-only: throttle receiver body consumption to the given KiB/s.
    #[arg(long, hide = true, value_parser = clap::value_parser!(u64).range(1..))]
    receive_rate_limit_kib: Option<u64>,

    /// Use plain HTTP instead of HTTPS. LocalSend uses HTTPS by default (matching
    /// the official app); pass this for easy interop/testing with HTTP-only peers.
    #[cfg(feature = "https")]
    #[arg(long)]
    no_https: bool,
}

pub async fn execute(command: ReceiveCommand) -> anyhow::Result<()> {
    if !command.directory.exists() {
        tokio::fs::create_dir_all(&command.directory).await?;
        println!(
            "Created download directory: {}",
            command.directory.display()
        );
    }

    println!("Starting LocalSend server on port {}", command.port);
    println!("Save directory: {}", command.directory.display());

    if let Some(ref pin) = command.pin {
        println!("PIN required: {}", pin);
    }

    if command.auto_accept {
        if command.accept_delay_ms == 0 {
            println!("Auto-accept mode ENABLED - files will be accepted without confirmation!");
        } else {
            println!(
                "Delayed auto-accept mode ENABLED - requests will wait {} ms",
                command.accept_delay_ms
            );
        }
    }

    #[cfg(feature = "https")]
    let https_enabled = !command.no_https;
    #[cfg(not(feature = "https"))]
    let https_enabled = false;

    println!(
        "Transport: {}",
        if https_enabled { "HTTPS" } else { "HTTP" }
    );

    let protocol_enum = if https_enabled {
        crate::protocol::Protocol::Https
    } else {
        crate::protocol::Protocol::Http
    };

    let mut builder = crate::server::LocalSendServer::builder()
        .alias("LocalSend-Rust".to_string())
        .port(command.port)
        .save_dir(&command.directory)
        .protocol(protocol_enum)
        // A delayed acceptance must flow through `TransferRequest`; enabling
        // the server's fast auto-accept path would bypass that event entirely.
        .auto_accept(server_auto_accept(
            command.auto_accept,
            command.accept_delay_ms,
        ));
    if let Some(kib_per_second) = command.receive_rate_limit_kib {
        let bytes_per_second = kib_per_second.saturating_mul(1_024);
        println!("Test receiver rate limit: {kib_per_second} KiB/s");
        builder = builder.receive_rate_limit(bytes_per_second);
    }
    if let Some(ref pin) = command.pin {
        builder = builder.pin(pin.clone());
    }
    let (mut server, mut events) = builder.build().await?;

    // Discovery must announce the SAME device identity the server uses.
    let mut discovery =
        crate::discovery::MulticastDiscovery::new_with_device(server.device().clone());
    println!("Starting multicast discovery...");
    discovery.start().await?;
    println!("Announcing presence to network...");
    discovery.announce_presence().await?;

    let auto_accept = command.auto_accept;
    let accept_delay_ms = command.accept_delay_ms;
    let event_loop = tokio::spawn(async move {
        while let Some(ev) = events.recv().await {
            match ev {
                crate::server::ServerEvent::TransferRequest(req) => {
                    println!(
                        "Incoming transfer from '{}' ({} file(s))",
                        req.sender().alias,
                        req.files().len()
                    );
                    if auto_accept {
                        if accept_delay_ms > 0 {
                            tokio::time::sleep(std::time::Duration::from_millis(accept_delay_ms))
                                .await;
                        }
                        req.accept();
                    } else {
                        // Headless interactive: y/n on stdin.
                        let accept = inquire::Confirm::new("Accept this transfer?")
                            .with_default(false)
                            .prompt()
                            .unwrap_or(false);
                        if accept {
                            req.accept();
                        } else {
                            req.decline();
                        }
                    }
                }
                crate::server::ServerEvent::TextReceived {
                    text, sender_alias, ..
                } => println!("Message from {}: {}", sender_alias, text),
                crate::server::ServerEvent::WebShareRequest(request) => {
                    println!("Browser download request from {}", request.ip());
                }
                crate::server::ServerEvent::WebShareDownloadProgress { .. }
                | crate::server::ServerEvent::WebShareSessionDone { .. } => {}
                crate::server::ServerEvent::FileReceiveProgress {
                    file_name,
                    bytes_received,
                    total_bytes,
                    ..
                } => {
                    eprintln!("Receiving {file_name}: {bytes_received}/{total_bytes} bytes");
                }
                crate::server::ServerEvent::FileReceived {
                    file_name,
                    path,
                    size,
                    sender_alias,
                    message_text,
                    ..
                } => {
                    let _ = message_text;
                    println!(
                        "Received '{}' ({} bytes) from {} -> {}",
                        file_name,
                        size,
                        sender_alias,
                        path.display()
                    );
                }
                crate::server::ServerEvent::SessionCompleted { session_id } => {
                    println!("Session {} complete", session_id);
                }
                crate::server::ServerEvent::SessionCancelled { session_id } => {
                    println!("Session {} cancelled", session_id)
                }
                crate::server::ServerEvent::SessionFailed {
                    session_id,
                    message,
                } => eprintln!("Session {} failed: {}", session_id, message),
                crate::server::ServerEvent::TransferRequestExpired { .. }
                | crate::server::ServerEvent::PeerRegistered(_) => {}
            }
        }
    });

    tokio::signal::ctrl_c().await?;

    println!("\nShutting down server...");
    event_loop.abort();
    server.stop();
    discovery.stop();

    Ok(())
}

fn server_auto_accept(auto_accept: bool, accept_delay_ms: u64) -> bool {
    auto_accept && accept_delay_ms == 0
}

#[cfg(test)]
mod tests {
    use super::{ReceiveCommand, server_auto_accept};
    use clap::Parser;

    #[test]
    fn parses_auto_accept_delay() {
        let command = ReceiveCommand::try_parse_from([
            "receive",
            "--auto-accept",
            "--accept-delay-ms",
            "15000",
        ])
        .expect("auto-accept delay should parse");

        assert!(command.auto_accept);
        assert_eq!(command.accept_delay_ms, 15_000);
    }

    #[test]
    fn rejects_accept_delay_without_auto_accept() {
        let error = ReceiveCommand::try_parse_from(["receive", "--accept-delay-ms", "15000"])
            .expect_err("delay without auto-accept should be rejected");

        assert!(error.to_string().contains("--auto-accept"));
    }

    #[test]
    fn delayed_auto_accept_uses_transfer_request_path() {
        assert!(server_auto_accept(true, 0));
        assert!(!server_auto_accept(true, 15_000));
        assert!(!server_auto_accept(false, 0));
    }

    #[test]
    fn parses_test_receiver_rate_limit() {
        let command =
            ReceiveCommand::try_parse_from(["receive", "--receive-rate-limit-kib", "512"])
                .expect("positive receiver limit should parse");

        assert_eq!(command.receive_rate_limit_kib, Some(512));
    }

    #[test]
    fn rejects_zero_receiver_rate_limit() {
        ReceiveCommand::try_parse_from(["receive", "--receive-rate-limit-kib", "0"])
            .expect_err("zero receiver limit should be rejected");
    }
}
