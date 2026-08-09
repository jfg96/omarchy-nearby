use crate::client::{LocalSendClient, TlsTrustPolicy};
use crate::core::device::{get_device_model, get_device_type, get_local_ip};
#[cfg(feature = "https")]
use crate::crypto::TlsCertificate;
use crate::crypto::generate_fingerprint;
use crate::discovery::Discovery;
use crate::error::LocalSendError;
use crate::protocol::{
    AnnouncementMessage, DEFAULT_MULTICAST_ADDRESS, DEFAULT_MULTICAST_PORT, DeviceInfo,
    PROTOCOL_VERSION, Protocol,
};
use if_addrs::{IfAddr, get_if_addrs};
use socket2::{Domain, Protocol as SocketProtocol, Socket, Type};
use std::collections::{BTreeMap, BTreeSet};
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tokio::net::UdpSocket;
use tokio::sync::Notify;
use tokio::sync::broadcast;

pub type Result<T> = std::result::Result<T, LocalSendError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MulticastConfig {
    pub address: Ipv4Addr,
    pub port: u16,
    pub interface_names: Option<BTreeSet<String>>,
}

impl MulticastConfig {
    pub fn new(
        address: Ipv4Addr,
        port: u16,
        interface_names: Option<BTreeSet<String>>,
    ) -> Result<Self> {
        if !address.is_multicast() {
            return Err(LocalSendError::InvalidMulticastAddress(address.to_string()));
        }
        if port == 0 {
            return Err(LocalSendError::InvalidPort(port.to_string()));
        }
        Ok(Self {
            address,
            port,
            interface_names,
        })
    }
}

impl Default for MulticastConfig {
    fn default() -> Self {
        Self {
            address: DEFAULT_MULTICAST_ADDRESS
                .parse()
                .expect("LocalSend's multicast constant must be a valid IPv4 address"),
            port: DEFAULT_MULTICAST_PORT,
            interface_names: None,
        }
    }
}

#[derive(Clone)]
pub struct MulticastDiscovery {
    local_device: DeviceInfo,
    config: MulticastConfig,
    sockets: Vec<Arc<UdpSocket>>,
    socket_interfaces: Vec<(String, Ipv4Addr)>,
    running: Arc<AtomicBool>,
    tx: Option<broadcast::Sender<DeviceInfo>>,
    stop_notify: Arc<Notify>,
    #[cfg(feature = "https")]
    tls_identity: Option<TlsCertificate>,
}

impl MulticastDiscovery {
    fn announcement_payload(&self) -> Result<(Vec<u8>, SocketAddr)> {
        if self.sockets.is_empty() {
            return Err(LocalSendError::network("Discovery not started"));
        }

        let announcement = AnnouncementMessage {
            alias: self.local_device.alias.clone(),
            version: self.local_device.version.clone(),
            device_model: self.local_device.device_model.clone(),
            device_type: self.local_device.device_type,
            fingerprint: self.local_device.fingerprint.clone(),
            port: self.local_device.port,
            protocol: self.local_device.protocol,
            download: self.local_device.download,
            announce: true,
            announcement: Some(true),
        };

        Ok((
            serde_json::to_vec(&announcement)?,
            SocketAddr::from((self.config.address, self.config.port)),
        ))
    }

    async fn send_announcement(&self) -> Result<()> {
        let (payload, multicast_addr) = self.announcement_payload()?;
        let mut any_sent = false;
        let mut last_error = None;
        for (socket, (name, interface)) in self.sockets.iter().zip(&self.socket_interfaces) {
            match socket.send_to(&payload, &multicast_addr).await {
                Ok(_) => {
                    any_sent = true;
                    eprintln!("announce: {name} ({interface}) -> sent");
                }
                Err(error) => {
                    eprintln!("announce: {name} ({interface}) -> failed: {error}");
                    last_error = Some(error);
                }
            }
        }
        if any_sent {
            Ok(())
        } else {
            Err(LocalSendError::network(format!(
                "Announcement failed on every interface: {}",
                last_error
                    .map(|error| error.to_string())
                    .unwrap_or_else(|| "no socket attempted".into())
            )))
        }
    }

    /// Send the first multicast announcement without a startup delay.
    pub async fn announce_presence_once(&self) -> Result<()> {
        self.send_announcement().await
    }

