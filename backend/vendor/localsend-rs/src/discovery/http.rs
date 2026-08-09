use crate::core::device::{get_device_model, get_device_type};
#[cfg(feature = "https")]
use crate::crypto::TlsCertificate;
use crate::crypto::generate_fingerprint;
use crate::discovery::Discovery;
use crate::error::LocalSendError;
use crate::protocol::{DeviceInfo, PROTOCOL_VERSION, Protocol};
use futures_util::{StreamExt, stream};
use reqwest::Client;
use std::collections::HashSet;
use std::future::Future;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::sync::broadcast;

pub type Result<T> = std::result::Result<T, LocalSendError>;

/// Concurrent HTTP probes in flight during a subnet scan. Matches the official
/// LocalSend client and localsend-ts (`concurrency: 50`).
const SCAN_CONCURRENCY: usize = 50;

/// How long to wait for a host's TCP connect. Live LAN devices answer well within this;
/// unreachable hosts (most of a `/24`) are abandoned after it, so it bounds the scan's
/// wall-clock. A host that fails to connect is not retried on the other scheme.
const CONNECT_TIMEOUT: Duration = Duration::from_millis(1000);

/// Overall per-probe timeout (connect + response).
const REQUEST_TIMEOUT: Duration = Duration::from_millis(2000);

pub struct HttpDiscovery {
    local_device: DeviceInfo,
    client: Client,
    running: Arc<AtomicBool>,
    tx: Option<broadcast::Sender<DeviceInfo>>,
}

impl HttpDiscovery {
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

