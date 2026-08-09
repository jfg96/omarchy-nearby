use anyhow::{Context, Result, anyhow};
use localsend_rs::LocalSendServer;
use localsend_rs::client::{LocalSendClient, TlsTrustPolicy};
use localsend_rs::core::build_file_metadata;
use localsend_rs::crypto::{TlsCertificate, generate_tls_certificate};
use localsend_rs::discovery::{Discovery, HttpDiscovery, MulticastDiscovery};
use localsend_rs::protocol::types::FileMetadataDetails;
use localsend_rs::protocol::{DeviceInfo, FileId, FileMetadata, Protocol};
use localsend_rs::server::{PendingRequest, ServerEvent};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::net::Ipv4Addr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::{mpsc, oneshot};

const PEER_TTL: Duration = Duration::from_secs(90);
const MULTICAST_GRACE: Duration = Duration::from_secs(1);
const CACHED_PROBE_CONCURRENCY: usize = 5;
const MAX_SUBNET_ADDRESSES: u32 = 1024;
const STALE_PARTIAL_AGE: Duration = Duration::from_secs(24 * 60 * 60);

#[derive(Debug, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
enum Command {
    DiscoveryStart {
        #[serde(default)]
        force_full: bool,
    },
    DiscoveryStop,
    Accept {
        request_id: String,
    },
    Decline {
        request_id: String,
    },
    SendFiles {
        transfer_id: String,
        device: DeviceInfo,
        paths: Vec<String>,
    },
    SendText {
        transfer_id: String,
        device: DeviceInfo,
        text: String,
    },
    CancelOutgoing {
        transfer_id: String,
    },
    Shutdown,
}

struct DiscoveryControl {
    stop: oneshot::Sender<()>,
    started: Instant,
}
struct OutgoingControl {
    id: String,
    cancel: oneshot::Sender<()>,
}
struct Peer {
    device: DeviceInfo,
    last_seen: Instant,
}

#[derive(Serialize, Deserialize)]
struct StoredIdentity {
    cert_pem: String,
    key_pem: String,
    fingerprint: String,
}

fn emit(value: Value) {
    println!("{}", value);
}

fn human_error(error: &anyhow::Error) -> &'static str {
    let text = format!("{error:#}").to_lowercase();
    if text.contains("permission") {
        "Permission denied"
    } else if text.contains("address already in use") || text.contains("addrinuse") {
        "Port 53317 is already in use"
    } else if text.contains("refused") || text.contains("unreachable") {
        "Device unavailable"
    } else if text.contains("timeout") || text.contains("timed out") {
        "Transfer timed out"
    } else if text.contains("403") || text.contains("rejected") {
        "Transfer declined"
    } else if text.contains("401") || text.contains("pin") {
        "PIN required"
    } else if text.contains("network") || text.contains("socket") {
        "Network unavailable"
    } else {
        "Transfer failed"
    }
}

fn event_device(device: &DeviceInfo) -> Value {
    json!({"alias":device.alias,"version":device.version,"deviceModel":device.device_model,
        "deviceType":device.device_type,"fingerprint":device.fingerprint,"port":device.port,
        "protocol":device.protocol,"download":device.download,"ip":device.ip})
}

fn valid_remote_text(text: &str, max: usize) -> String {
    text.chars()
        .filter(|c| !c.is_control() || *c == '\n' || *c == '\t')
        .take(max)
        .collect()
}

fn record_peer(registry: &Arc<Mutex<HashMap<String, Peer>>>, device: DeviceInfo) {
    if device.fingerprint.is_empty() || device.alias.is_empty() {
        return;
    }
    registry.lock().unwrap().insert(
        device.fingerprint.clone(),
        Peer {
            device: device.clone(),
            last_seen: Instant::now(),
        },
    );
    eprintln!("peer registered: {}", valid_remote_text(&device.alias, 128));
    emit(json!({"event":"device","device":event_device(&device)}));
}

fn expire_and_snapshot(registry: &Arc<Mutex<HashMap<String, Peer>>>) -> Vec<Value> {
    let mut peers = registry.lock().unwrap();
    peers.retain(|_, peer| peer.last_seen.elapsed() <= PEER_TTL);
    peers.values().map(|p| event_device(&p.device)).collect()
}

fn ipv4_prefix_length(netmask: Ipv4Addr) -> Option<u32> {
    let mask = u32::from(netmask);
    let prefix = mask.count_ones();
    let expected = if prefix == 0 {
        0
    } else {
        u32::MAX << (32 - prefix)
    };
    (mask == expected).then_some(prefix)
}