    /// Continue the bounded reliability retries after an initial announcement.
    /// Dropping this future cancels the remaining retries.
    pub async fn retry_announcements(&self) -> Result<()> {
        let mut any_sent = false;
        let mut last_error = None;
        for delay in [100, 500, 2000] {
            tokio::time::sleep(Duration::from_millis(delay)).await;
            match self.send_announcement().await {
                Ok(()) => any_sent = true,
                Err(error) => last_error = Some(error),
            }
        }
        if any_sent {
            Ok(())
        } else {
            Err(last_error
                .unwrap_or_else(|| LocalSendError::network("Announcement retries failed")))
        }
    }

    pub fn new(alias: String, port: u16, protocol: Protocol) -> Result<Self> {
        let device = DeviceInfo {
            alias,
            version: PROTOCOL_VERSION.to_string(),
            device_model: Some(get_device_model()),
            device_type: Some(get_device_type()),
            fingerprint: generate_fingerprint(),
            port,
            protocol,
            download: false,
            ip: None,
        };

        Ok(Self::new_with_device(device))
    }

    pub fn new_with_device(device: DeviceInfo) -> Self {
        Self::new_with_device_and_config(device, MulticastConfig::default())
            .expect("default multicast configuration must be valid")
    }

    pub fn new_with_device_and_config(device: DeviceInfo, config: MulticastConfig) -> Result<Self> {
        let config = MulticastConfig::new(config.address, config.port, config.interface_names)?;
        let (tx, _rx) = broadcast::channel(100);
        Ok(Self {
            local_device: device,
            config,
            sockets: Vec::new(),
            socket_interfaces: Vec::new(),
            running: Arc::new(AtomicBool::new(false)),
            tx: Some(tx),
            stop_notify: Arc::new(Notify::new()),
            #[cfg(feature = "https")]
            tls_identity: None,
        })
    }

    #[cfg(feature = "https")]
    pub fn new_with_device_and_tls_identity(device: DeviceInfo, identity: TlsCertificate) -> Self {
        let mut discovery = Self::new_with_device(device);
        discovery.tls_identity = Some(identity);
        discovery
    }

    /// Replace the identity used by future announcements without rebuilding
    /// sockets or losing the current discovery cache.
    pub fn set_local_device(&mut self, device: DeviceInfo) {
        self.local_device = device;
    }
}

#[async_trait::async_trait]
impl Discovery for MulticastDiscovery {
    async fn start(&mut self) -> std::result::Result<(), LocalSendError> {
        if self.running.load(Ordering::Relaxed) {
            return Err(LocalSendError::network("Discovery already running"));
        }

        let bind_addr = SocketAddr::from((Ipv4Addr::UNSPECIFIED, self.config.port));
        let interfaces = Self::multicast_interface_entries(self.config.interface_names.as_ref())?;
        eprintln!("interfaces selected:");
        let mut sockets = Vec::new();
        let mut socket_interfaces = Vec::new();
        for (name, interface) in interfaces {
            eprintln!("  {name} ({interface})");
            match create_reusable_udp_socket(&bind_addr, interface, self.config.address) {
                Ok(socket) => {
                    sockets.push(Arc::new(socket));
                    socket_interfaces.push((name, interface));
                }
                Err(error) => eprintln!("  {name} ({interface}) -> socket failed: {error}"),
            }
        }
        if sockets.is_empty() {
            return Err(LocalSendError::network("No usable multicast interface"));
        }

        self.sockets = sockets.clone();
        self.socket_interfaces = socket_interfaces;
        self.running.store(true, Ordering::Relaxed);

        for socket in sockets {
            let tx = self.tx.as_ref().unwrap().clone();
            let local_fingerprint = self.local_device.fingerprint.clone();
            let running = self.running.clone();
            let stop_notify = self.stop_notify.clone();
            let local_device = self.local_device.clone();
            let multicast_addr = SocketAddr::from((self.config.address, self.config.port));
            #[cfg(feature = "https")]
            let tls_identity = self.tls_identity.clone();

            tokio::spawn(async move {
                let mut buf = vec![0u8; 65536];

                while running.load(Ordering::Relaxed) {
                    let received = tokio::select! {
                        value = socket.recv_from(&mut buf) => Some(value),
                        _ = stop_notify.notified() => None,
                    };
                    match received {
                        Some(Ok((len, src))) => {
                            if len > 0 {
                                let msg = match String::from_utf8(buf[..len].to_vec()) {
                                    Ok(s) => s,
                                    Err(_) => continue,
                                };

                                if let Ok(announcement) =
                                    serde_json::from_str::<AnnouncementMessage>(&msg)
                                {
                                    if announcement.fingerprint == local_fingerprint {
                                        continue;
                                    }

                                    let device = DeviceInfo {
                                        alias: announcement.alias.clone(),
                                        version: announcement.version.clone(),
                                        device_model: announcement.device_model.clone(),
                                        device_type: announcement.device_type,
                                        fingerprint: announcement.fingerprint.clone(),
                                        port: announcement.port,
                                        protocol: announcement.protocol,
                                        download: announcement.download,
                                        ip: Some(src.ip().to_string()),
                                    };

                                    eprintln!("register/announcement received from {}", src.ip());

                                    let is_announcement = announcement.announce
                                        || announcement.announcement.unwrap_or(false);
                                    let _ = tx.send(device.clone());

                                    if is_announcement {
                                        let local_device = local_device.clone();
                                        let socket = socket.clone();
                                        #[cfg(feature = "https")]
                                        let tls_identity = tls_identity.clone();

                                        tokio::spawn(async move {
                                            Self::respond_to_announcement(
                                                &device,
                                                &local_device,
                                                &socket,
                                                multicast_addr,
                                                #[cfg(feature = "https")]
                                                tls_identity.as_ref(),
                                            )
                                            .await;
                                        });
                                    }
                                }
                            }
                        }
                        Some(Err(_)) => continue,
                        None => break,
                    }
                }
            });
        }

        Ok(())
    }

