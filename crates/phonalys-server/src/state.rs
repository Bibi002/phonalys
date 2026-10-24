//! État partagé du serveur. Un seul praticien par installation : chacun héberge
//! son propre coffre.

use crate::error::ApiError;
use data_encoding::BASE32_NOPAD;
use phonalys_auth::{SessionConfig, SessionStore};
use phonalys_core::audit::AuditAction;
use phonalys_crypto::KdfParams;
use phonalys_vault::{PasswordKey, UnlockedVault};
use phonalys_webauthn::{Challenge, RelyingParty};
use rand_core::{OsRng, RngCore};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};

pub const ACTOR: &str = "praticien";

#[derive(Debug, Clone)]
pub struct Config {
    pub vault_path: PathBuf,
    pub kdf: KdfParams,
    pub sessions: SessionConfig,
    /// Identité WebAuthn du serveur (domaine et origines autorisées).
    pub relying_party: RelyingParty,
    /// Dossier de l'application web compilée (`web/dist`). Absent : API seule.
    pub web_dir: Option<PathBuf>,
    /// Dossier des sauvegardes chiffrées.
    pub backup_dir: PathBuf,
}

/// Durée de validité d'un défi WebAuthn, en secondes.
pub const CHALLENGE_TTL_SECS: u64 = 120;

/// Connexion par clé en cours : la phrase de passe a été vérifiée, la signature
/// biométrique est attendue.
#[derive(Debug)]
pub(crate) struct PendingLogin {
    pub challenge: Challenge,
    pub expires_at: u64,
    /// Instance ouverte par la phrase de passe, si le coffre était verrouillé.
    pub fresh: Option<UnlockedVault>,
    /// Coffre lié au matériel : clé issue de la phrase de passe, en attente de la
    /// sortie PRF de l'appareil.
    pub password_key: Option<PasswordKey>,
}

#[derive(Debug)]
pub(crate) struct PendingRegistration {
    pub challenge: Challenge,
    pub expires_at: u64,
}

/// Ce que l'on fait de la sortie PRF d'une clé.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum BindPurpose {
    /// Lien au coffre : phrase de passe **et** clé (mode lié au matériel).
    Vault,
    /// Connexion en un geste : la clé seule.
    Quick,
}

/// Lien cryptographique d'une clé en cours (extension PRF).
#[derive(Debug)]
pub(crate) struct PendingBind {
    pub challenge: Challenge,
    pub credential_id: String,
    pub purpose: BindPurpose,
    pub expires_at: u64,
}

/// Connexion en un geste en cours : la signature et la sortie PRF sont attendues.
#[derive(Debug)]
pub(crate) struct PendingQuick {
    pub challenge: Challenge,
    pub expires_at: u64,
}

/// Réauthentification par clé en cours, pour une session donnée.
#[derive(Debug)]
pub(crate) struct PendingReauth {
    pub challenge: Challenge,
    pub session: zeroize::Zeroizing<String>,
    pub expires_at: u64,
}

/// Nouveau défi aléatoire.
pub(crate) fn new_challenge() -> Challenge {
    let mut raw = [0u8; 32];
    OsRng.fill_bytes(&mut raw);
    Challenge::from_random(raw)
}

#[derive(Debug)]
pub(crate) enum Phase {
    Locked,
    Unlocked(UnlockedVault),
}

#[derive(Debug)]
pub(crate) struct Inner {
    pub phase: Phase,
    pub sessions: SessionStore,
    /// Présent tant que l'installation n'est pas faite.
    pub setup_token: Option<String>,
    pub pending_login: Option<PendingLogin>,
    pub pending_registration: Option<PendingRegistration>,
    pub pending_bind: Option<PendingBind>,
    pub pending_quick: Option<PendingQuick>,
    pub pending_reauth: Option<PendingReauth>,
}

#[derive(Debug, Clone)]
pub struct AppState {
    pub(crate) config: Arc<Config>,
    inner: Arc<Mutex<Inner>>,
}

impl AppState {
    /// Crée l'état. Si aucun coffre n'existe, renvoie aussi le jeton d'installation à
    /// usage unique, à afficher sur la console du serveur : sans lui, personne sur le
    /// réseau ne peut s'approprier une installation vierge.
    pub fn new(config: Config) -> (Self, Option<String>) {
        let setup_token = (!config.vault_path.exists()).then(|| {
            let mut raw = [0u8; 15];
            OsRng.fill_bytes(&mut raw);
            BASE32_NOPAD.encode(&raw)
        });
        let state = Self {
            inner: Arc::new(Mutex::new(Inner {
                phase: Phase::Locked,
                sessions: SessionStore::new(config.sessions),
                setup_token: setup_token.clone(),
                pending_login: None,
                pending_registration: None,
                pending_bind: None,
                pending_quick: None,
                pending_reauth: None,
            })),
            config: Arc::new(config),
        };
        (state, setup_token)
    }

    pub(crate) fn lock_inner(&self) -> MutexGuard<'_, Inner> {
        // En release, `panic = "abort"` : un verrou empoisonné ne peut pas survivre.
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Exécute `f` sur le coffre déverrouillé.
    pub(crate) fn with_vault<T>(
        &self,
        f: impl FnOnce(&mut UnlockedVault) -> Result<T, ApiError>,
    ) -> Result<T, ApiError> {
        match &mut self.lock_inner().phase {
            Phase::Unlocked(vault) => f(vault),
            Phase::Locked => Err(ApiError::Unauthorized),
        }
    }

    /// Verrouille si plus aucune session n'est active.
    pub fn auto_lock(&self, now: u64) {
        let mut inner = self.lock_inner();
        // Une connexion par clé abandonnée ne doit pas garder de clés en mémoire.
        if inner
            .pending_login
            .as_ref()
            .is_some_and(|p| now > p.expires_at)
        {
            inner.pending_login = None;
        }
        if inner
            .pending_registration
            .as_ref()
            .is_some_and(|p| now > p.expires_at)
        {
            inner.pending_registration = None;
        }
        if inner
            .pending_bind
            .as_ref()
            .is_some_and(|p| now > p.expires_at)
        {
            inner.pending_bind = None;
        }
        if inner
            .pending_quick
            .as_ref()
            .is_some_and(|p| now > p.expires_at)
        {
            inner.pending_quick = None;
        }
        if inner
            .pending_reauth
            .as_ref()
            .is_some_and(|p| now > p.expires_at)
        {
            inner.pending_reauth = None;
        }
        if !inner.sessions.purge(now) {
            if let Phase::Unlocked(mut vault) = std::mem::replace(&mut inner.phase, Phase::Locked) {
                if let Err(e) = vault.audit(ACTOR, AuditAction::AutoLock, None) {
                    tracing::error!(error = %e, "audit du verrouillage automatique");
                }
                tracing::info!("coffre verrouillé automatiquement");
            }
        }
    }

    /// Le coffre est-il actuellement déverrouillé ?
    pub fn is_unlocked(&self) -> bool {
        matches!(self.lock_inner().phase, Phase::Unlocked(_))
    }
}

pub fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
