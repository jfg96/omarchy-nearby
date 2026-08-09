//! Main TUI application with async event loop.

use crate::client::LocalSendClient;
use crate::crypto::generate_fingerprint;
use crate::discovery::{Discovery, MulticastDiscovery};
use crate::protocol::{DeviceInfo, DeviceType, PROTOCOL_VERSION, Protocol, ReceivedFile};
use crate::server::{LocalSendServer, ServerEvent};

use super::popup::{MessageLevel, Popup};
use super::screens::{
    Screen, receive::ReceiveScreen, send_file::SendFileScreen, send_text::SendTextScreen,
    settings::SettingsScreen,
};
use super::theme::THEME;

use color_eyre::Result;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::{
    DefaultTerminal, Frame,
    layout::{Constraint, Layout, Rect},
    symbols,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Tabs},
};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use strum::IntoEnumIterator;
use tokio::sync::RwLock;
use tokio::time::Duration;
use tui_input::backend::crossterm::EventHandler;

/// Which send flow a background task belongs to, so its result updates the right screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SendKind {
    File,
    Text,
}

/// Progress and completion reported by a spawned send task back to the UI loop.
/// Send tasks run detached; without this channel a failure was silently swallowed
/// (`let _ = send_file(...).await`) and the progress gauge never moved.
#[derive(Debug)]
enum SendUpdate {
    /// Cumulative bytes sent for the in-flight file upload.
    Progress {
        generation: u64,
        sent: u64,
        total: u64,
    },
    /// The send finished. `error` is `None` on success, or the failure reason.
    Finished {
        generation: u64,
        kind: SendKind,
        label: String,
        error: Option<String>,
    },
    /// The receiver answered 401: a PIN is required. The UI prompts for one and
    /// retries the same send with it.
    NeedsPin { generation: u64, kind: SendKind },
}

/// Main TUI application state.
pub struct App {
    // Mode
    should_quit: bool,
    screen: Screen,

    // Device info
    device_info: DeviceInfo,
    port: u16,
    https: bool,
    save_dir: PathBuf,
    // Receiver-side PIN (senders must supply it) and the initial auto-accept
    // setting the server starts with.
    pin: Option<String>,
    initial_auto_accept: bool,

    // Shared state
    devices: Arc<RwLock<Vec<DeviceInfo>>>,
    received_files: Arc<RwLock<Vec<ReceivedFile>>>,
    events_rx: Option<tokio::sync::mpsc::UnboundedReceiver<ServerEvent>>,

    // Popup overlay
    popup: Option<Popup>,

    // Screen states
    send_text: SendTextScreen,
    send_file: SendFileScreen,
    receive: ReceiveScreen,
    settings: SettingsScreen,

    // Status message
    status_message: Option<(String, MessageLevel)>,

    // Background services
    discovery: Option<MulticastDiscovery>,
    server: Option<LocalSendServer>,

    // Back-channel from spawned send tasks (progress + result).
    send_tx: tokio::sync::mpsc::UnboundedSender<SendUpdate>,
    send_rx: tokio::sync::mpsc::UnboundedReceiver<SendUpdate>,
    // Bumped whenever a send starts or is cancelled; updates from an older
    // generation (a cancelled/abandoned task) are ignored so they can't clobber
    // a newer send or wedge `is_sending`.
    send_generation: u64,

    // Last device count we could read, shown when the devices lock is
    // momentarily held by the discovery writer (fallback instead of panic).
    last_device_count: usize,
    // While discovery is still warming up and no device has appeared yet, the
    // send screens say "Scanning…" instead of "No devices found". Reset on a
    // manual refresh.
    scan_deadline: std::time::Instant,
    // Render-on-change: only repaint when something actually changed, instead
    // of redrawing every tick (which pins the CPU and makes the screen churn).
    dirty: bool,
    // Which send flow is waiting on a PIN entry, so submitting the PIN retries
    // the right one.
    pending_pin_kind: Option<SendKind>,
    // Previous scanning state, so the scanning→done edge forces one repaint
    // (otherwise the last "Scanning…" frame lingers when nothing else changes).
    was_scanning: bool,
}

