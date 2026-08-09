use super::events::ServerEvent;
use super::state::ServerState;
use crate::protocol::{DeviceInfo, Protocol};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::sync::{RwLock, mpsc, oneshot};
use tokio::task::JoinHandle;

#[cfg(feature = "https")]
use axum_server::tls_rustls::RustlsConfig;

pub struct LocalSendServer {
    device: DeviceInfo,
    save_dir: PathBuf,
    handle: Option<JoinHandle<()>>,
    sweep_handle: Option<JoinHandle<()>>,
    shutdown_tx: Option<oneshot::Sender<()>>,
    https: bool,
    #[cfg(feature = "https")]
    tls_cert: Option<crate::crypto::TlsCertificate>,
    events_rx: Option<mpsc::UnboundedReceiver<ServerEvent>>,
    /// Shared with the running [`ServerState`] so `set_auto_accept` takes
    /// effect on in-flight requests, not just at `start()` time.
    auto_accept: Arc<AtomicBool>,
    accept_timeout: Duration,
    receive_rate_limit_bytes_per_second: Option<u64>,
    /// Receiver-side PIN, enforced by `pin::PinGate` in the request handler.
    pin: Option<String>,
    state: Option<Arc<RwLock<ServerState>>>,
}

impl LocalSendServer {
    /// Private constructor used by [`LocalSendServerBuilder::build`].
    fn from_parts(
        device: DeviceInfo,
        save_dir: PathBuf,
        https: bool,
        pin: Option<String>,
        auto_accept: bool,
        accept_timeout: Duration,
        receive_rate_limit_bytes_per_second: Option<u64>,
    ) -> std::result::Result<Self, crate::error::LocalSendError> {
        Ok(Self {
            device,
            save_dir,
            handle: None,
            sweep_handle: None,
            shutdown_tx: None,
            https,
            #[cfg(feature = "https")]
            tls_cert: None,
            events_rx: None,
            auto_accept: Arc::new(AtomicBool::new(auto_accept)),
            accept_timeout,
            receive_rate_limit_bytes_per_second,
            pin,
            state: None,
        })
    }

    /// Returns the actual bound port. If the server was started with an
    /// ephemeral port (`0`), this reflects the OS-assigned port after
    /// `start()`/`builder().build()` has returned.
    pub fn port(&self) -> u16 {
        self.device.port
    }

    pub fn device(&self) -> &DeviceInfo {
        &self.device
    }

    pub fn builder() -> LocalSendServerBuilder {
        LocalSendServerBuilder {
            alias: "LocalSend-Rust".to_string(),
            port: crate::protocol::DEFAULT_HTTP_PORT,
            save_dir: PathBuf::from("./downloads"),
            protocol: Protocol::Http,
            pin: None,
            auto_accept: false,
            accept_timeout: Duration::from_secs(60),
            receive_rate_limit_bytes_per_second: None,
            #[cfg(feature = "https")]
            tls_certificate: None,
        }
    }

    /// Take the event receiver. Returns `Some` once, after `start()`.
    pub fn take_events(&mut self) -> Option<mpsc::UnboundedReceiver<ServerEvent>> {
        self.events_rx.take()
    }

    /// Toggle auto-accept on a running server. Because the flag is shared with
    /// the live [`ServerState`], this affects requests that arrive afterward.
    pub fn set_auto_accept(&self, yes: bool) {
        self.auto_accept.store(yes, Ordering::Relaxed);
    }

    /// Current auto-accept setting.
    pub fn auto_accept(&self) -> bool {
        self.auto_accept.load(Ordering::Relaxed)
    }

    #[cfg(feature = "https")]
    pub fn set_tls_certificate(&mut self, cert: crate::crypto::TlsCertificate) {
        self.tls_cert = Some(cert);
    }

