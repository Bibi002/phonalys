//! Primitives cryptographiques de Phonalys.
//!
//! Hiérarchie de clés (chiffrement en enveloppe) :
//!
//! ```text
//! mot de passe + sel ──Argon2id──► KEK (clé de chiffrement de clé, jamais stockée)
//! KEK ──scelle──► clé maîtresse (stockée chiffrée)
//! clé maîtresse ──scelle──► clé de données par dossier patient (stockée chiffrée)
//! clé de données ──scelle──► contenu (résultats, notes, documents)
//! ```
//!
//! Chaque blob est chiffré avec XChaCha20-Poly1305 (AEAD, nonce aléatoire de 192 bits)
//! et lié par des données associées (AAD) à son identifiant : un blob déplacé vers un
//! autre dossier ne se déchiffre pas.

use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use rand_core::{OsRng, RngCore};
use subtle::ConstantTimeEq;
use zeroize::{Zeroize, ZeroizeOnDrop};

/// Taille des clés symétriques, en octets.
pub const KEY_LEN: usize = 32;
/// Taille des sels Argon2id, en octets.
pub const SALT_LEN: usize = 16;
const NONCE_LEN: usize = 24;
const FORMAT_V1: u8 = 1;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum CryptoError {
    #[error("paramètres de dérivation invalides")]
    InvalidParams,
    #[error("échec de la dérivation de clé")]
    Kdf,
    #[error("blob chiffré mal formé")]
    Malformed,
    #[error("version de format non prise en charge : {0}")]
    UnsupportedVersion(u8),
    #[error("échec de l'authentification : clé incorrecte ou données altérées")]
    Authentication,
    #[error("échec du hachage du mot de passe")]
    PasswordHash,
}

/// Clé symétrique de 256 bits, effacée de la mémoire à sa destruction.
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct SecretKey([u8; KEY_LEN]);

impl SecretKey {
    /// Génère une clé aléatoire depuis le générateur du système d'exploitation.
    pub fn generate() -> Self {
        let mut bytes = [0u8; KEY_LEN];
        OsRng.fill_bytes(&mut bytes);
        Self(bytes)
    }

    pub fn from_bytes(bytes: [u8; KEY_LEN]) -> Self {
        Self(bytes)
    }

    pub fn as_bytes(&self) -> &[u8; KEY_LEN] {
        &self.0
    }
}

impl PartialEq for SecretKey {
    fn eq(&self, other: &Self) -> bool {
        self.0.ct_eq(&other.0).into()
    }
}

impl Eq for SecretKey {}

impl std::fmt::Debug for SecretKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SecretKey(<masquée>)")
    }
}

/// Paramètres Argon2id. Les valeurs par défaut suivent la recommandation OWASP
/// renforcée : 256 MiB, 3 passes, 4 voies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KdfParams {
    pub memory_kib: u32,
    pub iterations: u32,
    pub parallelism: u32,
}

impl Default for KdfParams {
    fn default() -> Self {
        Self {
            memory_kib: 256 * 1024,
            iterations: 3,
            parallelism: 4,
        }
    }
}

impl KdfParams {
    /// Paramètres volontairement faibles, réservés aux tests.
    #[doc(hidden)]
    pub fn insecure_for_tests() -> Self {
        Self {
            memory_kib: 8 * 1024,
            iterations: 1,
            parallelism: 1,
        }
    }

    fn argon2(&self, output_len: Option<usize>) -> Result<Argon2<'static>, CryptoError> {
        let params = Params::new(
            self.memory_kib,
            self.iterations,
            self.parallelism,
            output_len,
        )
        .map_err(|_| CryptoError::InvalidParams)?;
        Ok(Argon2::new(Algorithm::Argon2id, Version::V0x13, params))
    }
}

/// Génère un sel aléatoire.
pub fn generate_salt() -> [u8; SALT_LEN] {
    let mut salt = [0u8; SALT_LEN];
    OsRng.fill_bytes(&mut salt);
    salt
}

/// Dérive une clé de chiffrement de clé (KEK) depuis un mot de passe.
pub fn derive_kek(
    password: &[u8],
    salt: &[u8; SALT_LEN],
    params: KdfParams,
) -> Result<SecretKey, CryptoError> {
    let mut out = [0u8; KEY_LEN];
    params
        .argon2(Some(KEY_LEN))?
        .hash_password_into(password, salt, &mut out)
        .map_err(|_| CryptoError::Kdf)?;
    Ok(SecretKey(out))
}

