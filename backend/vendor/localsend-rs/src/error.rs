use thiserror::Error;

/// Errors that can occur when using LocalSend
#[derive(Error, Debug)]
#[non_exhaustive]
pub enum LocalSendError {
    // ============================================================================
    // I/O and System Errors
    // ============================================================================
    #[error("IO error: {source}")]
    Io {
        #[from]
        source: std::io::Error,
    },

    #[error("Serde JSON error: {source}")]
    Serde {
        #[from]
        source: serde_json::Error,
    },

    // ============================================================================
    // Network Errors
    // ============================================================================
    #[error("HTTP client error: {source}")]
    Reqwest {
        #[from]
        source: reqwest::Error,
    },

    #[error("Address parse error: {source}")]
    AddrParse {
        #[from]
        source: std::net::AddrParseError,
    },

    #[error("Network error: {message}")]
    Network { message: String },

    #[error("Invalid port: {0}")]
    InvalidPort(String),

    #[error("Invalid multicast address: {0}")]
    InvalidMulticastAddress(String),

    // ============================================================================
    // Protocol Errors
    // ============================================================================
    #[error("Invalid device info: {message}")]
    InvalidDevice { message: String },

    #[error("Invalid file metadata: {message}")]
    InvalidFile { message: String },

    #[error("Protocol version mismatch: expected {expected}, got {actual}")]
    VersionMismatch { expected: String, actual: String },

    #[error("Invalid state transition: {message}")]
    InvalidState { message: String },

    // ============================================================================
    // Transfer Errors
    // ============================================================================
    #[error("Invalid token or session ID")]
    InvalidToken,

    #[error("Session {session_id} not found")]
    SessionNotFound { session_id: String },

    #[error("File {file_id} not found in session {session_id}")]
    FileNotFound { file_id: String, session_id: String },

    #[error("Transfer failed: {reason}")]
    TransferFailed {
        reason: String,
        session_id: Option<String>,
    },

    #[error("Session blocked by another transfer")]
    SessionBlocked,

    // ============================================================================
    // Authentication Errors
    // ============================================================================
    #[error("Invalid PIN")]
    InvalidPin,

    #[error("PIN required but not provided")]
    PinRequired,

    // ============================================================================
    // HTTP Status Errors
    // ============================================================================
    #[error("Request rejected by receiver (HTTP {status})")]
    Rejected { status: u16 },

    #[error("Request failed with HTTP {status}: {message}")]
    HttpFailed { status: u16, message: String },

    #[error("Too many requests")]
    RateLimited,
}

impl LocalSendError {
    /// Create a network error with a message
    pub fn network(msg: impl Into<String>) -> Self {
        Self::Network {
            message: msg.into(),
        }
    }

    /// Create an invalid device error with a message
    pub fn invalid_device(msg: impl Into<String>) -> Self {
        Self::InvalidDevice {
            message: msg.into(),
        }
    }

    /// Create an invalid file error with a message
    pub fn invalid_file(msg: impl Into<String>) -> Self {
        Self::InvalidFile {
            message: msg.into(),
        }
    }

    /// Create an invalid state error with a message
    pub fn invalid_state(msg: impl Into<String>) -> Self {
        Self::InvalidState {
            message: msg.into(),
        }
    }

    /// Create a transfer failed error
    pub fn transfer_failed(reason: impl Into<String>, session_id: Option<String>) -> Self {
        Self::TransferFailed {
            reason: reason.into(),
            session_id,
        }
    }

    /// Create an HTTP failed error
    pub fn http_failed(status: u16, message: impl Into<String>) -> Self {
        Self::HttpFailed {
            status,
            message: message.into(),
        }
    }
}

pub type Result<T> = std::result::Result<T, LocalSendError>;
