use crate::core::device::{get_device_model, get_device_type};
use crate::crypto::generate_fingerprint;
use crate::protocol::{DeviceInfo, DeviceType, PROTOCOL_VERSION, Protocol};

/// Builder for DeviceInfo with sensible defaults
#[derive(Clone, Debug)]
pub struct DeviceInfoBuilder {
    alias: String,
    port: u16,
    protocol: Protocol,
    fingerprint: Option<String>,
    device_model: Option<String>,
    device_type: Option<DeviceType>,
    download: bool,
    ip: Option<String>,
}

impl DeviceInfoBuilder {
    /// Create a new DeviceInfoBuilder with required fields
    pub fn new(alias: impl Into<String>, port: u16) -> Self {
        Self {
            alias: alias.into(),
            port,
            protocol: Protocol::Https, // Default to HTTPS for security
            fingerprint: None,
            device_model: None,
            device_type: None,
            download: false,
            ip: None,
        }
    }

    /// Set the protocol (HTTP or HTTPS)
    pub fn protocol(mut self, protocol: Protocol) -> Self {
        self.protocol = protocol;
        self
    }

    /// Enable HTTP mode (defaults to HTTPS)
    pub fn http(mut self) -> Self {
        self.protocol = Protocol::Http;
        self
    }

    /// Enable HTTPS mode (this is the default)
    pub fn https(mut self) -> Self {
        self.protocol = Protocol::Https;
        self
    }

    /// Set a custom fingerprint
    pub fn fingerprint(mut self, fingerprint: impl Into<String>) -> Self {
        self.fingerprint = Some(fingerprint.into());
        self
    }

    /// Set the device model
    pub fn device_model(mut self, model: impl Into<String>) -> Self {
        self.device_model = Some(model.into());
        self
    }

    /// Set the device type
    pub fn device_type(mut self, device_type: DeviceType) -> Self {
        self.device_type = Some(device_type);
        self
    }

    /// Enable download API
    pub fn enable_download(mut self) -> Self {
        self.download = true;
        self
    }

    /// Set the IP address
    pub fn ip(mut self, ip: impl Into<String>) -> Self {
        self.ip = Some(ip.into());
        self
    }

    /// Build the DeviceInfo
    pub fn build(self) -> DeviceInfo {
        DeviceInfo {
            alias: self.alias,
            version: PROTOCOL_VERSION.to_string(),
            device_model: self.device_model.or_else(|| Some(get_device_model())),
            device_type: self.device_type.or_else(|| Some(get_device_type())),
            fingerprint: self.fingerprint.unwrap_or_else(generate_fingerprint),
            port: self.port,
            protocol: self.protocol,
            download: self.download,
            ip: self.ip,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_device_info_builder_defaults() {
        let device = DeviceInfoBuilder::new("Test Device", 53317).build();

        assert_eq!(device.alias, "Test Device");
        assert_eq!(device.port, 53317);
        assert_eq!(device.protocol, Protocol::Https); // Default
        assert!(!device.fingerprint.is_empty());
        assert!(!device.download); // Default
    }

    #[test]
    fn test_device_info_builder_custom() {
        let device = DeviceInfoBuilder::new("Custom Device", 8080)
            .http()
            .enable_download()
            .device_type(DeviceType::Mobile)
            .device_model("iPhone 15")
            .fingerprint("custom-fp-123")
            .ip("192.168.1.100")
            .build();

        assert_eq!(device.alias, "Custom Device");
        assert_eq!(device.port, 8080);
        assert_eq!(device.protocol, Protocol::Http);
        assert_eq!(device.device_type, Some(DeviceType::Mobile));
        assert_eq!(device.device_model, Some("iPhone 15".to_string()));
        assert_eq!(device.fingerprint, "custom-fp-123");
        assert_eq!(device.ip, Some("192.168.1.100".to_string()));
        assert!(device.download);
    }

    #[test]
    fn test_builder_chaining() {
        let device = DeviceInfoBuilder::new("Chain Test", 9999)
            .https()
            .enable_download()
            .build();

        assert_eq!(device.protocol, Protocol::Https);
        assert!(device.download);
    }
}
