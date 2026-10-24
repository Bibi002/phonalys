//! Vérification WebAuthn / FIDO2 côté serveur, sans OpenSSL.
//!
//! La biométrie (Windows Hello, Touch ID, Face ID, empreinte Android, YubiKey Bio)
//! est vérifiée **sur l'appareil**. Phonalys ne reçoit qu'une signature, accompagnée
//! du drapeau UV (« utilisateur vérifié »), que nous rendons obligatoire. Aucune donnée
//! biométrique ne transite ni n'est stockée.
//!
//! Pris en charge : ES256 (P-256) et EdDSA (Ed25519), attestation « none » (les
//! autres formats sont acceptés, mais leur attestation est ignorée), détection des
//! authentificateurs clonés par compteur de signatures.

mod authdata;
mod cose;
#[cfg(feature = "test-authenticator")]
pub mod testing;

use authdata::AuthData;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
pub use cose::PublicKey;
use phonalys_crypto::constant_time_eq;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum WebAuthnError {
    #[error("encodage invalide")]
    Encoding,
    #[error("type de cérémonie inattendu")]
    WrongType,
    #[error("défi invalide ou expiré")]
    Challenge,
    #[error("origine non autorisée : {0}")]
    Origin(String),
    #[error("requête inter-origines refusée")]
    CrossOrigin,
    #[error("identifiant de partie de confiance (RP ID) incorrect")]
    RpId,
    #[error("présence de l'utilisateur non confirmée")]
    UserNotPresent,
    #[error("vérification de l'utilisateur (biométrie ou PIN) exigée")]
    UserNotVerified,
    #[error("données d'authentificateur mal formées")]
    AuthData,
    #[error("algorithme de clé non pris en charge")]
    UnsupportedKey,
    #[error("identifiant de clé incohérent")]
    CredentialMismatch,
    #[error("clé inconnue")]
    UnknownCredential,
    #[error("signature invalide")]
    Signature,
    #[error("authentificateur cloné détecté (compteur de signatures en recul)")]
    CloneDetected,
}

/// Partie de confiance : le serveur Phonalys du praticien.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelyingParty {
    /// Nom de domaine, par exemple `localhost` ou `phonalys.lan` (pas d'adresse IP).
    pub id: String,
    pub name: String,
    /// Origines exactes autorisées, par exemple `https://phonalys.lan:8443`.
    pub origins: Vec<String>,
}

/// Défi aléatoire de 256 bits, à usage unique.
#[derive(Clone, PartialEq, Eq)]
pub struct Challenge(pub [u8; 32]);

impl std::fmt::Debug for Challenge {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Challenge(..)")
    }
}

impl Challenge {
    pub fn from_random(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub fn to_base64url(&self) -> String {
        URL_SAFE_NO_PAD.encode(self.0)
    }
}

/// Clé enregistrée, conservée chiffrée dans le coffre.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredCredential {
    /// Identifiant de la clé, en base64url.
    pub id: String,
    pub public_key: PublicKey,
    pub sign_count: u32,
    pub label: String,
    /// La clé peut être synchronisée entre appareils (passkey iCloud / Google).
    pub backup_eligible: bool,
    /// Modèle d'authentificateur (AAGUID), en hexadécimal.
    pub aaguid: String,
}

/// Réponse du navigateur à `navigator.credentials.create()`, champs en base64url.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistrationResponse {
    pub id: String,
    #[serde(rename = "clientDataJSON")]
    pub client_data_json: String,
    #[serde(rename = "attestationObject")]
    pub attestation_object: String,
}

/// Réponse du navigateur à `navigator.credentials.get()`, champs en base64url.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthenticationResponse {
    pub id: String,
    #[serde(rename = "clientDataJSON")]
    pub client_data_json: String,
    #[serde(rename = "authenticatorData")]
    pub authenticator_data: String,
    pub signature: String,
}

#[derive(Deserialize)]
struct ClientData {
    #[serde(rename = "type")]
    ty: String,
    challenge: String,
    origin: String,
    #[serde(rename = "crossOrigin", default)]
    cross_origin: bool,
}

fn b64(s: &str) -> Result<Vec<u8>, WebAuthnError> {
    URL_SAFE_NO_PAD
        .decode(s.trim_end_matches('='))
        .map_err(|_| WebAuthnError::Encoding)
}

fn verify_client_data(
    raw: &[u8],
    expected_type: &str,
    challenge: &Challenge,
    rp: &RelyingParty,
) -> Result<(), WebAuthnError> {
    let cd: ClientData = serde_json::from_slice(raw).map_err(|_| WebAuthnError::Encoding)?;
    if cd.ty != expected_type {
        return Err(WebAuthnError::WrongType);
    }
    if !constant_time_eq(&b64(&cd.challenge)?, &challenge.0) {
        return Err(WebAuthnError::Challenge);
    }
    if !rp.origins.contains(&cd.origin) {
        return Err(WebAuthnError::Origin(cd.origin));
    }
    if cd.cross_origin {
        return Err(WebAuthnError::CrossOrigin);
    }
    Ok(())
}