impl App {
    /// Create a new App instance.
    pub fn new(
        port: u16,
        alias: Option<String>,
        https: bool,
        pin: Option<String>,
        auto_accept: bool,
    ) -> Result<Self> {
        let device_name = alias.unwrap_or_else(|| {
            format!("LocalSend-Rust-{}", &uuid::Uuid::new_v4().to_string()[..4])
        });

        let device_info = DeviceInfo {
            alias: device_name,
            version: PROTOCOL_VERSION.to_string(),
            device_model: Some(crate::core::device::get_device_model()),
            device_type: Some(DeviceType::Desktop),
            fingerprint: generate_fingerprint(),
            port,
            protocol: if https {
                Protocol::Https
            } else {
                Protocol::Http
            },
            download: false,
            ip: None,
        };

        let save_dir = PathBuf::from("./downloads");
        let devices = Arc::new(RwLock::new(Vec::new()));
        let received_files = Arc::new(RwLock::new(Vec::new()));
        let (send_tx, send_rx) = tokio::sync::mpsc::unbounded_channel();

        let mut settings =
            SettingsScreen::new(device_info.clone(), save_dir.to_string_lossy().into_owned());
        settings.auto_accept = auto_accept;

        Ok(Self {
            should_quit: false,
            screen: Screen::SendText,
            device_info: device_info.clone(),
            port,
            https,
            save_dir: save_dir.clone(),
            pin,
            initial_auto_accept: auto_accept,
            devices: devices.clone(),
            received_files: received_files.clone(),
            events_rx: None,
            popup: None,

            send_text: SendTextScreen::new(devices.clone()),
            send_file: SendFileScreen::new(devices.clone()),
            receive: ReceiveScreen::new(received_files.clone(), port, save_dir.clone()),
            settings,
            status_message: None,
            discovery: None,
            server: None,
            send_tx,
            send_rx,
            send_generation: 0,
            last_device_count: 0,
            scan_deadline: std::time::Instant::now() + Duration::from_secs(3),
            dirty: true,
            pending_pin_kind: None,
            was_scanning: true,
        })
    }

    /// Run the TUI application.
    pub async fn run(mut self, mut terminal: DefaultTerminal) -> Result<()> {
        // Start background services
        self.start_discovery().await?;
        self.start_server().await?;

        // Main event loop. We poll for input on a short tick but only repaint
        // when something actually changed (`self.dirty`), so an idle TUI does
        // not busy-repaint every 100 ms.
        let tick_rate = Duration::from_millis(100);

        while !self.should_quit {
            if self.dirty {
                terminal.draw(|frame| self.render(frame))?;
                self.dirty = false;
            }

            // Check for pending transfers (popup trigger) and background send
            // results. Either can change state, so they set `dirty` themselves.
            self.poll_server_events();
            self.poll_send_updates();

            // A device appearing/disappearing changes the device list and the
            // "Scanning…" hint, but arrives on a background task, not through
            // our input path — repaint if the visible count moved.
            self.refresh_device_count();

            // Handle events with timeout
            if event::poll(tick_rate)?
                && let Event::Key(key) = event::read()?
                && key.kind == KeyEventKind::Press
            {
                self.handle_key(key.code);
                self.dirty = true;
            }

            // Open a deferred PIN prompt once the screen is free of any other
            // popup (e.g. an incoming transfer the user just answered), so the
            // prompt never clobbers a live `TransferConfirm`.
            self.maybe_open_pin_prompt();
        }

        Ok(())
    }

    /// Show the PIN entry popup when a send is waiting on a PIN and no other
    /// popup is currently displayed. Called every loop iteration so a PIN
    /// prompt that had to wait behind an incoming-transfer dialog appears as
    /// soon as that dialog is dismissed.
    fn maybe_open_pin_prompt(&mut self) {
        if self.pending_pin_kind.is_some() && self.popup.is_none() {
            self.popup = Some(Popup::PinEntry {
                input: tui_input::Input::default(),
            });
            self.dirty = true;
        }
    }

    /// Start multicast discovery in background.
    async fn start_discovery(&mut self) -> Result<()> {
        let devices = self.devices.clone();
        let device_info = self.device_info.clone();

        let mut discovery = MulticastDiscovery::new_with_device(device_info.clone());

        discovery.on_discovered(move |device: DeviceInfo| {
            // Skip self
            if device.fingerprint == device_info.fingerprint {
                return;
            }

            // Discovery callback runs off the UI thread; if the reader holds
            // the lock this instant, drop this announce — the peer re-announces
            // periodically, so it will be picked up on a later round.
            let Ok(mut devices_guard) = devices.try_write() else {
                return;
            };
            let exists = devices_guard.iter().any(|d| {
                d.fingerprint == device.fingerprint || (d.ip == device.ip && d.port == device.port)
            });
            if !exists {
                devices_guard.push(device);
            }
        });

        discovery.start().await?;
        discovery.announce_presence().await?;

        self.discovery = Some(discovery);

        Ok(())
    }

    /// Start receiver server in background.
    async fn start_server(&mut self) -> Result<()> {
        // Ensure save directory exists
        if !self.save_dir.exists() {
            std::fs::create_dir_all(&self.save_dir)?;
        }

        let protocol = if self.https {
            Protocol::Https
        } else {
            Protocol::Http
        };

        let mut builder = LocalSendServer::builder()
            .alias(self.device_info.alias.clone())
            .port(self.port)
            .save_dir(self.save_dir.clone())
            .protocol(protocol)
            .auto_accept(self.initial_auto_accept);
        if let Some(ref pin) = self.pin {
            builder = builder.pin(pin.clone());
        }
        let (server, events) = builder.build().await?;

        self.events_rx = Some(events);
        self.server = Some(server);

        Ok(())
    }

