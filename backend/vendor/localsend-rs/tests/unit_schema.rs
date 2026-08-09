use localsend_rs::{AnnouncementMessage, DeviceInfo};

#[test]
fn device_info_accepts_spec_minimal_payload() {
    // No download, no deviceModel, no ip — all valid per spec.
    let json = r#"{
        "alias": "Phone", "version": "2.1", "deviceType": "mobile",
        "fingerprint": "abc", "port": 53317, "protocol": "http"
    }"#;
    let d: DeviceInfo = serde_json::from_str(json).expect("minimal payload must parse");
    assert!(!d.download);
    assert_eq!(d.port, 53317);
}

#[test]
fn device_info_tolerates_missing_port_and_protocol() {
    // Some peers omit port/protocol in prepare-upload's embedded info.
    let json = r#"{ "alias": "Phone", "version": "2.1", "fingerprint": "abc" }"#;
    let d: DeviceInfo = serde_json::from_str(json).expect("must parse");
    assert_eq!(d.port, 53317); // defaulted
    assert_eq!(d.protocol.as_str(), "http"); // defaulted
}

#[test]
fn announcement_accepts_minimal_payload() {
    let json = r#"{
        "alias": "Phone", "version": "2.1", "fingerprint": "abc",
        "port": 53317, "protocol": "http", "announce": true
    }"#;
    let a: AnnouncementMessage = serde_json::from_str(json).expect("must parse");
    assert!(a.announce);
    assert!(!a.download);
}
