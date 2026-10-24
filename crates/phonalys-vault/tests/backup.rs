//! Sauvegardes : copie cohérente, test de restauration, contrôle avant restauration.

#![allow(clippy::unwrap_used)]

use phonalys_crypto::KdfParams;
use phonalys_vault::{LockedVault, VaultError};

const PASSWORD: &[u8] = b"cheval agrafe batterie correcte";

#[test]
fn snapshot_is_verified_and_restorable() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("cabinet.phonalys-vault");
    let mut vault = LockedVault::create(&path, PASSWORD, KdfParams::insecure_for_tests()).unwrap();
    vault.put("patient", "p1", b"dossier 1").unwrap();
    vault.put("seance", "s1", b"seance 1").unwrap();
    vault
        .audit(
            "praticien",
            phonalys_core::audit::AuditAction::PatientCreated,
            Some("p1"),
        )
        .unwrap();

    let backup = dir.path().join("b.phonalys-backup");
    vault.snapshot(&backup).unwrap();
    assert!(matches!(
        vault.snapshot(&backup),
        Err(VaultError::AlreadyExists)
    ));
    let report = vault.verify_backup(&backup).unwrap();
    assert_eq!(report.records, 2);
    assert_eq!(report.audit_entries, 2);
    LockedVault::inspect(&backup).unwrap();

    // Restauration : la copie s'ouvre avec la phrase de passe d'origine.
    drop(vault);
    let restored = dir.path().join("restaure.phonalys-vault");
    std::fs::copy(&backup, &restored).unwrap();
    let v = LockedVault::open(&restored)
        .unwrap()
        .unlock(PASSWORD)
        .map_err(|(_, e)| e)
        .unwrap();
    assert_eq!(&v.get("seance", "s1").unwrap()[..], b"seance 1");
}

#[test]
fn tampered_backup_fails_verification() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("cabinet.phonalys-vault");
    let vault = LockedVault::create(&path, PASSWORD, KdfParams::insecure_for_tests()).unwrap();
    vault.put("patient", "p1", b"dossier").unwrap();
    let backup = dir.path().join("b.phonalys-backup");
    vault.snapshot(&backup).unwrap();

    let conn = rusqlite::Connection::open(&backup).unwrap();
    conn.execute("UPDATE records SET sealed = zeroblob(64)", [])
        .unwrap();
    drop(conn);
    assert!(matches!(
        vault.verify_backup(&backup),
        Err(VaultError::Integrity(_))
    ));
}

#[test]
fn other_files_are_refused_before_restore() {
    let dir = tempfile::tempdir().unwrap();
    let f = dir.path().join("photo.jpg");
    std::fs::write(&f, b"pas un coffre").unwrap();
    assert!(matches!(
        LockedVault::inspect(&f),
        Err(VaultError::Integrity(_))
    ));
    assert!(LockedVault::inspect(&dir.path().join("absent")).is_err());
}