        Ok(Self {
            local_device: device,
            client: build_discovery_client(
                #[cfg(feature = "https")]
                None,
            )?,
            running: Arc::new(AtomicBool::new(false)),
            tx: None,
        })
    }

    #[cfg(feature = "https")]
    pub fn new_with_device_and_identity(
        device: DeviceInfo,
        identity: &TlsCertificate,
    ) -> Result<Self> {
        Ok(Self {
            local_device: device,
            client: build_discovery_client(Some(identity))?,
            running: Arc::new(AtomicBool::new(false)),
            tx: None,
        })
    }

    /// Sweeps every host `x.y.z.1..=254` in the `/24` subnet of `base_ip` (excluding
    /// our own address), asking each one to register and returning the LocalSend devices
    /// that answered. Legacy peers which do not support `/register` are retried through
    /// `/info`. This finds any device whose HTTP server is reachable, even one that is
    /// missing multicast (lossy Wi-Fi, or a mobile app suspended in the background).
    ///
    /// Probes run concurrently ([`SCAN_CONCURRENCY`] at a time). LocalSend devices use
    /// self-signed certificates, so the client accepts them — the peer's real fingerprint
    /// is read from the response, so nothing is trusted blindly. Mirrors localsend-ts
    /// `HttpDiscovery` and the official `HttpScanDiscoveryService`.
    pub async fn scan_subnet(&self, base_ip: &str) -> Result<Vec<DeviceInfo>> {
        self.scan_ips(subnet_hosts(base_ip)?).await
    }

    /// Probe a caller-supplied set of hosts over LocalSend's HTTP discovery endpoints.
    /// This is useful for routed networks where the caller knows a
    /// reachable address but cannot enumerate it through multicast; it still
    /// performs the same TLS/HTTP negotiation, response decoding and
    /// fingerprint-based de-duplication as a subnet scan.
    pub async fn scan_ips(&self, ips: Vec<String>) -> Result<Vec<DeviceInfo>> {
        let mut devices = Vec::new();
        self.scan_ips_incremental(ips, |device| devices.push(device))
            .await?;
        Ok(devices)
    }

    /// Probe hosts through one globally bounded pool and publish each valid peer as soon as
    /// its registration request completes. Dropping this future cancels every in-flight probe.
    pub async fn scan_ips_incremental<F>(&self, ips: Vec<String>, on_discovered: F) -> Result<()>
    where
        F: FnMut(DeviceInfo),
    {
        self.scan_ips_incremental_with_limit(ips, SCAN_CONCURRENCY, on_discovered)
            .await
    }

    /// Incremental explicit-host scan with a caller-selected global concurrency bound.
    /// Intended for small priority sets such as recently cached peer addresses.
    pub async fn scan_ips_incremental_with_limit<F>(
        &self,
        ips: Vec<String>,
        concurrency: usize,
        mut on_discovered: F,
    ) -> Result<()>
    where
        F: FnMut(DeviceInfo),
    {
        let mut seen = HashSet::new();
        scan_incrementally(
            ips,
            concurrency,
            |ip| async move { self.probe_device(&ip).await },
            |device| {
                // Skip ourselves and de-duplicate by fingerprint. A peer may answer on
                // multiple addresses, but its first valid result is published immediately.
                if device.fingerprint.is_empty()
                    || device.fingerprint == self.local_device.fingerprint
                    || !seen.insert(device.fingerprint.clone())
                {
                    return;
                }
                on_discovered(device);
            },
        )
        .await;
        Ok(())
    }

    /// Revalidate known peers using their last advertised address, port and protocol.
    /// The alternate scheme is still attempted on the same custom port for compatibility.
    pub async fn scan_devices_incremental_with_limit<F>(
        &self,
        devices: Vec<DeviceInfo>,
        concurrency: usize,
        mut on_discovered: F,
    ) -> Result<()>
    where
        F: FnMut(DeviceInfo),
    {
        let mut seen = HashSet::new();
        scan_incrementally(
            devices,
            concurrency,
            |device| async move { self.probe_known_device(&device).await },
            |device| {
                if device.fingerprint.is_empty()
                    || device.fingerprint == self.local_device.fingerprint
                    || !seen.insert(device.fingerprint.clone())
                {
                    return;
                }
                on_discovered(device);
            },
        )
        .await;
        Ok(())
    }

    /// Probe a single host through `/register`, falling back to legacy `/info`. Tries the
    /// configured protocol first and,
    /// like localsend-ts, falls back to the other scheme so an HTTPS scan still finds an
    /// HTTP-only peer (and vice-versa). A host that is unreachable at the TCP level is not
    /// retried on the other scheme — it would fail there too — which keeps the scan fast
    /// over a subnet that is mostly empty.
    async fn probe_device(&self, ip: &str) -> Option<DeviceInfo> {
        for protocol in self.protocol_candidates() {
            match self
                .probe_device_with(ip, self.local_device.port, protocol)
                .await
            {
                ProbeOutcome::Found(device) => return Some(device),
                ProbeOutcome::Unreachable => return None,
                ProbeOutcome::Miss => continue,
            }
        }
        None
    }

    async fn probe_known_device(&self, target: &DeviceInfo) -> Option<DeviceInfo> {
        let ip = target.ip.as_deref()?;
        let protocols = match target.protocol {
            Protocol::Https => [Protocol::Https, Protocol::Http],
            Protocol::Http => [Protocol::Http, Protocol::Https],
        };
        for protocol in protocols {
            match self.probe_device_with(ip, target.port, protocol).await {
                ProbeOutcome::Found(device) => return Some(device),
                ProbeOutcome::Unreachable => return None,
                ProbeOutcome::Miss => continue,
            }
        }
        None
    }

    fn protocol_candidates(&self) -> [Protocol; 2] {
        match self.local_device.protocol {
            Protocol::Https => [Protocol::Https, Protocol::Http],
            Protocol::Http => [Protocol::Http, Protocol::Https],
        }
    }

    /// `ip`/`port`/`protocol` on the returned device are taken from the connection we
    /// actually made, because peers may omit them from discovery responses.
    async fn probe_device_with(&self, ip: &str, port: u16, protocol: Protocol) -> ProbeOutcome {
        let base_url = format!("{}://{}:{}", protocol, ip, port);
        let register_url = format!("{base_url}/api/localsend/v2/register");
        let response = match self
            .client
            .post(&register_url)
            .json(&self.local_device)
            .send()
            .await
        {
            Ok(response) => response,
            // Connect failures/timeouts mean nothing is listening on this host — the other
            // scheme won't fare better, so signal the caller to stop probing this host.
            // A TLS handshake against an HTTP-only LocalSend server is reported by
            // reqwest as a connect error too. Only a timeout proves the host is
            // unreachable for both schemes; connection errors must try the fallback.
            Err(e) if e.is_timeout() => return ProbeOutcome::Unreachable,
            Err(_) => return ProbeOutcome::Miss,
        };
        let mut device: DeviceInfo = if response.status().is_success() {
            match response.json().await {
                Ok(device) => device,
                Err(_) => match self.probe_legacy_info(&base_url).await {
                    Some(device) => device,
                    None => return ProbeOutcome::Miss,
                },
            }
        } else {
            match self.probe_legacy_info(&base_url).await {
                Some(device) => device,
                None => return ProbeOutcome::Miss,
            }
        };
        device.ip = Some(ip.to_string());
        device.port = port;
        device.protocol = protocol;
        tracing::info!(
            "[DISCOVER/TCP] {} ({}, model: {:?})",
            device.alias,
            ip,
            device.device_model
        );
        ProbeOutcome::Found(device)
    }

    async fn probe_legacy_info(&self, base_url: &str) -> Option<DeviceInfo> {
        let url = format!("{base_url}/api/localsend/v2/info");
        let response = self.client.get(url).send().await.ok()?;
        if !response.status().is_success() {
            return None;
        }
        response.json().await.ok()
    }
}

