//! Codes de récupération à usage unique, à imprimer et ranger hors ligne.
//! Seules leurs empreintes SHA-256 sont conservées (80 bits d'entropie par code).

use data_encoding::BASE32_NOPAD;
use phonalys_crypto::constant_time_eq;
use rand_core::{OsRng, RngCore};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const COUNT: usize = 10;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecoveryCodes {
    hashes: Vec<[u8; 32]>,
}

impl RecoveryCodes {
    /// Génère les codes : renvoie l'ensemble d'empreintes à stocker et les codes en
    /// clair, à n'afficher qu'une seule fois.
    pub fn generate() -> (Self, Vec<String>) {
        let codes: Vec<String> = (0..COUNT)
            .map(|_| {
                let mut raw = [0u8; 10];
                OsRng.fill_bytes(&mut raw);
                let s = BASE32_NOPAD.encode(&raw);
                format!("{}-{}-{}-{}", &s[0..4], &s[4..8], &s[8..12], &s[12..16])
            })
            .collect();
        let hashes = codes.iter().map(|c| digest(c)).collect();
        (Self { hashes }, codes)
    }

    pub fn remaining(&self) -> usize {
        self.hashes.len()
    }

    /// Consomme un code s'il est valide.
    pub fn redeem(&mut self, code: &str) -> bool {
        let candidate = digest(code);
        let mut found = None;
        for (i, h) in self.hashes.iter().enumerate() {
            if constant_time_eq(h, &candidate) && found.is_none() {
                found = Some(i);
            }
        }
        found.map(|i| self.hashes.swap_remove(i)).is_some()
    }
}

/// Empreinte d'un code normalisé (casse, tirets et espaces ignorés).
fn digest(code: &str) -> [u8; 32] {
    Sha256::digest(normalize(code).as_bytes()).into()
}

/// Forme canonique d'un code saisi : casse, tirets et espaces ignorés. C'est cette forme
/// qui sert de secret pour les enveloppes de récupération du coffre.
pub fn normalize(code: &str) -> String {
    code.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_uppercase())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_are_single_use_and_normalized() {
        let (mut set, codes) = RecoveryCodes::generate();
        assert_eq!(codes.len(), COUNT);
        assert_eq!(codes[0].len(), 19);
        assert!(set.redeem(&codes[3].to_lowercase().replace('-', " ")));
        assert!(!set.redeem(&codes[3]), "un code ne sert qu'une fois");
        assert_eq!(set.remaining(), COUNT - 1);
        assert!(!set.redeem("AAAA-BBBB-CCCC-DDDD"));
    }

    #[test]
    fn codes_are_unique() {
        let (_, codes) = RecoveryCodes::generate();
        let unique: std::collections::HashSet<_> = codes.iter().collect();
        assert_eq!(unique.len(), COUNT);
    }
}
