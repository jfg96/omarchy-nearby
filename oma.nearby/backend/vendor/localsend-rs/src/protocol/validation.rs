use crate::error::{LocalSendError, Result};
use crate::protocol::{DeviceInfo, FileMetadata, PROTOCOL_VERSION};

/// Validates protocol version compatibility
///
/// LocalSend protocol follows semantic versioning.
/// Major version must match, minor version can differ.
pub fn validate_protocol_version(version: &str) -> Result<()> {
    let parts: Vec<&str> = version.split('.').collect();
    let expected_parts: Vec<&str> = PROTOCOL_VERSION.split('.').collect();

    if parts.is_empty() || expected_parts.is_empty() {
        return Err(LocalSendError::VersionMismatch {
            expected: PROTOCOL_VERSION.to_string(),
            actual: version.to_string(),
        });
    }

    // Major version must match
    if parts[0] != expected_parts[0] {
        return Err(LocalSendError::VersionMismatch {
            expected: PROTOCOL_VERSION.to_string(),
            actual: version.to_string(),
        });
    }

    Ok(())
}

/// Validates that device info contains all required fields
pub fn validate_device_info(device: &DeviceInfo) -> Result<()> {
    if device.alias.trim().is_empty() {
        return Err(LocalSendError::invalid_device(
            "Device alias cannot be empty",
        ));
    }

    validate_protocol_version(&device.version)?;

    if device.fingerprint.trim().is_empty() {
        return Err(LocalSendError::invalid_device(
            "Device fingerprint cannot be empty",
        ));
    }

    Ok(())
}

/// Validates file metadata
pub fn validate_file_metadata(metadata: &FileMetadata) -> Result<()> {
    if metadata.id.as_str().trim().is_empty() {
        return Err(LocalSendError::invalid_file("File ID cannot be empty"));
    }

    if metadata.file_name.trim().is_empty() {
        return Err(LocalSendError::invalid_file("File name cannot be empty"));
    }

    if metadata.size == 0 && metadata.preview.is_none() {
        return Err(LocalSendError::invalid_file(
            "File size is 0 and no preview provided",
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_protocol_version_compatible() {
        // Same version
        assert!(validate_protocol_version("2.1").is_ok());

        // Different minor version (should be compatible)
        assert!(validate_protocol_version("2.0").is_ok());
        assert!(validate_protocol_version("2.2").is_ok());
    }

    #[test]
    fn test_validate_protocol_version_incompatible() {
        // Different major version
        assert!(validate_protocol_version("1.0").is_err());
        assert!(validate_protocol_version("3.0").is_err());

        // Invalid format
        assert!(validate_protocol_version("").is_err());
    }

    #[test]
    fn test_validate_device_info() {
        let mut device = DeviceInfo {
            alias: "Test Device".to_string(),
            version: PROTOCOL_VERSION.to_string(),
            device_model: None,
            device_type: None,
            fingerprint: "abc123".to_string(),
            port: 53317,
            protocol: crate::protocol::Protocol::Https,
            download: false,
            ip: None,
        };

        // Valid device
        assert!(validate_device_info(&device).is_ok());

        // Empty alias
        device.alias = "".to_string();
        assert!(validate_device_info(&device).is_err());
        device.alias = "Test Device".to_string();

        // Empty fingerprint
        device.fingerprint = "".to_string();
        assert!(validate_device_info(&device).is_err());
        device.fingerprint = "abc123".to_string();

        // Invalid version
        device.version = "3.0".to_string();
        assert!(validate_device_info(&device).is_err());
    }

    #[test]
    fn test_validate_file_metadata() {
        let mut metadata = FileMetadata {
            id: crate::protocol::FileId("file123".to_string()),
            file_name: "test.txt".to_string(),
            size: 1024,
            file_type: "text/plain".to_string(),
            sha256: None,
            preview: None,
            metadata: None,
        };

        // Valid metadata
        assert!(validate_file_metadata(&metadata).is_ok());

        // Empty ID
        metadata.id = crate::protocol::FileId("".to_string());
        assert!(validate_file_metadata(&metadata).is_err());
        metadata.id = crate::protocol::FileId("file123".to_string());

        // Empty file name
        metadata.file_name = "".to_string();
        assert!(validate_file_metadata(&metadata).is_err());
        metadata.file_name = "test.txt".to_string();

        // Zero size without preview (invalid for regular files)
        metadata.size = 0;
        assert!(validate_file_metadata(&metadata).is_err());

        // Zero size with preview (valid for text messages)
        metadata.preview = Some("Hello".to_string());
        assert!(validate_file_metadata(&metadata).is_ok());
    }
}
