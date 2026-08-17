use crate::client::trust_policy::TlsTrustPolicy;
#[cfg(feature = "https")]
use crate::crypto::TlsCertificate;
use crate::error::{LocalSendError, Result};
use crate::protocol::{
    DeviceInfo, FileId, FileMetadata, PrepareUploadRequest, PrepareUploadResponse, SessionId, Token,
};
use futures_util::{StreamExt, stream};
use reqwest::{Body, Client as HttpClient, StatusCode};
use std::collections::HashMap;
#[cfg(feature = "https")]
use std::sync::Arc;
use tokio::fs::File;
use tokio_util::io::ReaderStream;

pub type ProgressCallback = Box<dyn Fn(u64, u64, f64) + Send + Sync>;

fn prepare_upload_url(target: &DeviceInfo, ip: &str, pin: Option<&str>) -> Result<reqwest::Url> {
    let base = format!(
        "{}://{}:{}/api/localsend/v2/prepare-upload",
        target.protocol, ip, target.port
    );
    let mut url = reqwest::Url::parse(&base)
        .map_err(|error| LocalSendError::network(format!("Invalid target URL: {error}")))?;
    if let Some(pin) = pin {
        url.query_pairs_mut().append_pair("pin", pin);
    }
    Ok(url)
}

#[cfg(feature = "https")]
fn reqwest_identity(identity: &TlsCertificate) -> Result<reqwest::Identity> {
    let pem = format!("{}\n{}", identity.cert_pem, identity.key_pem);
    reqwest::Identity::from_pem(pem.as_bytes())
        .map_err(|e| LocalSendError::network(format!("Invalid client identity: {e}")))
}

#[derive(Clone)]
pub struct LocalSendClient {
    client: HttpClient,
    device: DeviceInfo,
}

impl LocalSendClient {
    pub fn new(device: DeviceInfo) -> Self {
        Self {
            client: HttpClient::new(),
            device,
        }
    }

    pub fn with_trust_policy(device: DeviceInfo, policy: TlsTrustPolicy) -> Result<Self> {
        Self::with_optional_identity(device, policy, None)
    }

    #[cfg(feature = "https")]
    pub fn with_trust_policy_and_identity(
        device: DeviceInfo,
        policy: TlsTrustPolicy,
        identity: &TlsCertificate,
    ) -> Result<Self> {
        Self::with_optional_identity(device, policy, Some(identity))
    }

    fn with_optional_identity(
        device: DeviceInfo,
        policy: TlsTrustPolicy,
        #[cfg(feature = "https")] identity: Option<&TlsCertificate>,
        #[cfg(not(feature = "https"))] _identity: Option<&()>,
    ) -> Result<Self> {
        let client = match policy {
            TlsTrustPolicy::InsecureForTests => {
                let mut builder = HttpClient::builder().danger_accept_invalid_certs(true);
                #[cfg(feature = "https")]
                if let Some(identity) = identity {
                    builder = builder.identity(reqwest_identity(identity)?);
                }
                builder.build().map_err(LocalSendError::from)?
            }
            TlsTrustPolicy::PinnedFingerprint(fingerprint) => {
                #[cfg(feature = "https")]
                {
                    let verifier = FingerprintVerifier::new(fingerprint)?;
                    let builder = rustls::ClientConfig::builder()
                        .dangerous()
                        .with_custom_certificate_verifier(Arc::new(verifier));
                    let tls_config = if let Some(identity) = identity {
                        let mut cert_reader = std::io::Cursor::new(identity.cert_pem.as_bytes());
                        let certs = rustls_pemfile::certs(&mut cert_reader)
                            .collect::<std::result::Result<Vec<_>, _>>()
                            .map_err(|e| {
                                LocalSendError::network(format!("Invalid client certificate: {e}"))
                            })?;
                        let mut key_reader = std::io::Cursor::new(identity.key_pem.as_bytes());
                        let key = rustls_pemfile::private_key(&mut key_reader)
                            .map_err(|e| {
                                LocalSendError::network(format!("Invalid client key: {e}"))
                            })?
                            .ok_or_else(|| LocalSendError::network("Missing client key"))?;
                        builder.with_client_auth_cert(certs, key).map_err(|e| {
                            LocalSendError::network(format!("Invalid client identity: {e}"))
                        })?
                    } else {
                        builder.with_no_client_auth()
                    };
                    HttpClient::builder()
                        .tls_backend_preconfigured(tls_config)
                        .build()
                        .map_err(LocalSendError::from)?
                }

                #[cfg(not(feature = "https"))]
                {
                    let _ = fingerprint;
                    return Err(LocalSendError::network(
                        "Pinned LocalSend TLS requires the https feature",
                    ));
                }
            }
        };

        Ok(Self { client, device })
    }