    /// Drain pending `ServerEvent`s and react (show popup, record received files).
    fn poll_server_events(&mut self) {
        let Some(rx) = self.events_rx.as_mut() else {
            return;
        };
        while let Ok(ev) = rx.try_recv() {
            self.dirty = true; // any server event may change what's on screen
            match ev {
                ServerEvent::TransferRequest(request) => match self.popup {
                    // Free slot: show the confirm dialog.
                    None => self.popup = Some(Popup::confirm(request)),
                    // A PIN prompt is deferrable — an incoming transfer takes the
                    // slot so we don't decline a peer just because our own send
                    // is waiting on a PIN. The PIN prompt reopens via
                    // `maybe_open_pin_prompt` once this dialog is answered.
                    Some(Popup::PinEntry { .. }) => {
                        self.popup = Some(Popup::confirm(request));
                    }
                    // Already showing another dialog (an incoming confirm, a
                    // message): stay busy and decline the new request.
                    Some(_) => request.decline(),
                },
                ServerEvent::TextReceived {
                    text, sender_alias, ..
                } => {
                    self.status_message = Some((
                        format!("Message from {sender_alias}: {text}"),
                        MessageLevel::Success,
                    ));
                }
                ServerEvent::WebShareRequest(_)
                | ServerEvent::WebShareDownloadProgress { .. }
                | ServerEvent::WebShareSessionDone { .. } => {}
                ServerEvent::FileReceiveProgress { .. } => {}
                ServerEvent::FileReceived {
                    file_name,
                    path,
                    size,
                    sender_alias,
                    message_text,
                    ..
                } => {
                    // A momentary write-lock contention must not crash the UI;
                    // if we can't record right now, drop this one row rather
                    // than panic (the event is advisory, not load-bearing).
                    if let Ok(mut files) = self.received_files.try_write() {
                        files.push(ReceivedFile {
                            file_name,
                            size,
                            sender: sender_alias,
                            time: chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
                            path,
                            message_text,
                        });
                    }
                }
                ServerEvent::SessionCompleted { .. } => {
                    self.status_message =
                        Some(("✓ Transfer complete".to_string(), MessageLevel::Success));
                }
                ServerEvent::SessionCancelled { .. } => {
                    self.status_message =
                        Some(("Transfer cancelled".to_string(), MessageLevel::Error))
                }
                ServerEvent::SessionFailed { message, .. } => {
                    self.status_message = Some((message, MessageLevel::Error))
                }
                ServerEvent::TransferRequestExpired { .. } | ServerEvent::PeerRegistered(_) => {}
            }
        }
    }

    /// Drain progress/results from background send tasks and reflect them in the UI.
    /// Updates from a superseded generation (a cancelled send) are dropped.
    fn poll_send_updates(&mut self) {
        while let Ok(update) = self.send_rx.try_recv() {
            self.dirty = true; // progress moved or the send finished
            match update {
                SendUpdate::Progress {
                    generation,
                    sent,
                    total,
                } => {
                    if generation != self.send_generation {
                        continue;
                    }
                    let ratio = if total > 0 {
                        (sent as f64 / total as f64).clamp(0.0, 1.0)
                    } else {
                        0.0
                    };
                    self.send_file.progress = ratio;
                }
                SendUpdate::Finished {
                    generation,
                    kind,
                    label,
                    error,
                } => {
                    if generation != self.send_generation {
                        continue; // a cancelled/superseded send — ignore its result
                    }
                    match kind {
                        SendKind::File => self.send_file.clear(),
                        SendKind::Text => self.send_text.clear(),
                    }
                    self.status_message = Some(match error {
                        None => (format!("✓ Sent {label}"), MessageLevel::Success),
                        Some(reason) => (format!("✗ Send failed: {reason}"), MessageLevel::Error),
                    });
                }
                SendUpdate::NeedsPin { generation, kind } => {
                    if generation != self.send_generation {
                        continue; // superseded/cancelled attempt
                    }
                    // The attempt ended at prepare-upload; drop the sending
                    // state and record that a PIN is needed. The prompt itself
                    // is opened by `maybe_open_pin_prompt` only once no other
                    // popup is up — overwriting `self.popup` here would drop an
                    // open incoming `TransferConfirm` (and its `PendingRequest`),
                    // silently declining a peer's transfer.
                    match kind {
                        SendKind::File => self.send_file.is_sending = false,
                        SendKind::Text => self.send_text.is_sending = false,
                    }
                    self.pending_pin_kind = Some(kind);
                    self.status_message =
                        Some(("PIN required by receiver".into(), MessageLevel::Info));
                }
            }
        }
    }

    /// Repaint when the visible device count changes (devices arrive on the
    /// discovery task, off our input path) and while discovery is still warming
    /// up (so the "Scanning…" placeholder and its transition are drawn).
    fn refresh_device_count(&mut self) {
        if let Ok(devices) = self.devices.try_read() {
            let count = devices.len();
            if count != self.last_device_count {
                self.last_device_count = count;
                self.dirty = true;
            }
        }
        // Keep repainting while scanning, and force one repaint on the
        // scanning→done edge so the placeholder updates from "Scanning…" to
        // "No devices found" even if nothing else changes afterward.
        let scanning = self.is_scanning();
        if scanning || scanning != self.was_scanning {
            self.dirty = true;
        }
        self.was_scanning = scanning;
    }

