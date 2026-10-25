//! Facteurs liés au matériel : la clé maîtresse ne s'ouvre qu'avec la phrase de passe
//! **et** un second secret (sortie PRF d'une clé biométrique, ou code de récupération).
//!
//! Chaque facteur est une double enveloppe :
//!
//! ```text
//! blob = sceau( KEK(phrase de passe),            ← couche extérieure
//!               sceau( HKDF(secret du facteur),  ← couche intérieure
//!                      clé maîtresse ) )
//! ```
//!
//! Changer de phrase de passe ne refait que les couches extérieures : les appareils ne
//! sont pas nécessaires. En mode « lié au matériel », l'enveloppe par phrase de passe
//! seule (`meta.wrapped_master`) est effacée : voler le fichier et la phrase de passe
//! ne suffit plus.
//!
//! **Connexion en un geste** (option, par clé) : une enveloppe à une seule couche,
//! `sceau(HKDF(sortie PRF), clé maîtresse)`. Une clé biométrique avec vérification de
//! l'utilisateur réunit déjà deux facteurs (l'appareil possédé et l'empreinte ou le
//! code de l'appareil). Après [`QUICK_MAX_FAILURES`] échecs consécutifs, ce raccourci
//! est refusé : il faut la phrase de passe **et** un second facteur.

use crate::{
    check_lockout, derive_stored_kek, failures, register_failure, savepoint, wrapped_master,
    LockedVault, UnlockedVault, VaultError, MASTER_CONTEXT,
};
use phonalys_core::audit::AuditAction;
use phonalys_crypto::{
    derive_kek, derive_subkey, generate_salt, open, seal, unwrap_key, wrap_key, CryptoError,
    KdfParams, SecretKey,
};
use rand_core::{OsRng, RngCore};
use rusqlite::{params, Connection};
use time::OffsetDateTime;
use zeroize::Zeroizing;

pub(crate) fn random_salt() -> [u8; 32] {
    let mut salt = [0u8; 32];
    OsRng.fill_bytes(&mut salt);
    salt
}

/// Clé dérivée de la phrase de passe (Argon2id). Elle n'est jamais stockée et s'efface
/// de la mémoire à sa destruction.
#[derive(Debug)]
pub struct PasswordKey {
    pub(crate) kek: SecretKey,
}

/// Échecs consécutifs au-delà desquels la connexion en un geste est refusée.
pub const QUICK_MAX_FAILURES: u32 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FactorKind {
    Passkey,
    Recovery,
    /// Connexion en un geste : la sortie PRF seule ouvre l'enveloppe.
    Quick,
}

impl FactorKind {
    fn as_str(self) -> &'static str {
        match self {
            FactorKind::Passkey => "passkey",
            FactorKind::Recovery => "recovery",
            FactorKind::Quick => "quick",
        }
    }
}

fn context(kind: FactorKind, id: &str) -> Vec<u8> {
    format!("phonalys:v2:factor:{}:{id}", kind.as_str()).into_bytes()
}

fn seal_factor(
    pk: &PasswordKey,
    master: &SecretKey,
    kind: FactorKind,
    id: &str,
    secret: &[u8],
) -> Result<Vec<u8>, CryptoError> {
    let ctx = context(kind, id);
    let inner = wrap_key(&derive_subkey(secret, &ctx), master, &ctx)?;
    seal(&pk.kek, &inner, &ctx)
}

fn open_outer(
    pk: &PasswordKey,
    blob: &[u8],
    kind: FactorKind,
    id: &str,
) -> Result<Zeroizing<Vec<u8>>, CryptoError> {
    open(&pk.kek, blob, &context(kind, id)).map(Zeroizing::new)
}

fn open_factor(
    pk: &PasswordKey,
    blob: &[u8],
    kind: FactorKind,
    id: &str,
    secret: &[u8],
) -> Result<SecretKey, CryptoError> {
    let ctx = context(kind, id);
    let inner = open_outer(pk, blob, kind, id)?;
    unwrap_key(&derive_subkey(secret, &ctx), &inner, &ctx)
}

