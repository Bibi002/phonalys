//! TLS 1.3 uniquement (rustls + ring), avec une **autorité de certification locale**
//! propre au cabinet.
//!
//! Les téléphones et tablettes installent une seule fois le certificat de cette
//! autorité (profil iOS, certificat Android) : la connexion est alors reconnue comme
//! sûre, ce qu'exigent WebAuthn (biométrie) et l'installation de l'application web.
//!
//! Une autorité installée sur un téléphone pourrait, en principe, garantir n'importe
//! quel site. Celle de Phonalys porte une **contrainte de noms** (RFC 5280, critique) :
//! elle ne vaut que pour les noms du réseau local (`.lan`, `.local`, `.home.arpa`,
//! `.internal`, `localhost`, les noms configurés) et les plages d'adresses privées.
//! Même volée, sa clé ne permettrait pas d'usurper une banque ou une messagerie.

use rcgen::{
    BasicConstraints, CertificateParams, CidrSubnet, DistinguishedName, DnType,
    ExtendedKeyUsagePurpose, GeneralSubtree, IsCa, KeyPair, KeyUsagePurpose, NameConstraints,
};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::net::IpAddr;
use std::path::Path;
use std::sync::Arc;
use time::{Duration, OffsetDateTime};

pub type BoxError = Box<dyn std::error::Error + Send + Sync>;

/// Domaines du réseau local couverts d'office par l'autorité.
pub const LOCAL_DOMAINS: &[&str] = &["localhost", "lan", "local", "home.arpa", "internal"];
/// Durée du certificat du serveur : sous la limite d'Apple pour les autorités ajoutées
/// par l'utilisateur. Il est renouvelé automatiquement 30 jours avant l'échéance.
const LEAF_DAYS: i64 = 397;
const RENEW_BEFORE_DAYS: i64 = 30;
const CA_YEARS: i64 = 10;

#[derive(Debug, Clone)]
pub struct CaInfo {
    /// Certificat de l'autorité (DER), à installer sur les appareils.
    pub der: Vec<u8>,
    /// Empreinte SHA-256 de l'autorité, au format `AA:BB:…`.
    pub fingerprint: String,
}

#[derive(Debug)]
pub struct TlsMaterial {
    pub config: Arc<rustls::ServerConfig>,
    /// Empreinte SHA-256 du certificat du serveur, au format `AA:BB:…`.
    pub fingerprint: String,
    pub ca: CaInfo,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Meta {
    /// Noms couverts par la contrainte de l'autorité.
    ca_domains: Vec<String>,
    /// Noms du certificat du serveur.
    leaf_names: Vec<String>,
    /// Échéance du certificat du serveur (secondes Unix).
    leaf_not_after: i64,
}

pub fn fingerprint(der: &[u8]) -> String {
    Sha256::digest(der)
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<_>>()
        .join(":")
}

fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let _ = std::fs::remove_file(path);
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    std::io::Write::write_all(&mut options.open(path)?, bytes)
}

fn covered(name: &str, domains: &[String]) -> bool {
    domains
        .iter()
        .any(|d| name == d || name.ends_with(&format!(".{d}")))
}

fn is_dns(name: &str) -> bool {
    name.parse::<IpAddr>().is_err()
}

/// Plages d'adresses privées autorisées par la contrainte de l'autorité.
fn private_ranges() -> Vec<GeneralSubtree> {
    let v4 = |a: [u8; 4], m: [u8; 4]| GeneralSubtree::IpAddress(CidrSubnet::V4(a, m));
    let mut ula = [0u8; 16];
    ula[0] = 0xfc;
    let mut ula_mask = [0u8; 16];
    ula_mask[0] = 0xfe;
    let mut loopback = [0u8; 16];
    loopback[15] = 1;
    vec![
        v4([127, 0, 0, 0], [255, 0, 0, 0]),
        v4([10, 0, 0, 0], [255, 0, 0, 0]),
        v4([172, 16, 0, 0], [255, 240, 0, 0]),
        v4([192, 168, 0, 0], [255, 255, 0, 0]),
        // VPN maillés (Tailscale, Headscale).
        v4([100, 64, 0, 0], [255, 192, 0, 0]),
        GeneralSubtree::IpAddress(CidrSubnet::V6(loopback, [0xff; 16])),
        GeneralSubtree::IpAddress(CidrSubnet::V6(ula, ula_mask)),
    ]
}