    /// True while discovery is still warming up and nothing has been found yet.
    fn is_scanning(&self) -> bool {
        self.last_device_count == 0 && std::time::Instant::now() < self.scan_deadline
    }

    /// Handle key press.
    fn handle_key(&mut self, key: KeyCode) {
        // Popup takes priority
        if self.popup.is_some() {
            self.handle_popup_key(key);
            return;
        }

        // Global keys
        match key {
            KeyCode::Char('q') => {
                self.should_quit = true;
                return;
            }
            KeyCode::Esc => {
                let handles_esc = match self.screen {
                    Screen::SendText => {
                        self.send_text.stage
                            == crate::tui::screens::send_text::SendTextStage::EnterMessage
                    }
                    Screen::SendFile => {
                        self.send_file.stage
                            == crate::tui::screens::send_file::SendFileStage::EnterFilePath
                    }
                    _ => false,
                };

                if !handles_esc {
                    self.status_message = Some(("Press q to quit".into(), MessageLevel::Info));
                }
            }
            KeyCode::Right | KeyCode::Tab => {
                // Only allow switching tabs if not in input mode
                let can_switch = match self.screen {
                    Screen::SendText => {
                        self.send_text.stage
                            == crate::tui::screens::send_text::SendTextStage::SelectDevice
                    }
                    Screen::SendFile => {
                        self.send_file.stage
                            == crate::tui::screens::send_file::SendFileStage::SelectDevice
                    }
                    _ => true,
                };

                if can_switch {
                    let screens: Vec<_> = Screen::iter().collect();
                    let current_index = screens.iter().position(|&s| s == self.screen).unwrap_or(0);
                    self.screen = screens[(current_index + 1) % screens.len()];
                    return;
                }
            }
            KeyCode::Left => {
                let can_switch = match self.screen {
                    Screen::SendText => {
                        self.send_text.stage
                            == crate::tui::screens::send_text::SendTextStage::SelectDevice
                    }
                    Screen::SendFile => {
                        self.send_file.stage
                            == crate::tui::screens::send_file::SendFileStage::SelectDevice
                    }
                    _ => true,
                };

                if can_switch {
                    let screens: Vec<_> = Screen::iter().collect();
                    let current_index = screens.iter().position(|&s| s == self.screen).unwrap_or(0);
                    self.screen = screens[(current_index + screens.len() - 1) % screens.len()];
                    return;
                }
            }
            _ => {}
        }

        match self.screen {
            Screen::SendText => self.handle_send_text_key(key),
            Screen::SendFile => self.handle_send_file_key(key),
            Screen::Receive => self.handle_receive_key(key),
            Screen::Settings => self.handle_settings_key(key),
        }

        // Check for refresh requests
        let mut refresh = self.send_text.consume_refresh();
        refresh |= self.send_file.consume_refresh();

        if refresh {
            // Best-effort clear; if the discovery writer holds the lock this
            // instant, skip — the next announce round repopulates anyway.
            if let Ok(mut devices) = self.devices.try_write() {
                devices.clear();
            }
            self.last_device_count = 0;
            // Re-enter the "Scanning…" window so the refresh reads as active.
            self.scan_deadline = std::time::Instant::now() + Duration::from_secs(3);
            if let Some(ref discovery) = self.discovery {
                let discovery = discovery.clone();
                tokio::spawn(async move {
                    let _ = discovery.announce_presence().await;
                });
            }
            self.status_message = Some(("Refreshing devices...".into(), MessageLevel::Info));
        }
    }