    fn stop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
        self.stop_notify.notify_waiters();
        self.sockets.clear();
        self.socket_interfaces.clear();
        self.tx = None;
    }

    async fn announce_presence(&self) -> std::result::Result<(), LocalSendError> {
        self.announce_presence_once().await?;
        if let Err(error) = self.retry_announcements().await {
            eprintln!("announcement retries failed after initial success: {error}");
        }
        Ok(())
    }

    fn on_discovered<F>(&mut self, callback: F)
    where
        F: Fn(DeviceInfo) + Send + Sync + 'static,
    {
        let tx = if let Some(ref t) = self.tx {
            t.clone()
        } else {
            return;
        };

        tokio::spawn(async move {
            let mut rx = tx.subscribe();
            while let Ok(device) = rx.recv().await {
                callback(device);
            }
        });
    }

    fn get_known_devices(&self) -> Vec<DeviceInfo> {
        vec![]
    }
}

impl MulticastDiscovery {
    fn multicast_interface_entries(
        interface_names: Option<&BTreeSet<String>>,
    ) -> Result<Vec<(String, Ipv4Addr)>> {
        let addresses = get_if_addrs()
            .map_err(|error| {
                LocalSendError::network(format!("Failed to list interfaces: {error}"))
            })?
            .into_iter()
            .filter(|interface| !interface.is_loopback())
            .filter_map(|interface| match interface.addr {
                IfAddr::V4(address) => Some((interface.name, address.ip, address.netmask)),
                IfAddr::V6(_) => None,
            });
        let primary = interface_names
            .is_none()
            .then(|| get_local_ip().ok())
            .flatten();
        Ok(select_multicast_candidate_entries(
            addresses,
            interface_names,
            primary,
        ))
    }

    pub fn local_ipv4_interfaces() -> Result<Vec<Ipv4Addr>> {
        Self::local_ipv4_interfaces_with_netmasks()
            .map(|addresses| addresses.into_iter().map(|(address, _)| address).collect())
    }

    pub fn local_ipv4_interfaces_with_netmasks() -> Result<Vec<(Ipv4Addr, Ipv4Addr)>> {
        let selected = Self::multicast_interface_entries(None)?;
        let netmasks = get_if_addrs()
            .map_err(|error| {
                LocalSendError::network(format!("Failed to list interfaces: {error}"))
            })?
            .into_iter()
            .filter_map(|interface| match interface.addr {
                IfAddr::V4(address) => Some(((interface.name, address.ip), address.netmask)),
                IfAddr::V6(_) => None,
            })
            .collect::<BTreeMap<_, _>>();
        Ok(selected
            .into_iter()
            .filter_map(|(name, address)| {
                netmasks
                    .get(&(name, address))
                    .copied()
                    .map(|netmask| (address, netmask))
            })
            .filter(|(address, _)| {
                !address.is_unspecified() && !address.is_loopback() && !address.is_link_local()
            })
            .collect())
    }