fn wraps(conn: &Connection, kind: FactorKind) -> Result<Vec<(String, Vec<u8>)>, VaultError> {
    let mut stmt = conn.prepare("SELECT id, blob FROM factor_wraps WHERE kind = ?1 ORDER BY id")?;
    let rows = stmt
        .query_map([kind.as_str()], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<_, _>>()?;
    Ok(rows)
}

/// Enveloppes à double couche (la couche extérieure dépend de la phrase de passe).
/// Les enveloppes « en un geste » n'en font pas partie.
fn all_wraps(conn: &Connection) -> Result<Vec<(FactorKind, String, Vec<u8>)>, VaultError> {
    let mut out = Vec::new();
    for kind in [FactorKind::Passkey, FactorKind::Recovery] {
        out.extend(
            wraps(conn, kind)?
                .into_iter()
                .map(|(id, blob)| (kind, id, blob)),
        );
    }
    Ok(out)
}

fn is_bound(conn: &Connection) -> Result<bool, VaultError> {
    Ok(wrapped_master(conn)?.is_empty())
}

fn prf_salt(conn: &Connection) -> Result<[u8; 32], VaultError> {
    let salt: Vec<u8> =
        conn.query_row("SELECT prf_salt FROM meta WHERE id = 1", [], |r| r.get(0))?;
    salt.try_into()
        .map_err(|_| VaultError::Integrity("sel PRF invalide".into()))
}

fn now_secs() -> i64 {
    OffsetDateTime::now_utc().unix_timestamp()
}

impl LockedVault {
    /// Dérive la clé de la phrase de passe, après contrôle du délai anti-acharnement.
    /// Ne valide pas la phrase de passe : c'est l'ouverture d'une enveloppe qui le fera.
    pub fn password_key_at(&self, password: &[u8], now: i64) -> Result<PasswordKey, VaultError> {
        check_lockout(&self.conn, now)?;
        Ok(PasswordKey {
            kek: derive_stored_kek(&self.conn, password)?,
        })
    }

    /// Dérive la clé de la phrase de passe **sans** contrôle du blocage, pour une
    /// connexion par code de récupération (voir `unlock_with_recovery_at`).
    pub fn recovery_password_key(&self, password: &[u8]) -> Result<PasswordKey, VaultError> {
        Ok(PasswordKey {
            kek: derive_stored_kek(&self.conn, password)?,
        })
    }

    pub fn is_hardware_bound(&self) -> Result<bool, VaultError> {
        is_bound(&self.conn)
    }

    /// Sel public à passer à l'extension PRF de WebAuthn.
    pub fn prf_salt(&self) -> Result<[u8; 32], VaultError> {
        prf_salt(&self.conn)
    }

    /// Identifiants des facteurs enregistrés (publics : identifiants de clés WebAuthn).
    pub fn factor_ids(&self, kind: FactorKind) -> Result<Vec<String>, VaultError> {
        Ok(wraps(&self.conn, kind)?
            .into_iter()
            .map(|(id, _)| id)
            .collect())
    }

    /// Déverrouille avec la phrase de passe et la sortie PRF d'une clé biométrique.
    #[allow(clippy::result_large_err)]
    pub fn unlock_with_factor_at(
        self,
        pk: &PasswordKey,
        kind: FactorKind,
        id: &str,
        secret: &[u8],
        now: i64,
    ) -> Result<UnlockedVault, (Self, VaultError)> {
        let attempt = || -> Result<SecretKey, VaultError> {
            check_lockout(&self.conn, now)?;
            let blob = wraps(&self.conn, kind)?
                .into_iter()
                .find(|(wid, _)| wid == id)
                .map(|(_, blob)| blob);
            match blob.map(|b| open_factor(pk, &b, kind, id, secret)) {
                Some(Ok(master)) => Ok(master),
                // Phrase de passe fausse, facteur faux ou inconnu : réponse identique.
                _ => {
                    register_failure(&self.conn, now)?;
                    Err(VaultError::FactorRejected)
                }
            }
        };
        match attempt() {
            Ok(master) => self.finish_unlock(master),
            Err(e) => Err((self, e)),
        }
    }

    /// La connexion en un geste est-elle encore permise (moins de
    /// [`QUICK_MAX_FAILURES`] échecs consécutifs) ?
    pub fn quick_unlock_allowed(&self) -> Result<bool, VaultError> {
        Ok(failures(&self.conn)? < QUICK_MAX_FAILURES)
    }

    /// Connexion en un geste : la sortie PRF d'une clé biométrique seule. Refusée après
    /// [`QUICK_MAX_FAILURES`] échecs consécutifs ([`VaultError::StepUpRequired`]).
    #[allow(clippy::result_large_err)]
    pub fn unlock_with_quick_at(
        self,
        id: &str,
        secret: &[u8],
        now: i64,
    ) -> Result<UnlockedVault, (Self, VaultError)> {
        let attempt = || -> Result<SecretKey, VaultError> {
            check_lockout(&self.conn, now)?;
            if !self.quick_unlock_allowed()? {
                return Err(VaultError::StepUpRequired);
            }
            let ctx = context(FactorKind::Quick, id);
            let opened = wraps(&self.conn, FactorKind::Quick)?
                .into_iter()
                .find(|(wid, _)| wid == id)
                .map(|(_, blob)| unwrap_key(&derive_subkey(secret, &ctx), &blob, &ctx));
            match opened {
                Some(Ok(master)) => Ok(master),
                _ => {
                    register_failure(&self.conn, now)?;
                    Err(VaultError::FactorRejected)
                }
            }
        };
        match attempt() {
            Ok(master) => self.finish_unlock(master),
            Err(e) => Err((self, e)),
        }
    }

    /// Déverrouille avec la phrase de passe et un code de récupération, qui est
    /// **consommé** : son enveloppe est détruite.
    #[allow(clippy::result_large_err)]
    pub fn unlock_with_recovery_at(
        self,
        pk: &PasswordKey,
        code: &[u8],
        now: i64,
    ) -> Result<UnlockedVault, (Self, VaultError)> {
        let attempt = || -> Result<SecretKey, VaultError> {
            // Pas de contrôle du blocage : le code de récupération sert justement à
            // débloquer. Ses 80 bits aléatoires le rendent impossible à deviner, et chaque
            // essai coûte une dérivation Argon2id.
            let _ = now;
            // Tous les essais sont faits, pour un temps de réponse indépendant du code.
            let mut found = None;
            for (id, blob) in wraps(&self.conn, FactorKind::Recovery)? {
                if let Ok(master) = open_factor(pk, &blob, FactorKind::Recovery, &id, code) {
                    found.get_or_insert((id, master));
                }
            }
            match found {
                Some((id, master)) => {
                    self.conn.execute(
                        "DELETE FROM factor_wraps WHERE kind = 'recovery' AND id = ?1",
                        [id],
                    )?;
                    Ok(master)
                }
                None => {
                    register_failure(&self.conn, now)?;
                    Err(VaultError::FactorRejected)
                }
            }
        };
        match attempt() {
            Ok(master) => self.finish_unlock(master),
            Err(e) => Err((self, e)),
        }
    }
}

impl UnlockedVault {
    /// Dérive et vérifie la clé de la phrase de passe pendant une session (pour lier une
    /// clé, changer la phrase de passe…). Un échec compte dans le délai anti-acharnement.
    pub fn password_key(&self, password: &[u8]) -> Result<PasswordKey, VaultError> {
        let now = now_secs();
        check_lockout(&self.conn, now)?;
        let pk = PasswordKey {
            kek: derive_stored_kek(&self.conn, password)?,
        };
        if self.verify_password_key(&pk)? {
            Ok(pk)
        } else {
            register_failure(&self.conn, now)?;
            Err(VaultError::WrongPassword)
        }
    }

    fn verify_password_key(&self, pk: &PasswordKey) -> Result<bool, VaultError> {
        let wrapped = wrapped_master(&self.conn)?;
        if !wrapped.is_empty() {
            return Ok(unwrap_key(&pk.kek, &wrapped, MASTER_CONTEXT).is_ok());
        }
        Ok(all_wraps(&self.conn)?
            .iter()
            .any(|(kind, id, blob)| open_outer(pk, blob, *kind, id).is_ok()))
    }

    fn require_password_key(&self, pk: &PasswordKey) -> Result<(), VaultError> {
        if self.verify_password_key(pk)? {
            Ok(())
        } else {
            Err(VaultError::WrongPassword)
        }
    }

    pub fn prf_salt(&self) -> Result<[u8; 32], VaultError> {
        prf_salt(&self.conn)
    }

    pub fn is_hardware_bound(&self) -> Result<bool, VaultError> {
        is_bound(&self.conn)
    }

    pub fn factor_ids(&self, kind: FactorKind) -> Result<Vec<String>, VaultError> {
        Ok(wraps(&self.conn, kind)?
            .into_iter()
            .map(|(id, _)| id)
            .collect())
    }

    /// Ajoute (ou remplace) un facteur.
    pub fn add_factor(
        &self,
        pk: &PasswordKey,
        kind: FactorKind,
        id: &str,
        secret: &[u8],
    ) -> Result<(), VaultError> {
        self.require_password_key(pk)?;
        let blob = seal_factor(pk, &self.master, kind, id, secret)?;
        self.conn.execute(
            "INSERT INTO factor_wraps (kind, id, blob) VALUES (?1, ?2, ?3)
             ON CONFLICT (kind, id) DO UPDATE SET blob = excluded.blob",
            params![kind.as_str(), id, blob],
        )?;
        Ok(())
    }

    /// Active la connexion en un geste pour une clé. La phrase de passe est exigée :
    /// sans elle, quelqu'un devant une session restée ouverte pourrait s'ouvrir un accès
    /// permanent avec sa propre clé.
    pub fn add_quick_factor(
        &self,
        pk: &PasswordKey,
        id: &str,
        secret: &[u8],
    ) -> Result<(), VaultError> {
        self.require_password_key(pk)?;
        let ctx = context(FactorKind::Quick, id);
        let blob = wrap_key(&derive_subkey(secret, &ctx), &self.master, &ctx)?;
        self.conn.execute(
            "INSERT INTO factor_wraps (kind, id, blob) VALUES ('quick', ?1, ?2)
             ON CONFLICT (kind, id) DO UPDATE SET blob = excluded.blob",
            params![id, blob],
        )?;
        Ok(())
    }

    /// Retire un facteur. En mode lié, la dernière clé biométrique ne peut pas partir.
    pub fn remove_factor(&self, kind: FactorKind, id: &str) -> Result<(), VaultError> {
        if kind == FactorKind::Passkey
            && self.is_hardware_bound()?
            && self
                .factor_ids(FactorKind::Passkey)?
                .iter()
                .all(|i| i == id)
        {
            return Err(VaultError::Precondition(
                "dernière clé d'un coffre lié au matériel",
            ));
        }
        self.conn.execute(
            "DELETE FROM factor_wraps WHERE kind = ?1 AND id = ?2",
            params![kind.as_str(), id],
        )?;
        Ok(())
    }

    /// Remplace tous les codes de récupération par de nouveaux.
    pub fn replace_recovery_factors(
        &self,
        pk: &PasswordKey,
        codes: &[Vec<u8>],
    ) -> Result<(), VaultError> {
        self.require_password_key(pk)?;
        let tx = &self.conn;
        savepoint(tx, || {
            tx.execute("DELETE FROM factor_wraps WHERE kind = 'recovery'", [])?;
            for (i, code) in codes.iter().enumerate() {
                let id = format!("r{i}");
                let blob = seal_factor(pk, &self.master, FactorKind::Recovery, &id, code)?;
                tx.execute(
                    "INSERT INTO factor_wraps (kind, id, blob) VALUES ('recovery', ?1, ?2)",
                    params![id, blob],
                )?;
            }
            Ok(())
        })
    }

    /// Active ou désactive le mode lié au matériel.
    pub fn set_hardware_bound(&mut self, pk: &PasswordKey, bound: bool) -> Result<(), VaultError> {
        self.require_password_key(pk)?;
        let wrapped = if bound {
            if self.factor_ids(FactorKind::Passkey)?.is_empty() {
                return Err(VaultError::Precondition("aucune clé biométrique liée"));
            }
            if self.factor_ids(FactorKind::Recovery)?.is_empty() {
                return Err(VaultError::Precondition("aucun code de récupération lié"));
            }
            Vec::new()
        } else {
            wrap_key(&pk.kek, &self.master, MASTER_CONTEXT)?
        };
        self.conn.execute(
            "UPDATE meta SET wrapped_master = ?1 WHERE id = 1",
            params![wrapped],
        )?;
        self.audit("praticien", AuditAction::SecurityPolicyChanged, None)
    }

    /// Change la phrase de passe. Seules les couches extérieures sont refaites :
    /// aucun appareil n'est nécessaire.
    pub fn change_password(
        &mut self,
        old: &PasswordKey,
        new_password: &[u8],
        params: KdfParams,
    ) -> Result<(), VaultError> {
        self.require_password_key(old)?;
        let salt = generate_salt();
        let new = PasswordKey {
            kek: derive_kek(new_password, &salt, params)?,
        };

        let tx = &self.conn;
        savepoint(tx, || {
            let bound = is_bound(tx)?;
            let wrapped = if bound {
                Vec::new()
            } else {
                wrap_key(&new.kek, &self.master, MASTER_CONTEXT)?
            };
            tx.execute(
                "UPDATE meta SET kdf_salt = ?1, kdf_memory_kib = ?2, kdf_iterations = ?3,
                 kdf_parallelism = ?4, wrapped_master = ?5 WHERE id = 1",
                params![
                    salt.as_slice(),
                    params.memory_kib,
                    params.iterations,
                    params.parallelism,
                    wrapped,
                ],
            )?;
            for (kind, id, blob) in all_wraps(tx)? {
                let inner = open_outer(old, &blob, kind, &id)?;
                let resealed = seal(&new.kek, &inner, &context(kind, &id))?;
                tx.execute(
                    "UPDATE factor_wraps SET blob = ?1 WHERE kind = ?2 AND id = ?3",
                    params![resealed, kind.as_str(), id],
                )?;
            }
            Ok(())
        })?;
        self.audit("praticien", AuditAction::PasswordChanged, None)
    }
}