    fn handle_popup_key(&mut self, key: KeyCode) {
        // Navigation/toggle keys mutate the confirm dialog in place.
        if let Some(Popup::TransferConfirm(state)) = &mut self.popup {
            match key {
                KeyCode::Up | KeyCode::Char('k') => {
                    state.move_up();
                    return;
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    state.move_down();
                    return;
                }
                KeyCode::Char(' ') => {
                    state.toggle();
                    return;
                }
                KeyCode::Char('a') | KeyCode::Char('A') => {
                    state.toggle_all();
                    return;
                }
                _ => {}
            }
        }

        // PIN entry: type into the field; Enter submits, Esc cancels.
        if let Some(Popup::PinEntry { input }) = &mut self.popup {
            match key {
                KeyCode::Enter => {
                    let pin = input.value().to_string();
                    self.popup = None;
                    if pin.is_empty() {
                        self.pending_pin_kind = None;
                        self.status_message =
                            Some(("Send cancelled (no PIN)".into(), MessageLevel::Info));
                    } else {
                        self.retry_pending_send_with_pin(pin);
                    }
                }
                KeyCode::Esc => {
                    self.popup = None;
                    self.pending_pin_kind = None;
                    // Supersede any straggler from the failed attempt.
                    self.send_generation = self.send_generation.wrapping_add(1);
                    self.status_message = Some(("Send cancelled".into(), MessageLevel::Info));
                }
                _ => {
                    input.handle_event(&Event::Key(event::KeyEvent::new(
                        key,
                        event::KeyModifiers::NONE,
                    )));
                }
            }
            return;
        }

        // Decision keys consume the request (which must happen exactly once).
        match &self.popup {
            Some(Popup::TransferConfirm(_)) => match key {
                KeyCode::Enter => {
                    if let Some(Popup::TransferConfirm(state)) = self.popup.take() {
                        let ids = state.selected_ids();
                        let all = ids.len() == state.files.len();
                        let request = state.request;
                        if ids.is_empty() {
                            request.decline();
                            self.status_message =
                                Some(("Transfer declined".into(), MessageLevel::Info));
                        } else if all {
                            request.accept();
                        } else {
                            self.status_message = Some((
                                format!("Accepting {} file(s)", ids.len()),
                                MessageLevel::Info,
                            ));
                            request.accept_files(ids);
                        }
                    }
                }
                KeyCode::Char('y') | KeyCode::Char('Y') => {
                    if let Some(Popup::TransferConfirm(state)) = self.popup.take() {
                        state.request.accept();
                    }
                }
                KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                    if let Some(Popup::TransferConfirm(state)) = self.popup.take() {
                        state.request.decline();
                        self.status_message =
                            Some(("Transfer declined".into(), MessageLevel::Info));
                    }
                }
                _ => {}
            },
            Some(Popup::Message { .. }) => {
                if matches!(key, KeyCode::Enter | KeyCode::Esc) {
                    self.popup = None;
                }
            }
            Some(Popup::TransferProgress { .. }) => {
                // Progress popup is non-interactive
            }
            // Handled above via an early return; kept for exhaustiveness.
            Some(Popup::PinEntry { .. }) => {}
            None => {}
        }
    }

    fn handle_send_text_key(&mut self, key: KeyCode) {
        use crate::tui::screens::send_text::SendTextStage;

        match self.send_text.stage {
            SendTextStage::SelectDevice => match key {
                KeyCode::Up | KeyCode::Char('k') => self.send_text.previous_device(),
                KeyCode::Down | KeyCode::Char('j') => self.send_text.next_device(),
                KeyCode::Enter => self.send_text.select_current_device(),
                KeyCode::Char('r') | KeyCode::Char('R') => self.send_text.request_refresh(),
                _ => {}
            },
            SendTextStage::EnterMessage => match key {
                KeyCode::Esc => {
                    // Leaving cancels any in-flight send (its result is ignored).
                    if self.send_text.is_sending {
                        self.send_generation = self.send_generation.wrapping_add(1);
                        self.send_text.is_sending = false;
                        self.status_message = Some(("Send cancelled".into(), MessageLevel::Info));
                    }
                    self.send_text.stage = SendTextStage::SelectDevice;
                }
                KeyCode::Enter => {
                    if self.send_text.is_sending {
                        return; // a send is already in flight
                    }
                    self.spawn_text_send(None);
                }
                _ => {
                    self.send_text
                        .input
                        .handle_event(&Event::Key(event::KeyEvent::new(
                            key,
                            event::KeyModifiers::NONE,
                        )));
                }
            },
        }
    }

    fn handle_send_file_key(&mut self, key: KeyCode) {
        use crate::tui::screens::send_file::SendFileStage;

        match self.send_file.stage {
            SendFileStage::SelectDevice => match key {
                KeyCode::Up | KeyCode::Char('k') => self.send_file.previous_device(),
                KeyCode::Down | KeyCode::Char('j') => self.send_file.next_device(),
                KeyCode::Enter => self.send_file.select_current_device(),
                KeyCode::Char('r') | KeyCode::Char('R') => self.send_file.request_refresh(),
                _ => {}
            },
            SendFileStage::EnterFilePath => match key {
                KeyCode::Esc => {
                    // Leaving cancels any in-flight send (its result is ignored),
                    // so a stalled upload never wedges the screen.
                    if self.send_file.is_sending {
                        self.send_generation = self.send_generation.wrapping_add(1);
                        self.status_message = Some(("Send cancelled".into(), MessageLevel::Info));
                    }
                    self.send_file.clear();
                }
                KeyCode::Enter => {
                    if self.send_file.is_sending {
                        return; // a send is already in flight
                    }
                    self.spawn_file_send(None);
                }
                _ if !self.send_file.is_sending => {
                    self.send_file
                        .input
                        .handle_event(&Event::Key(event::KeyEvent::new(
                            key,
                            event::KeyModifiers::NONE,
                        )));
                }
                _ => {}
            },
        }
    }

    /// Spawn a text-message send (optionally carrying a `pin`), reporting
    /// progress/result/needs-pin back over the send channel.
    fn spawn_text_send(&mut self, pin: Option<String>) {
        let Some(target) = self.send_text.selected_device.clone() else {
            return;
        };
        if self.send_text.message().is_empty() {
            return;
        }
        let message = self.send_text.message().to_string();
        let device_info = self.device_info.clone();
        let tx = self.send_tx.clone();
        self.send_generation = self.send_generation.wrapping_add(1);
        let generation = self.send_generation;

        self.send_text.is_sending = true;
        self.status_message = Some(("Sending message...".into(), MessageLevel::Info));

        tokio::spawn(async move {
            let client = LocalSendClient::new(device_info);
            let result = send_text_message(&client, &target, &message, pin.as_deref()).await;
            let _ = tx.send(send_update_from_result(
                generation,
                SendKind::Text,
                "message".to_string(),
                result,
            ));
        });
    }

    /// Spawn a file send (optionally carrying a `pin`), reporting
    /// progress/result/needs-pin back over the send channel.
    fn spawn_file_send(&mut self, pin: Option<String>) {
        let Some(target) = self.send_file.selected_device.clone() else {
            return;
        };
        if self.send_file.file_path().is_empty() {
            return;
        }
        let file_path = PathBuf::from(self.send_file.file_path());
        if !file_path.exists() {
            self.status_message = Some(("File not found".into(), MessageLevel::Error));
            return;
        }
        let device_info = self.device_info.clone();
        let tx = self.send_tx.clone();
        self.send_generation = self.send_generation.wrapping_add(1);
        let generation = self.send_generation;
        let label = file_path
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "file".to_string());

        // Keep the sending screen (with the gauge) up; the Finished update
        // clears it. Don't clear() here or the gauge never shows.
        self.send_file.is_sending = true;
        self.send_file.progress = 0.0;
        self.send_file.current_file = Some(label.clone());
        self.status_message = Some((format!("Sending {label}..."), MessageLevel::Info));

        tokio::spawn(async move {
            let client = LocalSendClient::new(device_info);
            let tx_prog = tx.clone();
            let cb: crate::client::client::ProgressCallback =
                Box::new(move |sent, total, _elapsed| {
                    let _ = tx_prog.send(SendUpdate::Progress {
                        generation,
                        sent,
                        total,
                    });
                });
            let result = send_file(&client, &target, &file_path, Some(cb), pin).await;
            let _ = tx.send(send_update_from_result(
                generation,
                SendKind::File,
                label,
                result,
            ));
        });
    }

    /// Retry the pending send with the PIN the user just entered.
    fn retry_pending_send_with_pin(&mut self, pin: String) {
        match self.pending_pin_kind.take() {
            Some(SendKind::File) => self.spawn_file_send(Some(pin)),
            Some(SendKind::Text) => self.spawn_text_send(Some(pin)),
            None => {}
        }
    }

    fn handle_receive_key(&mut self, key: KeyCode) {
        match key {
            KeyCode::Up | KeyCode::Char('k') => self.receive.previous(),
            KeyCode::Down | KeyCode::Char('j') => self.receive.next(),
            KeyCode::Enter => {
                if let Some(path) = self.receive.selected_path() {
                    match reveal_in_file_manager(&path) {
                        Ok(()) => {
                            self.status_message =
                                Some(("Revealed in file manager".into(), MessageLevel::Info));
                        }
                        Err(e) => {
                            self.status_message =
                                Some((format!("Could not reveal: {e}"), MessageLevel::Error));
                        }
                    }
                }
            }
            _ => {}
        }
    }

    fn handle_settings_key(&mut self, key: KeyCode) {
        if matches!(
            key,
            KeyCode::Char(' ') | KeyCode::Enter | KeyCode::Char('a') | KeyCode::Char('A')
        ) {
            let new = !self.settings.auto_accept;
            self.settings.auto_accept = new;
            // Live-toggle the running server's shared flag so it applies to
            // requests that arrive from now on.
            if let Some(server) = &self.server {
                server.set_auto_accept(new);
            }
            self.status_message = Some((
                format!("Auto-accept {}", if new { "ON" } else { "OFF" }),
                MessageLevel::Info,
            ));
        }
    }

    /// Render the TUI.
    fn render(&mut self, frame: &mut Frame) {
        let area = frame.area();

        // Tell the device-selection screens whether to show "Scanning…".
        let scanning = self.is_scanning();
        self.send_file.scanning = scanning;
        self.send_text.scanning = scanning;

        // Main layout: header, content, status bar
        let layout = Layout::vertical([
            Constraint::Length(3), // Header/Tabs
            Constraint::Min(0),    // Content
            Constraint::Length(1), // Status bar
        ])
        .split(area);

        // Header with Tabs
        self.render_header(frame, layout[0]);

        // Content based on screen
        match self.screen {
            Screen::SendText => self.send_text.render(layout[1], frame.buffer_mut()),
            Screen::SendFile => self.send_file.render(layout[1], frame.buffer_mut()),
            Screen::Receive => self.receive.render(layout[1], frame.buffer_mut()),
            Screen::Settings => frame.render_widget(&self.settings, layout[1]),
        }

        // Status bar
        self.render_status_bar(frame, layout[2]);

        // Popup overlay (if any)
        if let Some(ref popup) = self.popup {
            popup.render(frame);
        }
    }

    fn render_header(&self, frame: &mut Frame, area: Rect) {
        let block = Block::default().style(THEME.root).borders(Borders::BOTTOM);
        let inner = block.inner(area);
        frame.render_widget(block, area);

        let layout = Layout::horizontal([
            Constraint::Length(15), // Title
            Constraint::Min(0),     // Tabs
        ])
        .split(inner);

        // Title
        let title = Line::from(vec![Span::styled(" 🌐 LocalSend ", THEME.title)]);
        frame.render_widget(Paragraph::new(title), layout[0]);

        // Tabs
        let titles: Vec<String> = Screen::iter()
            .map(|s| match s {
                Screen::SendText => "📝 Text".to_string(),
                Screen::SendFile => "📁 File".to_string(),
                Screen::Receive => "📥 Inbox".to_string(),
                Screen::Settings => "⚙️ Settings".to_string(),
            })
            .collect();

        let current_index = Screen::iter().position(|s| s == self.screen).unwrap_or(0);
        let tabs = Tabs::new(titles)
            .block(Block::default())
            .select(current_index)
            .style(THEME.normal)
            .highlight_style(THEME.selected)
            .divider(symbols::DOT);

        frame.render_widget(tabs, layout[1]);
    }

    fn render_status_bar(&mut self, frame: &mut Frame, area: Rect) {
        // Fall back to the last known count if the writer holds the lock while
        // we paint — a momentary contention must never crash the UI.
        let devices_count = match self.devices.try_read() {
            Ok(devices) => {
                self.last_device_count = devices.len();
                devices.len()
            }
            Err(_) => self.last_device_count,
        };

        let mut spans = vec![
            Span::styled(format!("📲 {}", self.device_info.alias), THEME.device_alias),
            Span::raw(" | "),
            Span::styled(format!("📱 {} devices ", devices_count), THEME.status_bar),
            Span::raw("| "),
            Span::styled(format!("🟢 Listening on {} ", self.port), THEME.status_bar),
        ];

        if let Some((ref msg, level)) = self.status_message {
            spans.push(Span::raw("| "));
            let style = match level {
                MessageLevel::Success => THEME.status_success,
                MessageLevel::Error => THEME.status_error,
                MessageLevel::Info => THEME.status_info,
            };
            spans.push(Span::styled(msg.clone(), style));
        }

        let line = Line::from(spans);
        frame.render_widget(Paragraph::new(line).style(THEME.status_bar), area);
    }
}