    pub async fn register(&self, target: &DeviceInfo) -> Result<DeviceInfo> {
        let ip = target
            .ip
            .as_ref()
            .ok_or_else(|| LocalSendError::network("Target IP not provided"))?;
        let url = format!(
            "{}://{}:{}/api/localsend/v2/register",
            target.protocol, ip, target.port
        );

        let response = self.client.post(&url).json(&self.device).send().await?;
        let status = response.status();

        if status.is_success() {
            let bytes = response.bytes().await?;
            if bytes.is_empty() {
                return Ok(target.clone());
            }

            match serde_json::from_slice::<DeviceInfo>(&bytes) {
                Ok(info) => Ok(info),
                Err(_e) => {
                    // If we successfully posted our info (200 OK) but can't parse the response,
                    // we still consider registration successful because the other device received our info.
                    // This often happens if the other device sends a slightly different JSON format.
                    Ok(target.clone())
                }
            }
        } else if status == 401 || status == 403 {
            Err(LocalSendError::Rejected {
                status: status.as_u16(),
            })
        } else {
            Err(LocalSendError::http_failed(
                status.as_u16(),
                "Registration failed",
            ))
        }
    }

    pub async fn prepare_upload(
        &self,
        target: &DeviceInfo,
        files: HashMap<FileId, FileMetadata>,
        pin: Option<&str>,
    ) -> Result<PrepareUploadResponse> {
        let ip = target
            .ip
            .as_ref()
            .ok_or_else(|| LocalSendError::network("Target IP not provided"))?;
        let url = prepare_upload_url(target, ip, pin)?;

        let request = PrepareUploadRequest {
            info: self.device.clone(),
            files,
        };

        let response = self.client.post(url).json(&request).send().await?;

        let status = response.status();
        match status {
            StatusCode::OK => {
                let upload_response: PrepareUploadResponse = response.json().await?;
                Ok(upload_response)
            }
            StatusCode::NO_CONTENT => {
                // This happens when sending text messages or if the receiver accepted the metadata but needs no file transfer
                Ok(PrepareUploadResponse {
                    session_id: SessionId::from_string(String::new()),
                    files: HashMap::new(),
                })
            }
            StatusCode::UNAUTHORIZED => Err(LocalSendError::InvalidPin),
            StatusCode::FORBIDDEN => Err(LocalSendError::Rejected {
                status: status.as_u16(),
            }),
            StatusCode::CONFLICT => Err(LocalSendError::SessionBlocked),
            StatusCode::TOO_MANY_REQUESTS => Err(LocalSendError::RateLimited),
            StatusCode::INTERNAL_SERVER_ERROR => Err(LocalSendError::network("Server error")),
            _ => Err(LocalSendError::http_failed(
                status.as_u16(),
                "Prepare upload failed",
            )),
        }
    }

    pub async fn upload_file(
        &self,
        target: &DeviceInfo,
        session_id: &SessionId,
        file_id: &FileId,
        token: &Token,
        file_path: &std::path::Path,
        progress: Option<ProgressCallback>,
    ) -> Result<()> {
        self.upload_file_with_rate_limit(
            target, session_id, file_id, token, file_path, progress, None,
        )
        .await
    }