fn http_scan_targets(interfaces: Vec<(Ipv4Addr, Ipv4Addr)>) -> (Vec<String>, Vec<String>) {
    let usable: BTreeSet<Ipv4Addr> = interfaces
        .iter()
        .map(|(ip, _)| *ip)
        .into_iter()
        .filter(|ip| {
            !ip.is_unspecified()
                && !ip.is_loopback()
                && !ip.is_link_local()
                && !ip.is_multicast()
                && !ip.is_broadcast()
        })
        .collect();
    let mut subnets: BTreeMap<(u32, u32), BTreeSet<Ipv4Addr>> = BTreeMap::new();
    for (ip, netmask) in interfaces {
        if !usable.contains(&ip) {
            continue;
        }
        let Some(prefix) = ipv4_prefix_length(netmask) else {
            continue;
        };
        subnets
            .entry((u32::from(ip) & u32::from(netmask), prefix))
            .or_default()
            .insert(ip);
    }
    let labels = subnets
        .keys()
        .map(|(network, prefix)| format!("{}/{}", Ipv4Addr::from(*network), prefix))
        .collect();
    let targets: BTreeSet<Ipv4Addr> = subnets
        .iter()
        .flat_map(|((network, prefix), local_addresses)| {
            let address_count = 1u64 << (32 - prefix);
            let scan_ranges = if address_count > u64::from(MAX_SUBNET_ADDRESSES) {
                local_addresses
                    .iter()
                    .map(|address| (u32::from(*address) & 0xffff_ff00, 256u32))
                    .collect::<BTreeSet<_>>()
            } else {
                BTreeSet::from([(*network, address_count as u32)])
            };
            scan_ranges
                .into_iter()
                .flat_map(|(scan_network, scan_count)| {
                    (1..scan_count.saturating_sub(1))
                        .map(move |host| Ipv4Addr::from(scan_network + host))
                })
        })
        .filter(|candidate| !usable.contains(candidate))
        .collect();
    (
        labels,
        targets
            .into_iter()
            .map(|candidate| candidate.to_string())
            .collect(),
    )
}

fn cached_candidate_devices(
    registry: &Arc<Mutex<HashMap<String, Peer>>>,
    interfaces: &[(Ipv4Addr, Ipv4Addr)],
) -> Vec<DeviceInfo> {
    let mut candidates = BTreeMap::new();
    let mut peers = registry.lock().unwrap();
    peers.retain(|_, peer| peer.last_seen.elapsed() <= PEER_TTL);
    for peer in peers.values() {
        if let Some(ip) = peer.device.ip.as_deref()
            && let Ok(remote_ip) = ip.parse::<Ipv4Addr>()
            && !remote_ip.is_unspecified()
            && !remote_ip.is_loopback()
            && !remote_ip.is_link_local()
            && !remote_ip.is_multicast()
            && !remote_ip.is_broadcast()
            && interfaces.iter().any(|(local_ip, netmask)| {
                let mask = u32::from(*netmask);
                let remote = u32::from(remote_ip);
                let network = u32::from(*local_ip) & mask;
                ipv4_prefix_length(*netmask).is_some()
                    && (remote & mask) == network
                    && remote != network
                    && remote != (network | !mask)
                    && remote_ip != *local_ip
            })
        {
            let key = format!("{}\t{:05}\t{}", ip, peer.device.port, peer.device.protocol);
            candidates.insert(key, peer.device.clone());
        }
    }
    candidates.into_values().collect()
}

fn remaining_subnet_targets(
    targets: Vec<String>,
    attempted_cached: &HashSet<String>,
    cache_hit: bool,
) -> Option<Vec<String>> {
    if cache_hit {
        None
    } else {
        Some(
            targets
                .into_iter()
                .filter(|ip| !attempted_cached.contains(ip))
                .collect(),
        )
    }
}

async fn multicast_grace_confirmed_round_peer(
    registry: &Arc<Mutex<HashMap<String, Peer>>>,
    grace: Duration,
    round_started: Instant,
) -> bool {
    tokio::time::sleep(grace).await;
    let mut peers = registry.lock().unwrap();
    peers.retain(|_, peer| peer.last_seen.elapsed() <= PEER_TTL);
    peers.values().any(|peer| peer.last_seen >= round_started)
}

