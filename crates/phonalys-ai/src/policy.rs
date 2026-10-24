//! Politique réseau des points d'accès IA.
//!
//! La vérification porte sur les adresses **résolues**, et l'appelant doit se connecter
//! à ces adresses figées (voir [`ValidatedEndpoint::addrs`]) : un DNS piégé qui
//! répondrait autrement au moment de la connexion (« DNS rebinding ») est sans effet.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, ToSocketAddrs};
use url::{Host, Url};

/// Hôtes de fournisseurs d'IA en ligne, refusés même via un proxy ou un DNS local.
const BLOCKED_HOST_SUFFIXES: &[&str] = &[
    "openai.com",
    "chatgpt.com",
    "anthropic.com",
    "claude.ai",
    "googleapis.com",
    "gemini.google.com",
    "aistudio.google.com",
    "mistral.ai",
    "cohere.ai",
    "cohere.com",
    "groq.com",
    "together.xyz",
    "together.ai",
    "perplexity.ai",
    "deepseek.com",
    "x.ai",
    "openrouter.ai",
    "huggingface.co",
    "replicate.com",
    "fireworks.ai",
    "azure.com",
    "openai.azure.com",
    "amazonaws.com",
    "cloudflare.com",
];

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum PolicyError {
    #[error("URL invalide")]
    InvalidUrl,
    #[error("schéma non autorisé : seuls http (boucle locale) et https sont acceptés")]
    Scheme,
    #[error("identifiants dans l'URL interdits")]
    CredentialsInUrl,
    #[error("hôte de fournisseur d'IA en ligne bloqué : {0}")]
    BlockedProvider(String),
    #[error("l'adresse {0} n'est pas privée : les IA hors de votre réseau sont interdites")]
    PublicAddress(IpAddr),
    #[error(
        "http en clair n'est autorisé que sur cette machine ; utilisez https sur le réseau local"
    )]
    PlaintextOnNetwork,
    #[error("résolution DNS impossible")]
    Resolution,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EndpointPolicy {
    /// Autorise http en clair sur le réseau privé (déconseillé ; désactivé par défaut).
    pub allow_plaintext_lan: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedEndpoint {
    pub url: Url,
    /// Adresses auxquelles se connecter, et uniquement celles-ci.
    pub addrs: Vec<SocketAddr>,
}

impl EndpointPolicy {
    /// Valide une URL en résolvant son nom d'hôte via le système.
    pub fn validate(&self, raw: &str) -> Result<ValidatedEndpoint, PolicyError> {
        let url = self.check_url(raw)?;
        let host = url.host_str().ok_or(PolicyError::InvalidUrl)?;
        let port = url.port_or_known_default().ok_or(PolicyError::InvalidUrl)?;
        let addrs: Vec<SocketAddr> = (host.trim_start_matches('[').trim_end_matches(']'), port)
            .to_socket_addrs()
            .map_err(|_| PolicyError::Resolution)?
            .collect();
        self.check_resolved(url, addrs)
    }

    /// Contrôles statiques sur l'URL, avant toute résolution.
    pub fn check_url(&self, raw: &str) -> Result<Url, PolicyError> {
        let url = Url::parse(raw).map_err(|_| PolicyError::InvalidUrl)?;
        if !matches!(url.scheme(), "http" | "https") {
            return Err(PolicyError::Scheme);
        }
        if !url.username().is_empty() || url.password().is_some() {
            return Err(PolicyError::CredentialsInUrl);
        }
        if let Some(Host::Domain(domain)) = url.host() {
            let domain = domain.trim_end_matches('.').to_ascii_lowercase();
            if BLOCKED_HOST_SUFFIXES
                .iter()
                .any(|s| domain == *s || domain.ends_with(&format!(".{s}")))
            {
                return Err(PolicyError::BlockedProvider(domain));
            }
        }
        if url.host().is_none() {
            return Err(PolicyError::InvalidUrl);
        }
        Ok(url)
    }

    /// Contrôles sur les adresses résolues. Toutes doivent être privées.
    pub fn check_resolved(
        &self,
        url: Url,
        addrs: Vec<SocketAddr>,
    ) -> Result<ValidatedEndpoint, PolicyError> {
        if addrs.is_empty() {
            return Err(PolicyError::Resolution);
        }
        for addr in &addrs {
            let ip = canonical(addr.ip());
            if !is_private(ip) {
                return Err(PolicyError::PublicAddress(ip));
            }
            if url.scheme() == "http" && !ip.is_loopback() && !self.allow_plaintext_lan {
                return Err(PolicyError::PlaintextOnNetwork);
            }
        }
        Ok(ValidatedEndpoint { url, addrs })
    }
}

/// Convertit une IPv6 de type `::ffff:a.b.c.d` en IPv4 pour éviter les contournements.
fn canonical(ip: IpAddr) -> IpAddr {
    match ip {
        IpAddr::V6(v6) => v6.to_ipv4_mapped().map_or(ip, IpAddr::V4),
        v4 => v4,
    }
}

/// Adresses considérées comme appartenant au praticien.
fn is_private(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_private_v4(v4),
        IpAddr::V6(v6) => is_private_v6(v6),
    }
}