async fn scan_incrementally<Target, Output, Probe, ProbeFuture, OnDiscovered>(
    targets: Vec<Target>,
    concurrency: usize,
    probe: Probe,
    mut on_discovered: OnDiscovered,
) where
    Probe: Fn(Target) -> ProbeFuture,
    ProbeFuture: Future<Output = Option<Output>>,
    OnDiscovered: FnMut(Output),
{
    let mut pending = stream::iter(targets)
        .map(probe)
        .buffer_unordered(concurrency.max(1));
    while let Some(result) = pending.next().await {
        if let Some(item) = result {
            on_discovered(item);
        }
    }
}

/// Result of probing one host on one scheme.
enum ProbeOutcome {
    /// A LocalSend device answered.
    Found(DeviceInfo),
    /// Nothing is listening (connect failed/timed out) — don't try the other scheme.
    Unreachable,
    /// The host answered but not as a LocalSend peer on this scheme — try the next one.
    Miss,
}

/// Host addresses `x.y.z.1..=254` in the `/24` of `base_ip`, excluding `base_ip` itself.
fn subnet_hosts(base_ip: &str) -> Result<Vec<String>> {
    let octets: Vec<&str> = base_ip.split('.').collect();
    if octets.len() != 4 {
        return Err(LocalSendError::network(format!(
            "Invalid base IP for subnet scan: {base_ip}"
        )));
    }
    let prefix = format!("{}.{}.{}", octets[0], octets[1], octets[2]);
    Ok((1u8..=254)
        .map(|host| format!("{prefix}.{host}"))
        .filter(|ip| ip != base_ip)
        .collect())
}

/// A reqwest client tuned for LAN discovery: accepts the self-signed certificates that
/// every LocalSend device presents, and bounds each probe so the scan finishes promptly.
fn build_discovery_client(
    #[cfg(feature = "https")] identity: Option<&TlsCertificate>,
) -> Result<Client> {
    let mut builder = Client::builder().danger_accept_invalid_certs(true);
    #[cfg(feature = "https")]
    if let Some(identity) = identity {
        let pem = format!("{}\n{}", identity.cert_pem, identity.key_pem);
        builder = builder
            .identity(reqwest::Identity::from_pem(pem.as_bytes()).map_err(|e| {
                LocalSendError::network(format!("Invalid discovery identity: {e}"))
            })?);
    }
    builder
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(REQUEST_TIMEOUT)
        .build()
        .map_err(LocalSendError::from)
}

#[async_trait::async_trait]
impl Discovery for HttpDiscovery {
    async fn start(&mut self) -> std::result::Result<(), LocalSendError> {
        if self.running.load(Ordering::Relaxed) {
            return Err(LocalSendError::network("Discovery already running"));
        }

        self.running.store(true, Ordering::Relaxed);

        let (tx, _rx) = broadcast::channel(100);
        self.tx = Some(tx);

        tracing::debug!("HttpDiscovery: passive; call scan_subnet() explicitly");

        Ok(())
    }

