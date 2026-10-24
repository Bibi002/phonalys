//! Mode « lié au matériel » : phrase de passe + facteur physique.

#![allow(clippy::unwrap_used)]

use phonalys_crypto::KdfParams;
use phonalys_vault::{FactorKind, LockedVault, UnlockedVault, VaultError};
use rusqlite::Connection;
use std::path::{Path, PathBuf};

const PASSWORD: &[u8] = b"cheval agrafe batterie correcte";
const PRF_PHONE: &[u8] = b"sortie-prf-du-telephone-32-octets";
const PRF_KEY: &[u8] = b"sortie-prf-de-la-yubikey-32octets";
const CODE_A: &[u8] = b"AAAABBBBCCCCDDDD";
const CODE_B: &[u8] = b"EEEEFFFFGGGGHHHH";

fn params() -> KdfParams {
    KdfParams::insecure_for_tests()
}

fn locked(path: &Path) -> LockedVault {
    LockedVault::open(path).unwrap()
}

/// Coffre lié : deux clés biométriques et deux codes de récupération.
fn bound_vault() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("cabinet.phonalys-vault");
    let mut vault = LockedVault::create(&path, PASSWORD, params()).unwrap();
    vault.put("patient", "p1", b"dossier").unwrap();
    let pk = vault.password_key(PASSWORD).unwrap();
    vault
        .add_factor(&pk, FactorKind::Passkey, "telephone", PRF_PHONE)
        .unwrap();
    vault
        .add_factor(&pk, FactorKind::Passkey, "yubikey", PRF_KEY)
        .unwrap();
    vault
        .replace_recovery_factors(&pk, &[CODE_A.to_vec(), CODE_B.to_vec()])
        .unwrap();
    vault.set_hardware_bound(&pk, true).unwrap();
    drop(vault);
    (dir, path)
}

fn unlock_factor(
    path: &Path,
    password: &[u8],
    id: &str,
    prf: &[u8],
    now: i64,
) -> Result<UnlockedVault, VaultError> {
    let v = locked(path);
    let pk = v.password_key_at(password, now).unwrap();
    v.unlock_with_factor_at(&pk, FactorKind::Passkey, id, prf, now)
        .map_err(|(_, e)| e)
}

fn unlock_recovery(path: &Path, code: &[u8], now: i64) -> Result<UnlockedVault, VaultError> {
    let v = locked(path);
    let pk = v.password_key_at(PASSWORD, now).unwrap();
    v.unlock_with_recovery_at(&pk, code, now)
        .map_err(|(_, e)| e)
}

#[test]
fn passphrase_alone_no_longer_opens_a_bound_vault() {
    let (_dir, path) = bound_vault();
    let v = locked(&path);
    assert!(v.is_hardware_bound().unwrap());
    assert!(matches!(
        v.unlock_at(PASSWORD, 1_000),
        Err((_, VaultError::HardwareBound))
    ));
}

#[test]
fn each_bound_device_opens_the_vault() {
    let (_dir, path) = bound_vault();
    for (id, prf) in [("telephone", PRF_PHONE), ("yubikey", PRF_KEY)] {
        let vault = unlock_factor(&path, PASSWORD, id, prf, 1_000).unwrap();
        assert_eq!(vault.get("patient", "p1").unwrap().as_slice(), b"dossier");
    }
}

#[test]
fn wrong_device_secret_or_password_is_rejected_and_counted() {
    let (_dir, path) = bound_vault();
    // Bon appareil, mauvaise sortie PRF (un autre appareil, ou un faux).
    assert!(matches!(
        unlock_factor(&path, PASSWORD, "telephone", PRF_KEY, 1_000),
        Err(VaultError::FactorRejected)
    ));
    // Bonne sortie PRF, mauvaise phrase de passe.
    assert!(matches!(
        unlock_factor(&path, b"mauvaise", "telephone", PRF_PHONE, 1_000),
        Err(VaultError::FactorRejected)
    ));
    // Appareil inconnu.
    assert!(matches!(
        unlock_factor(&path, PASSWORD, "inconnu", PRF_PHONE, 1_000),
        Err(VaultError::FactorRejected)
    ));
    // Ces 3 échecs + 1 : le délai s'applique.
    let _ = unlock_factor(&path, PASSWORD, "inconnu", PRF_PHONE, 1_000);
    let v = locked(&path);
    assert!(matches!(
        v.password_key_at(PASSWORD, 1_000),
        Err(VaultError::TooManyAttempts(_))
    ));
}

