#![allow(clippy::unwrap_used)]

use phonalys_core::audit::AuditAction;
use phonalys_crypto::KdfParams;
use phonalys_vault::{lockout_delay, LockedVault, UnlockedVault, VaultError};
use rusqlite::Connection;
use std::path::{Path, PathBuf};

const PASSWORD: &[u8] = b"cheval agrafe batterie correcte";

fn params() -> KdfParams {
    KdfParams::insecure_for_tests()
}

fn new_vault() -> (tempfile::TempDir, PathBuf, UnlockedVault) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("cabinet.phonalys-vault");
    let vault = LockedVault::create(&path, PASSWORD, params()).unwrap();
    (dir, path, vault)
}

fn reopen(path: &Path, password: &[u8]) -> Result<UnlockedVault, VaultError> {
    LockedVault::open(path)
        .unwrap()
        .unlock(password)
        .map_err(|(_, e)| e)
}

#[test]
fn records_roundtrip_across_lock_and_reopen() {
    let (_dir, path, vault) = new_vault();
    let id = UnlockedVault::new_id();
    vault
        .put("patient", &id, b"Lea Dupont, nee le 15/03/2018")
        .unwrap();
    drop(vault.lock().unwrap());

    let vault = reopen(&path, PASSWORD).unwrap();
    assert_eq!(
        vault.get("patient", &id).unwrap().as_slice(),
        b"Lea Dupont, nee le 15/03/2018"
    );
    assert_eq!(vault.list("patient").unwrap(), vec![id]);
}

#[test]
fn nothing_sensitive_is_stored_in_clear() {
    let (_dir, path, vault) = new_vault();
    vault.put("patient", "p1", b"SECRET-DUPONT-LEA").unwrap();
    vault.put_secret("totp", b"SECRET-TOTP-SEED").unwrap();
    drop(vault);

    // Inclut le fichier principal et le journal WAL.
    let mut bytes = std::fs::read(&path).unwrap();
    if let Ok(wal) = std::fs::read(path.with_extension("phonalys-vault-wal")) {
        bytes.extend(wal);
    }
    for needle in [&b"SECRET-DUPONT-LEA"[..], b"SECRET-TOTP-SEED", PASSWORD] {
        assert!(
            !bytes.windows(needle.len()).any(|w| w == needle),
            "trouvé en clair : {:?}",
            String::from_utf8_lossy(needle)
        );
    }
}

#[test]
fn wrong_password_is_rejected_and_delayed() {
    let (_dir, path, vault) = new_vault();
    drop(vault);

    let mut locked = LockedVault::open(&path).unwrap();
    for _ in 0..3 {
        let (back, err) = locked.unlock_at(b"mauvais", 1_000).unwrap_err();
        assert!(matches!(err, VaultError::WrongPassword));
        locked = back;
    }
    // 4e échec : un délai est imposé.
    let (back, _) = locked.unlock_at(b"mauvais", 1_000).unwrap_err();
    let (back, err) = back.unlock_at(PASSWORD, 1_000).unwrap_err();
    assert!(matches!(err, VaultError::TooManyAttempts(1)), "{err:?}");
    // Une fois le délai écoulé, le bon mot de passe fonctionne.
    let vault = back.unlock_at(PASSWORD, 1_001).unwrap();

    // Un second facteur faux prolonge le délai, même avec le bon mot de passe.
    vault.register_failure(1_001).unwrap();
    let (back, err) = vault
        .lock()
        .unwrap()
        .unlock_at(PASSWORD, 1_002)
        .unwrap_err();
    assert!(matches!(err, VaultError::TooManyAttempts(1)), "{err:?}");

    // Seule une authentification complète remet le compteur à zéro.
    let vault = back.unlock_at(PASSWORD, 1_003).unwrap();
    vault.reset_lockout().unwrap();
    let (back, _) = vault
        .lock()
        .unwrap()
        .unlock_at(b"mauvais", 2_000)
        .unwrap_err();
    assert!(
        back.unlock_at(PASSWORD, 2_000).is_ok(),
        "1er échec après remise à zéro : pas de délai"
    );
}

#[test]
fn lockout_delay_grows_and_is_capped() {
    assert_eq!(
        (0..=3).map(lockout_delay).collect::<Vec<_>>(),
        vec![0, 0, 0, 0]
    );
    assert_eq!(lockout_delay(4), 1);
    assert_eq!(lockout_delay(5), 2);
    assert_eq!(lockout_delay(10), 64);
    assert_eq!(lockout_delay(1_000), 15 * 60);
}