    /// Uploads an in-memory payload without materializing it as a temporary
    /// file. This is intended for small generated content such as text shares.
    pub async fn upload_bytes(
        &self,
        target: &DeviceInfo,
        session_id: &SessionId,
        file_id: &FileId,
        token: &Token,
        bytes: Vec<u8>,
        progress: Option<ProgressCallback>,
    ) -> Result<()> {
        let ip = target
            .ip
            .as_ref()
            .ok_or_else(|| LocalSendError::network("Target IP not provided"))?;
        let url = format!(
            "{}://{}:{}/api/localsend/v2/upload?sessionId={}&fileId={}&token={}",
            target.protocol, ip, target.port, session_id, file_id, token
        );
        let total_bytes = bytes.len() as u64;
        let started = std::time::Instant::now();
        let data = bytes::Bytes::from(bytes);
        let body_stream =
            stream::once(async move { Ok::<_, std::io::Error>(data) }).inspect(move |chunk| {
                if let (Ok(chunk), Some(callback)) = (chunk, progress.as_ref()) {
                    callback(
                        chunk.len() as u64,
                        total_bytes,
                        started.elapsed().as_secs_f64(),
                    );
                }
            });
        let response = self
            .client
            .post(&url)
            .header(reqwest::header::CONTENT_LENGTH, total_bytes)
            .body(Body::wrap_stream(body_stream))
            .send()
            .await?;

        match response.status() {
            StatusCode::OK | StatusCode::NO_CONTENT => Ok(()),
            status => Err(LocalSendError::http_failed(
                status.as_u16(),
                "In-memory upload failed",
            )),
        }
    }

    /// Uploads a file while optionally pacing the source stream. The rate
    /// limit is intended for deterministic integration tests; normal callers
    /// should use [`Self::upload_file`].
    #[allow(clippy::too_many_arguments)]
    pub async fn upload_file_with_rate_limit(
        &self,
        target: &DeviceInfo,
        session_id: &SessionId,
        file_id: &FileId,
        token: &Token,
        file_path: &std::path::Path,
        progress: Option<ProgressCallback>,
        rate_limit_bytes_per_second: Option<u64>,
    ) -> Result<()> {
        let ip = target
            .ip
            .as_ref()
            .ok_or_else(|| LocalSendError::network("Target IP not provided"))?;
        let url = format!(
            "{}://{}:{}/api/localsend/v2/upload?sessionId={}&fileId={}&token={}",
            target.protocol, ip, target.port, session_id, file_id, token
        );

        // Stream the file instead of loading it all into memory
        let file = File::open(file_path).await?;
        let total_bytes = file.metadata().await?.len();
        let started = std::time::Instant::now();
        let progress = progress.map(std::sync::Arc::new);

        // Wrap the file stream so every chunk that goes out over the wire
        // also advances a running byte counter and reports it upstream.
        let throttle_started = tokio::time::Instant::now();
        let throttled_bytes = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
        let throttle_counter = throttled_bytes.clone();
        let rate_limit_bytes_per_second = rate_limit_bytes_per_second.filter(|rate| *rate > 0);
        let paced = ReaderStream::new(file).then(move |chunk| {
            let target_elapsed = chunk.as_ref().ok().and_then(|bytes| {
                let cumulative = throttle_counter
                    .fetch_add(bytes.len() as u64, std::sync::atomic::Ordering::Relaxed)
                    + bytes.len() as u64;
                rate_limit_bytes_per_second
                    .map(|rate| std::time::Duration::from_secs_f64(cumulative as f64 / rate as f64))
            });
            async move {
                if let Some(target_elapsed) = target_elapsed {
                    let delay = target_elapsed.saturating_sub(throttle_started.elapsed());
                    if !delay.is_zero() {
                        tokio::time::sleep(delay).await;
                    }
                }
                chunk
            }
        });

        let counter_progress = progress.clone();
        let mut sent: u64 = 0;
        let counted = paced.inspect(move |chunk| {
            if let (Ok(c), Some(cb)) = (chunk, counter_progress.as_ref()) {
                sent += c.len() as u64;
                cb(sent, total_bytes, started.elapsed().as_secs_f64());
            }
        });
        let body = Body::wrap_stream(counted);

        let response = self
            .client
            .post(&url)
            .header(reqwest::header::CONTENT_LENGTH, total_bytes)
            .body(body)
            .send()
            .await?;

        let status = response.status();
        match status {
            StatusCode::OK | StatusCode::NO_CONTENT => Ok(()),
            _ => Err(LocalSendError::http_failed(
                status.as_u16(),
                "File upload failed",
            )),
        }
    }

    pub async fn cancel(&self, target: &DeviceInfo, session_id: &SessionId) -> Result<()> {
        let ip = target
            .ip
            .as_ref()
            .ok_or_else(|| LocalSendError::network("Target IP not provided"))?;
        let url = format!(
            "{}://{}:{}/api/localsend/v2/cancel?sessionId={}",
            target.protocol, ip, target.port, session_id
        );
        let response = self.client.post(&url).send().await?;
        if response.status().is_success() {
            Ok(())
        } else {
            Err(LocalSendError::http_failed(
                response.status().as_u16(),
                "Cancel failed",
            ))
        }
    }
}