#[test]
fn recovery_codes_are_cryptographic_and_single_use() {
    let (_dir, path) = bound_vault();
    assert!(matches!(
        unlock_recovery(&path, b"MAUVAISCODE00000", 1_000),
        Err(VaultError::FactorRejected)
    ));
    let vault = unlock_recovery(&path, CODE_A, 1_000).unwrap();
    assert_eq!(vault.factor_ids(FactorKind::Recovery).unwrap().len(), 1);
    vault.reset_lockout().unwrap();
    drop(vault);
    assert!(
        matches!(
            unlock_recovery(&path, CODE_A, 1_000),
            Err(VaultError::FactorRejected)
        ),
        "code déjà consommé"
    );
    assert!(unlock_recovery(&path, CODE_B, 1_000).is_ok());
}

#[test]
fn password_change_keeps_devices_working_without_them() {
    let (_dir, path) = bound_vault();
    let mut vault = unlock_factor(&path, PASSWORD, "telephone", PRF_PHONE, 1_000).unwrap();
    let old = vault.password_key(PASSWORD).unwrap();
    vault
        .change_password(&old, b"nouvelle phrase tres longue", params())
        .unwrap();
    drop(vault);

    assert!(matches!(
        unlock_factor(&path, PASSWORD, "yubikey", PRF_KEY, 2_000),
        Err(VaultError::FactorRejected)
    ));
    let vault = unlock_factor(
        &path,
        b"nouvelle phrase tres longue",
        "yubikey",
        PRF_KEY,
        2_000,
    )
    .unwrap();
    vault.reset_lockout().unwrap();
    drop(vault);
    assert!(
        locked(&path).is_hardware_bound().unwrap(),
        "le mode lié est conservé"
    );
}

#[test]
fn binding_requires_device_and_recovery_and_last_key_is_protected() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("v");
    let mut vault = LockedVault::create(&path, PASSWORD, params()).unwrap();
    let pk = vault.password_key(PASSWORD).unwrap();
    assert!(matches!(
        vault.set_hardware_bound(&pk, true),
        Err(VaultError::Precondition(_))
    ));
    vault
        .add_factor(&pk, FactorKind::Passkey, "k1", PRF_PHONE)
        .unwrap();
    assert!(
        matches!(
            vault.set_hardware_bound(&pk, true),
            Err(VaultError::Precondition(_))
        ),
        "codes requis"
    );
    vault
        .replace_recovery_factors(&pk, &[CODE_A.to_vec()])
        .unwrap();
    vault.set_hardware_bound(&pk, true).unwrap();
    assert!(matches!(
        vault.remove_factor(FactorKind::Passkey, "k1"),
        Err(VaultError::Precondition(_))
    ));

    // Désactiver le mode lié rétablit l'ouverture par phrase de passe seule.
    vault.set_hardware_bound(&pk, false).unwrap();
    vault.remove_factor(FactorKind::Passkey, "k1").unwrap();
    drop(vault);
    assert!(locked(&path).unlock_at(PASSWORD, 1_000).is_ok());
}

#[test]
fn factor_secrets_never_reach_the_disk() {
    let (_dir, path) = bound_vault();
    let mut bytes = std::fs::read(&path).unwrap();
    if let Ok(wal) = std::fs::read(path.with_extension("phonalys-vault-wal")) {
        bytes.extend(wal);
    }
    for needle in [PRF_PHONE, PRF_KEY, CODE_A, CODE_B] {
        assert!(!bytes.windows(needle.len()).any(|w| w == needle));
    }
}

