//! État partagé du serveur. Un seul praticien par installation : chacun héberge
//! son propre coffre.

use crate::error::ApiError;
use data_encoding::BASE32_NOPAD;
use phonalys_auth::{SessionConfig, SessionStore};
use phonalys_core::audit::AuditAction;
use phonalys_crypto::KdfParams;
use phonalys_vault::UnlockedVault;
use rand_core::{OsRng, RngCore};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};

pub const ACTOR: &str = "praticien";

#[derive(Debug, Clone)]
pub struct Config {
    pub vault_path: PathBuf,
    pub kdf: KdfParams,
    pub sessions: SessionConfig,
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
