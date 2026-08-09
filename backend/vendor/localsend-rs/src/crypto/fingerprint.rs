/// Generate a unique fingerprint for device identification
pub fn generate_fingerprint() -> String {
    uuid::Uuid::new_v4().to_string()
}