/// Paramètres de l'autorité : identiques à chaque démarrage (même nom, même clé), ce
/// qui permet de signer de nouveaux certificats sans relire l'ancien.
fn ca_params(domains: &[String]) -> Result<CertificateParams, BoxError> {
    let mut params = CertificateParams::default();
    let mut dn = DistinguishedName::new();
    dn.push(DnType::CommonName, "Phonalys - autorité locale du cabinet");
    dn.push(DnType::OrganizationName, "Phonalys");
    params.distinguished_name = dn;
    params.is_ca = IsCa::Ca(BasicConstraints::Constrained(0));
    params.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign];
    let mut permitted: Vec<GeneralSubtree> = domains
        .iter()
        .map(|d| GeneralSubtree::DnsName(d.clone()))
        .collect();
    permitted.extend(private_ranges());
    params.name_constraints = Some(NameConstraints {
        permitted_subtrees: permitted,
        excluded_subtrees: vec![],
    });
    let now = OffsetDateTime::now_utc();
    params.not_before = now - Duration::days(1);
    params.not_after = now + Duration::days(365 * CA_YEARS);
    Ok(params)
}

/// Charge l'autorité et le certificat du serveur, ou les crée. `extra_names` : noms
/// d'hôte et adresses supplémentaires (par exemple `phonalys.lan`, `192.168.1.20`).
pub fn load_or_create(dir: &Path, extra_names: &[String]) -> Result<TlsMaterial, BoxError> {
    load_or_create_at(dir, extra_names, OffsetDateTime::now_utc())
}