    fn client_for_announcement(
        local_device: DeviceInfo,
        target_device: &DeviceInfo,
        #[cfg(feature = "https")] identity: Option<&TlsCertificate>,
    ) -> Result<LocalSendClient> {
        match target_device.protocol {
            Protocol::Http => Ok(LocalSendClient::new(local_device)),
            Protocol::Https => {
                let policy = TlsTrustPolicy::PinnedFingerprint(target_device.fingerprint.clone());
                #[cfg(feature = "https")]
                if let Some(identity) = identity {
                    return LocalSendClient::with_trust_policy_and_identity(
                        local_device,
                        policy,
                        identity,
                    );
                }
                LocalSendClient::with_trust_policy(local_device, policy)
            }
        }
    }

    pub async fn scan(
        &mut self,
        duration: Duration,
        devices: Arc<RwLock<Vec<DeviceInfo>>>,
    ) -> Result<()> {
        if !self.running.load(Ordering::Relaxed) {
            self.start().await?;
        }

        // Register a callback to update the devices list during the scan
        let devices_clone = devices.clone();
        self.on_discovered(move |device| {
            let mut guard = devices_clone.write().unwrap();
            if !guard.iter().any(|d| d.fingerprint == device.fingerprint) {
                guard.push(device);
            }
        });

        // Clear devices
        devices.write().unwrap().clear();

        // Announce
        self.announce_presence().await?;

        // Wait for responses
        tokio::time::sleep(duration).await;

        Ok(())
    }

    async fn respond_to_announcement(
        target_device: &DeviceInfo,
        local_device: &DeviceInfo,
        socket: &UdpSocket,
        multicast_addr: SocketAddr,
        #[cfg(feature = "https")] identity: Option<&TlsCertificate>,
    ) {
        tracing::debug!(
            "Responding to announcement from {} ({:?})",
            target_device.alias,
            target_device.ip
        );

        // The discovery announcement contains the peer's certificate fingerprint.
        // Use it for HTTPS registration instead of system CA verification.
        match Self::client_for_announcement(
            local_device.clone(),
            target_device,
            #[cfg(feature = "https")]
            identity,
        ) {
            Ok(client) => match client.register(target_device).await {
                Ok(_) => {
                    tracing::debug!(
                        "Successfully registered with {} via HTTP",
                        target_device.alias
                    );
                    return;
                }
                Err(error) => {
                    // If HTTP failed, we just fall back to UDP. This is common if the other device
                    // has a strict firewall or if we couldn't parse their response.
                    // It's not a critical error.
                    tracing::debug!(
                        "HTTP registration failed ({}), falling back to UDP...",
                        error
                    );
                }
            },
            Err(error) => {
                tracing::debug!(
                    "Could not configure pinned registration ({}), falling back to UDP...",
                    error
                );
            }
        }

        // Fallback: Send UDP response
        let announcement = AnnouncementMessage {
            alias: local_device.alias.clone(),
            version: local_device.version.clone(),
            device_model: local_device.device_model.clone(),
            device_type: local_device.device_type,
            fingerprint: local_device.fingerprint.clone(),
            port: local_device.port,
            protocol: local_device.protocol,
            download: local_device.download,
            announce: false,
            announcement: Some(false),
        };

        if let Ok(msg) = serde_json::to_string(&announcement) {
            let buf = msg.as_bytes();
            if let Err(e) = socket.send_to(buf, multicast_addr).await {
                tracing::debug!("Failed to send UDP fallback response: {}", e);
            } else {
                tracing::debug!("Sent UDP fallback response to multicast group");
            }
        }
    }
}

fn select_interface_entries(
    addresses: impl IntoIterator<Item = (String, Ipv4Addr, Ipv4Addr)>,
    interface_names: Option<&BTreeSet<String>>,
    primary: Option<Ipv4Addr>,
) -> Vec<(String, Ipv4Addr)> {
    let mut selected = addresses
        .into_iter()
        .filter(|(name, _, _)| interface_names.is_none_or(|names| names.contains(name)))
        .filter(|(_, address, _)| !address.is_unspecified() && !address.is_loopback())
        .filter(|(_, address, netmask)| {
            primary.is_none_or(|primary| same_subnet(*address, primary, *netmask))
        })
        .map(|(name, address, _)| (name, address))
        .collect::<Vec<_>>();
    selected.sort_by_key(|(name, address)| (Some(*address) != primary, name.clone(), *address));
    selected.dedup();
    selected
}

