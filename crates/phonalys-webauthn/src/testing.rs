//! Authentificateur logiciel simulé, **réservé aux tests**. Il produit des réponses
//! identiques à celles d'un navigateur et permet de simuler des attaques.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::{AuthenticationResponse, RegistrationResponse};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use ciborium::Value;
use rand_core::{OsRng, RngCore};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Algorithm {
    Es256,
    Ed25519,
}

#[derive(Debug)]
enum Key {
    Es256(p256::ecdsa::SigningKey),
    Ed25519(ed25519_dalek::SigningKey),
}

#[derive(Debug)]
pub struct SoftAuthenticator {
    key: Key,
    pub credential_id: Vec<u8>,
    pub counter: u32,
    /// Simule un appareil sans biométrie ni PIN.
    pub user_verified: bool,
    /// Secret interne de l'extension PRF (hmac-secret), qui ne quitte jamais l'appareil.
    prf_secret: [u8; 32],
}

fn int(i: i64) -> Value {
    Value::Integer(i.into())
}

fn encode(value: &Value) -> Vec<u8> {
    let mut out = Vec::new();
    ciborium::ser::into_writer(value, &mut out).unwrap();
    out
}

impl SoftAuthenticator {
    pub fn new(alg: Algorithm) -> Self {
        let key = match alg {
            Algorithm::Es256 => Key::Es256(loop {
                // Un scalaire aléatoire de 256 bits est presque toujours valide.
                let mut bytes = [0u8; 32];
                OsRng.fill_bytes(&mut bytes);
                if let Ok(k) = p256::ecdsa::SigningKey::from_slice(&bytes) {
                    break k;
                }
            }),
            Algorithm::Ed25519 => {
                let mut seed = [0u8; 32];
                OsRng.fill_bytes(&mut seed);
                Key::Ed25519(ed25519_dalek::SigningKey::from_bytes(&seed))
            }
        };
        let mut credential_id = vec![0u8; 16];
        OsRng.fill_bytes(&mut credential_id);
        let mut prf_secret = [0u8; 32];
        OsRng.fill_bytes(&mut prf_secret);
        Self {
            key,
            credential_id,
            counter: 0,
            user_verified: true,
            prf_secret,
        }
    }

    /// Sortie PRF pour un sel : déterministe, propre à l'appareil.
    pub fn prf(&self, salt: &[u8]) -> Vec<u8> {
        let mut h = Sha256::new();
        h.update(b"WebAuthn PRF\0");
        h.update(self.prf_secret);
        h.update(salt);
        h.finalize().to_vec()
    }

    pub fn prf_b64(&self, salt: &[u8]) -> String {
        URL_SAFE_NO_PAD.encode(self.prf(salt))
    }

    fn cose_key(&self) -> Value {
        match &self.key {
            Key::Es256(k) => {
                let point = k.verifying_key().to_sec1_point(false);
                Value::Map(vec![
                    (int(1), int(2)),
                    (int(3), int(-7)),
                    (int(-1), int(1)),
                    (int(-2), Value::Bytes(point.x().unwrap().to_vec())),
                    (int(-3), Value::Bytes(point.y().unwrap().to_vec())),
                ])
            }
            Key::Ed25519(k) => Value::Map(vec![
                (int(1), int(1)),
                (int(3), int(-8)),
                (int(-1), int(6)),
                (int(-2), Value::Bytes(k.verifying_key().to_bytes().to_vec())),
            ]),
        }
    }

    fn auth_data(&self, rp_id: &str, attested: bool) -> Vec<u8> {
        let mut flags = 0x01;
        if self.user_verified {
            flags |= 0x04;
        }
        if attested {
            flags |= 0x40;
        }
        let mut data = Sha256::digest(rp_id.as_bytes()).to_vec();
        data.push(flags);
        data.extend_from_slice(&self.counter.to_be_bytes());
        if attested {
            data.extend_from_slice(&[0u8; 16]);
            data.extend_from_slice(&(self.credential_id.len() as u16).to_be_bytes());
            data.extend_from_slice(&self.credential_id);
            data.extend(encode(&self.cose_key()));
        }
        data
    }

    fn client_data(ty: &str, challenge_b64: &str, origin: &str) -> Vec<u8> {
        serde_json::json!({ "type": ty, "challenge": challenge_b64, "origin": origin, "crossOrigin": false })
            .to_string()
            .into_bytes()
    }

    pub fn register(&self, rp_id: &str, origin: &str, challenge_b64: &str) -> RegistrationResponse {
        let attestation = Value::Map(vec![
            (Value::Text("fmt".into()), Value::Text("none".into())),
            (Value::Text("attStmt".into()), Value::Map(vec![])),
            (
                Value::Text("authData".into()),
                Value::Bytes(self.auth_data(rp_id, true)),
            ),
        ]);
        RegistrationResponse {
            id: URL_SAFE_NO_PAD.encode(&self.credential_id),
            client_data_json: URL_SAFE_NO_PAD.encode(Self::client_data(
                "webauthn.create",
                challenge_b64,
                origin,
            )),
            attestation_object: URL_SAFE_NO_PAD.encode(encode(&attestation)),
        }
    }

    /// Signe une connexion ; incrémente le compteur comme un vrai authentificateur.
    pub fn authenticate(
        &mut self,
        rp_id: &str,
        origin: &str,
        challenge_b64: &str,
    ) -> AuthenticationResponse {
        self.counter += 1;
        let auth_data = self.auth_data(rp_id, false);
        let client_data = Self::client_data("webauthn.get", challenge_b64, origin);
        let mut message = auth_data.clone();
        message.extend_from_slice(&Sha256::digest(&client_data));
        let signature = match &self.key {
            Key::Es256(k) => {
                use p256::ecdsa::signature::Signer;
                let sig: p256::ecdsa::Signature = k.sign(&message);
                sig.to_der().as_bytes().to_vec()
            }
            Key::Ed25519(k) => {
                use ed25519_dalek::Signer;
                k.sign(&message).to_bytes().to_vec()
            }
        };
        AuthenticationResponse {
            id: URL_SAFE_NO_PAD.encode(&self.credential_id),
            client_data_json: URL_SAFE_NO_PAD.encode(client_data),
            authenticator_data: URL_SAFE_NO_PAD.encode(auth_data),
            signature: URL_SAFE_NO_PAD.encode(signature),
        }
    }
}
