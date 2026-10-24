//! Client des moteurs d'inférence locaux (Ollama, API compatible OpenAI locale).
//!
//! Volontairement minimal : HTTP/1.1, une requête par connexion, et **connexion
//! uniquement aux adresses validées** par [`EndpointPolicy`] (jamais de nouvelle
//! résolution DNS entre le contrôle et la connexion). En HTTPS sur le réseau local,
//! le certificat du serveur d'IA est **épinglé** par son empreinte SHA-256 : aucune
//! autorité de certification publique n'intervient.

use crate::policy::{EndpointPolicy, PolicyError, ValidatedEndpoint};
use crate::provider::{ChatMessage, ChatRequest, ProviderKind};
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::WebPkiSupportedAlgorithms;
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{DigitallySignedStruct, SignatureScheme};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpStream;

/// Taille maximale d'une réponse du moteur.
pub const MAX_RESPONSE_BYTES: usize = 8 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum AiError {
    #[error(transparent)]
    Policy(#[from] PolicyError),
    #[error("empreinte du certificat requise en HTTPS (format AA:BB:…, 64 chiffres hexadécimaux)")]
    PinRequired,
    #[error("empreinte du certificat invalide")]
    InvalidPin,
    #[error("moteur d'IA injoignable : vérifiez qu'il est démarré et l'adresse indiquée")]
    Connect,
    #[error("le moteur d'IA n'a pas répondu à temps")]
    Timeout,
    #[error("connexion sécurisée refusée : l'empreinte du certificat ne correspond pas")]
    Tls,
    #[error("le moteur d'IA a répondu {0} : {1}")]
    Status(u16, String),
    #[error("réponse du moteur d'IA illisible")]
    BadResponse,
    #[error("réponse du moteur d'IA trop volumineuse")]
    TooLarge,
}

/// Moteur d'inférence configuré par le praticien.
#[derive(Debug, Clone)]
pub struct Engine {
    pub kind: ProviderKind,
    /// Adresse de base : `http://127.0.0.1:11434` (Ollama), `http://localhost:1234/v1`…
    pub endpoint: String,
    /// Empreinte SHA-256 du certificat du serveur d'IA, exigée en HTTPS.
    pub cert_sha256: Option<String>,
    pub policy: EndpointPolicy,
}

/// `AA:BB:…` ou `aabb…` → 32 octets.
pub fn parse_pin(s: &str) -> Result<[u8; 32], AiError> {
    let clean: String = s
        .chars()
        .filter(|c| !matches!(c, ':' | ' ' | '-'))
        .collect();
    let bytes = hex::decode(clean).map_err(|_| AiError::InvalidPin)?;
    bytes.try_into().map_err(|_| AiError::InvalidPin)
}

impl Engine {
    /// Valide l'adresse (statiquement, puis après résolution) sans se connecter.
    pub async fn resolve(&self) -> Result<ValidatedEndpoint, AiError> {
        let url = self.policy.check_url(&self.endpoint)?;
        if url.scheme() == "https" {
            parse_pin(self.cert_sha256.as_deref().ok_or(AiError::PinRequired)?)?;
        }
        let host = url
            .host_str()
            .ok_or(PolicyError::InvalidUrl)?
            .trim_start_matches('[')
            .trim_end_matches(']')
            .to_owned();
        let port = url.port_or_known_default().ok_or(PolicyError::InvalidUrl)?;
        let addrs: Vec<_> = tokio::net::lookup_host((host.as_str(), port))
            .await
            .map_err(|_| PolicyError::Resolution)?
            .collect();
        Ok(self.policy.check_resolved(url, addrs)?)
    }

    /// Modèles installés sur le moteur (sert aussi de test de connexion).
    pub async fn models(&self, timeout: Duration) -> Result<Vec<String>, AiError> {
        let (path, field, name) = match self.kind {
            ProviderKind::Ollama => ("api/tags", "models", "name"),
            ProviderKind::OpenAiCompatibleLocal => ("models", "data", "id"),
        };
        let v = self.call("GET", path, None, timeout).await?;
        let list = v
            .get(field)
            .and_then(Value::as_array)
            .ok_or(AiError::BadResponse)?;
        Ok(list
            .iter()
            .filter_map(|m| m.get(name).and_then(Value::as_str).map(str::to_owned))
            .collect())
    }

    /// Une réponse complète (sans flux) du modèle.
    pub async fn chat(&self, req: &ChatRequest, timeout: Duration) -> Result<String, AiError> {
        let messages: Vec<&ChatMessage> = req.messages.iter().collect();
        let (path, body) = match self.kind {
            ProviderKind::Ollama => (
                "api/chat",
                json!({
                    "model": req.model,
                    "messages": messages,
                    "stream": false,
                    "options": { "temperature": req.temperature.unwrap_or(0.3) },
                }),
            ),
            ProviderKind::OpenAiCompatibleLocal => (
                "chat/completions",
                json!({
                    "model": req.model,
                    "messages": messages,
                    "stream": false,
                    "temperature": req.temperature.unwrap_or(0.3),
                }),
            ),
        };
        let body = serde_json::to_vec(&body).map_err(|_| AiError::BadResponse)?;
        let v = self.call("POST", path, Some(&body), timeout).await?;
        let text = match self.kind {
            ProviderKind::Ollama => v.pointer("/message/content"),
            ProviderKind::OpenAiCompatibleLocal => v.pointer("/choices/0/message/content"),
        };
        text.and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or(AiError::BadResponse)
    }

    async fn call(
        &self,
        method: &str,
        path: &str,
        body: Option<&[u8]>,
        timeout: Duration,
    ) -> Result<Value, AiError> {
        let ep = self.resolve().await?;
        let fut = async {
            let raw = request(&ep, self.cert_sha256.as_deref(), method, path, body).await?;
            let (status, bytes) = parse_response(&raw)?;
            if !(200..300).contains(&status) {
                let snippet: String = String::from_utf8_lossy(&bytes).chars().take(200).collect();
                return Err(AiError::Status(status, snippet));
            }
            serde_json::from_slice(&bytes).map_err(|_| AiError::BadResponse)
        };
        tokio::time::timeout(timeout, fut)
            .await
            .map_err(|_| AiError::Timeout)?
    }
}

/// Chemin de la requête : chemin de base de l'adresse, puis `path`.
fn join_path(ep: &ValidatedEndpoint, path: &str) -> String {
    let base = ep.url.path().trim_end_matches('/');
    format!("{base}/{path}")
}

fn host_header(ep: &ValidatedEndpoint) -> String {
    let host = match ep.url.host() {
        Some(url::Host::Ipv6(ip)) => format!("[{ip}]"),
        Some(h) => h.to_string(),
        None => String::new(),
    };
    match ep.url.port() {
        Some(p) => format!("{host}:{p}"),
        None => host,
    }
}

async fn request(
    ep: &ValidatedEndpoint,
    pin: Option<&str>,
    method: &str,
    path: &str,
    body: Option<&[u8]>,
) -> Result<Vec<u8>, AiError> {
    let mut head = format!(
        "{method} {} HTTP/1.1\r\nHost: {}\r\nUser-Agent: Phonalys\r\nAccept: application/json\r\nConnection: close\r\n",
        join_path(ep, path),
        host_header(ep)
    );
    if let Some(b) = body {
        head.push_str(&format!(
            "Content-Type: application/json\r\nContent-Length: {}\r\n",
            b.len()
        ));
    }
    head.push_str("\r\n");
    let mut payload = head.into_bytes();
    payload.extend_from_slice(body.unwrap_or_default());

    let mut tcp = None;
    for addr in &ep.addrs {
        if let Ok(s) = TcpStream::connect(addr).await {
            tcp = Some(s);
            break;
        }
    }
    let tcp = tcp.ok_or(AiError::Connect)?;

    if ep.url.scheme() == "https" {
        let pin = parse_pin(pin.ok_or(AiError::PinRequired)?)?;
        let host = ep
            .url
            .host_str()
            .unwrap_or_default()
            .trim_start_matches('[')
            .trim_end_matches(']')
            .to_owned();
        let name = ServerName::try_from(host).map_err(|_| PolicyError::InvalidUrl)?;
        let connector = tokio_rustls::TlsConnector::from(Arc::new(pinned_config(pin)?));
        let tls = connector
            .connect(name, tcp)
            .await
            .map_err(|_| AiError::Tls)?;
        exchange(tls, &payload).await
    } else {
        exchange(tcp, &payload).await
    }
}

async fn exchange<S: AsyncRead + AsyncWrite + Unpin>(
    mut stream: S,
    payload: &[u8],
) -> Result<Vec<u8>, AiError> {
    stream
        .write_all(payload)
        .await
        .map_err(|_| AiError::Connect)?;
    stream.flush().await.map_err(|_| AiError::Connect)?;
    let mut out = Vec::new();
    let mut limited = stream.take(MAX_RESPONSE_BYTES as u64 + 64 * 1024);
    limited
        .read_to_end(&mut out)
        .await
        .map_err(|_| AiError::BadResponse)?;
    if out.len() > MAX_RESPONSE_BYTES + 32 * 1024 {
        return Err(AiError::TooLarge);
    }
    Ok(out)
}

/// Statut et corps d'une réponse HTTP/1.1 (longueur fixe, segmentée ou jusqu'à la fin).
pub fn parse_response(raw: &[u8]) -> Result<(u16, Vec<u8>), AiError> {
    let split = raw
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .ok_or(AiError::BadResponse)?;
    let head = std::str::from_utf8(&raw[..split]).map_err(|_| AiError::BadResponse)?;
    let body = &raw[split + 4..];
    let mut lines = head.split("\r\n");
    let status: u16 = lines
        .next()
        .and_then(|l| l.split(' ').nth(1))
        .and_then(|s| s.parse().ok())
        .ok_or(AiError::BadResponse)?;
    let mut chunked = false;
    let mut length = None;
    for line in lines {
        let Some((k, v)) = line.split_once(':') else {
            continue;
        };
        let (k, v) = (k.trim().to_ascii_lowercase(), v.trim());
        if k == "transfer-encoding" && v.to_ascii_lowercase().contains("chunked") {
            chunked = true;
        } else if k == "content-length" {
            length = v.parse::<usize>().ok();
        }
    }
    let body = if chunked {
        dechunk(body)?
    } else if let Some(n) = length {
        body.get(..n).ok_or(AiError::BadResponse)?.to_vec()
    } else {
        body.to_vec()
    };
    if body.len() > MAX_RESPONSE_BYTES {
        return Err(AiError::TooLarge);
    }
    Ok((status, body))
}

fn dechunk(mut data: &[u8]) -> Result<Vec<u8>, AiError> {
    let mut out = Vec::new();
    loop {
        let eol = data
            .windows(2)
            .position(|w| w == b"\r\n")
            .ok_or(AiError::BadResponse)?;
        let size_line = std::str::from_utf8(&data[..eol]).map_err(|_| AiError::BadResponse)?;
        let size_hex = size_line.split(';').next().unwrap_or_default().trim();
        let size = usize::from_str_radix(size_hex, 16).map_err(|_| AiError::BadResponse)?;
        data = &data[eol + 2..];
        if size == 0 {
            return Ok(out);
        }
        if out.len() + size > MAX_RESPONSE_BYTES {
            return Err(AiError::TooLarge);
        }
        out.extend_from_slice(data.get(..size).ok_or(AiError::BadResponse)?);
        data = data.get(size + 2..).ok_or(AiError::BadResponse)?;
    }
}

/// TLS 1.3, et le seul certificat accepté est celui dont l'empreinte a été épinglée.
fn pinned_config(pin: [u8; 32]) -> Result<rustls::ClientConfig, AiError> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let verifier = PinnedCert {
        pin,
        algs: provider.signature_verification_algorithms,
    };
    Ok(rustls::ClientConfig::builder_with_provider(provider)
        .with_protocol_versions(&[&rustls::version::TLS13])
        .map_err(|_| AiError::Tls)?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(verifier))
        .with_no_client_auth())
}

