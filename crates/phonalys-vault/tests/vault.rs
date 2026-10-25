#![allow(clippy::unwrap_used)]

use phonalys_core::audit::AuditAction;
use phonalys_crypto::KdfParams;
use phonalys_vault::{lockout_delay, LockedVault, UnlockedVault, VaultError, LOCKOUT_SECS};
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
fn a_vault_is_used_by_one_instance_at_a_time() {
    let (_dir, path, _vault) = new_vault();
    let first = phonalys_vault::InstanceLock::acquire(&path).unwrap();
    assert!(matches!(
        phonalys_vault::InstanceLock::acquire(&path),
        Err(VaultError::InUse)
    ));
    drop(first);
    assert!(phonalys_vault::InstanceLock::acquire(&path).is_ok());
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
    assert_eq!(locked.remaining_attempts().unwrap(), 4);
    for left in [3, 2, 1] {
        let (back, err) = locked.unlock_at(b"mauvais", 1_000).unwrap_err();
        assert!(matches!(err, VaultError::WrongPassword));
        assert_eq!(back.remaining_attempts().unwrap(), left);
        locked = back;
    }
    // 4e échec : blocage de 15 minutes, même avec le bon mot de passe.
    let (back, _) = locked.unlock_at(b"mauvais", 1_000).unwrap_err();
    assert_eq!(back.remaining_attempts().unwrap(), 0);
    let (back, err) = back.unlock_at(PASSWORD, 1_000).unwrap_err();
    assert!(
        matches!(err, VaultError::TooManyAttempts(LOCKOUT_SECS)),
        "{err:?}"
    );
    // Une fois le blocage écoulé, le bon mot de passe fonctionne.
    let vault = back.unlock_at(PASSWORD, 1_000 + LOCKOUT_SECS).unwrap();

    // Un second facteur faux prolonge le blocage, même avec le bon mot de passe.
    let t = 1_000 + LOCKOUT_SECS;
    vault.register_failure(t).unwrap();
    let (back, err) = vault
        .lock()
        .unwrap()
        .unlock_at(PASSWORD, t + 1)
        .unwrap_err();
    assert!(matches!(err, VaultError::TooManyAttempts(_)), "{err:?}");

    // Seule une authentification complète remet le compteur à zéro.
    let vault = back.unlock_at(PASSWORD, t + LOCKOUT_SECS).unwrap();
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
fn four_attempts_then_fifteen_minutes() {
    assert_eq!(
        (0..=3).map(lockout_delay).collect::<Vec<_>>(),
        vec![0, 0, 0, 0]
    );
    assert_eq!(lockout_delay(4), 15 * 60);
    assert_eq!(lockout_delay(5), 15 * 60);
    assert_eq!(lockout_delay(u32::MAX), 15 * 60);
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

/// Un journal altéré ne doit jamais rendre les dossiers inaccessibles : le coffre
/// s'ouvre, l'altération est signalée, et le journal continue sans réutiliser de numéro.
#[test]
fn tampered_audit_opens_with_an_alert() {
    let (_dir, path, mut vault) = new_vault();
    vault.put("patient", "a", b"dossier").unwrap();
    vault
        .audit("lp", AuditAction::PatientViewed, Some("p1"))
        .unwrap();
    assert!(vault.audit_alert().is_none());
    drop(vault);

    let conn = Connection::open(&path).unwrap();
    conn.execute("DELETE FROM audit WHERE seq = 0", []).unwrap();
    drop(conn);

    let mut vault = reopen(&path, PASSWORD).unwrap();
    assert!(vault.audit_alert().is_some());
    assert_eq!(vault.get("patient", "a").unwrap().as_slice(), b"dossier");
    vault.audit("lp", AuditAction::Lock, None).unwrap();
    drop(vault);
    assert!(reopen(&path, PASSWORD).unwrap().audit_alert().is_some());
}

#[test]
fn deleting_the_last_audit_entries_is_detected() {
    let (_dir, path, mut vault) = new_vault();
    for _ in 0..3 {
        vault
            .audit("lp", AuditAction::PatientViewed, Some("p1"))
            .unwrap();
    }
    drop(vault);
    assert!(reopen(&path, PASSWORD).unwrap().audit_alert().is_none());

    let conn = Connection::open(&path).unwrap();
    conn.execute(
        "DELETE FROM audit WHERE seq = (SELECT MAX(seq) FROM audit)",
        [],
    )
    .unwrap();
    drop(conn);
    let alert = reopen(&path, PASSWORD)
        .unwrap()
        .audit_alert()
        .map(str::to_owned);
    assert!(alert.unwrap().contains("dernières entrées"));
}

/// Disque plein pendant l'écriture d'une entrée : l'opération échoue, mais le coffre
/// reste cohérent (avant ce correctif, il refusait ensuite de s'ouvrir).
#[test]
fn failed_audit_write_leaves_a_consistent_vault() {
    let (_dir, path, mut vault) = new_vault();
    let conn = Connection::open(&path).unwrap();
    conn.execute_batch(
        "CREATE TRIGGER plein BEFORE INSERT ON audit BEGIN SELECT RAISE(ABORT, 'disque plein'); END;",
    )
    .unwrap();
    assert!(vault.audit("lp", AuditAction::PatientViewed, None).is_err());
    conn.execute_batch("DROP TRIGGER plein").unwrap();
    vault.audit("lp", AuditAction::Lock, None).unwrap();
    drop(vault);
    drop(conn);

    let vault = reopen(&path, PASSWORD).unwrap();
    assert!(vault.audit_alert().is_none());
    let actions: Vec<_> = vault.audit_entries().iter().map(|e| e.action).collect();
    assert_eq!(actions, [AuditAction::VaultCreated, AuditAction::Lock]);
}

#[test]
fn atomic_writes_all_or_nothing() {
    let (_dir, path, mut vault) = new_vault();
    let failed: Result<(), VaultError> = vault.atomic(|v| {
        v.put("patient", "a", b"dossier")?;
        v.audit("lp", AuditAction::PatientCreated, Some("a"))?;
        Err(VaultError::Precondition("erreur au milieu"))
    });
    assert!(failed.is_err());
    assert!(matches!(
        vault.get("patient", "a"),
        Err(VaultError::RecordNotFound)
    ));
    assert_eq!(vault.audit_entries().len(), 1);

    vault
        .atomic(|v| {
            v.put("patient", "b", b"dossier")?;
            v.audit("lp", AuditAction::PatientCreated, Some("b"))
        })
        .unwrap();
    drop(vault);
    let vault = reopen(&path, PASSWORD).unwrap();
    assert!(vault.audit_alert().is_none());
    assert_eq!(vault.audit_entries().len(), 2);
    assert_eq!(vault.get("patient", "b").unwrap().as_slice(), b"dossier");
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
