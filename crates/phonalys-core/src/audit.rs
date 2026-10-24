//! Journal d'audit inaltérable.
//!
//! Chaque entrée porte le HMAC-SHA256 de son contenu et du HMAC précédent. Modifier,
//! supprimer, insérer ou réordonner une entrée casse la chaîne, et la recalculer exige
//! la clé d'audit, qui n'est disponible que dans un coffre déverrouillé.

use hmac::{Hmac, Mac};
use phonalys_crypto::{constant_time_eq, SecretKey};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use time::OffsetDateTime;

type HmacSha256 = Hmac<Sha256>;

const GENESIS: &str = "phonalys-audit-v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditAction {
    VaultCreated,
    Login,
    LoginFailed,
    RecoveryCodeUsed,
    PasskeyLogin,
    PasskeyRegistered,
    PasskeyRemoved,
    /// Une clé a été liée cryptographiquement au coffre (extension PRF).
    FactorBound,
    /// Le mode lié au matériel a été activé ou désactivé.
    HardwareBindingChanged,
    /// La connexion en un geste a été activée ou retirée pour une clé.
    QuickUnlockChanged,
    SecurityPolicyChanged,
    PasswordChanged,
    Unlock,
    Lock,
    AutoLock,
    PatientCreated,
    PatientViewed,
    PatientUpdated,
    PatientErased,
    AssessmentScored,
    ReportGenerated,
    AiRequest,
    DocumentImported,
    DocumentOpened,
    Export,
    SettingsChanged,
    /// Séance, bilan, objectif ou résultat d'exercice (cible : `type/identifiant`).
    RecordCreated,
    RecordViewed,
    RecordUpdated,
    RecordErased,
    AuditViewed,
    /// Nouvelle authentification avant une opération sensible (documents).
    Reauthenticated,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditEntry {
    pub seq: u64,
    #[serde(with = "time::serde::rfc3339")]
    pub at: OffsetDateTime,
    pub actor: String,
    pub action: AuditAction,
    /// Identifiant pseudonyme de la cible — jamais de nom ni de donnée clinique.
    pub target: Option<String>,
    pub prev_mac: String,
    pub mac: String,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum AuditError {
    #[error("rupture de la chaîne d'audit à l'entrée {0}")]
    Broken(u64),
}

#[derive(Debug)]
pub struct AuditLog {
    key: SecretKey,
    entries: Vec<AuditEntry>,
}

impl AuditLog {
    pub fn new(key: SecretKey) -> Self {
        Self {
            key,
            entries: Vec::new(),
        }
    }

    /// Recharge un journal persistant ; refuse une chaîne altérée.
    pub fn from_entries(key: SecretKey, entries: Vec<AuditEntry>) -> Result<Self, AuditError> {
        verify_chain(&key, &entries)?;
        Ok(Self { key, entries })
    }

    pub fn entries(&self) -> &[AuditEntry] {
        &self.entries
    }

    pub fn append(
        &mut self,
        actor: &str,
        action: AuditAction,
        target: Option<&str>,
    ) -> &AuditEntry {
        self.append_at(OffsetDateTime::now_utc(), actor, action, target)
    }

    fn append_at(
        &mut self,
        at: OffsetDateTime,
        actor: &str,
        action: AuditAction,
        target: Option<&str>,
    ) -> &AuditEntry {
        let prev_mac = self
            .entries
            .last()
            .map_or_else(|| GENESIS.to_owned(), |e| e.mac.clone());
        let mut entry = AuditEntry {
            seq: self.entries.len() as u64,
            at,
            actor: actor.to_owned(),
            action,
            target: target.map(str::to_owned),
            prev_mac,
            mac: String::new(),
        };
        entry.mac = mac_of(&self.key, &entry);
        self.entries.push(entry);
        &self.entries[self.entries.len() - 1]
    }

    /// Vérifie toute la chaîne.
    pub fn verify(&self) -> Result<(), AuditError> {
        verify_chain(&self.key, &self.entries)
    }
}

/// Vérifie une chaîne d'entrées (par exemple relue depuis le disque).
pub fn verify_chain(key: &SecretKey, entries: &[AuditEntry]) -> Result<(), AuditError> {
    let mut expected_prev = GENESIS.to_owned();
    for (i, entry) in entries.iter().enumerate() {
        let ok = entry.seq == i as u64
            && entry.prev_mac == expected_prev
            && constant_time_eq(entry.mac.as_bytes(), mac_of(key, entry).as_bytes());
        if !ok {
            return Err(AuditError::Broken(i as u64));
        }
        expected_prev = entry.mac.clone();
    }
    Ok(())
}

fn mac_of(key: &SecretKey, e: &AuditEntry) -> String {
    let canonical = serde_json::json!({
        "seq": e.seq,
        "at": e.at.unix_timestamp_nanos().to_string(),
        "actor": e.actor,
        "action": e.action,
        "target": e.target,
        "prev": e.prev_mac,
    });
    // HMAC accepte des clés de toute longueur : cet appel ne peut pas échouer.
    let mut mac =
        <HmacSha256 as Mac>::new_from_slice(key.as_bytes()).unwrap_or_else(|_| unreachable!());
    mac.update(canonical.to_string().as_bytes());
    hex::encode(mac.finalize().into_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> AuditLog {
        let mut log = AuditLog::new(SecretKey::generate());
        log.append("lp-001", AuditAction::Login, None);
        log.append("lp-001", AuditAction::PatientViewed, Some("pt-7f3a"));
        log.append("lp-001", AuditAction::ReportGenerated, Some("pt-7f3a"));
        log
    }

    #[test]
    fn intact_chain_verifies() {
        assert_eq!(sample().verify(), Ok(()));
    }

    #[test]
    fn reload_verifies_chain() {
        let log = sample();
        let mut entries = log.entries().to_vec();
        assert!(AuditLog::from_entries(log.key.clone(), entries.clone()).is_ok());
        entries[0].actor = "intrus".into();
        assert_eq!(
            AuditLog::from_entries(log.key.clone(), entries).unwrap_err(),
            AuditError::Broken(0)
        );
    }

    #[test]
    fn tampering_is_detected() {
        let log = sample();
        let mut entries = log.entries().to_vec();
        entries[1].target = Some("pt-autre".into());
        assert_eq!(verify_chain(&log.key, &entries), Err(AuditError::Broken(1)));
    }

    #[test]
    fn deletion_and_reordering_are_detected() {
        let log = sample();
        let mut deleted = log.entries().to_vec();
        deleted.remove(1);
        assert!(verify_chain(&log.key, &deleted).is_err());

        let mut swapped = log.entries().to_vec();
        swapped.swap(1, 2);
        assert!(verify_chain(&log.key, &swapped).is_err());
    }

    #[test]
    fn forging_without_key_fails() {
        let log = sample();
        let mut forged = AuditLog::new(SecretKey::generate());
        for e in log.entries() {
            forged.append_at(e.at, &e.actor, e.action, e.target.as_deref());
        }
        assert!(verify_chain(&log.key, forged.entries()).is_err());
    }
}