#[test]
fn format_1_vault_is_migrated() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ancien");
    let vault = LockedVault::create(&path, PASSWORD, params()).unwrap();
    vault.put("patient", "p1", b"ancien dossier").unwrap();
    drop(vault);
    // Ramène le fichier au format de la phase 2.
    let conn = Connection::open(&path).unwrap();
    conn.execute_batch("ALTER TABLE meta DROP COLUMN prf_salt; DROP TABLE factor_wraps; UPDATE meta SET format = 1;")
        .unwrap();
    drop(conn);

    let v = locked(&path);
    assert_eq!(v.prf_salt().unwrap().len(), 32);
    let vault = v.unlock_at(PASSWORD, 1_000).map_err(|(_, e)| e).unwrap();
    assert_eq!(
        vault.get("patient", "p1").unwrap().as_slice(),
        b"ancien dossier"
    );
}

// --- Connexion en un geste ------------------------------------------------------------

fn quick(path: &Path, id: &str, prf: &[u8], now: i64) -> Result<UnlockedVault, VaultError> {
    locked(path)
        .unlock_with_quick_at(id, prf, now)
        .map_err(|(_, e)| e)
}

#[test]
fn quick_unlock_opens_with_the_device_alone_and_survives_a_password_change() {
    let (_dir, path) = bound_vault();
    let mut v = unlock_factor(&path, PASSWORD, "telephone", PRF_PHONE, 0).unwrap();
    let pk = v.password_key(PASSWORD).unwrap();
    v.add_quick_factor(&pk, "telephone", PRF_PHONE).unwrap();
    v.change_password(&pk, b"nouvelle phrase tout aussi longue", params())
        .unwrap();
    drop(v);

    let v = quick(&path, "telephone", PRF_PHONE, 10).unwrap();
    assert_eq!(&v.get("patient", "p1").unwrap()[..], b"dossier");
    drop(v);
    // Une clé sans enveloppe « en un geste » ou une mauvaise sortie PRF : refus.
    assert!(matches!(
        quick(&path, "yubikey", PRF_KEY, 20),
        Err(VaultError::FactorRejected)
    ));
    assert!(matches!(
        quick(&path, "telephone", PRF_KEY, 20),
        Err(VaultError::FactorRejected)
    ));
}

#[test]
fn quick_unlock_requires_the_passphrase_to_be_enabled() {
    let (_dir, path) = bound_vault();
    let v = unlock_factor(&path, PASSWORD, "telephone", PRF_PHONE, 0).unwrap();
    assert!(v.password_key(b"mauvaise phrase de passe").is_err());
    assert!(v.factor_ids(FactorKind::Quick).unwrap().is_empty());
}

#[test]
fn three_failures_require_two_factors_again() {
    let (_dir, path) = bound_vault();
    let v = unlock_factor(&path, PASSWORD, "telephone", PRF_PHONE, 0).unwrap();
    v.add_quick_factor(&v.password_key(PASSWORD).unwrap(), "telephone", PRF_PHONE)
        .unwrap();
    drop(v);

    for _ in 0..3 {
        assert!(matches!(
            quick(&path, "telephone", b"fausse sortie", 10),
            Err(VaultError::FactorRejected)
        ));
    }
    assert!(!locked(&path).quick_unlock_allowed().unwrap());
    // Même la bonne clé seule ne suffit plus…
    assert!(matches!(
        quick(&path, "telephone", PRF_PHONE, 10),
        Err(VaultError::StepUpRequired)
    ));
    // …il faut la phrase de passe et un second facteur, qui rétablissent le raccourci.
    let v = unlock_factor(&path, PASSWORD, "telephone", PRF_PHONE, 10).unwrap();
    v.reset_lockout().unwrap();
    drop(v);
    assert!(quick(&path, "telephone", PRF_PHONE, 20).is_ok());
}