    fn stop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
        self.tx = None;
    }

    async fn announce_presence(&self) -> std::result::Result<(), LocalSendError> {
        Err(LocalSendError::network(
            "HTTP discovery doesn't support announce",
        ))
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

#[cfg(test)]
mod tests {
    use super::{HttpDiscovery, scan_incrementally, subnet_hosts};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    #[test]
    fn subnet_hosts_covers_1_to_254_excluding_self() {
        // `subnet_hosts` is a pure function; the address is an arbitrary input, not a real
        // host. Uses 192.0.2.0/24 (RFC 5737 TEST-NET-1, reserved for documentation) so the
        // test is obviously independent of whatever network the machine is on.
        let hosts = subnet_hosts("192.0.2.10").expect("valid base ip");

        // .1..=254 minus our own address.
        assert_eq!(hosts.len(), 253);
        assert!(!hosts.contains(&"192.0.2.10".to_string()));
        assert!(hosts.contains(&"192.0.2.1".to_string()));
        assert!(hosts.contains(&"192.0.2.254".to_string()));
        // Never the network/broadcast-ish .0 / .255.
        assert!(!hosts.contains(&"192.0.2.0".to_string()));
        assert!(!hosts.contains(&"192.0.2.255".to_string()));
    }

    #[test]
    fn subnet_hosts_rejects_a_malformed_base_ip() {
        assert!(subnet_hosts("not.an.ip").is_err());
        assert!(subnet_hosts("192.0.2").is_err());
    }

    #[cfg(feature = "https")]
    #[tokio::test]
    async fn scan_subnet_finds_a_self_signed_https_server() {
        use crate::{LocalSendServer, Protocol};

        let output = tempfile::tempdir().expect("output directory");
        let (mut server, _events) = LocalSendServer::builder()
            .alias("scan-target")
            .port(0)
            .save_dir(output.path())
            .protocol(Protocol::Https)
            .build()
            .await
            .expect("start HTTPS receiver");
        let expected_fingerprint = server.device().fingerprint.clone();

        // Probe just the loopback host the server is on, over the (self-signed) TLS the
        // real devices use. This exercises the public explicit-target path used by
        // routed E2E environments.
        let discovery = HttpDiscovery::new("scanner".into(), server.port(), Protocol::Https)
            .expect("build discovery");
        let found = discovery
            .scan_ips(vec!["127.0.0.1".to_string()])
            .await
            .expect("scan explicit loopback target");

        let target = found
            .iter()
            .find(|d| d.fingerprint == expected_fingerprint)
            .expect("the self-signed HTTPS server must be discovered over TLS");
        assert_eq!(target.alias, "scan-target");
        assert_eq!(target.ip.as_deref(), Some("127.0.0.1"));
        assert_eq!(target.port, server.port());
        assert_eq!(target.protocol, Protocol::Https);

        server.stop();
    }

    #[tokio::test]
    async fn https_scanner_falls_back_to_an_http_server() {
        use crate::{LocalSendServer, Protocol};

        let output = tempfile::tempdir().expect("output directory");
        let (mut server, mut events) = LocalSendServer::builder()
            .alias("http-scan-target")
            .port(0)
            .save_dir(output.path())
            .protocol(Protocol::Http)
            .build()
            .await
            .expect("start HTTP receiver");
        let expected_fingerprint = server.device().fingerprint.clone();

        // CrossCopy advertises HTTPS, so its scanner tries TLS first. The TLS
        // handshake against this HTTP endpoint fails with reqwest's connect flag;
        // discovery must still retry the same port over HTTP.
        let discovery = HttpDiscovery::new("scanner".into(), server.port(), Protocol::Https)
            .expect("build discovery");
        let found = discovery
            .scan_ips(vec!["127.0.0.1".to_string()])
            .await
            .expect("scan localhost");

        let target = found
            .iter()
            .find(|device| device.fingerprint == expected_fingerprint)
            .expect("the HTTP server must be discovered after the HTTPS attempt");
        assert_eq!(target.alias, "http-scan-target");
        assert_eq!(target.protocol, Protocol::Http);
        let registered = tokio::time::timeout(Duration::from_secs(1), events.recv())
            .await
            .expect("scanner must POST /register")
            .expect("server event channel remains open");
        assert!(matches!(
            registered,
            crate::server::ServerEvent::PeerRegistered(device) if device.alias == "scanner"
        ));

        server.stop();
    }

    #[tokio::test]
    async fn cached_probe_uses_the_peers_custom_port_and_protocol() {
        use crate::{LocalSendServer, Protocol};

        let output = tempfile::tempdir().expect("output directory");
        let (mut server, _events) = LocalSendServer::builder()
            .alias("custom-port-target")
            .port(0)
            .save_dir(output.path())
            .protocol(Protocol::Http)
            .build()
            .await
            .expect("start HTTP receiver");
        let mut cached = server.device().clone();
        cached.ip = Some("127.0.0.1".into());

        let discovery =
            HttpDiscovery::new("scanner".into(), 53317, Protocol::Https).expect("build discovery");
        let mut found = Vec::new();
        discovery
            .scan_devices_incremental_with_limit(vec![cached], 1, |device| found.push(device))
            .await
            .expect("probe cached peer");

        assert_eq!(found.len(), 1);
        assert_eq!(found[0].alias, "custom-port-target");
        assert_eq!(found[0].port, server.port());
        assert_eq!(found[0].protocol, Protocol::Http);
        server.stop();
    }

    #[tokio::test(start_paused = true)]
    async fn fast_peer_is_published_while_slow_host_is_still_pending() {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let task = tokio::spawn(async move {
            scan_incrementally(
                vec!["slow".to_string(), "fast".to_string()],
                2,
                |host| async move {
                    if host == "fast" {
                        tokio::time::sleep(Duration::from_millis(10)).await;
                    } else {
                        tokio::time::sleep(Duration::from_secs(30)).await;
                    }
                    Some(host)
                },
                move |host| tx.send(host).expect("receiver remains open"),
            )
            .await;
        });
        tokio::task::yield_now().await;
        tokio::time::advance(Duration::from_millis(10)).await;
        tokio::task::yield_now().await;
        assert_eq!(rx.try_recv().expect("fast result emitted"), "fast");
        assert!(!task.is_finished(), "slow host must still be pending");
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
    }

    #[tokio::test(start_paused = true)]
    async fn one_global_limit_bounds_all_targets_and_cancellation_drops_probes() {
        struct ActiveProbe(Arc<AtomicUsize>);
        impl Drop for ActiveProbe {
            fn drop(&mut self) {
                self.0.fetch_sub(1, Ordering::SeqCst);
            }
        }

        let active = Arc::new(AtomicUsize::new(0));
        let maximum = Arc::new(AtomicUsize::new(0));
        let targets = (0..20)
            .map(|index| {
                if index % 2 == 0 {
                    format!("10.0.0.{index}")
                } else {
                    format!("192.168.1.{index}")
                }
            })
            .collect();
        let task_active = active.clone();
        let task_maximum = maximum.clone();
        let task = tokio::spawn(async move {
            scan_incrementally(
                targets,
                3,
                move |_| {
                    let active = task_active.clone();
                    let maximum = task_maximum.clone();
                    async move {
                        let current = active.fetch_add(1, Ordering::SeqCst) + 1;
                        maximum.fetch_max(current, Ordering::SeqCst);
                        let _guard = ActiveProbe(active);
                        std::future::pending::<Option<()>>().await
                    }
                },
                |_| {},
            )
            .await;
        });
        tokio::task::yield_now().await;
        assert_eq!(maximum.load(Ordering::SeqCst), 3);
        assert_eq!(active.load(Ordering::SeqCst), 3);
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
        assert_eq!(active.load(Ordering::SeqCst), 0, "no probe survives cancel");
    }

    #[tokio::test]
    #[ignore = "requires CROSSCOPY_E2E_LOCALSEND_TARGET to name a reachable LocalSend peer"]
    async fn scan_ips_finds_an_explicit_e2e_peer() {
        use crate::Protocol;

        let target = std::env::var("CROSSCOPY_E2E_LOCALSEND_TARGET")
            .expect("set CROSSCOPY_E2E_LOCALSEND_TARGET to a LocalSend peer IP");
        let discovery = HttpDiscovery::new("e2e-scanner".into(), 53317, Protocol::Https)
            .expect("build discovery client");

        let found = discovery
            .scan_ips(vec![target.clone()])
            .await
            .expect("probe explicit E2E target");
        assert!(
            found
                .iter()
                .any(|peer| peer.ip.as_deref() == Some(target.as_str())),
            "the explicit LocalSend target must answer HTTP discovery"
        );
    }
}