/// Classify a finished send into the right UI update: a plain success/failure,
/// or a `NeedsPin` when the receiver answered 401 (so the UI can prompt).
fn send_update_from_result(
    generation: u64,
    kind: SendKind,
    label: String,
    result: anyhow::Result<()>,
) -> SendUpdate {
    match result {
        Ok(()) => SendUpdate::Finished {
            generation,
            kind,
            label,
            error: None,
        },
        Err(e) => {
            if e.downcast_ref::<crate::error::LocalSendError>()
                .is_some_and(|le| matches!(le, crate::error::LocalSendError::InvalidPin))
            {
                SendUpdate::NeedsPin { generation, kind }
            } else {
                SendUpdate::Finished {
                    generation,
                    kind,
                    label,
                    error: Some(e.to_string()),
                }
            }
        }
    }
}

async fn send_text_message(
    client: &LocalSendClient,
    target: &DeviceInfo,
    message: &str,
    pin: Option<&str>,
) -> anyhow::Result<()> {
    use crate::core::file::{build_file_metadata_from_bytes, generate_file_id};

    let file_data = message.as_bytes().to_vec();
    let file_name = "message.txt".to_string();
    let file_id = generate_file_id();

    let mut metadata = build_file_metadata_from_bytes(
        file_id,
        file_name,
        "text/plain".to_string(),
        file_data.clone(),
    );
    metadata.preview = Some(message.to_string());

    let mut files = HashMap::new();
    files.insert(metadata.id.clone(), metadata.clone());

    let response = client.prepare_upload(target, files, pin).await?;

    if response.session_id.is_empty() {
        // 204 No Content - text message sent via preview
        return Ok(());
    }

    // Write to temp file and upload
    if let Some(token) = response.files.get(&metadata.id) {
        let temp_path = std::env::temp_dir().join(format!("localsend_text_{}.txt", metadata.id));
        tokio::fs::write(&temp_path, &file_data).await?;

        client
            .upload_file(
                target,
                &response.session_id,
                &metadata.id,
                token,
                &temp_path,
                None,
            )
            .await?;

        let _ = tokio::fs::remove_file(temp_path).await;
    }

    Ok(())
}