#[test]
fn swapped_ciphertexts_are_detected() {
    let (_dir, path, vault) = new_vault();
    vault.put("patient", "a", b"dossier A").unwrap();
    vault.put("patient", "b", b"dossier B").unwrap();
    drop(vault);

    // Un attaquant échange le contenu chiffré de deux dossiers directement dans SQLite.
    let conn = Connection::open(&path).unwrap();
    conn.execute_batch(
        "UPDATE records SET id = 'tmp' WHERE id = 'a';
         UPDATE records SET id = 'a' WHERE id = 'b';
         UPDATE records SET id = 'b' WHERE id = 'tmp';",
    )
    .unwrap();
    drop(conn);

    let vault = reopen(&path, PASSWORD).unwrap();
    assert!(matches!(
        vault.get("patient", "a"),
        Err(VaultError::Integrity(_))
    ));
}

#[test]
fn tampered_audit_blocks_unlock() {
    let (_dir, path, mut vault) = new_vault();
    vault
        .audit("lp", AuditAction::PatientViewed, Some("p1"))
        .unwrap();
    drop(vault);

    let conn = Connection::open(&path).unwrap();
    conn.execute("DELETE FROM audit WHERE seq = 0", []).unwrap();
    drop(conn);

    assert!(matches!(
        reopen(&path, PASSWORD),
        Err(VaultError::Integrity(_))
    ));
}

#[test]
fn audit_persists_across_sessions() {
    let (_dir, path, mut vault) = new_vault();
    vault.audit("lp", AuditAction::Login, None).unwrap();
    drop(vault);
    let mut vault = reopen(&path, PASSWORD).unwrap();
    vault.audit("lp", AuditAction::Lock, None).unwrap();
    let actions: Vec<_> = vault.audit_entries().iter().map(|e| e.action).collect();
    assert_eq!(
        actions,
        vec![
            AuditAction::VaultCreated,
            AuditAction::Login,
            AuditAction::Lock
        ]
    );
}

#[test]
fn change_password_keeps_data() {
    let (_dir, path, mut vault) = new_vault();
    vault.put("note", "n1", b"observation").unwrap();
    assert!(matches!(
        vault.password_key(b"faux"),
        Err(VaultError::WrongPassword)
    ));
    let old = vault.password_key(PASSWORD).unwrap();
    vault
        .change_password(&old, b"nouvelle phrase de passe tres longue", params())
        .unwrap();
    drop(vault);

    assert!(matches!(
        reopen(&path, PASSWORD),
        Err(VaultError::WrongPassword)
    ));
    let vault = reopen(&path, b"nouvelle phrase de passe tres longue").unwrap();
    assert_eq!(vault.get("note", "n1").unwrap().as_slice(), b"observation");
}

#[test]
fn create_refuses_to_overwrite_and_open_requires_file() {
    let (_dir, path, vault) = new_vault();
    drop(vault);
    assert!(matches!(
        LockedVault::create(&path, PASSWORD, params()),
        Err(VaultError::AlreadyExists)
    ));
    assert!(matches!(
        LockedVault::open(&path.with_extension("absent")),
        Err(VaultError::NotFound)
    ));
}

#[test]
fn delete_and_kind_validation() {
    let (_dir, _path, vault) = new_vault();
    vault.put("document", "d1", b"pdf").unwrap();
    vault.delete("document", "d1").unwrap();
    assert!(matches!(
        vault.get("document", "d1"),
        Err(VaultError::RecordNotFound)
    ));
    assert!(matches!(
        vault.delete("document", "d1"),
        Err(VaultError::RecordNotFound)
    ));
    assert!(matches!(
        vault.put("../x", "1", b""),
        Err(VaultError::InvalidKind)
    ));
}

#[test]
fn json_helpers() {
    #[derive(serde::Serialize, serde::Deserialize, PartialEq, Debug)]
    struct Patient {
        first_name: String,
        birth: String,
    }
    let (_dir, _path, vault) = new_vault();
    let p = Patient {
        first_name: "Tom".into(),
        birth: "2017-09-01".into(),
    };
    vault.put_json("patient", "p", &p).unwrap();
    assert_eq!(vault.get_json::<Patient>("patient", "p").unwrap(), p);
}