#[derive(Debug)]
struct PinnedCert {
    pin: [u8; 32],
    algs: WebPkiSupportedAlgorithms,
}

impl ServerCertVerifier for PinnedCert {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        let got: [u8; 32] = Sha256::digest(end_entity.as_ref()).into();
        // L'empreinte n'est pas secrète : une comparaison simple suffit.
        if got == self.pin {
            Ok(ServerCertVerified::assertion())
        } else {
            Err(rustls::Error::InvalidCertificate(
                rustls::CertificateError::ApplicationVerificationFailure,
            ))
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(message, cert, dss, &self.algs)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(message, cert, dss, &self.algs)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.algs.supported_schemes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::Role;
    use tokio::net::TcpListener;

    /// Faux moteur : répond une fois `response` et renvoie la requête reçue.
    async fn fake_engine(response: Vec<u8>) -> (String, tokio::task::JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let task = tokio::spawn(async move {
            let (mut s, _) = listener.accept().await.unwrap();
            let mut buf = vec![0u8; 64 * 1024];
            let n = s.read(&mut buf).await.unwrap();
            s.write_all(&response).await.unwrap();
            s.shutdown().await.unwrap();
            String::from_utf8_lossy(&buf[..n]).into_owned()
        });
        (format!("http://127.0.0.1:{}", addr.port()), task)
    }

    fn engine(kind: ProviderKind, endpoint: String) -> Engine {
        Engine {
            kind,
            endpoint,
            cert_sha256: None,
            policy: EndpointPolicy::default(),
        }
    }

    fn request() -> ChatRequest {
        ChatRequest {
            model: "mistral".into(),
            messages: vec![ChatMessage {
                role: Role::User,
                content: "Bonjour".into(),
            }],
            temperature: None,
        }
    }

    #[tokio::test]
    async fn ollama_chat_with_chunked_response() {
        let body = r#"{"message":{"role":"assistant","content":"Bilan rédigé."}}"#;
        let (a, b) = body.split_at(10);
        let response = format!(
            "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n{:x}\r\n{a}\r\n{:x}\r\n{b}\r\n0\r\n\r\n",
            a.len(),
            b.len()
        );
        let (url, seen) = fake_engine(response.into_bytes()).await;
        let text = engine(ProviderKind::Ollama, url)
            .chat(&request(), Duration::from_secs(5))
            .await
            .unwrap();
        assert_eq!(text, "Bilan rédigé.");
        let req = seen.await.unwrap();
        assert!(req.starts_with("POST /api/chat HTTP/1.1\r\n"), "{req}");
        assert!(req.contains("\"stream\":false"));
    }

    #[tokio::test]
    async fn openai_compatible_path_and_models() {
        let body = r#"{"data":[{"id":"qwen2.5-7b"},{"id":"llama3.1"}]}"#;
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        );
        let (url, seen) = fake_engine(response.into_bytes()).await;
        let models = engine(ProviderKind::OpenAiCompatibleLocal, format!("{url}/v1/"))
            .models(Duration::from_secs(5))
            .await
            .unwrap();
        assert_eq!(models, ["qwen2.5-7b", "llama3.1"]);
        assert!(seen.await.unwrap().starts_with("GET /v1/models HTTP/1.1"));
    }

