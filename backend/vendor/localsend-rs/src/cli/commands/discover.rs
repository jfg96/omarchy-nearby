use crate::DeviceInfo;
use crate::discovery::Discovery;
use clap::Parser;
use serde_json;
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Parser, Debug)]
#[command(name = "discover", about = "Discover LocalSend devices on network")]
pub struct DiscoverCommand {
    #[arg(short, long, default_value = "10")]
    timeout: u64,

    #[arg(short, long)]
    json: bool,
}

pub async fn execute(command: DiscoverCommand) -> anyhow::Result<()> {
    let mut discovery = crate::discovery::MulticastDiscovery::new(
        "LocalSend-Rust".to_string(),
        53317,
        crate::protocol::Protocol::Https,
    )?;

    let devices = Arc::new(Mutex::new(Vec::<DeviceInfo>::new()));
    let devices_clone = devices.clone();

    discovery.on_discovered(move |device: DeviceInfo| {
        let mut devices = devices_clone.lock().unwrap();
        // Avoid duplicates
        if !devices.iter().any(|d| d.fingerprint == device.fingerprint) {
            devices.push(device);
        }
    });

    discovery.start().await?;
    discovery.announce_presence().await?;

    let timeout_duration = Duration::from_secs(command.timeout);
    tokio::time::sleep(timeout_duration).await;

    discovery.stop();

    let devices = devices.lock().unwrap();

    if command.json {
        println!("{}", serde_json::to_string_pretty(&*devices)?);
    } else {
        display_devices(&devices);
    }

    Ok(())
}

fn display_devices(devices: &[DeviceInfo]) {
    if devices.is_empty() {
        println!("No devices discovered");
        return;
    }

    println!("Discovered {} device(s):", devices.len());
    for device in devices {
        println!("  - {} (port {})", device.alias, device.port);
    }
}