fn load_or_create_at(
    dir: &Path,
    extra_names: &[String],
    now: OffsetDateTime,
) -> Result<TlsMaterial, BoxError> {
    let (ca_key_path, ca_cert_path) = (dir.join("ca-key.der"), dir.join("ca-cert.der"));
    let (key_path, cert_path) = (dir.join("tls-key.der"), dir.join("tls-cert.der"));
    let meta_path = dir.join("tls.json");

    let mut names = vec![
        "localhost".to_owned(),
        "127.0.0.1".to_owned(),
        "::1".to_owned(),
    ];
    for n in extra_names {
        let n = n.trim().trim_end_matches('.').to_ascii_lowercase();
        if !n.is_empty() && !names.contains(&n) {
            names.push(n);
        }
    }
    let mut meta: Meta = std::fs::read(&meta_path)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default();

    // Autorité : créée au premier démarrage, ou recréée si un nom configuré sort de sa
    // contrainte (les appareils devront alors réinstaller le profil).
    let mut ca_new = false;
    let wanted: Vec<String> = names
        .iter()
        .filter(|n| {
            is_dns(n)
                && !covered(
                    n,
                    &LOCAL_DOMAINS
                        .iter()
                        .map(|d| (*d).to_owned())
                        .collect::<Vec<_>>(),
                )
        })
        .cloned()
        .collect();
    let uncovered = names
        .iter()
        .any(|n| is_dns(n) && !covered(n, &meta.ca_domains));
    if !ca_key_path.exists() || !ca_cert_path.exists() || uncovered {
        if ca_key_path.exists() {
            tracing::warn!("nouveau nom hors de l'autorité locale : autorité recréée, réinstallez le profil sur les appareils");
        }
        let mut domains: Vec<String> = LOCAL_DOMAINS.iter().map(|d| (*d).to_owned()).collect();
        domains.extend(wanted);
        let key = KeyPair::generate()?;
        let cert = ca_params(&domains)?.self_signed(&key)?;
        write_private(&ca_key_path, &key.serialize_der())?;
        std::fs::write(&ca_cert_path, cert.der())?;
        meta.ca_domains = domains;
        ca_new = true;
    }
    let ca_key = KeyPair::try_from(std::fs::read(&ca_key_path)?.as_slice())?;
    let ca_der = std::fs::read(&ca_cert_path)?;
    let issuer = ca_params(&meta.ca_domains)?.self_signed(&ca_key)?;

    // Certificat du serveur : renouvelé à l'approche de l'échéance ou si les noms changent.
    let renew = ca_new
        || !key_path.exists()
        || !cert_path.exists()
        || meta.leaf_names != names
        || meta.leaf_not_after - now.unix_timestamp() < RENEW_BEFORE_DAYS * 86_400;
    if renew {
        let mut params = CertificateParams::new(names.clone())?;
        let mut dn = DistinguishedName::new();
        dn.push(DnType::CommonName, "Phonalys");
        params.distinguished_name = dn;
        params.key_usages = vec![KeyUsagePurpose::DigitalSignature];
        params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
        params.not_before = now - Duration::days(1);
        params.not_after = now + Duration::days(LEAF_DAYS);
        let key = KeyPair::generate()?;
        let cert = params.signed_by(&key, &issuer, &ca_key)?;
        write_private(&key_path, &key.serialize_der())?;
        std::fs::write(&cert_path, cert.der())?;
        meta.leaf_names = names;
        meta.leaf_not_after = (now + Duration::days(LEAF_DAYS)).unix_timestamp();
        tracing::info!("certificat du serveur émis par l'autorité locale");
    }
    std::fs::write(&meta_path, serde_json::to_vec_pretty(&meta)?)?;

    let cert = std::fs::read(&cert_path)?;
    let key = std::fs::read(&key_path)?;
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let mut config = rustls::ServerConfig::builder_with_provider(provider)
        .with_protocol_versions(&[&rustls::version::TLS13])?
        .with_no_client_auth()
        .with_single_cert(
            vec![
                CertificateDer::from(cert.clone()),
                CertificateDer::from(ca_der.clone()),
            ],
            PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key)),
        )?;
    config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
    Ok(TlsMaterial {
        config: Arc::new(config),
        fingerprint: fingerprint(&cert),
        ca: CaInfo {
            fingerprint: fingerprint(&ca_der),
            der: ca_der,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustls::client::WebPkiServerVerifier;
    use rustls::pki_types::{ServerName, UnixTime};
    use rustls::RootCertStore;

    /// Vérifie le certificat du serveur comme le ferait un téléphone ayant installé
    /// l'autorité.
    fn verify(dir: &Path, name: &str) -> Result<(), rustls::Error> {
        let mut roots = RootCertStore::empty();
        roots
            .add(CertificateDer::from(
                std::fs::read(dir.join("ca-cert.der")).unwrap(),
            ))
            .unwrap();
        let verifier = WebPkiServerVerifier::builder_with_provider(
            Arc::new(roots),
            Arc::new(rustls::crypto::ring::default_provider()),
        )
        .build()
        .unwrap();
        let leaf = CertificateDer::from(std::fs::read(dir.join("tls-cert.der")).unwrap());
        verifier
            .verify_server_cert(
                &leaf,
                &[],
                &ServerName::try_from(name.to_owned()).unwrap(),
                &[],
                UnixTime::now(),
            )
            .map(|_| ())
    }

    use rustls::client::danger::ServerCertVerifier;

    #[test]
    fn server_certificate_chains_to_the_local_authority() {
        let dir = tempfile::tempdir().unwrap();
        let m =
            load_or_create(dir.path(), &["phonalys.lan".into(), "192.168.1.20".into()]).unwrap();
        assert_eq!(m.fingerprint.len(), 32 * 3 - 1);
        assert_ne!(m.fingerprint, m.ca.fingerprint);
        verify(dir.path(), "phonalys.lan").unwrap();
        verify(dir.path(), "localhost").unwrap();
        verify(dir.path(), "192.168.1.20").unwrap();
        // Un autre nom n'est pas couvert par le certificat.
        assert!(verify(dir.path(), "autre.lan").is_err());
    }

    #[test]
    fn material_is_reused_then_renewed_before_expiry() {
        let dir = tempfile::tempdir().unwrap();
        let names = ["cabinet.lan".to_owned()];
        let now = OffsetDateTime::now_utc();
        let first = load_or_create_at(dir.path(), &names, now).unwrap();
        let again = load_or_create_at(dir.path(), &names, now).unwrap();
        assert_eq!(first.fingerprint, again.fingerprint);
        // Près de l'échéance : nouveau certificat, même autorité (rien à réinstaller).
        let later =
            load_or_create_at(dir.path(), &names, now + Duration::days(LEAF_DAYS - 10)).unwrap();
        assert_ne!(first.fingerprint, later.fingerprint);
        assert_eq!(first.ca.fingerprint, later.ca.fingerprint);
        // Nouveau nom local : nouveau certificat, même autorité.
        let renamed = load_or_create_at(dir.path(), &["accueil.lan".into()], now).unwrap();
        assert_eq!(first.ca.fingerprint, renamed.ca.fingerprint);
        verify(dir.path(), "accueil.lan").unwrap();
    }

    #[test]
    fn authority_is_constrained_to_local_names() {
        let dir = tempfile::tempdir().unwrap();
        // Un nom public explicitement configuré est ajouté à la contrainte…
        let m = load_or_create(dir.path(), &["cabinet.example.be".into()]).unwrap();
        verify(dir.path(), "cabinet.example.be").unwrap();
        // … ce qui recrée l'autorité si elle existait sans lui.
        let other = load_or_create(dir.path(), &["autre.example.org".into()]).unwrap();
        assert_ne!(m.ca.fingerprint, other.ca.fingerprint);

        // Même avec la clé de l'autorité, un certificat pour un site public est refusé.
        let ca_key = KeyPair::try_from(
            std::fs::read(dir.path().join("ca-key.der"))
                .unwrap()
                .as_slice(),
        )
        .unwrap();
        let meta: Meta =
            serde_json::from_slice(&std::fs::read(dir.path().join("tls.json")).unwrap()).unwrap();
        let issuer = ca_params(&meta.ca_domains)
            .unwrap()
            .self_signed(&ca_key)
            .unwrap();
        let mut forged = CertificateParams::new(vec!["banque.example.com".to_owned()]).unwrap();
        forged.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
        let key = KeyPair::generate().unwrap();
        let forged = forged.signed_by(&key, &issuer, &ca_key).unwrap();
        std::fs::write(dir.path().join("tls-cert.der"), forged.der()).unwrap();
        assert!(verify(dir.path(), "banque.example.com").is_err());
    }
}