    #[tokio::test]
    async fn errors_are_reported_without_hanging() {
        let (url, _) = fake_engine(
            b"HTTP/1.1 404 Not Found\r\nContent-Length: 25\r\n\r\n{\"error\":\"model missing\"}"
                .to_vec(),
        )
        .await;
        let e = engine(ProviderKind::Ollama, url)
            .chat(&request(), Duration::from_secs(5))
            .await
            .unwrap_err();
        assert!(
            matches!(e, AiError::Status(404, ref m) if m.contains("model missing")),
            "{e}"
        );

        // Un moteur qui ne répond jamais : délai dépassé.
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
        let e = engine(ProviderKind::Ollama, url)
            .chat(&request(), Duration::from_millis(300))
            .await
            .unwrap_err();
        assert!(matches!(e, AiError::Timeout), "{e}");
        drop(listener);
    }

    #[tokio::test]
    async fn cloud_and_public_endpoints_are_refused_before_any_connection() {
        for url in ["https://api.openai.com/v1", "http://api.anthropic.com"] {
            let e = engine(ProviderKind::OpenAiCompatibleLocal, url.into())
                .resolve()
                .await
                .unwrap_err();
            assert!(
                matches!(e, AiError::Policy(PolicyError::BlockedProvider(_))),
                "{url}: {e}"
            );
        }
        let e = engine(ProviderKind::Ollama, "http://8.8.8.8:11434".into())
            .resolve()
            .await
            .unwrap_err();
        assert!(
            matches!(e, AiError::Policy(PolicyError::PublicAddress(_))),
            "{e}"
        );
        let e = engine(ProviderKind::Ollama, "https://127.0.0.1:11434".into())
            .resolve()
            .await
            .unwrap_err();
        assert!(matches!(e, AiError::PinRequired), "{e}");
    }

