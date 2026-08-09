#[cfg(feature = "https")]
use crate::error::Result;

#[cfg(feature = "https")]
#[derive(Clone)]
pub struct TlsCertificate {
    pub cert_pem: String,
    pub key_pem: String,
    pub cert_der: Vec<u8>,
    pub fingerprint: String,
}

#[cfg(feature = "https")]
pub fn generate_tls_certificate() -> Result<TlsCertificate> {
    use rcgen::generate_simple_self_signed;

    let cert = generate_simple_self_signed(vec!["localhost".to_string()]).map_err(|e| {
        crate::error::LocalSendError::network(format!("Failed to generate TLS certificate: {}", e))
    })?;

    let cert_der = cert.cert.der().to_vec();
    let fingerprint = super::hash::sha256_from_bytes(&cert_der);

    Ok(TlsCertificate {
        cert_pem: cert.cert.pem(),
        key_pem: cert.signing_key.serialize_pem(),
        cert_der,
        fingerprint,
    })
}
