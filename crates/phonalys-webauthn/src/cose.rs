//! Clés publiques COSE (RFC 9052/9053) et vérification des signatures.

use crate::WebAuthnError;
use ciborium::Value;
use serde::{Deserialize, Serialize};

const KTY: i128 = 1;
const ALG: i128 = 3;
const CRV: i128 = -1;
const X: i128 = -2;
const Y: i128 = -3;

const KTY_OKP: i128 = 1;
const KTY_EC2: i128 = 2;
const ALG_ES256: i128 = -7;
const ALG_EDDSA: i128 = -8;
const CRV_P256: i128 = 1;
const CRV_ED25519: i128 = 6;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "alg", rename_all = "snake_case")]
pub enum PublicKey {
    /// ECDSA P-256 / SHA-256, point SEC1 non compressé.
    Es256 { sec1: Vec<u8> },
    /// Ed25519.
    Ed25519 { key: Vec<u8> },
}

fn get_int(map: &[(Value, Value)], key: i128) -> Option<i128> {
    map.iter()
        .find(|(k, _)| k.as_integer().map(i128::from) == Some(key))
        .and_then(|(_, v)| v.as_integer())
        .map(i128::from)
}

fn get_bytes(map: &[(Value, Value)], key: i128) -> Option<&Vec<u8>> {
    map.iter()
        .find(|(k, _)| k.as_integer().map(i128::from) == Some(key))
        .and_then(|(_, v)| v.as_bytes())
}

impl PublicKey {
    pub(crate) fn from_cose(value: &Value) -> Result<Self, WebAuthnError> {
        let map = value.as_map().ok_or(WebAuthnError::UnsupportedKey)?;
        let unsupported = || WebAuthnError::UnsupportedKey;
        match (get_int(map, KTY), get_int(map, ALG), get_int(map, CRV)) {
            (Some(KTY_EC2), Some(ALG_ES256), Some(CRV_P256)) => {
                let x = get_bytes(map, X)
                    .filter(|b| b.len() == 32)
                    .ok_or_else(unsupported)?;
                let y = get_bytes(map, Y)
                    .filter(|b| b.len() == 32)
                    .ok_or_else(unsupported)?;
                let mut sec1 = Vec::with_capacity(65);
                sec1.push(0x04);
                sec1.extend_from_slice(x);
                sec1.extend_from_slice(y);
                // Refuse un point hors de la courbe dès l'inscription.
                p256::ecdsa::VerifyingKey::from_sec1_bytes(&sec1).map_err(|_| unsupported())?;
                Ok(PublicKey::Es256 { sec1 })
            }
            (Some(KTY_OKP), Some(ALG_EDDSA), Some(CRV_ED25519)) => {
                let x = get_bytes(map, X).ok_or_else(unsupported)?;
                let key: [u8; 32] = x.as_slice().try_into().map_err(|_| unsupported())?;
                ed25519_dalek::VerifyingKey::from_bytes(&key).map_err(|_| unsupported())?;
                Ok(PublicKey::Ed25519 { key: key.to_vec() })
            }
            _ => Err(unsupported()),
        }
    }

    pub(crate) fn verify(&self, message: &[u8], signature: &[u8]) -> Result<(), WebAuthnError> {
        let bad = || WebAuthnError::Signature;
        match self {
            PublicKey::Es256 { sec1 } => {
                use p256::ecdsa::signature::Verifier;
                let key = p256::ecdsa::VerifyingKey::from_sec1_bytes(sec1).map_err(|_| bad())?;
                let sig = p256::ecdsa::Signature::from_der(signature).map_err(|_| bad())?;
                // Les authentificateurs peuvent produire un S « haut » : on normalise.
                let sig = sig.normalize_s();
                key.verify(message, &sig).map_err(|_| bad())
            }
            PublicKey::Ed25519 { key } => {
                let key: [u8; 32] = key.as_slice().try_into().map_err(|_| bad())?;
                let key = ed25519_dalek::VerifyingKey::from_bytes(&key).map_err(|_| bad())?;
                let sig = ed25519_dalek::Signature::from_slice(signature).map_err(|_| bad())?;
                key.verify_strict(message, &sig).map_err(|_| bad())
            }
        }
    }
}
