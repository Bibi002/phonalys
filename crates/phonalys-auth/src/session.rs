//! Sessions en mémoire. Les jetons font 256 bits et seule leur empreinte SHA-256 est
//! gardée : une fuite de la mémoire du serveur ne livre pas de jeton réutilisable.

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use rand_core::{OsRng, RngCore};
use sha2::{Digest, Sha256};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SessionConfig {
    /// Verrouillage après inactivité, en secondes.
    pub idle_timeout_secs: u64,
    /// Durée de vie maximale d'une session, en secondes.
    pub absolute_timeout_secs: u64,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            idle_timeout_secs: 5 * 60,
            absolute_timeout_secs: 8 * 3600,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Session {
    created_at: u64,
    last_seen: u64,
}

#[derive(Debug, Default)]
pub struct SessionStore {
    config: SessionConfig,
    sessions: HashMap<[u8; 32], Session>,
}

impl SessionStore {
    pub fn new(config: SessionConfig) -> Self {
        Self {
            config,
            sessions: HashMap::new(),
        }
    }

    pub fn config(&self) -> SessionConfig {
        self.config
    }

    /// Ouvre une session et renvoie le jeton à placer dans le cookie.
    pub fn create(&mut self, now: u64) -> String {
        let mut raw = [0u8; 32];
        OsRng.fill_bytes(&mut raw);
        self.sessions.insert(
            Sha256::digest(raw).into(),
            Session {
                created_at: now,
                last_seen: now,
            },
        );
        URL_SAFE_NO_PAD.encode(raw)
    }

    /// Valide un jeton et prolonge la session. Une session expirée est supprimée.
    pub fn touch(&mut self, token: &str, now: u64) -> bool {
        let Some(key) = Self::key(token) else {
            return false;
        };
        let Some(session) = self.sessions.get_mut(&key) else {
            return false;
        };
        let expired = now.saturating_sub(session.last_seen) > self.config.idle_timeout_secs
            || now.saturating_sub(session.created_at) > self.config.absolute_timeout_secs;
        if expired {
            self.sessions.remove(&key);
            return false;
        }
        session.last_seen = now;
        true
    }

    pub fn revoke(&mut self, token: &str) {
        if let Some(key) = Self::key(token) {
            self.sessions.remove(&key);
        }
    }

    pub fn revoke_all(&mut self) {
        self.sessions.clear();
    }

    /// Supprime les sessions expirées et indique s'il en reste une active.
    pub fn purge(&mut self, now: u64) -> bool {
        let c = self.config;
        self.sessions.retain(|_, s| {
            now.saturating_sub(s.last_seen) <= c.idle_timeout_secs
                && now.saturating_sub(s.created_at) <= c.absolute_timeout_secs
        });
        !self.sessions.is_empty()
    }

    fn key(token: &str) -> Option<[u8; 32]> {
        let raw = URL_SAFE_NO_PAD.decode(token).ok()?;
        (raw.len() == 32).then(|| Sha256::digest(&raw).into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> SessionStore {
        SessionStore::new(SessionConfig {
            idle_timeout_secs: 300,
            absolute_timeout_secs: 3600,
        })
    }

    #[test]
    fn idle_timeout_locks() {
        let mut s = store();
        let t = s.create(0);
        assert!(s.touch(&t, 299));
        assert!(s.touch(&t, 598));
        assert!(!s.touch(&t, 899), "inactif trop longtemps");
        assert!(!s.touch(&t, 900), "supprimée");
    }

    #[test]
    fn absolute_timeout_applies_even_when_active() {
        let mut s = store();
        let t = s.create(0);
        let mut now = 0;
        while now < 3600 {
            now += 200;
            assert_eq!(s.touch(&t, now), now <= 3600);
        }
        assert!(!s.touch(&t, 3700));
    }

    #[test]
    fn revoke_and_garbage_tokens() {
        let mut s = store();
        let t = s.create(0);
        assert!(!s.touch("n'importe quoi", 1));
        assert!(!s.touch(&URL_SAFE_NO_PAD.encode([0u8; 32]), 1));
        s.revoke(&t);
        assert!(!s.touch(&t, 1));
    }

    #[test]
    fn purge_reports_activity() {
        let mut s = store();
        let _a = s.create(0);
        assert!(s.purge(100));
        assert!(!s.purge(1000));
    }
}
