//! Prelude module for convenient imports
//!
//! Use `use localsend_rs::prelude::*;` to import commonly used types

// Core types
pub use crate::core::{
    DeviceInfoBuilder, Session, build_file_metadata, build_file_metadata_from_bytes,
    generate_file_id, get_device_model, get_device_type, get_local_ip, get_mime_type,
};

// Protocol types
pub use crate::protocol::{
    DEFAULT_HTTP_PORT, DEFAULT_MULTICAST_ADDRESS, DEFAULT_MULTICAST_PORT, DeviceInfo, DeviceType,
    FileId, FileMetadata, PROTOCOL_VERSION, Port, PrepareUploadRequest, PrepareUploadResponse,
    Protocol, ReceivedFile, RegisterMessage, SessionId, Token, validate_device_info,
    validate_file_metadata, validate_protocol_version,
};

// Crypto
pub use crate::crypto::{generate_fingerprint, sha256_from_bytes, sha256_from_file};

#[cfg(feature = "https")]
pub use crate::crypto::{TlsCertificate, generate_tls_certificate};

// Client & Server
pub use crate::client::LocalSendClient;
pub use crate::server::LocalSendServer;

// Discovery
pub use crate::discovery::{Discovery, HttpDiscovery, MulticastDiscovery};

// Error handling
pub use crate::error::{LocalSendError, Result};
