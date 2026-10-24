//! TLS 1.3 uniquement (rustls + ring). Au premier démarrage, un certificat
//! auto-signé est généré ; son empreinte SHA-256 est affichée pour que chaque
//! appareil puisse l'épingler lors de l'appairage.

use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use sha2::{Digest, Sha256};
use std::path::Path;
use std::sync::Arc;

pub type BoxError = Box<dyn std::error::Error + Send + Sync>;

#[derive(Debug)]
pub struct TlsMaterial {
    pub config: Arc<rustls::ServerConfig>,
    /// Empreinte SHA-256 du certificat, au format `AA:BB:…`.
    pub fingerprint: String,
}

fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    std::io::Write::write_all(&mut options.open(path)?, bytes)
}

/// Charge le certificat du serveur, ou le crée. `extra_names` : noms d'hôte et IP
/// supplémentaires (par exemple `phonalys.lan`, `192.168.1.20`).
pub fn load_or_create(dir: &Path, extra_names: &[String]) -> Result<TlsMaterial, BoxError> {
    let cert_path = dir.join("tls-cert.der");
    let key_path = dir.join("tls-key.der");
    if !cert_path.exists() || !key_path.exists() {
        let mut names = vec![
            "localhost".to_owned(),
            "127.0.0.1".to_owned(),
            "::1".to_owned(),
        ];
        names.extend(extra_names.iter().cloned());
        let generated = rcgen::generate_simple_self_signed(names)?;
        let _ = std::fs::remove_file(&cert_path);
        let _ = std::fs::remove_file(&key_path);
        write_private(&key_path, &generated.key_pair.serialize_der())?;
        std::fs::write(&cert_path, generated.cert.der())?;
    }
    let cert = std::fs::read(&cert_path)?;
    let key = std::fs::read(&key_path)?;
    let fingerprint = Sha256::digest(&cert)
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<_>>()
        .join(":");

    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let mut config = rustls::ServerConfig::builder_with_provider(provider)
        .with_protocol_versions(&[&rustls::version::TLS13])?
        .with_no_client_auth()
        .with_single_cert(
            vec![CertificateDer::from(cert)],
            PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key)),
        )?;
    config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
    Ok(TlsMaterial {
        config: Arc::new(config),
        fingerprint,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn certificate_is_created_once_and_reused() {
        let dir = tempfile::tempdir().unwrap();
        let first = load_or_create(dir.path(), &["phonalys.lan".into()]).unwrap();
        let second = load_or_create(dir.path(), &[]).unwrap();
        assert_eq!(first.fingerprint, second.fingerprint);
        assert_eq!(first.fingerprint.len(), 32 * 3 - 1);
    }
}