/// Chiffre `plaintext` en le liant à `aad`.
///
/// Format : `version (1) || nonce (24) || texte chiffré + tag (16)`.
pub fn seal(key: &SecretKey, plaintext: &[u8], aad: &[u8]) -> Result<Vec<u8>, CryptoError> {
    let cipher = XChaCha20Poly1305::new(key.as_bytes().into());
    let mut nonce = [0u8; NONCE_LEN];
    OsRng.fill_bytes(&mut nonce);
    let ciphertext = cipher
        .encrypt(
            XNonce::from_slice(&nonce),
            Payload {
                msg: plaintext,
                aad,
            },
        )
        .map_err(|_| CryptoError::Authentication)?;

    let mut out = Vec::with_capacity(1 + NONCE_LEN + ciphertext.len());
    out.push(FORMAT_V1);
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&ciphertext);
    Ok(out)
}

/// Déchiffre un blob produit par [`seal`] ; échoue si la clé, l'AAD ou le blob diffèrent.
pub fn open(key: &SecretKey, blob: &[u8], aad: &[u8]) -> Result<Vec<u8>, CryptoError> {
    let (&version, rest) = blob.split_first().ok_or(CryptoError::Malformed)?;
    if version != FORMAT_V1 {
        return Err(CryptoError::UnsupportedVersion(version));
    }
    if rest.len() < NONCE_LEN + 16 {
        return Err(CryptoError::Malformed);
    }
    let (nonce, ciphertext) = rest.split_at(NONCE_LEN);
    XChaCha20Poly1305::new(key.as_bytes().into())
        .decrypt(
            XNonce::from_slice(nonce),
            Payload {
                msg: ciphertext,
                aad,
            },
        )
        .map_err(|_| CryptoError::Authentication)
}

/// Scelle une clé avec une autre (enveloppe). `context` identifie l'usage de la clé.
pub fn wrap_key(
    wrapping: &SecretKey,
    key: &SecretKey,
    context: &[u8],
) -> Result<Vec<u8>, CryptoError> {
    seal(wrapping, key.as_bytes(), context)
}

/// Ouvre une clé scellée par [`wrap_key`].
pub fn unwrap_key(
    wrapping: &SecretKey,
    wrapped: &[u8],
    context: &[u8],
) -> Result<SecretKey, CryptoError> {
    let mut bytes = open(wrapping, wrapped, context)?;
    let result = <[u8; KEY_LEN]>::try_from(bytes.as_slice())
        .map(SecretKey)
        .map_err(|_| CryptoError::Malformed);
    bytes.zeroize();
    result
}

/// Hache un mot de passe de connexion (format PHC, Argon2id, sel aléatoire).
pub fn hash_password(password: &[u8], params: KdfParams) -> Result<String, CryptoError> {
    use argon2::password_hash::{PasswordHasher, SaltString};
    let salt = SaltString::generate(&mut OsRng);
    params
        .argon2(None)?
        .hash_password(password, &salt)
        .map(|h| h.to_string())
        .map_err(|_| CryptoError::PasswordHash)
}

/// Vérifie un mot de passe contre un hash PHC. Les paramètres sont lus dans le hash.
pub fn verify_password(password: &[u8], phc: &str) -> bool {
    use argon2::password_hash::{PasswordHash, PasswordVerifier};
    match PasswordHash::new(phc) {
        Ok(parsed) => Argon2::default().verify_password(password, &parsed).is_ok(),
        Err(_) => false,
    }
}

/// Comparaison en temps constant de deux secrets.
pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    a.ct_eq(b).into()
}

