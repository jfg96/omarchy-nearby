use anyhow::{Context, Result, anyhow};
use localsend_rs::crypto::{TlsCertificate, generate_tls_certificate, sha256_from_bytes};
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

const MAX_IDENTITY_BYTES: usize = 128 * 1024;
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Serialize, Deserialize)]
struct StoredIdentity {
    cert_pem: String,
    key_pem: String,
    fingerprint: String,
}

fn ensure_private_directory(path: &Path) -> Result<()> {
    fs::create_dir_all(path).context("could not create Nearby state directory")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
            .context("could not protect Nearby state directory")?;
    }
    Ok(())
}

fn read_identity(path: &Path) -> Result<Option<Vec<u8>>> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error).context("could not inspect TLS identity"),
    };
    if metadata.file_type().is_symlink() {
        return Err(anyhow!("TLS identity must not be a symbolic link"));
    }
    if !metadata.is_file() {
        return Err(anyhow!("TLS identity must be a regular file"));
    }
    if metadata.len() > MAX_IDENTITY_BYTES as u64 {
        return Err(anyhow!("TLS identity exceeds 128 KiB"));
    }

    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW);
    }
    let file = options.open(path).context("could not open TLS identity")?;
    if !file
        .metadata()
        .context("could not inspect opened TLS identity")?
        .is_file()
    {
        return Err(anyhow!("TLS identity must be a regular file"));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(fs::Permissions::from_mode(0o600))
            .context("could not protect TLS identity")?;
    }

    let mut data = Vec::with_capacity(metadata.len() as usize);
    file.take((MAX_IDENTITY_BYTES + 1) as u64)
        .read_to_end(&mut data)
        .context("could not read TLS identity")?;
    if data.len() > MAX_IDENTITY_BYTES {
        return Err(anyhow!("TLS identity exceeds 128 KiB"));
    }
    Ok(Some(data))
}

fn validate(stored: &StoredIdentity) -> Result<TlsCertificate> {
    let mut cert_pem = stored.cert_pem.as_bytes();
    let certificates = rustls_pemfile::certs(&mut cert_pem)
        .collect::<std::result::Result<Vec<_>, _>>()
        .context("could not parse TLS certificate")?;
    let cert_der = certificates
        .first()
        .ok_or_else(|| anyhow!("TLS identity contains no certificate"))?
        .as_ref()
        .to_vec();

    let mut key_pem = stored.key_pem.as_bytes();
    let private_key = rustls_pemfile::private_key(&mut key_pem)
        .context("could not parse TLS private key")?
        .ok_or_else(|| anyhow!("TLS identity contains no private key"))?;

    rustls::crypto::ring::default_provider()
        .install_default()
        .ok();
    rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certificates, private_key)
        .context("TLS certificate and private key do not match")?;

    let fingerprint = sha256_from_bytes(&cert_der);
    Ok(TlsCertificate {
        cert_pem: stored.cert_pem.clone(),
        key_pem: stored.key_pem.clone(),
        cert_der,
        fingerprint,
    })
}