async fn start_active_discovery(
    discovery: MulticastDiscovery,
    identity: DeviceInfo,
    certificate: TlsCertificate,
    registry: Arc<Mutex<HashMap<String, Peer>>>,
    force_full: bool,
) -> DiscoveryControl {
    let (stop_tx, mut stop_rx) = oneshot::channel();
    let started = Instant::now();
    let snapshot = expire_and_snapshot(&registry);
    emit(json!({"event":"peer_snapshot","devices":snapshot}));
    emit(json!({"event":"discovery_started"}));
    eprintln!("discovery started: +0 ms");
    tokio::spawn(async move {
        let run = async {
            if let Err(error) = discovery.announce_presence_once().await {
                eprintln!(
                    "initial multicast failed: +{} ms: {error}",
                    started.elapsed().as_millis()
                );
            } else {
                eprintln!(
                    "initial multicast sent: +{} ms",
                    started.elapsed().as_millis()
                );
            }

            let retries = discovery.retry_announcements();
            tokio::pin!(retries);
            let grace = multicast_grace_confirmed_round_peer(&registry, MULTICAST_GRACE, started);
            tokio::pin!(grace);
            let retries_finished = tokio::select! {
                found = &mut grace => {
                    eprintln!("grace period finished: +{} ms", started.elapsed().as_millis());
                    if found && !force_full {
                        eprintln!("HTTP fallback skipped: recent multicast peer available");
                        eprintln!("discovery finished: +{} ms", started.elapsed().as_millis());
                        return;
                    }
                    false
                }
                result = &mut retries => {
                    if let Err(error) = result { eprintln!("multicast retries failed: {error}"); }
                    let found = grace.await;
                    eprintln!("grace period finished: +{} ms", started.elapsed().as_millis());
                    if found && !force_full {
                        eprintln!("HTTP fallback skipped: recent multicast peer available");
                        eprintln!("discovery finished: +{} ms", started.elapsed().as_millis());
                        return;
                    }
                    true
                }
            };

            eprintln!(
                "HTTP fallback started: +{} ms",
                started.elapsed().as_millis()
            );
            let fallback = async {
                let scanner = HttpDiscovery::new_with_device_and_identity(identity, &certificate)
                    .context("could not build discovery client")?;
                let interfaces = MulticastDiscovery::local_ipv4_interfaces_with_netmasks()
                    .context("could not enumerate interfaces")?;
                let (subnets, targets) = http_scan_targets(interfaces.clone());
                eprintln!("subnets selected: {subnets:?}");

                let cached = if force_full {
                    Vec::new()
                } else {
                    cached_candidate_devices(&registry, &interfaces)
                };
                let cached_labels: Vec<String> = cached
                    .iter()
                    .map(|device| {
                        format!(
                            "{}://{}:{}",
                            device.protocol,
                            device.ip.as_deref().unwrap_or("unknown"),
                            device.port
                        )
                    })
                    .collect();
                eprintln!("cached candidates: {cached_labels:?}");
                let attempted_cached: HashSet<String> = cached
                    .iter()
                    .filter_map(|device| device.ip.clone())
                    .collect();
                for device in &cached {
                    eprintln!(
                        "cached probe started: {}://{}:{} +{} ms",
                        device.protocol,
                        device.ip.as_deref().unwrap_or("unknown"),
                        device.port,
                        started.elapsed().as_millis()
                    );
                }
                let mut confirmed_cached = HashSet::new();
                scanner
                    .scan_devices_incremental_with_limit(
                        cached.clone(),
                        CACHED_PROBE_CONCURRENCY,
                        |device| {
                            let ip = device.ip.clone().unwrap_or_default();
                            confirmed_cached.insert(ip.clone());
                            eprintln!(
                                "cached peer confirmed: {} alias={} +{} ms",
                                ip,
                                valid_remote_text(&device.alias, 128),
                                started.elapsed().as_millis()
                            );
                            record_peer(&registry, device);
                            eprintln!(
                                "peer emitted to frontend: +{} ms",
                                started.elapsed().as_millis()
                            );
                        },
                    )
                    .await?;
                for device in cached
                    .iter()
                    .filter(|device| !confirmed_cached.contains(device.ip.as_deref().unwrap_or("")))
                {
                    eprintln!(
                        "cached probe failed: {}://{}:{} +{} ms",
                        device.protocol,
                        device.ip.as_deref().unwrap_or("unknown"),
                        device.port,
                        started.elapsed().as_millis()
                    );
                }

                let Some(remaining_targets) = remaining_subnet_targets(
                    targets,
                    &attempted_cached,
                    !force_full && !confirmed_cached.is_empty(),
                ) else {
                    eprintln!("CACHE HIT: full subnet scan skipped");
                    eprintln!("HTTP scan finished: +{} ms", started.elapsed().as_millis());
                    return Ok::<(), anyhow::Error>(());
                };
                eprintln!("CACHE MISS: no cached peer confirmed");
                eprintln!(
                    "full subnet scan started: +{} ms",
                    started.elapsed().as_millis()
                );
                scanner
                    .scan_ips_incremental(remaining_targets, |device| {
                        eprintln!(
                            "HTTP peer found: {} alias={} +{} ms",
                            device.ip.as_deref().unwrap_or("unknown"),
                            valid_remote_text(&device.alias, 128),
                            started.elapsed().as_millis(),
                        );
                        record_peer(&registry, device);
                        eprintln!(
                            "peer emitted to frontend: +{} ms",
                            started.elapsed().as_millis()
                        );
                    })
                    .await?;
                eprintln!("HTTP scan finished: +{} ms", started.elapsed().as_millis());
                Ok::<(), anyhow::Error>(())
            };
            tokio::pin!(fallback);
            let fallback_result = if retries_finished {
                fallback.await
            } else {
                tokio::select! {
                    result = &mut fallback => result,
                    result = &mut retries => {
                        if let Err(error) = result { eprintln!("multicast retries failed: {error}"); }
                        fallback.await
                    }
                }
            };
            if let Err(error) = fallback_result {
                eprintln!("HTTP fallback failed: {error:#}");
            }
            eprintln!("discovery finished: +{} ms", started.elapsed().as_millis());
        };
        tokio::select! {
            _ = run => {},
            _ = &mut stop_rx => { eprintln!("discovery cancelled: +{} ms", started.elapsed().as_millis()); emit(json!({"event":"discovery_stopped"})); return; }
        }
        let _ = stop_rx.await;
        eprintln!("discovery stopped: +{} ms", started.elapsed().as_millis());
        emit(json!({"event":"discovery_stopped"}));
    });
    DiscoveryControl {
        stop: stop_tx,
        started,
    }
}