    #[tokio::test]
    async fn https_requires_the_pinned_certificate() {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
        let der = cert.cert.der().clone();
        let key = rustls::pki_types::PrivateKeyDer::Pkcs8(
            rustls::pki_types::PrivatePkcs8KeyDer::from(cert.key_pair.serialize_der()),
        );
        let server = rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(vec![der.clone()], key)
            .unwrap();
        let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(server));
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            while let Ok((s, _)) = listener.accept().await {
                let acceptor = acceptor.clone();
                tokio::spawn(async move {
                    if let Ok(mut tls) = acceptor.accept(s).await {
                        let mut buf = [0u8; 4096];
                        let _ = tls.read(&mut buf).await;
                        let body = r#"{"models":[{"name":"mistral"}]}"#;
                        let res = format!(
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{body}",
                            body.len()
                        );
                        let _ = tls.write_all(res.as_bytes()).await;
                        let _ = tls.shutdown().await;
                    }
                });
            }
        });
        let good = hex::encode(Sha256::digest(der.as_ref()));
        let mut e = engine(ProviderKind::Ollama, format!("https://localhost:{port}"));
        e.cert_sha256 = Some(good.to_uppercase());
        assert_eq!(e.models(Duration::from_secs(5)).await.unwrap(), ["mistral"]);

        e.cert_sha256 = Some("00".repeat(32));
        assert!(matches!(
            e.models(Duration::from_secs(5)).await,
            Err(AiError::Tls)
        ));
    }

    #[test]
    fn pins_accept_common_formats() {
        let colons = (0..32).map(|_| "AB").collect::<Vec<_>>().join(":");
        assert_eq!(parse_pin(&colons).unwrap(), [0xAB; 32]);
        assert_eq!(parse_pin(&"ab".repeat(32)).unwrap(), [0xAB; 32]);
        assert!(parse_pin("abcd").is_err());
        assert!(parse_pin(&"zz".repeat(32)).is_err());
    }

    #[test]
    fn malformed_responses_are_rejected() {
        assert!(parse_response(b"garbage").is_err());
        assert!(parse_response(b"HTTP/1.1 200 OK\r\nContent-Length: 10\r\n\r\nabc").is_err());
        assert!(
            parse_response(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\nzz\r\n").is_err()
        );
        assert_eq!(
            parse_response(b"HTTP/1.1 200 OK\r\n\r\n{}").unwrap(),
            (200, b"{}".to_vec())
        );
    }
}