fn is_private_v4(ip: Ipv4Addr) -> bool {
    let [a, b, ..] = ip.octets();
    ip.is_loopback()            // 127.0.0.0/8
        || ip.is_private()      // 10/8, 172.16/12, 192.168/16
        || (a == 100 && (64..=127).contains(&b)) // 100.64/10 : VPN maillés (Tailscale, Headscale)
}

fn is_private_v6(ip: Ipv6Addr) -> bool {
    ip.is_loopback() || (ip.segments()[0] & 0xfe00) == 0xfc00 // fc00::/7, adresses locales uniques
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resolved(url: &str, ips: &[&str]) -> Result<ValidatedEndpoint, PolicyError> {
        let p = EndpointPolicy::default();
        let url = p.check_url(url)?;
        let addrs = ips
            .iter()
            .map(|ip| SocketAddr::new(ip.parse().unwrap(), 443))
            .collect();
        p.check_resolved(url, addrs)
    }

    #[test]
    fn local_models_are_allowed() {
        assert!(resolved("http://127.0.0.1:11434", &["127.0.0.1"]).is_ok());
        assert!(resolved("http://localhost:1234/v1", &["::1"]).is_ok());
        assert!(resolved("https://ia.maison.lan", &["192.168.1.20"]).is_ok());
        assert!(resolved("https://serveur.tailnet", &["100.101.102.103"]).is_ok());
        assert!(resolved("https://ia.lan", &["fd12:3456::1"]).is_ok());
    }

    #[test]
    fn cloud_providers_are_blocked_by_name() {
        for url in [
            "https://api.openai.com/v1",
            "https://api.anthropic.com",
            "https://generativelanguage.googleapis.com",
            "https://API.MISTRAL.AI.",
            "https://monprojet.openai.azure.com",
        ] {
            assert!(
                matches!(
                    resolved(url, &["10.0.0.1"]),
                    Err(PolicyError::BlockedProvider(_))
                ),
                "{url}"
            );
        }
    }

    #[test]
    fn public_addresses_are_blocked() {
        assert_eq!(
            resolved("https://mon-ia.example", &["8.8.8.8"]),
            Err(PolicyError::PublicAddress("8.8.8.8".parse().unwrap()))
        );
        // Une seule adresse publique parmi plusieurs suffit à refuser.
        assert!(resolved("https://mixte.example", &["192.168.1.2", "1.1.1.1"]).is_err());
        // IPv4 encapsulée dans IPv6.
        assert!(resolved("https://x.example", &["::ffff:8.8.8.8"]).is_err());
        // Lien local et adresse non spécifiée.
        assert!(resolved("https://x.example", &["169.254.169.254"]).is_err());
        assert!(resolved("https://x.example", &["0.0.0.0"]).is_err());
    }

    #[test]
    fn plaintext_only_on_loopback() {
        assert_eq!(
            resolved("http://ia.lan:11434", &["192.168.1.20"]),
            Err(PolicyError::PlaintextOnNetwork)
        );
        let lax = EndpointPolicy {
            allow_plaintext_lan: true,
        };
        let url = lax.check_url("http://ia.lan:11434").unwrap();
        assert!(lax
            .check_resolved(url, vec!["192.168.1.20:11434".parse().unwrap()])
            .is_ok());
    }

    #[test]
    fn malformed_urls_are_rejected() {
        let p = EndpointPolicy::default();
        assert_eq!(p.check_url("file:///etc/passwd"), Err(PolicyError::Scheme));
        assert_eq!(p.check_url("ftp://127.0.0.1"), Err(PolicyError::Scheme));
        assert_eq!(
            p.check_url("http://user:pw@127.0.0.1"),
            Err(PolicyError::CredentialsInUrl)
        );
        assert_eq!(p.check_url("pas une url"), Err(PolicyError::InvalidUrl));
    }

    #[test]
    fn real_resolution_of_loopback() {
        assert!(EndpointPolicy::default()
            .validate("http://127.0.0.1:11434")
            .is_ok());
        assert!(EndpointPolicy::default()
            .validate("http://[::1]:11434")
            .is_ok());
    }
}