async fn send_payload(
    transfer_id: String,
    identity: DeviceInfo,
    target: DeviceInfo,
    paths: Vec<String>,
    text: Option<String>,
    mut cancel_rx: oneshot::Receiver<()>,
) -> Result<()> {
    if target.ip.as_deref().unwrap_or("").is_empty() {
        return Err(anyhow!("device disappeared"));
    }
    let client = if target.protocol == Protocol::Https {
        LocalSendClient::with_trust_policy(
            identity,
            TlsTrustPolicy::new([target.fingerprint.clone()]),
        )?
    } else {
        LocalSendClient::new(identity)
    };
    let _ = client.register(&target).await;
    let mut metadata = HashMap::new();
    let mut sources: HashMap<String, PathBuf> = HashMap::new();
    let mut text_source = None;
    if let Some(body) = text {
        if body.trim().is_empty() {
            return Err(anyhow!("clipboard empty"));
        }
        let id = FileId::new();
        text_source = Some((id.as_str().to_string(), body.clone()));
        metadata.insert(
            id.clone(),
            FileMetadata {
                id,
                file_name: "message.txt".into(),
                size: body.len() as u64,
                file_type: "text/plain".into(),
                sha256: None,
                preview: Some(body),
                metadata: Some(FileMetadataDetails {
                    modified: None,
                    accessed: None,
                }),
            },
        );
    } else {
        if paths.is_empty() {
            return Err(anyhow!("no files selected"));
        }
        for raw in paths {
            let path = PathBuf::from(raw);
            if !path.is_file() {
                return Err(anyhow!("file is unavailable"));
            }
            let meta = build_file_metadata(&path).await?;
            sources.insert(meta.id.as_str().to_string(), path);
            metadata.insert(meta.id.clone(), meta);
        }
    }
    let total: u64 = metadata.values().map(|f| f.size).sum();
    let summary = metadata
        .values()
        .next()
        .map(|f| f.file_name.clone())
        .unwrap_or_default();
    emit(
        json!({"event":"outgoing_preparing","transferId":transfer_id,"name":summary,"count":metadata.len(),"total":total,"target":valid_remote_text(&target.alias,128)}),
    );
    let prepared = tokio::select! {
        result = client.prepare_upload(&target, metadata, None) => result?,
        _ = &mut cancel_rx => { emit(json!({"event":"outgoing_cancelled","transferId":transfer_id})); return Ok(()); }
    };
    if prepared.session_id.as_str().is_empty() {
        emit(json!({"event":"outgoing_done","transferId":transfer_id,"target":target.alias}));
        return Ok(());
    }
    let mut completed = 0u64;
    for (file_id, token) in &prepared.files {
        let temp_text;
        let path = if let Some(path) = sources.get(file_id.as_str()) {
            path.clone()
        } else {
            let body = text_source
                .as_ref()
                .filter(|(id, _)| id == file_id.as_str())
                .map(|(_, b)| b.as_str())
                .unwrap_or("");
            temp_text =
                std::env::temp_dir().join(format!("omarchy-nearby-{}-{file_id}.txt", transfer_id));
            tokio::fs::write(&temp_text, body).await?;
            temp_text.clone()
        };
        let size = tokio::fs::metadata(&path).await?.len();
        let base = completed;
        let alias = target.alias.clone();
        let id = transfer_id.clone();
        let last_emit = Arc::new(Mutex::new(Instant::now() - Duration::from_secs(1)));
        let progress_clock = last_emit.clone();
        let upload = client.upload_file(&target, &prepared.session_id, file_id, token, &path, Some(Box::new(move |sent,_,_| {
            let mut last = progress_clock.lock().unwrap();
            if base + sent < total && last.elapsed() < Duration::from_millis(75) { return; }
            *last = Instant::now();
            emit(json!({"event":"outgoing_progress","transferId":id,"bytes":base+sent,"total":total,"target":alias}));
        })));
        let (result, cancelled) = tokio::select! {
            result = upload => (result.map_err(anyhow::Error::from), false),
            _ = &mut cancel_rx => { let _ = client.cancel(&target,&prepared.session_id).await; emit(json!({"event":"outgoing_cancelled","transferId":transfer_id})); (Ok(()), true) }
        };
        if text_source.is_some() {
            let _ = tokio::fs::remove_file(&path).await;
        }
        result?;
        if cancelled {
            return Ok(());
        }
        completed += size;
    }
    emit(json!({"event":"outgoing_done","transferId":transfer_id,"target":target.alias}));
    Ok(())
}

