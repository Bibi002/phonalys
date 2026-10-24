//! Mots de passe à usage unique basés sur le temps (RFC 6238, HMAC-SHA1, 6 chiffres,
//! pas de 30 s) : compatible Aegis, 2FAS, Ente Auth, Google Authenticator…

use data_encoding::BASE32_NOPAD;
use hmac::{Hmac, Mac};
use phonalys_crypto::constant_time_eq;
use rand_core::{OsRng, RngCore};
use sha1::Sha1;
use zeroize::{Zeroize, ZeroizeOnDrop};

const STEP_SECS: u64 = 30;
const DIGITS: u32 = 6;
/// Tolérance de dérive d'horloge : un pas avant et après.
const WINDOW: i64 = 1;

#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct Totp {
    secret: Vec<u8>,
}

impl std::fmt::Debug for Totp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Totp(<masqué>)")
    }
}

impl Totp {
    /// Nouveau secret de 160 bits (taille recommandée par la RFC 4226).
    pub fn generate() -> Self {
        let mut secret = vec![0u8; 20];
        OsRng.fill_bytes(&mut secret);
        Self { secret }
    }

    pub fn from_secret(secret: Vec<u8>) -> Self {
        Self { secret }
    }

    pub fn secret(&self) -> &[u8] {
        &self.secret
    }

    /// Secret en base32, pour une saisie manuelle dans l'application d'authentification.
    pub fn secret_base32(&self) -> String {
        BASE32_NOPAD.encode(&self.secret)
    }

    /// URI `otpauth://` à afficher en QR code.
    pub fn provisioning_uri(&self, issuer: &str, account: &str) -> String {
        let enc = |s: &str| {
            s.bytes()
                .map(|b| match b {
                    b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                        (b as char).to_string()
                    }
                    _ => format!("%{b:02X}"),
                })
                .collect::<String>()
        };
        format!(
            "otpauth://totp/{}:{}?secret={}&issuer={}&algorithm=SHA1&digits={DIGITS}&period={STEP_SECS}",
            enc(issuer),
            enc(account),
            self.secret_base32(),
            enc(issuer)
        )
    }

    /// Code au pas `counter`.
    fn code_at_step(&self, counter: u64) -> u32 {
        let Ok(mut mac) = <Hmac<Sha1> as Mac>::new_from_slice(&self.secret) else {
            return u32::MAX; // HMAC accepte toute longueur de clé : branche inatteignable.
        };
        mac.update(&counter.to_be_bytes());
        let digest = mac.finalize().into_bytes();
        let offset = (digest[19] & 0x0f) as usize;
        let binary = u32::from_be_bytes([
            digest[offset] & 0x7f,
            digest[offset + 1],
            digest[offset + 2],
            digest[offset + 3],
        ]);
        binary % 10u32.pow(DIGITS)
    }

    /// Code courant, formaté sur 6 chiffres.
    pub fn code_at(&self, unix_secs: u64) -> String {
        format!(
            "{:0width$}",
            self.code_at_step(unix_secs / STEP_SECS),
            width = DIGITS as usize
        )
    }

    /// Vérifie un code. Renvoie le pas utilisé : l'appelant doit refuser tout pas
    /// inférieur ou égal au dernier accepté (protection contre le rejeu).
    pub fn verify(&self, code: &str, unix_secs: u64, last_used_step: Option<u64>) -> Option<u64> {
        let code: String = code.chars().filter(|c| !c.is_whitespace()).collect();
        if code.len() != DIGITS as usize || !code.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let current = (unix_secs / STEP_SECS) as i64;
        let mut matched = None;
        // Toutes les fenêtres sont évaluées, pour un temps d'exécution constant.
        for delta in -WINDOW..=WINDOW {
            let Ok(step) = u64::try_from(current + delta) else {
                continue;
            };
            let expected = format!(
                "{:0width$}",
                self.code_at_step(step),
                width = DIGITS as usize
            );
            if constant_time_eq(expected.as_bytes(), code.as_bytes()) && matched.is_none() {
                matched = Some(step);
            }
        }
        matched.filter(|step| last_used_step.is_none_or(|last| *step > last))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rfc() -> Totp {
        Totp::from_secret(b"12345678901234567890".to_vec())
    }

    #[test]
    fn rfc6238_vectors_sha1_truncated_to_6_digits() {
        // Annexe B de la RFC 6238 (8 chiffres) : on garde les 6 derniers.
        for (t, code) in [
            (59, "287082"),
            (1_111_111_109, "081804"),
            (1_111_111_111, "050471"),
            (1_234_567_890, "005924"),
            (2_000_000_000, "279037"),
            (20_000_000_000, "353130"),
        ] {
            assert_eq!(rfc().code_at(t), code, "t={t}");
        }
    }

    #[test]
    fn accepts_adjacent_window_rejects_far() {
        let t = 1_111_111_109;
        let code = rfc().code_at(t);
        assert!(rfc().verify(&code, t + 30, None).is_some());
        assert!(rfc().verify(&code, t - 30, None).is_some());
        assert!(rfc().verify(&code, t + 90, None).is_none());
    }

    #[test]
    fn replay_is_rejected() {
        let t = 1_111_111_109;
        let code = rfc().code_at(t);
        let step = rfc().verify(&code, t, None).unwrap();
        assert!(rfc().verify(&code, t, Some(step)).is_none());
    }

    #[test]
    fn malformed_codes_are_rejected() {
        for bad in ["", "12345", "1234567", "12a456", "１２３４５６"] {
            assert!(rfc().verify(bad, 59, None).is_none(), "{bad}");
        }
        assert!(rfc().verify("287 082", 59, None).is_some());
    }

    #[test]
    fn provisioning_uri_is_escaped() {
        let uri = rfc().provisioning_uri("Phonalys", "cabinet de Léa");
        assert!(uri.starts_with("otpauth://totp/Phonalys:cabinet%20de%20L%C3%A9a?secret=GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ"));
    }

    #[test]
    fn debug_hides_secret() {
        assert_eq!(format!("{:?}", rfc()), "Totp(<masqué>)");
    }
}
