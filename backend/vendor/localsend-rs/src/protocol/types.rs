use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;

// ============================================================================
// Newtype Patterns for Type Safety
// ============================================================================

/// Protocol type for LocalSend communication
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Protocol {
    #[default]
    Http,
    Https,
}

impl Protocol {
    pub fn as_str(&self) -> &'static str {
        match self {
            Protocol::Http => "http",
            Protocol::Https => "https",
        }
    }
}

impl fmt::Display for Protocol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl From<&str> for Protocol {
    fn from(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "https" => Protocol::Https,
            _ => Protocol::Http,
        }
    }
}

impl From<String> for Protocol {
    fn from(s: String) -> Self {
        Protocol::from(s.as_str())
    }
}

/// Session identifier for file transfer sessions
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SessionId(String);

impl SessionId {
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }

    pub fn from_string(s: String) -> Self {
        Self(s)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl fmt::Display for SessionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Default for SessionId {
    fn default() -> Self {
        Self::new()
    }
}

/// File identifier for individual files in a transfer
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct FileId(pub String);

impl FileId {
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }

    pub fn from_string(s: String) -> Self {
        Self(s)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for FileId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Default for FileId {
    fn default() -> Self {
        Self::new()
    }
}

/// Authorization token for file uploads
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Token(String);

impl Token {
    /// Random per-file upload token (128-bit, hex).
    pub fn random() -> Self {
        Self(uuid::Uuid::new_v4().simple().to_string())
    }

    pub fn from_string(s: String) -> Self {
        Self(s)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Network port with validation
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Port(u16);

impl Port {
    pub fn new(port: u16) -> Result<Self, crate::error::LocalSendError> {
        if port == 0 {
            return Err(crate::error::LocalSendError::InvalidPort(
                "Port cannot be 0".to_string(),
            ));
        }
        Ok(Port(port))
    }

    pub fn new_unchecked(port: u16) -> Self {
        Port(port)
    }

    pub fn get(&self) -> u16 {
        self.0
    }
}

impl fmt::Display for Port {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Default for Port {
    fn default() -> Self {
        Port(crate::protocol::constants::DEFAULT_HTTP_PORT)
    }
}

// ============================================================================
// Device Types
// ============================================================================

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum DeviceType {
    Mobile,
    #[default]
    Desktop,
    Web,
    Headless,
    Server,
}

impl DeviceType {
    /// Parse a wire `deviceType` string. Unknown values (for example HarmonyOS reports
    /// `"tablet"`, which is not in the protocol enum) map to [`DeviceType::Desktop`],
    /// matching the official app's `@MappableEnum(defaultValue: DeviceType.desktop)` and
    /// localsend-ts's `lenientDeviceType`. Keeping this lenient means such a peer is still
    /// discovered instead of being dropped on a failed deserialization.
    fn from_wire(value: &str) -> Self {
        match value.to_lowercase().as_str() {
            "mobile" => DeviceType::Mobile,
            "desktop" => DeviceType::Desktop,
            "web" => DeviceType::Web,
            "headless" => DeviceType::Headless,
            "server" => DeviceType::Server,
            _ => DeviceType::Desktop,
        }
    }
}

// Hand-written so an unknown `deviceType` degrades to `Desktop` instead of failing the
// whole payload (see `from_wire`). Serialization stays derived and unchanged.
impl<'de> Deserialize<'de> for DeviceType {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        Ok(DeviceType::from_wire(&raw))
    }
}

fn default_port() -> u16 {
    crate::protocol::constants::DEFAULT_HTTP_PORT
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DeviceInfo {
    pub alias: String,
    pub version: String,
    #[serde(rename = "deviceModel", default)]
    pub device_model: Option<String>,
    #[serde(rename = "deviceType", default)]
    pub device_type: Option<DeviceType>,
    pub fingerprint: String,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default = "Protocol::default")]
    pub protocol: Protocol,
    #[serde(default)]
    pub download: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ip: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FileMetadata {
    pub id: FileId,
    #[serde(rename = "fileName")]
    pub file_name: String,
    pub size: u64,
    #[serde(rename = "fileType")]
    pub file_type: String,
    pub sha256: Option<String>,
    pub preview: Option<String>,
    pub metadata: Option<FileMetadataDetails>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FileMetadataDetails {
    pub modified: Option<String>,
    pub accessed: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PrepareUploadRequest {
    pub info: DeviceInfo,
    pub files: HashMap<FileId, FileMetadata>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PrepareUploadResponse {
    #[serde(rename = "sessionId")]
    pub session_id: SessionId,
    pub files: HashMap<FileId, Token>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReceivedFile {
    pub file_name: String,
    pub size: u64,
    pub sender: String,
    pub time: String,
    /// Absolute path the file was written to (post-collision-rename).
    pub path: std::path::PathBuf,
    /// Present when the item is a text message; the inbox shows this inline.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message_text: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AnnouncementMessage {
    pub alias: String,
    pub version: String,
    #[serde(rename = "deviceModel", default)]
    pub device_model: Option<String>,
    #[serde(rename = "deviceType", default)]
    pub device_type: Option<DeviceType>,
    pub fingerprint: String,
    pub port: u16,
    pub protocol: Protocol,
    #[serde(default)]
    pub download: bool,
    #[serde(default)]
    pub announce: bool,
    #[serde(default)]
    pub announcement: Option<bool>,
}

pub type RegisterMessage = DeviceInfo;

impl DeviceInfo {
    pub fn new(alias: String, port: u16, protocol: Protocol) -> Self {
        Self {
            alias,
            version: crate::protocol::constants::PROTOCOL_VERSION.to_string(),
            device_model: None,
            device_type: None,
            fingerprint: String::new(),
            port,
            protocol,
            download: false,
            ip: None,
        }
    }

    // This function is added based on the provided "Code Edit" and instruction.
    // It assumes `_src` is a type that has an `ip()` method returning an IP address.
    // For example, `std::net::SocketAddr` or similar.
    pub fn from_announcement<T: std::net::ToSocketAddrs>(
        announcement: AnnouncementMessage,
        _src: T,
    ) -> Self {
        let socket_addr = _src.to_socket_addrs().ok().and_then(|mut i| i.next());
        Self {
            alias: announcement.alias,
            version: announcement.version,
            device_model: announcement.device_model,
            device_type: announcement.device_type,
            fingerprint: announcement.fingerprint,
            port: announcement.port,
            protocol: announcement.protocol,
            download: announcement.download,
            ip: socket_addr.map(|s| s.ip().to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{DeviceInfo, DeviceType};

    #[test]
    fn known_device_types_round_trip() {
        for (wire, expected) in [
            ("mobile", DeviceType::Mobile),
            ("desktop", DeviceType::Desktop),
            ("web", DeviceType::Web),
            ("headless", DeviceType::Headless),
            ("server", DeviceType::Server),
        ] {
            let parsed: DeviceType =
                serde_json::from_value(serde_json::json!(wire)).expect("known type parses");
            assert_eq!(parsed, expected);
            // Serialization stays stable/lowercase.
            assert_eq!(
                serde_json::to_value(expected).unwrap(),
                serde_json::json!(wire)
            );
        }
    }

    #[test]
    fn unknown_device_type_degrades_to_desktop() {
        // HarmonyOS advertises "tablet", which is not in the protocol enum. It must not
        // fail the payload — the official app and localsend-ts both fall back to desktop.
        let parsed: DeviceType =
            serde_json::from_value(serde_json::json!("tablet")).expect("unknown type is lenient");
        assert_eq!(parsed, DeviceType::Desktop);
    }

    #[test]
    fn info_payload_from_a_harmonyos_tablet_deserializes() {
        // Shape of the `/info` body a HarmonyOS tablet returns: `deviceType: "tablet"` is
        // not in the protocol enum and previously failed the whole payload. Values are
        // synthetic — the only field under test is the unknown `deviceType`.
        let body = serde_json::json!({
            "alias": "Example Tablet",
            "version": "2.1",
            "deviceModel": "Example MatePad",
            "deviceType": "tablet",
            "fingerprint": "A1B2C3D4A1B2C3D4A1B2C3D4A1B2C3D4A1B2C3D4A1B2C3D4A1B2C3D4A1B2C3D4",
            "download": false
        });

        let device: DeviceInfo = serde_json::from_value(body).expect("tablet info parses");
        assert_eq!(device.alias, "Example Tablet");
        assert_eq!(device.device_type, Some(DeviceType::Desktop));
        assert_eq!(device.fingerprint.len(), 64);
    }
}