    pub async fn start(&mut self) -> std::result::Result<(), crate::error::LocalSendError> {
        let (events_tx, events_rx) = mpsc::unbounded_channel();
        self.events_rx = Some(events_rx);

        let addr = format!("0.0.0.0:{}", self.device.port);
        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
        self.shutdown_tx = Some(shutdown_tx);

        if self.https {
            #[cfg(feature = "https")]
            {
                rustls::crypto::ring::default_provider()
                    .install_default()
                    .ok();
                let (cert_pem, key_pem) = if let Some(ref cert) = self.tls_cert {
                    (cert.cert_pem.clone(), cert.key_pem.clone())
                } else {
                    let cert = crate::crypto::generate_tls_certificate()?;
                    (cert.cert_pem, cert.key_pem)
                };

                let tls_config =
                    RustlsConfig::from_pem(cert_pem.into_bytes(), key_pem.into_bytes())
                        .await
                        .map_err(|e| {
                            crate::error::LocalSendError::network(format!(
                                "TLS config error: {}",
                                e
                            ))
                        })?;

                // Bind before spawn so the real (possibly OS-assigned) port is
                // known before the ServerState/router are built.
                let std_listener = std::net::TcpListener::bind(&addr)?;
                std_listener.set_nonblocking(true)?;
                let bound_port = std_listener.local_addr()?.port();
                self.device.port = bound_port;

                let state = Arc::new(RwLock::new(ServerState {
                    device: self.device.clone(),
                    current_session: None,
                    save_dir: self.save_dir.clone(),
                    events_tx,
                    auto_accept: self.auto_accept.clone(),
                    accept_timeout: self.accept_timeout,
                    receive_rate_limit_bytes_per_second: self.receive_rate_limit_bytes_per_second,
                    pin_gate: crate::server::pin::PinGate::new(self.pin.clone()),
                    web_share: None,
                }));
                self.state = Some(state.clone());
                let router = super::routes::create_router(state.clone());

                let server = axum_server::from_tcp_rustls(std_listener, tls_config)
                    .map_err(|e| {
                        crate::error::LocalSendError::network(format!(
                            "Failed to serve HTTPS listener: {}",
                            e
                        ))
                    })?
                    .serve(router.into_make_service_with_connect_info::<std::net::SocketAddr>());

                let handle = tokio::spawn(async move {
                    tracing::info!("Starting HTTPS server on port {}", bound_port);

                    tokio::select! {
                        res = server => {
                            if let Err(e) = res {
                                tracing::error!("HTTPS server error: {}", e);
                            }
                        }
                        _ = shutdown_rx => {
                            tracing::info!("Stopping HTTPS server");
                        }
                    }
                });

                self.handle = Some(handle);
                self.sweep_handle = Some(spawn_session_sweep(state));
                Ok(())
            }
            #[cfg(not(feature = "https"))]
            {
                Err(crate::error::LocalSendError::network(
                    "HTTPS support not enabled. Please build with --features https",
                ))
            }
        } else {
            // Bind before spawn so the real (possibly OS-assigned) port is
            // known before the ServerState/router are built.
            let listener = TcpListener::bind(&addr).await?;
            let bound_port = listener.local_addr()?.port();
            self.device.port = bound_port;
            tracing::info!("Starting HTTP server on port {}", bound_port);

            let state = Arc::new(RwLock::new(ServerState {
                device: self.device.clone(),
                current_session: None,
                save_dir: self.save_dir.clone(),
                events_tx,
                auto_accept: self.auto_accept.clone(),
                accept_timeout: self.accept_timeout,
                receive_rate_limit_bytes_per_second: self.receive_rate_limit_bytes_per_second,
                pin_gate: crate::server::pin::PinGate::new(self.pin.clone()),
                web_share: None,
            }));
            self.state = Some(state.clone());
            let router = super::routes::create_router(state.clone());

            let handle = tokio::spawn(async move {
                let server = axum::serve(
                    listener,
                    router.into_make_service_with_connect_info::<std::net::SocketAddr>(),
                )
                .with_graceful_shutdown(async {
                    let _ = shutdown_rx.await;
                });

                if let Err(e) = server.await {
                    tracing::error!("HTTP server error: {}", e);
                }
            });

            self.handle = Some(handle);
            self.sweep_handle = Some(spawn_session_sweep(state));
            Ok(())
        }
    }

    pub fn stop(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
        if let Some(handle) = self.sweep_handle.take() {
            handle.abort();
        }
        if let Some(handle) = self.handle.take() {
            handle.abort();
        }
    }

    pub async fn start_web_share(
        &mut self,
        files: Vec<super::web_share::WebShareFile>,
        pin: Option<String>,
        auto_accept: bool,
    ) -> crate::Result<()> {
        if files.is_empty() {
            return Err(crate::error::LocalSendError::invalid_state(
                "Web share requires at least one file",
            ));
        }
        let state = self
            .state
            .as_ref()
            .ok_or_else(|| crate::error::LocalSendError::invalid_state("Server is not running"))?;
        let mut state = state.write().await;
        state.web_share = Some(super::web_share::WebShareState::new(
            files,
            pin,
            auto_accept,
        ));
        state.device.download = true;
        self.device.download = true;
        Ok(())
    }

    pub async fn stop_web_share(&mut self) -> crate::Result<()> {
        let state = self
            .state
            .as_ref()
            .ok_or_else(|| crate::error::LocalSendError::invalid_state("Server is not running"))?;
        let mut state = state.write().await;
        state.web_share = None;
        state.device.download = false;
        self.device.download = false;
        Ok(())
    }

    pub async fn respond_web_share(
        &self,
        session_id: &crate::protocol::SessionId,
        accepted: bool,
    ) -> crate::Result<()> {
        let state = self
            .state
            .as_ref()
            .ok_or_else(|| crate::error::LocalSendError::invalid_state("Server is not running"))?;
        let mut state = state.write().await;
        let sender = state
            .web_share
            .as_mut()
            .and_then(|web| web.sessions.get_mut(session_id))
            .and_then(|session| session.response_tx.take())
            .ok_or_else(|| {
                crate::error::LocalSendError::invalid_state(
                    "Unknown or already answered Web Share request",
                )
            })?;
        sender.send(accepted).map_err(|_| {
            crate::error::LocalSendError::invalid_state("Web Share requester disconnected")
        })
    }
}