#[cfg(feature = "https")]
#[derive(Debug)]
struct FingerprintVerifier {
    expected_fingerprint: String,
    signature_verifier: Arc<dyn rustls::client::danger::ServerCertVerifier>,
}

#[cfg(feature = "https")]
impl FingerprintVerifier {
    fn new(expected_fingerprint: String) -> Result<Self> {
        let expected_fingerprint =
            crate::client::trust_policy::normalize_fingerprint(&expected_fingerprint)
                .ok_or_else(|| LocalSendError::network("Invalid LocalSend TLS fingerprint"))?;

        rustls::crypto::ring::default_provider()
            .install_default()
            .ok();
        let placeholder_certificate = crate::crypto::generate_tls_certificate()?;
        let mut roots = rustls::RootCertStore::empty();
        roots
            .add(rustls::pki_types::CertificateDer::from(
                placeholder_certificate.cert_der,
            ))
            .map_err(|error| {
                LocalSendError::network(format!("Invalid TLS verifier root: {error}"))
            })?;
        let signature_verifier = rustls::client::WebPkiServerVerifier::builder(Arc::new(roots))
            .build()
            .map_err(|error| {
                LocalSendError::network(format!("TLS verifier setup failed: {error}"))
            })?;

        Ok(Self {
            expected_fingerprint,
            signature_verifier,
        })
    }
}

#[cfg(feature = "https")]
impl rustls::client::danger::ServerCertVerifier for FingerprintVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &rustls::pki_types::CertificateDer<'_>,
        _intermediates: &[rustls::pki_types::CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> std::result::Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        let actual = crate::crypto::sha256_from_bytes(end_entity.as_ref());
        if crate::client::trust_policy::normalize_fingerprint(&actual)
            .is_some_and(|actual| actual == self.expected_fingerprint)
        {
            Ok(rustls::client::danger::ServerCertVerified::assertion())
        } else {
            Err(rustls::Error::General(
                "LocalSend TLS certificate fingerprint mismatch".into(),
            ))
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &rustls::pki_types::CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> std::result::Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        self.signature_verifier
            .verify_tls12_signature(message, cert, dss)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &rustls::pki_types::CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> std::result::Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        self.signature_verifier
            .verify_tls13_signature(message, cert, dss)
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        self.signature_verifier.supported_verify_schemes()
    }
}

#[cfg(test)]
mod tests {
    use super::{LocalSendClient, prepare_upload_url};
    use crate::client::TlsTrustPolicy;
    use crate::protocol::{DeviceInfo, Protocol};

    #[cfg(feature = "https")]
    #[test]
    fn with_trust_policy_keeps_strict_policy_insecure_flag() {
        let device = DeviceInfo::new("alias".to_string(), 53317, Protocol::Https);
        let policy = TlsTrustPolicy::new(vec!["a".repeat(64)]);

        let client = LocalSendClient::with_trust_policy(device, policy.clone()).unwrap();

        assert!(!policy.allows_insecure());
        assert!(!policy.allows(""));
        // Client must construct without panicking and remain usable for the device payload.
        assert_eq!(client.device.alias, "alias");
    }

    #[test]
    fn prepare_upload_url_round_trips_reserved_and_unicode_pin_characters() {
        let mut target = DeviceInfo::new("receiver".to_string(), 53317, Protocol::Http);
        target.ip = Some("192.0.2.2".to_string());

        for pin in ["with space", "a+b", "a&b", "a#b", "a%b", "contraseña"] {
            let url =
                prepare_upload_url(&target, target.ip.as_deref().unwrap(), Some(pin)).unwrap();
            let received = url
                .query_pairs()
                .find_map(|(key, value)| (key == "pin").then(|| value.into_owned()));
            assert_eq!(received.as_deref(), Some(pin));
        }
    }

    #[cfg(not(feature = "https"))]
    #[test]
    fn pinned_policy_requires_the_https_feature() {
        let device = DeviceInfo::new("alias".to_string(), 53317, Protocol::Https);
        let policy = TlsTrustPolicy::new(vec!["a".repeat(64)]);

        assert!(matches!(
            LocalSendClient::with_trust_policy(device, policy),
            Err(error) if error.to_string().contains("https feature")
        ));
    }
}