fn save(path: &Path, certificate: &TlsCertificate) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow!("TLS identity path has no parent"))?;
    let stored = StoredIdentity {
        cert_pem: certificate.cert_pem.clone(),
        key_pem: certificate.key_pem.clone(),
        fingerprint: certificate.fingerprint.clone(),
    };
    let mut data =
        serde_json::to_vec_pretty(&stored).context("could not serialize TLS identity")?;
    data.push(b'\n');
    if data.len() > MAX_IDENTITY_BYTES {
        return Err(anyhow!("TLS identity exceeds 128 KiB"));
    }

    let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let temporary = parent.join(format!(
        ".identity.json.tmp-{}-{sequence}",
        std::process::id()
    ));
    let result = (|| -> Result<()> {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options
            .open(&temporary)
            .context("could not create temporary TLS identity")?;
        file.write_all(&data)
            .context("could not write TLS identity")?;
        file.sync_all().context("could not flush TLS identity")?;
        drop(file);
        fs::rename(&temporary, path).context("could not replace TLS identity")?;
        fs::File::open(parent)
            .and_then(|directory| directory.sync_all())
            .context("could not flush Nearby state directory")?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

pub fn load_or_create(state_dir: &Path) -> Result<TlsCertificate> {
    ensure_private_directory(state_dir)?;
    let path = state_dir.join("identity.json");
    let Some(data) = read_identity(&path)? else {
        let certificate = generate_tls_certificate().context("could not generate TLS identity")?;
        save(&path, &certificate)?;
        return Ok(certificate);
    };

    let stored: StoredIdentity =
        serde_json::from_slice(&data).context("could not parse TLS identity")?;
    let certificate = validate(&stored)?;
    if stored.fingerprint != certificate.fingerprint {
        save(&path, &certificate).context("could not repair TLS identity fingerprint")?;
    }
    Ok(certificate)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_directory(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "omarchy-nearby-identity-{name}-{}-{}",
            std::process::id(),
            TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ))
    }

    fn remove_test_directory(path: &Path) {
        let _ = fs::remove_dir_all(path);
    }

    #[test]
    fn identity_is_private_stable_and_has_derived_fingerprint() {
        let directory = test_directory("stable");
        let first = load_or_create(&directory).unwrap();
        let second = load_or_create(&directory).unwrap();
        assert_eq!(first.cert_pem, second.cert_pem);
        assert_eq!(first.key_pem, second.key_pem);
        assert_eq!(first.fingerprint, sha256_from_bytes(&first.cert_der));

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&directory).unwrap().permissions().mode() & 0o777,
                0o700
            );
            assert_eq!(
                fs::metadata(directory.join("identity.json"))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
        remove_test_directory(&directory);
    }

    #[test]
    fn wrong_fingerprint_is_repaired_without_rotating_key_material() {
        let directory = test_directory("fingerprint");
        let original = load_or_create(&directory).unwrap();
        let path = directory.join("identity.json");
        let mut stored: StoredIdentity = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        stored.fingerprint = "wrong".into();
        fs::write(&path, serde_json::to_vec_pretty(&stored).unwrap()).unwrap();

        let repaired = load_or_create(&directory).unwrap();
        assert_eq!(repaired.cert_pem, original.cert_pem);
        assert_eq!(repaired.key_pem, original.key_pem);
        assert_eq!(repaired.fingerprint, original.fingerprint);
        let persisted: StoredIdentity = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(persisted.fingerprint, original.fingerprint);
        remove_test_directory(&directory);
    }

    #[test]
    fn corrupt_oversized_and_non_regular_identities_fail_closed() {
        for (name, data) in [
            ("corrupt", b"not json".to_vec()),
            ("oversized", vec![b'x'; MAX_IDENTITY_BYTES + 1]),
        ] {
            let directory = test_directory(name);
            fs::create_dir_all(&directory).unwrap();
            fs::write(directory.join("identity.json"), data).unwrap();
            assert!(load_or_create(&directory).is_err());
            remove_test_directory(&directory);
        }

        let directory = test_directory("directory");
        fs::create_dir_all(directory.join("identity.json")).unwrap();
        assert!(load_or_create(&directory).is_err());
        remove_test_directory(&directory);
    }

    #[cfg(unix)]
    #[test]
    fn symbolic_link_identity_is_rejected_without_touching_its_target() {
        use std::os::unix::fs::symlink;

        let directory = test_directory("symlink");
        fs::create_dir_all(&directory).unwrap();
        let target = directory.join("target");
        fs::write(&target, b"keep me").unwrap();
        symlink(&target, directory.join("identity.json")).unwrap();
        assert!(load_or_create(&directory).is_err());
        assert_eq!(fs::read(&target).unwrap(), b"keep me");
        remove_test_directory(&directory);
    }

    #[test]
    fn mismatched_certificate_and_key_fail_without_rotation() {
        let directory = test_directory("mismatch");
        fs::create_dir_all(&directory).unwrap();
        let certificate = generate_tls_certificate().unwrap();
        let other = generate_tls_certificate().unwrap();
        let stored = StoredIdentity {
            cert_pem: certificate.cert_pem,
            key_pem: other.key_pem,
            fingerprint: certificate.fingerprint,
        };
        let path = directory.join("identity.json");
        let bytes = serde_json::to_vec_pretty(&stored).unwrap();
        fs::write(&path, &bytes).unwrap();
        assert!(load_or_create(&directory).is_err());
        assert_eq!(fs::read(&path).unwrap(), bytes);
        remove_test_directory(&directory);
    }
}