/// Every 60s, reclaim a session that's been idle past its 300s TTL (R5: a
/// sender that vanishes mid-transfer must not permanently wedge the single
/// upload slot). The lock is only held for the duration of the check itself
/// -- no `.await` happens while it's held.
fn spawn_session_sweep(state: Arc<RwLock<ServerState>>) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(60));
        loop {
            tick.tick().await;
            let mut s = state.write().await;
            if let Some(session) = &s.current_session
                && session.is_timed_out(300)
            {
                let session_id = session.id.clone();
                tracing::info!("Sweeping timed-out session {}", session_id);
                s.current_session = None;
                let _ = s.events_tx.send(ServerEvent::SessionFailed {
                    session_id,
                    message: "Transfer timed out".into(),
                });
            }
        }
    })
}

/// Builder for [`LocalSendServer`]; the canonical construction path.
///
/// `build()` binds the listener, starts serving, and returns the server
/// together with its [`ServerEvent`] receiver — the server is already
/// listening when `build()` returns. Pass `port(0)` for an OS-assigned
/// ephemeral port, then read the real port back via [`LocalSendServer::port`].
pub struct LocalSendServerBuilder {
    alias: String,
    port: u16,
    save_dir: PathBuf,
    protocol: Protocol,
    pin: Option<String>,
    auto_accept: bool,
    accept_timeout: Duration,
    receive_rate_limit_bytes_per_second: Option<u64>,
    #[cfg(feature = "https")]
    tls_certificate: Option<crate::crypto::TlsCertificate>,
}

impl LocalSendServerBuilder {
    pub fn alias(mut self, alias: impl Into<String>) -> Self {
        self.alias = alias.into();
        self
    }

    pub fn port(mut self, port: u16) -> Self {
        self.port = port;
        self
    }

    pub fn save_dir(mut self, dir: impl AsRef<std::path::Path>) -> Self {
        self.save_dir = dir.as_ref().to_path_buf();
        self
    }

    pub fn protocol(mut self, protocol: Protocol) -> Self {
        self.protocol = protocol;
        self
    }

    pub fn pin(mut self, pin: impl Into<String>) -> Self {
        self.pin = Some(pin.into());
        self
    }

    pub fn auto_accept(mut self, yes: bool) -> Self {
        self.auto_accept = yes;
        self
    }

    pub fn accept_timeout(mut self, d: Duration) -> Self {
        self.accept_timeout = d;
        self
    }

    /// Limits receiver body consumption for deterministic integration tests.
    /// Production callers should leave this unset.
    pub fn receive_rate_limit(mut self, bytes_per_second: u64) -> Self {
        self.receive_rate_limit_bytes_per_second =
            (bytes_per_second > 0).then_some(bytes_per_second);
        self
    }

    #[cfg(feature = "https")]
    pub fn tls_certificate(mut self, certificate: crate::crypto::TlsCertificate) -> Self {
        self.tls_certificate = Some(certificate);
        self
    }

    pub async fn build(
        self,
    ) -> crate::Result<(LocalSendServer, mpsc::UnboundedReceiver<ServerEvent>)> {
        let https = matches!(self.protocol, Protocol::Https);

        #[cfg(feature = "https")]
        let tls_cert = if https {
            Some(match self.tls_certificate {
                Some(certificate) => certificate,
                None => crate::crypto::generate_tls_certificate()?,
            })
        } else {
            None
        };
        #[cfg(not(feature = "https"))]
        if https {
            return Err(crate::error::LocalSendError::network(
                "HTTPS support not enabled; build with --features https",
            ));
        }

        // HTTPS identity = SHA-256 of the cert (spec); HTTP = random string.
        let fingerprint = {
            #[cfg(feature = "https")]
            if let Some(ref cert) = tls_cert {
                cert.fingerprint.clone()
            } else {
                crate::crypto::generate_fingerprint()
            }
            #[cfg(not(feature = "https"))]
            crate::crypto::generate_fingerprint()
        };

        let device = DeviceInfo {
            alias: self.alias,
            version: crate::protocol::PROTOCOL_VERSION.to_string(),
            device_model: Some(crate::core::device::get_device_model()),
            device_type: Some(crate::core::device::get_device_type()),
            fingerprint,
            port: self.port,
            protocol: self.protocol,
            download: false,
            ip: None,
        };

        let mut server = LocalSendServer::from_parts(
            device,
            self.save_dir,
            https,
            self.pin,
            self.auto_accept,
            self.accept_timeout,
            self.receive_rate_limit_bytes_per_second,
        )?;
        #[cfg(feature = "https")]
        if let Some(cert) = tls_cert {
            server.set_tls_certificate(cert);
        }
        server.start().await?;
        let events = server.take_events().expect("events available after start");
        Ok((server, events))
    }
}