fn select_multicast_candidate_entries(
    addresses: impl IntoIterator<Item = (String, Ipv4Addr, Ipv4Addr)>,
    interface_names: Option<&BTreeSet<String>>,
    primary: Option<Ipv4Addr>,
) -> Vec<(String, Ipv4Addr)> {
    let addresses = addresses.into_iter().collect::<Vec<_>>();
    if interface_names.is_some() {
        return select_interface_entries(addresses, interface_names, None);
    }
    let addresses = without_container_bridges(addresses, primary);
    let mut candidates = select_interface_entries(addresses.iter().cloned(), None, primary);
    for candidate in select_interface_entries(addresses, None, None) {
        if !candidates
            .iter()
            .any(|(_, address)| *address == candidate.1)
        {
            candidates.push(candidate);
        }
    }
    candidates
}

fn without_container_bridges(
    addresses: Vec<(String, Ipv4Addr, Ipv4Addr)>,
    primary: Option<Ipv4Addr>,
) -> Vec<(String, Ipv4Addr, Ipv4Addr)> {
    let kept = addresses
        .iter()
        .filter(|(name, address, _)| Some(*address) == primary || !is_container_bridge(name))
        .cloned()
        .collect::<Vec<_>>();
    if kept.is_empty() { addresses } else { kept }
}

fn is_container_bridge(name: &str) -> bool {
    const PREFIXES: &[&str] = &[
        "br-", "docker", "veth", "virbr", "vboxnet", "vmnet", "lxcbr", "lxdbr", "podman", "cni-",
    ];
    PREFIXES.iter().any(|prefix| name.starts_with(prefix))
}

fn same_subnet(address: Ipv4Addr, primary: Ipv4Addr, netmask: Ipv4Addr) -> bool {
    let netmask = u32::from_be_bytes(netmask.octets());
    u32::from_be_bytes(address.octets()) & netmask == u32::from_be_bytes(primary.octets()) & netmask
}

/// Creates a UDP socket with port reuse enabled.
///
/// This is critical for LocalSend discovery because:
/// 1. The protocol uses a fixed multicast port (53317).
/// 2. Multiple instances (e.g., a background receiver and a short-lived discovery command)
///    need to join the same multicast group simultaneously.
///
/// By enabling SO_REUSEADDR (and SO_REUSEPORT on Unix), the OS allows multiple
/// processes to bind to the same UDP port. For multicast traffic, the OS will
/// clone incoming packets and deliver them to all participating sockets.
fn create_reusable_udp_socket(
    bind_addr: &SocketAddr,
    interface: Ipv4Addr,
    multicast_addr: Ipv4Addr,
) -> Result<UdpSocket> {
    let domain = if bind_addr.is_ipv4() {
        Domain::IPV4
    } else {
        Domain::IPV6
    };

    let socket = Socket::new(domain, Type::DGRAM, Some(SocketProtocol::UDP))
        .map_err(|e| LocalSendError::network(format!("Failed to create socket: {}", e)))?;

    // Enable address reuse (supported on most platforms including Windows)
    socket
        .set_reuse_address(true)
        .map_err(|e| LocalSendError::network(format!("Failed to set reuse_address: {}", e)))?;

    // Enable port reuse on Unix platforms to allow multiple processes to bind exactly to the same port
    #[cfg(all(unix, not(target_os = "solaris"), not(target_os = "illumos")))]
    socket
        .set_reuse_port(true)
        .map_err(|e| LocalSendError::network(format!("Failed to set reuse_port: {}", e)))?;

    socket
        .bind(&(*bind_addr).into())
        .map_err(|e| LocalSendError::network(format!("Failed to bind to {}: {}", bind_addr, e)))?;

    socket
        .join_multicast_v4(&multicast_addr, &interface)
        .map_err(|error| {
            LocalSendError::network(format!("Failed to join multicast on {interface}: {error}"))
        })?;
    socket.set_multicast_if_v4(&interface).map_err(|error| {
        LocalSendError::network(format!(
            "Failed to select multicast interface {interface}: {error}"
        ))
    })?;

    // Convert to tokio UdpSocket after configuring the multicast interface.
    let std_socket: std::net::UdpSocket = socket.into();
    std_socket
        .set_nonblocking(true)
        .map_err(|e| LocalSendError::network(format!("Failed to set non-blocking: {}", e)))?;

    UdpSocket::from_std(std_socket)
        .map_err(|e| LocalSendError::network(format!("Failed to convert to tokio socket: {}", e)))
}