fn check_flags(auth: &AuthData, rp: &RelyingParty) -> Result<(), WebAuthnError> {
    let expected = Sha256::digest(rp.id.as_bytes());
    if !constant_time_eq(&auth.rp_id_hash, &expected) {
        return Err(WebAuthnError::RpId);
    }
    if !auth.user_present() {
        return Err(WebAuthnError::UserNotPresent);
    }
    if !auth.user_verified() {
        return Err(WebAuthnError::UserNotVerified);
    }
    Ok(())
}

/// Options à passer à `navigator.credentials.create({ publicKey })`.
pub fn registration_options(
    rp: &RelyingParty,
    user_id: &[u8],
    user_name: &str,
    challenge: &Challenge,
    existing: &[StoredCredential],
) -> Value {
    json!({
        "rp": { "id": rp.id, "name": rp.name },
        "user": { "id": URL_SAFE_NO_PAD.encode(user_id), "name": user_name, "displayName": user_name },
        "challenge": challenge.to_base64url(),
        "pubKeyCredParams": [
            { "type": "public-key", "alg": -8 },
            { "type": "public-key", "alg": -7 }
        ],
        "timeout": 120000,
        "attestation": "none",
        "authenticatorSelection": { "residentKey": "preferred", "userVerification": "required" },
        "excludeCredentials": existing.iter().map(|c| json!({ "type": "public-key", "id": c.id })).collect::<Vec<_>>(),
    })
}

/// Options à passer à `navigator.credentials.get({ publicKey })`.
pub fn authentication_options(
    rp: &RelyingParty,
    challenge: &Challenge,
    credentials: &[StoredCredential],
) -> Value {
    json!({
        "rpId": rp.id,
        "challenge": challenge.to_base64url(),
        "timeout": 120000,
        "userVerification": "required",
        "allowCredentials": credentials.iter().map(|c| json!({ "type": "public-key", "id": c.id })).collect::<Vec<_>>(),
    })
}

/// Vérifie une inscription et renvoie la clé à stocker.
pub fn finish_registration(
    rp: &RelyingParty,
    challenge: &Challenge,
    response: &RegistrationResponse,
    label: &str,
) -> Result<StoredCredential, WebAuthnError> {
    verify_client_data(
        &b64(&response.client_data_json)?,
        "webauthn.create",
        challenge,
        rp,
    )?;

    let attestation: ciborium::Value =
        ciborium::de::from_reader(b64(&response.attestation_object)?.as_slice())
            .map_err(|_| WebAuthnError::Encoding)?;
    let auth_bytes = attestation
        .as_map()
        .and_then(|m| m.iter().find(|(k, _)| k.as_text() == Some("authData")))
        .and_then(|(_, v)| v.as_bytes())
        .ok_or(WebAuthnError::AuthData)?;

    let auth = AuthData::parse(auth_bytes)?;
    check_flags(&auth, rp)?;
    let (sign_count, backup_eligible) = (auth.sign_count, auth.backup_eligible());
    let attested = auth.attested.ok_or(WebAuthnError::AuthData)?;
    if !constant_time_eq(&attested.credential_id, &b64(&response.id)?) {
        return Err(WebAuthnError::CredentialMismatch);
    }

    Ok(StoredCredential {
        id: URL_SAFE_NO_PAD.encode(&attested.credential_id),
        public_key: attested.public_key,
        sign_count,
        label: label.to_owned(),
        backup_eligible,
        aaguid: attested.aaguid.iter().map(|b| format!("{b:02x}")).collect(),
    })
}

/// Vérifie une connexion. Met à jour le compteur de la clé utilisée et renvoie son index.
pub fn finish_authentication(
    rp: &RelyingParty,
    challenge: &Challenge,
    response: &AuthenticationResponse,
    credentials: &mut [StoredCredential],
) -> Result<usize, WebAuthnError> {
    let index = credentials
        .iter()
        .position(|c| c.id == response.id.trim_end_matches('='))
        .ok_or(WebAuthnError::UnknownCredential)?;

    let client_data = b64(&response.client_data_json)?;
    verify_client_data(&client_data, "webauthn.get", challenge, rp)?;

    let auth_bytes = b64(&response.authenticator_data)?;
    let auth = AuthData::parse(&auth_bytes)?;
    check_flags(&auth, rp)?;

    let mut signed = auth_bytes.clone();
    signed.extend_from_slice(&Sha256::digest(&client_data));
    credentials[index]
        .public_key
        .verify(&signed, &b64(&response.signature)?)?;

    let stored = credentials[index].sign_count;
    if (auth.sign_count != 0 || stored != 0) && auth.sign_count <= stored {
        return Err(WebAuthnError::CloneDetected);
    }
    credentials[index].sign_count = auth.sign_count;
    Ok(index)
}

#[cfg(all(test, feature = "test-authenticator"))]
mod tests;