fn xdg_download_dir(home: &Path) -> PathBuf {
    if let Ok(value) = std::env::var("XDG_DOWNLOAD_DIR") {
        return PathBuf::from(value);
    }
    let config = std::env::var("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| home.join(".config"))
        .join("user-dirs.dirs");
    if let Ok(text) = std::fs::read_to_string(config) {
        for line in text.lines() {
            if let Some(raw) = line.strip_prefix("XDG_DOWNLOAD_DIR=") {
                let value = raw
                    .trim_matches('"')
                    .replace("$HOME", &home.to_string_lossy());
                return PathBuf::from(value);
            }
        }
    }
    home.join("Downloads")
}

fn is_nearby_partial_name(name: &str) -> bool {
    name.starts_with(".nearby-") && name.ends_with(".part")
}

async fn cleanup_stale_partial_files(directory: &Path, minimum_age: Duration) -> Result<usize> {
    let mut entries = tokio::fs::read_dir(directory).await?;
    let mut removed = 0;
    while let Some(entry) = entries.next_entry().await? {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if !is_nearby_partial_name(name) || !entry.file_type().await?.is_file() {
            continue;
        }
        let Ok(modified) = entry.metadata().await?.modified() else {
            continue;
        };
        if modified.elapsed().unwrap_or_default() < minimum_age {
            continue;
        }
        if tokio::fs::remove_file(entry.path()).await.is_ok() {
            removed += 1;
        }
    }
    Ok(removed)
}

fn load_identity(home: &Path) -> Result<TlsCertificate> {
    let state_dir = std::env::var("XDG_STATE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| home.join(".local/state"))
        .join("omarchy-nearby");
    std::fs::create_dir_all(&state_dir)?;
    let path = state_dir.join("identity.json");
    if let Ok(data) = std::fs::read(&path) {
        if let Ok(stored) = serde_json::from_slice::<StoredIdentity>(&data) {
            return Ok(TlsCertificate {
                cert_pem: stored.cert_pem,
                key_pem: stored.key_pem,
                cert_der: Vec::new(),
                fingerprint: stored.fingerprint,
            });
        }
    }
    let cert = generate_tls_certificate()?;
    let stored = StoredIdentity {
        cert_pem: cert.cert_pem.clone(),
        key_pem: cert.key_pem.clone(),
        fingerprint: cert.fingerprint.clone(),
    };
    std::fs::write(&path, serde_json::to_vec_pretty(&stored)?)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(cert)
}

#[tokio::main]
async fn main() -> Result<()> {
    let home = PathBuf::from(std::env::var("HOME").context("HOME is not set")?);
    let download_dir = xdg_download_dir(&home);
    tokio::fs::create_dir_all(&download_dir)
        .await
        .context("download destination unavailable")?;
    let canonical_download = tokio::fs::canonicalize(&download_dir)
        .await
        .context("download destination unavailable")?;
    match cleanup_stale_partial_files(&canonical_download, STALE_PARTIAL_AGE).await {
        Ok(removed) if removed > 0 => eprintln!("removed {removed} stale Nearby partial files"),
        Ok(_) => {}
        Err(error) => eprintln!("could not clean stale Nearby partial files: {error}"),
    }
    let alias = std::env::var("HOSTNAME")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "Omarchy".into());
    let certificate = load_identity(&home).context("TLS identity unavailable")?;
    let (server, mut events) = LocalSendServer::builder()
        .alias(alias)
        .port(53317)
        .save_dir(&canonical_download)
        .protocol(Protocol::Https)
        .tls_certificate(certificate.clone())
        .auto_accept(false)
        .build()
        .await
        .context("receiver could not start")?;
    let identity = server.device().clone();
    let registry = Arc::new(Mutex::new(HashMap::<String, Peer>::new()));
    let (peer_tx, mut peer_rx) = mpsc::unbounded_channel::<DeviceInfo>();
    let mut passive =
        MulticastDiscovery::new_with_device_and_tls_identity(identity.clone(), certificate.clone());
    let passive_tx = peer_tx.clone();
    passive.on_discovered(move |device| {
        let _ = passive_tx.send(device);
    });
    passive
        .start()
        .await
        .context("passive receiver discovery could not start")?;
    emit(
        json!({"event":"ready","helperVersion":env!("CARGO_PKG_VERSION"),"alias":identity.alias,"directory":canonical_download,"port":server.port(),"fingerprint":identity.fingerprint}),
    );
    let pending = Arc::new(Mutex::new(HashMap::<String, PendingRequest>::new()));
    let pending_events = pending.clone();
    let peer_events = peer_tx.clone();
    tokio::spawn(async move {
        while let Some(event) = events.recv().await {
            match event {
                ServerEvent::PeerRegistered(device) => {
                    let _ = peer_events.send(device);
                }
                ServerEvent::TransferRequest(request) => {
                    let id = request.request_id().to_string();
                    let sender = valid_remote_text(&request.sender().alias, 128);
                    let mut files: Vec<Value> = request.files().values().map(|f| json!({"id":f.id,"name":valid_remote_text(&f.file_name,255),"size":f.size,"type":valid_remote_text(&f.file_type,128),"preview":f.preview.as_deref().map(|v|valid_remote_text(v,4096))})).collect();
                    files.sort_by_key(|f| f["name"].as_str().unwrap_or("").to_owned());
                    let total = request.files().values().map(|f| f.size).sum::<u64>();
                    pending_events.lock().unwrap().insert(id.clone(), request);
                    emit(
                        json!({"event":"incoming_request","requestId":id,"sender":sender,"files":files,"total":total}),
                    );
                }
                ServerEvent::TransferRequestExpired { request_id } => {
                    pending_events.lock().unwrap().remove(&request_id);
                    emit(json!({"event":"incoming_expired","requestId":request_id}));
                }
                ServerEvent::TextReceived {
                    session_id,
                    text,
                    sender_alias,
                } => emit(
                    json!({"event":"incoming_text","sessionId":session_id,"sender":valid_remote_text(&sender_alias,128),"text":valid_remote_text(&text,1_048_576)}),
                ),
                ServerEvent::FileReceiveProgress {
                    session_id,
                    file_name,
                    sender_alias,
                    bytes_received,
                    total_bytes,
                    file_count,
                    ..
                } => emit(
                    json!({"event":"incoming_progress","sessionId":session_id,"name":valid_remote_text(&file_name,255),"sender":valid_remote_text(&sender_alias,128),"bytes":bytes_received,"total":total_bytes,"count":file_count}),
                ),
                ServerEvent::FileReceived {
                    session_id,
                    path,
                    file_name,
                    sender_alias,
                    size,
                    ..
                } => emit(
                    json!({"event":"file_received","sessionId":session_id,"path":path,"name":valid_remote_text(&file_name,255),"sender":valid_remote_text(&sender_alias,128),"size":size}),
                ),
                ServerEvent::SessionCompleted { session_id } => {
                    emit(json!({"event":"incoming_done","sessionId":session_id}))
                }
                ServerEvent::SessionCancelled { session_id } => {
                    emit(json!({"event":"incoming_cancelled","sessionId":session_id}))
                }
                ServerEvent::SessionFailed {
                    session_id,
                    message,
                } => emit(
                    json!({"event":"incoming_failed","sessionId":session_id,"message":message}),
                ),
                _ => {}
            }
        }
    });
    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    let mut discovery: Option<DiscoveryControl> = None;
    let mut outgoing: Option<OutgoingControl> = None;
    let (out_done_tx, mut out_done_rx) = mpsc::unbounded_channel::<String>();
    let mut expiry = tokio::time::interval(Duration::from_secs(5));
    loop {
        tokio::select! {
            _ = expiry.tick() => { let snapshot=expire_and_snapshot(&registry); if discovery.is_some(){emit(json!({"event":"peer_snapshot","devices":snapshot}));} }
            Some(device) = peer_rx.recv() => {
                if let Some(control) = discovery.as_ref() {
                    eprintln!(
                        "peer discovered via multicast: +{} ms ({})",
                        control.started.elapsed().as_millis(),
                        device.ip.as_deref().unwrap_or("unknown")
                    );
                }
                record_peer(&registry,device)
            },
            Some(done_id) = out_done_rx.recv() => if outgoing.as_ref().is_some_and(|o|o.id==done_id) { outgoing=None; },
            line = lines.next_line() => {
                let Some(line)=line? else {break};
                let command: Command = match serde_json::from_str(&line) { Ok(v)=>v, Err(_)=>{emit(json!({"event":"error","message":"Invalid backend command"}));continue;} };
                match command {
                    Command::DiscoveryStart { force_full } => {
                        if force_full && let Some(control)=discovery.take(){let _=control.stop.send(());}
                        if discovery.is_none() { discovery=Some(start_active_discovery(passive.clone(),identity.clone(),certificate.clone(),registry.clone(),force_full).await); }
                    },
                    Command::DiscoveryStop => if let Some(control)=discovery.take(){let _=control.stop.send(());},
                    Command::Accept { request_id } => { let ok=pending.lock().unwrap().remove(&request_id).is_some_and(|req|req.accept()); emit(if ok {json!({"event":"incoming_accepted","requestId":request_id})} else {json!({"event":"incoming_expired","requestId":request_id})}); },
                    Command::Decline { request_id } => { let ok=pending.lock().unwrap().remove(&request_id).is_some_and(|req|req.decline()); emit(if ok {json!({"event":"incoming_declined","requestId":request_id})} else {json!({"event":"incoming_expired","requestId":request_id})}); },
                    Command::SendFiles { transfer_id,device,paths } => {
                        if outgoing.is_some(){emit(json!({"event":"outgoing_failed","transferId":transfer_id,"message":"Another transfer is already active"}));continue;}
                        let (tx,rx)=oneshot::channel(); outgoing=Some(OutgoingControl{id:transfer_id.clone(),cancel:tx});
                        let (identity,done)=(identity.clone(),out_done_tx.clone()); tokio::spawn(async move {if let Err(e)=send_payload(transfer_id.clone(),identity,device,paths,None,rx).await{emit(json!({"event":"outgoing_failed","transferId":transfer_id,"message":human_error(&e)}));} let _=done.send(transfer_id);});
                    }
                    Command::SendText { transfer_id,device,text } => {
                        if outgoing.is_some(){emit(json!({"event":"outgoing_failed","transferId":transfer_id,"message":"Another transfer is already active"}));continue;}
                        let (tx,rx)=oneshot::channel(); outgoing=Some(OutgoingControl{id:transfer_id.clone(),cancel:tx});
                        let (identity,done)=(identity.clone(),out_done_tx.clone()); tokio::spawn(async move {if let Err(e)=send_payload(transfer_id.clone(),identity,device,vec![],Some(text),rx).await{emit(json!({"event":"outgoing_failed","transferId":transfer_id,"message":human_error(&e)}));} let _=done.send(transfer_id);});
                    }
                    Command::CancelOutgoing { transfer_id } => if outgoing.as_ref().is_some_and(|o|o.id==transfer_id) { if let Some(control)=outgoing.take(){let _=control.cancel.send(());} },
                    Command::Shutdown => break,
                }
            }
        }
    }
    if let Some(control) = discovery.take() {
        let _ = control.stop.send(());
    }
    if let Some(control) = outgoing.take() {
        let _ = control.cancel.send(());
    }
    passive.stop();
    drop(server);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn peer_registry_keeps_valid_and_expires_stale() {
        let d = DeviceInfo::new("Phone".into(), 53317, Protocol::Http);
        let key = d.fingerprint.clone();
        let map = Arc::new(Mutex::new(HashMap::from([(
            key.clone(),
            Peer {
                device: d,
                last_seen: Instant::now(),
            },
        )])));
        assert_eq!(expire_and_snapshot(&map).len(), 1);
        map.lock().unwrap().get_mut(&key).unwrap().last_seen =
            Instant::now() - PEER_TTL - Duration::from_secs(1);
        assert!(expire_and_snapshot(&map).is_empty());
    }
    #[test]
    fn xdg_downloads_parsing_falls_back() {
        assert!(xdg_download_dir(Path::new("/tmp/home")).is_absolute());
    }
    #[test]
    fn partial_cleanup_pattern_is_strict() {
        assert!(is_nearby_partial_name(".nearby-session-file.part"));
        assert!(!is_nearby_partial_name("nearby-session-file.part"));
        assert!(!is_nearby_partial_name(".nearby-session-file.txt"));
        assert!(!is_nearby_partial_name("notes.part"));
    }

    #[tokio::test]
    async fn cleanup_removes_only_matching_old_enough_files() {
        let directory = std::env::temp_dir().join(format!(
            "omarchy-nearby-cleanup-test-{}-{}",
            std::process::id(),
            Instant::now().elapsed().as_nanos()
        ));
        tokio::fs::create_dir_all(&directory).await.unwrap();
        let partial = directory.join(".nearby-session-file.part");
        let unrelated = directory.join("notes.part");
        tokio::fs::write(&partial, b"partial").await.unwrap();
        tokio::fs::write(&unrelated, b"keep").await.unwrap();

        assert_eq!(
            cleanup_stale_partial_files(&directory, Duration::ZERO)
                .await
                .unwrap(),
            1
        );
        assert!(!partial.exists());
        assert!(unrelated.exists());
        tokio::fs::remove_file(unrelated).await.unwrap();
        tokio::fs::remove_dir(directory).await.unwrap();
    }
    #[test]
    fn outgoing_contract_requires_transfer_id() {
        let raw = r#"{"command":"send_files","transfer_id":"x","device":{"alias":"Phone","version":"2.1","deviceModel":"iPhone","deviceType":"mobile","fingerprint":"abc","port":53317,"protocol":"https","download":false,"ip":"192.0.2.2"},"paths":["/tmp/a"]}"#;
        assert!(
            matches!(serde_json::from_str::<Command>(raw).unwrap(),Command::SendFiles{transfer_id,..} if transfer_id=="x")
        );
    }

    #[test]
    fn discovery_start_defaults_to_cached_mode_and_accepts_forced_scan() {
        assert!(matches!(
            serde_json::from_str::<Command>(r#"{"command":"discovery_start"}"#).unwrap(),
            Command::DiscoveryStart { force_full: false }
        ));
        assert!(matches!(
            serde_json::from_str::<Command>(r#"{"command":"discovery_start","force_full":true}"#)
                .unwrap(),
            Command::DiscoveryStart { force_full: true }
        ));
    }

    #[test]
    fn duplicate_interfaces_produce_one_subnet_scan() {
        let (subnets, targets) = http_scan_targets(vec![
            (
                Ipv4Addr::new(192, 168, 1, 10),
                Ipv4Addr::new(255, 255, 255, 0),
            ),
            (
                Ipv4Addr::new(192, 168, 1, 11),
                Ipv4Addr::new(255, 255, 255, 0),
            ),
            (
                Ipv4Addr::new(192, 168, 1, 10),
                Ipv4Addr::new(255, 255, 255, 0),
            ),
        ]);
        assert_eq!(subnets, vec!["192.168.1.0/24"]);
        assert_eq!(targets.len(), 252);
        assert!(!targets.contains(&"192.168.50.10".to_string()));
        assert!(!targets.contains(&"192.168.50.11".to_string()));
    }

    #[test]
    fn multiple_subnets_are_combined_into_one_target_pool() {
        let (subnets, targets) = http_scan_targets(vec![
            (Ipv4Addr::new(10, 0, 0, 2), Ipv4Addr::new(255, 255, 255, 0)),
            (
                Ipv4Addr::new(192, 168, 1, 2),
                Ipv4Addr::new(255, 255, 255, 0),
            ),
        ]);
        assert_eq!(subnets, vec!["10.0.0.0/24", "192.168.1.0/24"]);
        assert_eq!(targets.len(), 506);
        assert!(targets.contains(&"10.0.0.1".to_string()));
        assert!(targets.contains(&"192.168.1.1".to_string()));
    }

    #[test]
    fn real_netmask_controls_scan_range() {
        let (subnets, targets) = http_scan_targets(vec![(
            Ipv4Addr::new(192, 168, 1, 10),
            Ipv4Addr::new(255, 255, 254, 0),
        )]);
        assert_eq!(subnets, vec!["192.168.0.0/23"]);
        assert_eq!(targets.len(), 509);
        assert!(targets.contains(&"192.168.0.1".to_string()));
        assert!(targets.contains(&"192.168.1.254".to_string()));
        assert!(!targets.contains(&"192.168.0.0".to_string()));
        assert!(!targets.contains(&"192.168.1.255".to_string()));
        assert!(!targets.contains(&"192.168.1.10".to_string()));
    }

    #[test]
    fn large_subnet_is_capped_to_local_24() {
        let (subnets, targets) = http_scan_targets(vec![(
            Ipv4Addr::new(10, 20, 30, 40),
            Ipv4Addr::new(255, 255, 0, 0),
        )]);
        assert_eq!(subnets, vec!["10.20.0.0/16"]);
        assert_eq!(targets.len(), 253);
        assert!(targets.contains(&"10.20.30.1".to_string()));
        assert!(!targets.contains(&"10.20.31.1".to_string()));
    }

    #[test]
    fn cache_candidates_are_recent_valid_lan_ips_and_deduplicated() {
        let mut phone = DeviceInfo::new("Phone".into(), 42424, Protocol::Http);
        phone.fingerprint = "phone".into();
        phone.ip = Some("192.168.50.139".into());
        let mut duplicate_ip = DeviceInfo::new("Tablet".into(), 42424, Protocol::Http);
        duplicate_ip.fingerprint = "tablet".into();
        duplicate_ip.ip = Some("192.168.50.139".into());
        let mut invalid = DeviceInfo::new("Invalid".into(), 53317, Protocol::Https);
        invalid.fingerprint = "invalid".into();
        invalid.ip = Some("127.0.0.1".into());
        let mut expired = DeviceInfo::new("Expired".into(), 53317, Protocol::Https);
        expired.fingerprint = "expired".into();
        expired.ip = Some("192.168.50.173".into());
        let registry = Arc::new(Mutex::new(HashMap::from([
            (
                phone.fingerprint.clone(),
                Peer {
                    device: phone,
                    last_seen: Instant::now(),
                },
            ),
            (
                duplicate_ip.fingerprint.clone(),
                Peer {
                    device: duplicate_ip,
                    last_seen: Instant::now(),
                },
            ),
            (
                invalid.fingerprint.clone(),
                Peer {
                    device: invalid,
                    last_seen: Instant::now(),
                },
            ),
            (
                expired.fingerprint.clone(),
                Peer {
                    device: expired,
                    last_seen: Instant::now() - PEER_TTL - Duration::from_secs(1),
                },
            ),
        ])));
        let interfaces = vec![(
            Ipv4Addr::new(192, 168, 50, 10),
            Ipv4Addr::new(255, 255, 255, 0),
        )];
        let candidates = cached_candidate_devices(&registry, &interfaces);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].ip.as_deref(), Some("192.168.50.139"));
        assert_eq!(candidates[0].port, 42424);
        assert_eq!(candidates[0].protocol, Protocol::Http);
    }

    #[test]
    fn cache_candidate_uses_real_subnet_beyond_capped_scan_range() {
        let mut peer = DeviceInfo::new("Peer".into(), 42424, Protocol::Http);
        peer.fingerprint = "peer".into();
        peer.ip = Some("10.20.31.50".into());
        let registry = Arc::new(Mutex::new(HashMap::from([(
            peer.fingerprint.clone(),
            Peer {
                device: peer,
                last_seen: Instant::now(),
            },
        )])));
        let interfaces = vec![(Ipv4Addr::new(10, 20, 30, 40), Ipv4Addr::new(255, 255, 0, 0))];
        let candidates = cached_candidate_devices(&registry, &interfaces);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].ip.as_deref(), Some("10.20.31.50"));
    }

    #[test]
    fn cache_hit_skips_full_scan_and_cache_miss_removes_attempted_ip() {
        let targets = vec!["192.168.50.1".into(), "192.168.50.139".into()];
        let attempted = HashSet::from(["192.168.50.139".to_string()]);
        assert!(remaining_subnet_targets(targets.clone(), &attempted, true).is_none());
        assert_eq!(
            remaining_subnet_targets(targets, &attempted, false).unwrap(),
            vec!["192.168.50.1"]
        );
    }

    #[test]
    fn changed_ip_replaces_cached_address_for_same_peer_identity() {
        let mut cached = DeviceInfo::new("Test Phone".into(), 53317, Protocol::Https);
        cached.fingerprint = "patient-avocado".into();
        cached.ip = Some("192.168.50.139".into());
        let fingerprint = cached.fingerprint.clone();
        let registry = Arc::new(Mutex::new(HashMap::from([(
            fingerprint.clone(),
            Peer {
                device: cached.clone(),
                last_seen: Instant::now(),
            },
        )])));
        let mut moved = cached;
        moved.ip = Some("192.168.50.173".into());
        record_peer(&registry, moved);
        assert_eq!(
            registry.lock().unwrap()[&fingerprint].device.ip.as_deref(),
            Some("192.168.50.173")
        );
    }

    #[tokio::test(start_paused = true)]
    async fn multicast_peer_during_grace_skips_fallback_decision() {
        let registry = Arc::new(Mutex::new(HashMap::new()));
        let round_started = Instant::now();
        let future =
            multicast_grace_confirmed_round_peer(&registry, MULTICAST_GRACE, round_started);
        tokio::pin!(future);
        tokio::task::yield_now().await;
        let device = DeviceInfo::new("Phone".into(), 53317, Protocol::Http);
        registry.lock().unwrap().insert(
            device.fingerprint.clone(),
            Peer {
                device,
                last_seen: Instant::now(),
            },
        );
        tokio::time::advance(MULTICAST_GRACE).await;
        assert!(future.await, "a fresh multicast peer must suppress HTTP");
    }

    #[tokio::test(start_paused = true)]
    async fn empty_registry_after_grace_requests_http_fallback() {
        let registry = Arc::new(Mutex::new(HashMap::new()));
        let future =
            multicast_grace_confirmed_round_peer(&registry, MULTICAST_GRACE, Instant::now());
        tokio::pin!(future);
        tokio::task::yield_now().await;
        tokio::time::advance(MULTICAST_GRACE).await;
        assert!(
            !future.await,
            "an empty registry must request HTTP fallback"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn cached_peer_does_not_suppress_fallback_for_a_new_round() {
        let device = DeviceInfo::new("Cached Mac".into(), 53317, Protocol::Http);
        let registry = Arc::new(Mutex::new(HashMap::from([(
            device.fingerprint.clone(),
            Peer {
                device,
                last_seen: Instant::now(),
            },
        )])));
        tokio::time::advance(Duration::from_millis(1)).await;
        let round_started = Instant::now();
        let future =
            multicast_grace_confirmed_round_peer(&registry, MULTICAST_GRACE, round_started);
        tokio::pin!(future);
        tokio::task::yield_now().await;
        tokio::time::advance(MULTICAST_GRACE).await;
        assert!(
            !future.await,
            "a cached peer not refreshed in this round must not suppress HTTP"
        );
        assert_eq!(registry.lock().unwrap().len(), 1, "cache remains visible");
    }

    #[tokio::test(start_paused = true)]
    async fn cancelling_repeated_grace_tasks_leaves_none_running() {
        for _ in 0..20 {
            let registry = Arc::new(Mutex::new(HashMap::new()));
            let task = tokio::spawn(async move {
                multicast_grace_confirmed_round_peer(&registry, MULTICAST_GRACE, Instant::now())
                    .await
            });
            tokio::task::yield_now().await;
            task.abort();
            assert!(task.await.unwrap_err().is_cancelled());
        }
    }
}
