//! Analyse des « authenticator data » (WebAuthn §6.1).

use crate::cose::PublicKey;
use crate::WebAuthnError;

const UP: u8 = 0x01;
const UV: u8 = 0x04;
const BE: u8 = 0x08;
const AT: u8 = 0x40;

#[derive(Debug)]
pub(crate) struct Attested {
    pub aaguid: [u8; 16],
    pub credential_id: Vec<u8>,
    pub public_key: PublicKey,
}

#[derive(Debug)]
pub(crate) struct AuthData {
    pub rp_id_hash: [u8; 32],
    pub flags: u8,
    pub sign_count: u32,
    pub attested: Option<Attested>,
}

impl AuthData {
    pub fn user_present(&self) -> bool {
        self.flags & UP != 0
    }

    pub fn user_verified(&self) -> bool {
        self.flags & UV != 0
    }

    pub fn backup_eligible(&self) -> bool {
        self.flags & BE != 0
    }

    pub fn parse(bytes: &[u8]) -> Result<Self, WebAuthnError> {
        let err = || WebAuthnError::AuthData;
        if bytes.len() < 37 {
            return Err(err());
        }
        let rp_id_hash: [u8; 32] = bytes[0..32].try_into().map_err(|_| err())?;
        let flags = bytes[32];
        let sign_count = u32::from_be_bytes(bytes[33..37].try_into().map_err(|_| err())?);

        let attested = if flags & AT != 0 {
            let rest = &bytes[37..];
            if rest.len() < 18 {
                return Err(err());
            }
            let aaguid: [u8; 16] = rest[0..16].try_into().map_err(|_| err())?;
            let id_len = u16::from_be_bytes([rest[16], rest[17]]) as usize;
            // La spécification limite l'identifiant à 1023 octets.
            if id_len == 0 || id_len > 1023 || rest.len() < 18 + id_len {
                return Err(err());
            }
            let credential_id = rest[18..18 + id_len].to_vec();
            let mut cursor = std::io::Cursor::new(&rest[18 + id_len..]);
            let cose: ciborium::Value =
                ciborium::de::from_reader(&mut cursor).map_err(|_| err())?;
            Some(Attested {
                aaguid,
                credential_id,
                public_key: PublicKey::from_cose(&cose)?,
            })
        } else {
            None
        };

        Ok(Self {
            rp_id_hash,
            flags,
            sign_count,
            attested,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_truncated_input() {
        assert!(AuthData::parse(&[0; 36]).is_err());
        let mut with_at = vec![0u8; 37];
        with_at[32] = AT;
        assert!(AuthData::parse(&with_at).is_err());
    }

    #[test]
    fn parses_minimal_assertion_data() {
        let mut data = vec![7u8; 32];
        data.push(UP | UV);
        data.extend_from_slice(&42u32.to_be_bytes());
        let auth = AuthData::parse(&data).unwrap();
        assert!(auth.user_present() && auth.user_verified() && !auth.backup_eligible());
        assert_eq!(auth.sign_count, 42);
        assert!(auth.attested.is_none());
    }
}