#[cfg(test)]
mod tests {
    use super::{MulticastConfig, MulticastDiscovery, select_multicast_candidate_entries};
    use crate::LocalSendError;
    use std::collections::BTreeSet;
    use std::net::Ipv4Addr;

    #[derive(Clone)]
    struct TestInterface {
        name: String,
        address: Ipv4Addr,
        netmask: Ipv4Addr,
    }

    impl TestInterface {
        fn ipv4(name: &str, address: &str) -> Self {
            Self {
                name: name.into(),
                address: address.parse().unwrap(),
                netmask: Ipv4Addr::new(255, 255, 255, 0),
            }
        }
    }

    #[test]
    fn multicast_config_rejects_non_multicast_address() {
        let result = MulticastConfig::new("192.168.1.1".parse().unwrap(), 53317, None);
        assert!(matches!(
            result,
            Err(LocalSendError::InvalidMulticastAddress(_))
        ));
    }

    #[test]
    fn live_discovery_identity_can_toggle_browser_download_advertising() {
        let mut original = crate::DeviceInfo::new("CrossCopy".into(), 53317, crate::Protocol::Http);
        original.download = false;
        let mut discovery = MulticastDiscovery::new_with_device(original.clone());
        original.download = true;

        discovery.set_local_device(original.clone());

        assert_eq!(discovery.local_device, original);
    }

    #[test]
    fn interface_filter_keeps_only_named_ipv4_interfaces() {
        let interfaces = vec![
            TestInterface::ipv4("en0", "192.168.1.10"),
            TestInterface::ipv4("utun3", "10.0.0.2"),
        ];
        let selected = select_multicast_candidate_entries(
            interfaces
                .into_iter()
                .map(|interface| (interface.name, interface.address, interface.netmask)),
            Some(&BTreeSet::from(["en0".into()])),
            None,
        );
        assert_eq!(
            selected,
            vec![("en0".into(), "192.168.1.10".parse::<Ipv4Addr>().unwrap())]
        );
    }

    #[cfg(feature = "https")]
    use crate::{DeviceInfo, LocalSendServer, Protocol};

    #[cfg(feature = "https")]
    #[tokio::test]
    async fn announcement_client_pins_the_discovered_https_certificate() {
        let output = tempfile::tempdir().expect("output directory");
        let (mut server, _events) = LocalSendServer::builder()
            .alias("discovered HTTPS peer")
            .port(0)
            .save_dir(output.path())
            .protocol(Protocol::Https)
            .build()
            .await
            .expect("start HTTPS receiver");

        let mut peer = server.device().clone();
        peer.ip = Some("127.0.0.1".into());
        let local = DeviceInfo::new("discovery client".into(), 0, Protocol::Https);
        let client = MulticastDiscovery::client_for_announcement(
            local,
            &peer,
            #[cfg(feature = "https")]
            None,
        )
        .expect("build a client for the announced peer");

        client
            .register(&peer)
            .await
            .expect("the announced certificate fingerprint should be pinned");

        server.stop();
    }

    #[test]
    fn multicast_prioritizes_primary_keeps_other_lan_and_skips_container_bridge() {
        assert_eq!(
            select_multicast_candidate_entries(
                [
                    (
                        "unspecified".into(),
                        Ipv4Addr::UNSPECIFIED,
                        Ipv4Addr::new(255, 255, 255, 0)
                    ),
                    (
                        "loopback".into(),
                        Ipv4Addr::LOCALHOST,
                        Ipv4Addr::new(255, 0, 0, 0)
                    ),
                    (
                        "en0".into(),
                        Ipv4Addr::new(192, 168, 6, 10),
                        Ipv4Addr::new(255, 255, 255, 0)
                    ),
                    (
                        "en1".into(),
                        Ipv4Addr::new(192, 168, 6, 101),
                        Ipv4Addr::new(255, 255, 255, 0)
                    ),
                    (
                        "br-test".into(),
                        Ipv4Addr::new(192, 168, 139, 3),
                        Ipv4Addr::new(255, 255, 254, 0)
                    ),
                ],
                None,
                Some(Ipv4Addr::new(192, 168, 6, 101))
            ),
            vec![
                ("en1".into(), Ipv4Addr::new(192, 168, 6, 101)),
                ("en0".into(), Ipv4Addr::new(192, 168, 6, 10)),
            ]
        );
    }
}
