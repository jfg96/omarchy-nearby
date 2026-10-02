//! Every multicast announcement asks the receiver to connect back to the
//! address and port it names. Announcements are single unauthenticated
//! datagrams, so the replies they trigger must be bounded.
//!
//! The discovery socket is bound to the wildcard address, so these tests
//! deliver announcements as unicast datagrams over loopback; the receive path
//! is the same one multicast traffic takes.

use localsend_rs::discovery::{Discovery, MulticastConfig, MulticastDiscovery};
use localsend_rs::protocol::DEFAULT_MULTICAST_ADDRESS;
use localsend_rs::{DeviceInfo, Protocol};
use serde_json::json;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use tokio::net::{TcpListener, UdpSocket};

/// A peer that accepts reply connections, counts them and never answers.
async fn silent_peer() -> (u16, Arc<AtomicUsize>) {
    let listener = TcpListener::bind((Ipv4Addr::UNSPECIFIED, 0)).await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let accepted = Arc::new(AtomicUsize::new(0));
    let counter = accepted.clone();
    tokio::spawn(async move {
        let mut held = Vec::new();
        while let Ok((stream, _)) = listener.accept().await {
            counter.fetch_add(1, Ordering::SeqCst);
            held.push(stream);
        }
    });
    (port, accepted)
}

/// Start passive discovery on a free UDP port, or `None` when this machine
/// has no IPv4 interface able to join the multicast group.
async fn discovery() -> Option<(MulticastDiscovery, u16)> {
    let port = std::net::UdpSocket::bind((Ipv4Addr::LOCALHOST, 0))
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let mut device = DeviceInfo::new("Receiver".into(), 53317, Protocol::Http);
    device.fingerprint = "receiver".into();
    let config = MulticastConfig {
        address: DEFAULT_MULTICAST_ADDRESS.parse().unwrap(),
        port,
        interface_names: None,
    };
    let mut discovery = MulticastDiscovery::new_with_device_and_config(device, config).unwrap();
    match discovery.start().await {
        Ok(()) => Some((discovery, port)),
        Err(error) => {
            eprintln!("skipping: multicast discovery unavailable here: {error}");
            None
        }
    }
}

async fn announce(from: Ipv4Addr, to_port: u16, reply_port: u16, index: usize) {
    let socket = UdpSocket::bind((from, 0)).await.unwrap();
    let message = json!({
        "alias": "Flood",
        "version": "2.1",
        "fingerprint": format!("flood-{index}"),
        "port": reply_port,
        "protocol": "http",
        "announce": true,
        "announcement": true,
    });
    socket
        .send_to(
            message.to_string().as_bytes(),
            SocketAddr::from((Ipv4Addr::LOCALHOST, to_port)),
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn a_burst_from_one_source_opens_one_reply_connection() {
    let Some((mut discovery, port)) = discovery().await else {
        return;
    };
    let (reply_port, accepted) = silent_peer().await;
    for index in 0..50 {
        announce(Ipv4Addr::LOCALHOST, port, reply_port, index).await;
    }
    tokio::time::sleep(Duration::from_secs(2)).await;
    let accepted = accepted.load(Ordering::SeqCst);
    discovery.stop();
    assert_eq!(
        accepted, 1,
        "50 announcements from one address opened {accepted} reply connections"
    );
}

#[tokio::test]
async fn replies_to_many_sources_are_bounded() {
    let Some((mut discovery, port)) = discovery().await else {
        return;
    };
    let (reply_port, accepted) = silent_peer().await;
    for index in 0..40 {
        let source = Ipv4Addr::new(127, 0, 0, 10 + index as u8);
        announce(source, port, reply_port, index).await;
    }
    tokio::time::sleep(Duration::from_secs(2)).await;
    let accepted = accepted.load(Ordering::SeqCst);
    discovery.stop();
    assert!(
        (1..=8).contains(&accepted),
        "announcements from 40 addresses opened {accepted} concurrent reply connections"
    );
}
