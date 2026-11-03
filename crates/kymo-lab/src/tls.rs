//! The lab host's certificate. Made on first start as a self-signed one, and the platform pins
//! its fingerprint, so no certificate authority and no domain name are needed. A certificate
//! and key supplied by the operator are used as they are.

use std::path::Path;
use std::sync::Arc;

use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use tokio_rustls::TlsAcceptor;

use crate::client::fingerprint_of;

pub struct Identity {
    pub cert_pem: String,
    pub key_pem: String,
    /// SHA-256 of the certificate, `AB:CD:...`. What the platform pins.
    pub fingerprint: String,
}

fn der_of(cert_pem: &str) -> Result<CertificateDer<'static>, String> {
    CertificateDer::from_pem_slice(cert_pem.as_bytes()).map_err(|e| format!("certificate: {e}"))
}

impl Identity {
    pub fn from_pem(cert_pem: String, key_pem: String) -> Result<Self, String> {
        let fingerprint = fingerprint_of(der_of(&cert_pem)?.as_ref());
        PrivateKeyDer::from_pem_slice(key_pem.as_bytes())
            .map_err(|e| format!("private key: {e}"))?;
        Ok(Self {
            cert_pem,
            key_pem,
            fingerprint,
        })
    }
}

/// Reads `cert.pem` and `key.pem` from `dir`, or makes a self-signed pair valid for `names`
/// (host names and addresses the platform will connect to) and keeps it there.
pub fn load_or_create(dir: &Path, names: &[String]) -> Result<Identity, String> {
    let cert_path = dir.join("cert.pem");
    let key_path = dir.join("key.pem");
    if cert_path.exists() && key_path.exists() {
        let cert = std::fs::read_to_string(&cert_path)
            .map_err(|e| format!("{}: {e}", cert_path.display()))?;
        let key = std::fs::read_to_string(&key_path)
            .map_err(|e| format!("{}: {e}", key_path.display()))?;
        return Identity::from_pem(cert, key);
    }
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut names: Vec<String> = names.to_vec();
    for default in ["localhost", "127.0.0.1"] {
        if !names.iter().any(|n| n == default) {
            names.push(default.to_owned());
        }
    }
    let mut params = rcgen::CertificateParams::new(names).map_err(|e| e.to_string())?;
    let mut dn = rcgen::DistinguishedName::new();
    dn.push(rcgen::DnType::CommonName, "kymo-lab");
    params.distinguished_name = dn;
    // A pinned certificate is trusted by fingerprint, so a long life costs nothing.
    params.not_before = rcgen::date_time_ymd(2024, 1, 1);
    params.not_after = rcgen::date_time_ymd(2064, 1, 1);
    let key =
        rcgen::KeyPair::generate_for(&rcgen::PKCS_ECDSA_P256_SHA256).map_err(|e| e.to_string())?;
    let cert = params.self_signed(&key).map_err(|e| e.to_string())?;
    let (cert_pem, key_pem) = (cert.pem(), key.serialize_pem());
    std::fs::write(&cert_path, &cert_pem).map_err(|e| e.to_string())?;
    write_private(&key_path, &key_pem)?;
    Identity::from_pem(cert_pem, key_pem)
}

#[cfg(unix)]
fn write_private(path: &Path, text: &str) -> Result<(), String> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    file.write_all(text.as_bytes()).map_err(|e| e.to_string())
}

#[cfg(not(unix))]
fn write_private(path: &Path, text: &str) -> Result<(), String> {
    std::fs::write(path, text).map_err(|e| e.to_string())
}

pub fn acceptor(identity: &Identity) -> Result<TlsAcceptor, String> {
    let cert = der_of(&identity.cert_pem)?;
    let key = PrivateKeyDer::from_pem_slice(identity.key_pem.as_bytes())
        .map_err(|e| format!("private key: {e}"))?;
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let config = rustls::ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|e| e.to_string())?
        .with_no_client_auth()
        .with_single_cert(vec![cert], key)
        .map_err(|e| e.to_string())?;
    Ok(TlsAcceptor::from(Arc::new(config)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_certificate_is_made_once_and_reloaded_unchanged() {
        let dir = std::env::temp_dir().join(format!("kymo-lab-tls-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let first = load_or_create(&dir, &["lab.example.org".into()]).unwrap();
        let again = load_or_create(&dir, &[]).unwrap();
        assert_eq!(
            first.fingerprint, again.fingerprint,
            "the second start keeps the first certificate"
        );
        assert_eq!(first.fingerprint.split(':').count(), 32);
        assert!(acceptor(&first).is_ok());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(dir.join("key.pem"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(mode, 0o600, "the private key is for the owner only");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