/// Send a file to a device, reporting per-chunk progress through `progress`.
async fn send_file(
    client: &LocalSendClient,
    target: &DeviceInfo,
    file_path: &Path,
    progress: Option<crate::client::client::ProgressCallback>,
    pin: Option<String>,
) -> anyhow::Result<()> {
    use crate::core::file::build_file_metadata;

    let metadata = build_file_metadata(file_path).await?;

    let mut files = HashMap::new();
    files.insert(metadata.id.clone(), metadata.clone());

    let response = client.prepare_upload(target, files, pin.as_deref()).await?;

    let token = response.files.get(&metadata.id).ok_or_else(|| {
        anyhow::anyhow!("receiver declined the file (no upload token was issued)")
    })?;

    client
        .upload_file(
            target,
            &response.session_id,
            &metadata.id,
            token,
            file_path,
            progress,
        )
        .await?;

    Ok(())
}

/// Open the platform file manager focused on `path`. Best-effort: spawns the
/// OS "open/reveal" helper and returns as soon as it launches (does not wait).
fn reveal_in_file_manager(path: &Path) -> std::io::Result<()> {
    use std::process::Command;

    #[cfg(target_os = "macos")]
    {
        // `-R` reveals the file in Finder (selects it in its folder).
        Command::new("open").arg("-R").arg(path).spawn()?;
    }
    #[cfg(target_os = "windows")]
    {
        // `explorer /select,<path>` opens the folder with the file selected.
        Command::new("explorer")
            .arg(format!("/select,{}", path.display()))
            .spawn()?;
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        // No portable "reveal + select"; open the containing directory instead.
        let target = path.parent().unwrap_or(path);
        Command::new("xdg-open").arg(target).spawn()?;
    }
    Ok(())
}