/// Dérive une clé de 256 bits d'un secret à haute entropie (sortie PRF d'un
/// authentificateur, code de récupération…) avec HKDF-SHA256. `info` sépare les usages.
/// Ne convient pas aux mots de passe : utiliser [`derive_kek`].
pub fn derive_subkey(ikm: &[u8], info: &[u8]) -> SecretKey {
    let hk = hkdf::Hkdf::<sha2::Sha256>::new(Some(b"phonalys:v1:hkdf"), ikm);
    let mut out = [0u8; KEY_LEN];
    // 32 octets ≤ 255 × 32 : l'expansion ne peut pas échouer.
    if hk.expand(info, &mut out).is_err() {
        out.zeroize();
    }
    SecretKey(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn seal_open_roundtrip() {
        let key = SecretKey::generate();
        let blob = seal(&key, b"score brut: 42", b"patient:1").unwrap();
        assert_eq!(open(&key, &blob, b"patient:1").unwrap(), b"score brut: 42");
    }

    #[test]
    fn wrong_key_is_rejected() {
        let blob = seal(&SecretKey::generate(), b"x", b"").unwrap();
        assert_eq!(
            open(&SecretKey::generate(), &blob, b""),
            Err(CryptoError::Authentication)
        );
    }

    #[test]
    fn moved_blob_is_rejected() {
        let key = SecretKey::generate();
        let blob = seal(&key, b"x", b"patient:1").unwrap();
        assert_eq!(
            open(&key, &blob, b"patient:2"),
            Err(CryptoError::Authentication)
        );
    }

    #[test]
    fn nonces_are_unique() {
        let key = SecretKey::generate();
        assert_ne!(
            seal(&key, b"x", b"").unwrap(),
            seal(&key, b"x", b"").unwrap()
        );
    }

    #[test]
    fn malformed_and_unknown_versions_are_rejected() {
        let key = SecretKey::generate();
        assert_eq!(open(&key, &[], b""), Err(CryptoError::Malformed));
        assert_eq!(open(&key, &[1, 2, 3], b""), Err(CryptoError::Malformed));
        let mut blob = seal(&key, b"x", b"").unwrap();
        blob[0] = 9;
        assert_eq!(
            open(&key, &blob, b""),
            Err(CryptoError::UnsupportedVersion(9))
        );
    }

    #[test]
    fn key_hierarchy_roundtrip() {
        let salt = generate_salt();
        let kek = derive_kek(
            b"phrase de passe longue",
            &salt,
            KdfParams::insecure_for_tests(),
        )
        .unwrap();
        let master = SecretKey::generate();
        let wrapped = wrap_key(&kek, &master, b"master").unwrap();

        let kek_again = derive_kek(
            b"phrase de passe longue",
            &salt,
            KdfParams::insecure_for_tests(),
        )
        .unwrap();
        assert_eq!(unwrap_key(&kek_again, &wrapped, b"master").unwrap(), master);

        let bad_kek =
            derive_kek(b"mauvaise phrase", &salt, KdfParams::insecure_for_tests()).unwrap();
        assert!(unwrap_key(&bad_kek, &wrapped, b"master").is_err());
        assert!(unwrap_key(&kek, &wrapped, b"autre-usage").is_err());
    }

    #[test]
    fn password_hash_verifies() {
        let phc = hash_password(b"correct horse", KdfParams::insecure_for_tests()).unwrap();
        assert!(phc.starts_with("$argon2id$"));
        assert!(verify_password(b"correct horse", &phc));
        assert!(!verify_password(b"wrong", &phc));
        assert!(!verify_password(b"correct horse", "pas un hash"));
    }

    #[test]
    fn subkeys_are_deterministic_and_separated() {
        assert_eq!(derive_subkey(b"prf", b"a"), derive_subkey(b"prf", b"a"));
        assert_ne!(derive_subkey(b"prf", b"a"), derive_subkey(b"prf", b"b"));
        assert_ne!(derive_subkey(b"prf1", b"a"), derive_subkey(b"prf2", b"a"));
    }

    #[test]
    fn debug_never_leaks_key() {
        assert_eq!(
            format!("{:?}", SecretKey::from_bytes([7; KEY_LEN])),
            "SecretKey(<masquée>)"
        );
    }

    proptest! {
        #[test]
        fn roundtrip_any_payload(data in proptest::collection::vec(any::<u8>(), 0..4096),
                                 aad in proptest::collection::vec(any::<u8>(), 0..64)) {
            let key = SecretKey::generate();
            let blob = seal(&key, &data, &aad).unwrap();
            prop_assert_eq!(open(&key, &blob, &aad).unwrap(), data);
        }

        #[test]
        fn any_bit_flip_is_detected(data in proptest::collection::vec(any::<u8>(), 1..256),
                                    pos in any::<prop::sample::Index>(), bit in 0u8..8) {
            let key = SecretKey::generate();
            let mut blob = seal(&key, &data, b"").unwrap();
            let i = 1 + pos.index(blob.len() - 1);
            blob[i] ^= 1 << bit;
            prop_assert!(open(&key, &blob, b"").is_err());
        }
    }
}
