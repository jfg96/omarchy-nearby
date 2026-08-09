//! Public event stream for library consumers (the headless accept API).

use crate::protocol::{DeviceInfo, FileId, FileMetadata, SessionId};
use std::collections::HashMap;
use std::path::PathBuf;
use tokio::sync::oneshot;

/// Events emitted by [`crate::server::LocalSendServer`].
#[derive(Debug)]
pub enum ServerEvent {
    /// A peer registered itself with us over HTTP (a discovery sighting).
    PeerRegistered(DeviceInfo),
    /// A sender wants to transfer files. Respond via the [`PendingRequest`].
    /// Dropping the request (or ignoring it past the accept timeout) declines it.
    TransferRequest(PendingRequest),
    /// The receiver did not answer a transfer request before accept_timeout.
    TransferRequestExpired {
        request_id: String,
    },
    /// A LocalSend text message accepted from its inline `preview` payload.
    /// Text is never persisted automatically; consumers may offer explicit
    /// copy/save actions appropriate to their platform.
    TextReceived {
        session_id: SessionId,
        text: String,
        sender_alias: String,
    },
    /// A browser is waiting for approval to download the active Web Share.
    WebShareRequest(PendingWebShareRequest),
    WebShareDownloadProgress {
        session_id: SessionId,
        file_id: FileId,
        bytes_sent: u64,
        total_bytes: u64,
    },
    WebShareSessionDone {
        session_id: SessionId,
    },
    /// Cumulative payload bytes written for an active receive session.
    FileReceiveProgress {
        session_id: SessionId,
        file_id: FileId,
        file_name: String,
        sender_alias: String,
        bytes_received: u64,
        total_bytes: u64,
        file_count: usize,
    },
    /// One file finished writing to disk.
    FileReceived {
        session_id: SessionId,
        file_id: FileId,
        file_name: String,
        path: PathBuf,
        size: u64,
        sender_alias: String,
        /// Retained for source compatibility. First-class text messages are
        /// emitted as [`ServerEvent::TextReceived`].
        message_text: Option<String>,
    },
    /// All accepted files of a session were validated and committed.
    SessionCompleted {
        session_id: SessionId,
    },
    /// The sender explicitly cancelled an accepted session.
    SessionCancelled {
        session_id: SessionId,
    },
    /// An accepted session failed and cannot complete.
    SessionFailed {
        session_id: SessionId,
        message: String,
    },
}

#[derive(Clone, Debug)]
pub struct PendingWebShareRequest {
    session_id: SessionId,
    ip: std::net::IpAddr,
}

impl PendingWebShareRequest {
    pub(crate) fn new(session_id: SessionId, ip: std::net::IpAddr) -> Self {
        Self { session_id, ip }
    }

    pub fn session_id(&self) -> &SessionId {
        &self.session_id
    }

    pub fn ip(&self) -> std::net::IpAddr {
        self.ip
    }
}

/// The consumer's answer to a transfer request.
#[derive(Debug, Clone, PartialEq)]
pub enum TransferDecision {
    Accept,
    AcceptFiles(Vec<FileId>),
    Decline,
}

/// Handle to answer an incoming `prepare-upload`. Consume it exactly once.
#[derive(Debug)]
pub struct PendingRequest {
    request_id: String,
    sender: DeviceInfo,
    files: HashMap<FileId, FileMetadata>,
    responder: oneshot::Sender<TransferDecision>,
}

impl PendingRequest {
    // Not yet called outside tests: handler wiring lands in Task 2.2.
    #[allow(dead_code)]
    pub(crate) fn new(
        sender: DeviceInfo,
        files: HashMap<FileId, FileMetadata>,
    ) -> (Self, oneshot::Receiver<TransferDecision>) {
        let (tx, rx) = oneshot::channel();
        (
            Self {
                request_id: FileId::new().to_string(),
                sender,
                files,
                responder: tx,
            },
            rx,
        )
    }

    pub fn sender(&self) -> &DeviceInfo {
        &self.sender
    }

    pub fn request_id(&self) -> &str {
        &self.request_id
    }

    pub fn files(&self) -> &HashMap<FileId, FileMetadata> {
        &self.files
    }

    /// Accept every offered file. No-op if the sender already timed out.
    pub fn accept(self) -> bool {
        self.responder.send(TransferDecision::Accept).is_ok()
    }

    /// Accept a subset of the offered files (empty = decline).
    pub fn accept_files(self, ids: Vec<FileId>) -> bool {
        self.responder
            .send(TransferDecision::AcceptFiles(ids))
            .is_ok()
    }

    pub fn decline(self) -> bool {
        self.responder.send(TransferDecision::Decline).is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{DeviceInfo, Protocol};
    use std::collections::HashMap;

    fn req() -> (
        PendingRequest,
        tokio::sync::oneshot::Receiver<TransferDecision>,
    ) {
        let sender = DeviceInfo::new("s".to_string(), 53317, Protocol::Http);
        PendingRequest::new(sender, HashMap::new())
    }

    #[tokio::test]
    async fn accept_sends_accept_decision() {
        let (r, rx) = req();
        r.accept();
        assert!(matches!(rx.await, Ok(TransferDecision::Accept)));
    }

    #[tokio::test]
    async fn decline_sends_decline_decision() {
        let (r, rx) = req();
        r.decline();
        assert!(matches!(rx.await, Ok(TransferDecision::Decline)));
    }

    #[tokio::test]
    async fn dropping_request_closes_channel() {
        let (r, rx) = req();
        drop(r);
        assert!(rx.await.is_err()); // handler treats closed channel as decline
    }
}