/// Main entry point for the TUI.
pub async fn run_tui(
    port: u16,
    alias: Option<String>,
    https: bool,
    pin: Option<String>,
    auto_accept: bool,
) -> Result<()> {
    color_eyre::install()?;

    let terminal = ratatui::init();
    let app_result = App::new(port, alias, https, pin, auto_accept)?
        .run(terminal)
        .await;
    ratatui::restore();

    app_result
}

#[cfg(test)]
mod tests {
    use super::{App, SendKind, SendUpdate, send_update_from_result};
    use crate::tui::popup::{MessageLevel, Popup};

    // App::new does no I/O (it binds nothing until run()), so it's safe to build
    // one for pure state-machine assertions.
    fn test_app() -> App {
        App::new(0, Some("t".to_string()), false, None, false).expect("build app")
    }

    #[test]
    fn pin_prompt_opens_when_no_popup_is_shown() {
        let mut app = test_app();
        app.pending_pin_kind = Some(SendKind::File);
        app.maybe_open_pin_prompt();
        assert!(matches!(app.popup, Some(Popup::PinEntry { .. })));
    }

    #[test]
    fn pin_prompt_defers_behind_an_open_popup() {
        let mut app = test_app();
        app.pending_pin_kind = Some(SendKind::File);
        // Another popup (e.g. an incoming TransferConfirm) is already up. The PIN
        // prompt must NOT replace it, or it would drop that popup's PendingRequest
        // and silently decline the peer's transfer.
        app.popup = Some(Popup::Message {
            text: "busy".into(),
            level: MessageLevel::Info,
        });
        app.maybe_open_pin_prompt();
        assert!(matches!(app.popup, Some(Popup::Message { .. })));
    }

    #[test]
    fn no_pin_prompt_without_a_pending_kind() {
        let mut app = test_app();
        app.pending_pin_kind = None;
        app.maybe_open_pin_prompt();
        assert!(app.popup.is_none());
    }

    #[test]
    fn incoming_transfer_preempts_an_open_pin_prompt() {
        use crate::protocol::{DeviceInfo, Protocol};
        use crate::server::events::{PendingRequest, ServerEvent};
        use std::collections::HashMap;

        let mut app = test_app();
        let (tx, rx) = tokio::sync::mpsc::channel(4);
        app.events_rx = Some(rx);
        // A PIN prompt is up (our own send is waiting on a PIN).
        app.pending_pin_kind = Some(SendKind::File);
        app.popup = Some(Popup::PinEntry {
            input: tui_input::Input::default(),
        });

        let sender = DeviceInfo::new("peer".to_string(), 53317, Protocol::Http);
        let (req, mut decision_rx) = PendingRequest::new(sender, HashMap::new());
        tx.try_send(ServerEvent::TransferRequest(req)).unwrap();

        app.poll_server_events();

        // The incoming confirm dialog takes the slot instead of being declined,
        // and the deferred PIN is still pending (reopens once this is answered).
        assert!(matches!(app.popup, Some(Popup::TransferConfirm(_))));
        assert_eq!(app.pending_pin_kind, Some(SendKind::File));
        // The request was NOT answered (no decision delivered yet).
        assert!(matches!(
            decision_rx.try_recv(),
            Err(tokio::sync::oneshot::error::TryRecvError::Empty)
        ));
    }

    #[test]
    fn invalid_pin_error_maps_to_needs_pin() {
        let err = anyhow::Error::new(crate::error::LocalSendError::InvalidPin);
        let update = send_update_from_result(1, SendKind::File, "f".into(), Err(err));
        assert!(matches!(
            update,
            SendUpdate::NeedsPin {
                generation: 1,
                kind: SendKind::File
            }
        ));
    }

    #[test]
    fn other_error_maps_to_finished_with_reason() {
        let err = anyhow::anyhow!("boom");
        let update = send_update_from_result(2, SendKind::Text, "m".into(), Err(err));
        match update {
            SendUpdate::Finished {
                generation: 2,
                error: Some(reason),
                ..
            } => assert!(reason.contains("boom")),
            other => panic!("expected Finished with error, got {other:?}"),
        }
    }

    #[test]
    fn ok_maps_to_finished_success() {
        let update = send_update_from_result(3, SendKind::File, "f".into(), Ok(()));
        assert!(matches!(
            update,
            SendUpdate::Finished {
                generation: 3,
                error: None,
                ..
            }
        ));
    }
}
