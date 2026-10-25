//! Coffre chiffré de Phonalys.
//!
//! Un coffre est un fichier SQLite où **rien de sensible n'est en clair** :
//!
//! | Table      | Contenu |
//! |------------|---------|
//! | `meta`     | sel et paramètres Argon2id, clé maîtresse scellée par la KEK |
//! | `secrets`  | secrets internes (clé d'audit, TOTP…) scellés par la clé maîtresse |
//! | `records`  | enregistrements : clé de données scellée + contenu scellé par cette clé |
//! | `audit`    | journal chaîné par HMAC (identifiants pseudonymes uniquement) |
//! | `lockout`  | compteur d'échecs, nécessairement lisible avant déverrouillage |
//!
//! Le mot de passe n'est jamais stocké, même haché : c'est l'échec d'authentification
//! AEAD lors de l'ouverture de la clé maîtresse qui signale un mot de passe faux.
//!
//! Limite assumée : le délai après échecs ralentit les essais *via l'application*.
//! Un attaquant qui copie le fichier n'est freiné que par Argon2id (256 MiB par essai),
//! d'où l'exigence d'une phrase de passe robuste — ou, mieux, le mode « lié au
//! matériel » (voir [`factors`]) où la phrase de passe seule ne suffit plus.

mod factors;
pub use factors::{FactorKind, PasswordKey, QUICK_MAX_FAILURES};

use phonalys_core::audit::{AuditAction, AuditEntry, AuditLog};
use phonalys_crypto::{
    derive_kek, generate_salt, open, seal, unwrap_key, wrap_key, CryptoError, KdfParams, SecretKey,
    SALT_LEN,
};
use rusqlite::{params, Connection, OpenFlags, OptionalExtension};
use std::path::Path;
use time::OffsetDateTime;
use zeroize::Zeroizing;

const FORMAT_VERSION: i64 = 2;
const MASTER_CONTEXT: &[u8] = b"phonalys:v1:master";
const AUDIT_KEY_SECRET: &str = "audit-key";
/// Ancrage du journal : numéro et HMAC de la dernière entrée, scellés avec elle. Une
/// suppression des dernières entrées (que la chaîne seule ne voit pas) est ainsi détectée.
const AUDIT_HEAD_SECRET: &str = "audit-head";
/// Nombre d'échecs tolérés avant d'imposer un délai.
pub const MAX_ATTEMPTS: u32 = 4;
/// Durée du blocage après `MAX_ATTEMPTS` échecs consécutifs, en secondes. Un code de
/// récupération permet de débloquer immédiatement.
pub const LOCKOUT_SECS: i64 = 15 * 60;

#[derive(Debug, thiserror::Error)]
pub enum VaultError {
    #[error("un coffre existe déjà à cet emplacement")]
    AlreadyExists,
    #[error("aucun coffre à cet emplacement")]
    NotFound,
    #[error(
        "Phonalys est déjà ouvert sur cet ordinateur (application ou serveur du cabinet) :          un même coffre ne peut être utilisé que par une seule instance à la fois"
    )]
    InUse,
    #[error("mot de passe incorrect")]
    WrongPassword,
    #[error("trop de tentatives : réessayez dans {0} s")]
    TooManyAttempts(i64),
    #[error("format de coffre non pris en charge : {0}")]
    UnsupportedFormat(i64),
    #[error("enregistrement introuvable")]
    RecordNotFound,
    #[error("type d'enregistrement invalide")]
    InvalidKind,
    #[error("coffre lié au matériel : une clé biométrique ou un code de récupération est requis")]
    HardwareBound,
    #[error("second facteur refusé")]
    FactorRejected,
    #[error("après plusieurs échecs, la phrase de passe et un second facteur sont exigés")]
    StepUpRequired,
    #[error("opération impossible : {0}")]
    Precondition(&'static str),
    #[error("intégrité compromise : {0}")]
    Integrity(String),
    #[error("erreur de stockage : {0}")]
    Storage(#[from] rusqlite::Error),
    #[error("erreur de sérialisation : {0}")]
    Serialization(#[from] serde_json::Error),
}

impl From<CryptoError> for VaultError {
    fn from(e: CryptoError) -> Self {
        VaultError::Integrity(e.to_string())
    }
}

fn configure(conn: &Connection) -> Result<(), VaultError> {
    conn.execute_batch(
        "PRAGMA journal_mode = WAL;
         PRAGMA synchronous = FULL;
         PRAGMA secure_delete = ON;
         PRAGMA foreign_keys = ON;
         PRAGMA trusted_schema = OFF;",
    )?;
    Ok(())
}

const SCHEMA: &str = "
CREATE TABLE meta (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    format INTEGER NOT NULL,
    kdf_salt BLOB NOT NULL,
    kdf_memory_kib INTEGER NOT NULL,
    kdf_iterations INTEGER NOT NULL,
    kdf_parallelism INTEGER NOT NULL,
    wrapped_master BLOB NOT NULL,
    created_at TEXT NOT NULL,
    prf_salt BLOB NOT NULL
);
CREATE TABLE secrets (name TEXT PRIMARY KEY, sealed BLOB NOT NULL);
CREATE TABLE factor_wraps (
    kind TEXT NOT NULL,
    id TEXT NOT NULL,
    blob BLOB NOT NULL,
    PRIMARY KEY (kind, id)
);
CREATE TABLE records (
    kind TEXT NOT NULL,
    id TEXT NOT NULL,
    wrapped_dek BLOB NOT NULL,
    sealed BLOB NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (kind, id)
);
CREATE TABLE audit (seq INTEGER PRIMARY KEY, entry TEXT NOT NULL);
CREATE TABLE lockout (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    failures INTEGER NOT NULL,
    next_attempt_at INTEGER NOT NULL
);
INSERT INTO lockout (id, failures, next_attempt_at) VALUES (1, 0, 0);
";

/// Migration du format 1 (phase 2) vers le format 2 (facteurs liés au matériel).
const MIGRATION_1_TO_2: &str = "
ALTER TABLE meta ADD COLUMN prf_salt BLOB;
CREATE TABLE factor_wraps (
    kind TEXT NOT NULL,
    id TEXT NOT NULL,
    blob BLOB NOT NULL,
    PRIMARY KEY (kind, id)
);
";

fn migrate(conn: &Connection) -> Result<(), VaultError> {
    let format: i64 = conn.query_row("SELECT format FROM meta WHERE id = 1", [], |r| r.get(0))?;
    match format {
        FORMAT_VERSION => Ok(()),
        1 => {
            let tx = conn.unchecked_transaction()?;
            tx.execute_batch(MIGRATION_1_TO_2)?;
            tx.execute(
                "UPDATE meta SET prf_salt = ?1, format = ?2 WHERE id = 1",
                params![factors::random_salt().as_slice(), FORMAT_VERSION],
            )?;
            tx.commit()?;
            Ok(())
        }
        other => Err(VaultError::UnsupportedFormat(other)),
    }
}

/// Exécute `f` dans un point de sauvegarde SQLite : tout ou rien. Les points de
/// sauvegarde s'emboîtent, contrairement aux transactions.
pub(crate) fn savepoint<T>(
    conn: &Connection,
    f: impl FnOnce() -> Result<T, VaultError>,
) -> Result<T, VaultError> {
    conn.execute_batch("SAVEPOINT phonalys")?;
    let result = f().and_then(|v| {
        conn.execute_batch("RELEASE phonalys")?;
        Ok(v)
    });
    if result.is_err() {
        // Disque plein, erreur d'écriture… : rien de partiel ne reste.
        let _ = conn.execute_batch("ROLLBACK TO phonalys; RELEASE phonalys");
    }
    result
}

/// Restreint les droits du fichier au seul propriétaire (Unix).
/// Sous Windows, le fichier hérite des ACL du profil utilisateur.
fn restrict_permissions(path: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    }
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

/// Garantit qu'un seul processus utilise un coffre : deux instances (double lancement,
/// application de bureau et serveur du cabinet sur le même ordinateur) écriraient
/// chacune leur propre suite du journal d'audit. Le verrou du système est libéré à la
/// destruction de la valeur, et aussi si le processus s'arrête brutalement.
#[derive(Debug)]
pub struct InstanceLock {
    _file: std::fs::File,
}

impl InstanceLock {
    /// Prend le verrou du coffre `vault_path` (fichier voisin `.phonalys-lock`).
    pub fn acquire(vault_path: &Path) -> Result<Self, VaultError> {
        let path = vault_path.with_extension("phonalys-lock");
        let file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&path)
            .map_err(|e| VaultError::Integrity(format!("verrou du coffre : {e}")))?;
        match file.try_lock() {
            Ok(()) => Ok(Self { _file: file }),
            Err(std::fs::TryLockError::WouldBlock) => Err(VaultError::InUse),
            Err(std::fs::TryLockError::Error(e)) => {
                Err(VaultError::Integrity(format!("verrou du coffre : {e}")))
            }
        }
    }
}

/// Coffre verrouillé : seul le déverrouillage est possible.
#[derive(Debug)]
pub struct LockedVault {
    conn: Connection,
}

/// Coffre déverrouillé. Les clés sont effacées de la mémoire à sa destruction.
#[derive(Debug)]
pub struct UnlockedVault {
    conn: Connection,
    master: SecretKey,
    audit: AuditLog,
    /// Altération du journal constatée à l'ouverture.
    audit_alert: Option<String>,
}

impl LockedVault {
    /// Crée un nouveau coffre et le renvoie déverrouillé.
    pub fn create(
        path: &Path,
        password: &[u8],
        params: KdfParams,
    ) -> Result<UnlockedVault, VaultError> {
        if path.exists() {
            return Err(VaultError::AlreadyExists);
        }
        let conn = Connection::open(path)?;
        restrict_permissions(path).map_err(|e| VaultError::Integrity(e.to_string()))?;
        configure(&conn)?;
        conn.execute_batch(SCHEMA)?;

        let salt = generate_salt();
        let kek = derive_kek(password, &salt, params)?;
        let master = SecretKey::generate();
        conn.execute(
            "INSERT INTO meta VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                FORMAT_VERSION,
                salt.as_slice(),
                params.memory_kib,
                params.iterations,
                params.parallelism,
                wrap_key(&kek, &master, MASTER_CONTEXT)?,
                now_rfc3339(),
                factors::random_salt().as_slice(),
            ],
        )?;

        let audit_key = SecretKey::generate();
        put_secret(&conn, &master, AUDIT_KEY_SECRET, audit_key.as_bytes())?;
        let mut vault = UnlockedVault {
            conn,
            master,
            audit: AuditLog::new(audit_key),
            audit_alert: None,
        };
        vault.audit("praticien", AuditAction::VaultCreated, None)?;
        Ok(vault)
    }

    pub fn open(path: &Path) -> Result<Self, VaultError> {
        if !path.exists() {
            return Err(VaultError::NotFound);
        }
        let conn = Connection::open(path)?;
        configure(&conn)?;
        migrate(&conn)?;
        Ok(Self { conn })
    }

    // L'erreur rend le coffre à l'appelant ; elle est rare (échec d'authentification),
    // sa taille n'a donc pas d'importance.
    #[allow(clippy::result_large_err)]
    pub fn unlock(self, password: &[u8]) -> Result<UnlockedVault, (Self, VaultError)> {
        let now = OffsetDateTime::now_utc().unix_timestamp();
        self.unlock_at(password, now)
    }

    /// Déverrouille à l'instant `now` (secondes Unix) ; en cas d'échec, rend le coffre.
    #[allow(clippy::result_large_err)]
    pub fn unlock_at(self, password: &[u8], now: i64) -> Result<UnlockedVault, (Self, VaultError)> {
        match self.try_unlock(password, now) {
            Ok(master) => self.finish_unlock(master),
            Err(e) => Err((self, e)),
        }
    }

    /// Termine un déverrouillage : recharge et vérifie le journal d'audit.
    #[allow(clippy::result_large_err)]
    fn finish_unlock(self, master: SecretKey) -> Result<UnlockedVault, (Self, VaultError)> {
        match Self::load_audit(&self.conn, &master) {
            Ok((audit, audit_alert)) => Ok(UnlockedVault {
                conn: self.conn,
                master,
                audit,
                audit_alert,
            }),
            Err(e) => Err((self, e)),
        }
    }

    fn try_unlock(&self, password: &[u8], now: i64) -> Result<SecretKey, VaultError> {
        let wrapped = wrapped_master(&self.conn)?;
        if wrapped.is_empty() {
            // Sans facteur matériel, la phrase de passe seule ne peut rien ouvrir.
            check_lockout(&self.conn, now)?;
            return Err(VaultError::HardwareBound);
        }
        let pk = self.password_key_at(password, now)?;

        // Le compteur n'est remis à zéro qu'après l'authentification complète
        // (voir `UnlockedVault::reset_lockout`) : un second facteur faux compte aussi.
        match unwrap_key(&pk.kek, &wrapped, MASTER_CONTEXT) {
            Ok(master) => Ok(master),
            Err(_) => {
                register_failure(&self.conn, now)?;
                Err(VaultError::WrongPassword)
            }
        }
    }
}

fn check_lockout(conn: &Connection, now: i64) -> Result<(), VaultError> {
    let next_at: i64 = conn.query_row(
        "SELECT next_attempt_at FROM lockout WHERE id = 1",
        [],
        |r| r.get(0),
    )?;
    if now < next_at {
        return Err(VaultError::TooManyAttempts(next_at - now));
    }
    Ok(())
}

fn wrapped_master(conn: &Connection) -> Result<Vec<u8>, VaultError> {
    Ok(
        conn.query_row("SELECT wrapped_master FROM meta WHERE id = 1", [], |r| {
            r.get(0)
        })?,
    )
}

/// Dérive la KEK de la phrase de passe avec les paramètres stockés (Argon2id).
fn derive_stored_kek(conn: &Connection, password: &[u8]) -> Result<SecretKey, VaultError> {
    let (salt, m, t, p): (Vec<u8>, u32, u32, u32) = conn.query_row(
        "SELECT kdf_salt, kdf_memory_kib, kdf_iterations, kdf_parallelism FROM meta WHERE id = 1",
        [],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
    )?;
    let salt: [u8; SALT_LEN] = salt
        .try_into()
        .map_err(|_| VaultError::Integrity("sel invalide".into()))?;
    let params = KdfParams {
        memory_kib: m,
        iterations: t,
        parallelism: p,
    };
    Ok(derive_kek(password, &salt, params)?)
}

fn register_failure(conn: &Connection, now: i64) -> Result<(), VaultError> {
    let failures: u32 = conn.query_row("SELECT failures FROM lockout WHERE id = 1", [], |r| {
        r.get(0)
    })?;
    let failures = failures.saturating_add(1);
    conn.execute(
        "UPDATE lockout SET failures = ?1, next_attempt_at = ?2 WHERE id = 1",
        params![failures, now + lockout_delay(failures)],
    )?;
    Ok(())
}

/// Délai imposé après `failures` échecs consécutifs : aucun pendant les
/// `MAX_ATTEMPTS - 1` premiers, puis un blocage de `LOCKOUT_SECS` à chaque nouvel échec.
pub fn lockout_delay(failures: u32) -> i64 {
    if failures < MAX_ATTEMPTS {
        0
    } else {
        LOCKOUT_SECS
    }
}

fn failures(conn: &Connection) -> Result<u32, VaultError> {
    Ok(
        conn.query_row("SELECT failures FROM lockout WHERE id = 1", [], |r| {
            r.get(0)
        })?,
    )
}

/// Essais restants avant le blocage (0 : bloqué, ou dernier essai après un blocage).
fn remaining(conn: &Connection) -> Result<u32, VaultError> {
    Ok(MAX_ATTEMPTS.saturating_sub(failures(conn)?))
}

impl LockedVault {
    /// Essais restants avant le blocage de la connexion normale.
    pub fn remaining_attempts(&self) -> Result<u32, VaultError> {
        remaining(&self.conn)
    }

    /// Erreur [`VaultError::TooManyAttempts`] si la connexion est bloquée à `now`.
    pub fn check_lockout_at(&self, now: i64) -> Result<(), VaultError> {
        check_lockout(&self.conn, now)
    }
}

impl UnlockedVault {
    /// Verrouille : les clés sont effacées, seul le fichier chiffré subsiste.
    pub fn lock(self) -> Result<LockedVault, VaultError> {
        let UnlockedVault { conn, .. } = self;
        Ok(LockedVault { conn })
    }

    /// Essais restants avant le blocage de la connexion normale.
    pub fn remaining_attempts(&self) -> Result<u32, VaultError> {
        remaining(&self.conn)
    }

    /// Signale un échec du second facteur (le mot de passe était correct).
    pub fn register_failure(&self, now: i64) -> Result<(), VaultError> {
        register_failure(&self.conn, now)
    }

    /// Remet le compteur d'échecs à zéro, après une authentification complète réussie.
    pub fn reset_lockout(&self) -> Result<(), VaultError> {
        self.conn.execute(
            "UPDATE lockout SET failures = 0, next_attempt_at = 0 WHERE id = 1",
            [],
        )?;
        Ok(())
    }

    /// Enregistre (ou remplace) un enregistrement chiffré. Une nouvelle clé de données
    /// est générée à chaque écriture.
    pub fn put(&self, kind: &str, id: &str, plaintext: &[u8]) -> Result<(), VaultError> {
        check_kind(kind)?;
        let dek = SecretKey::generate();
        let wrapped = wrap_key(&self.master, &dek, &dek_context(kind, id))?;
        let sealed = seal(&dek, plaintext, &record_aad(kind, id))?;
        self.conn.execute(
            "INSERT INTO records (kind, id, wrapped_dek, sealed, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT (kind, id) DO UPDATE SET wrapped_dek = excluded.wrapped_dek,
               sealed = excluded.sealed, updated_at = excluded.updated_at",
            params![kind, id, wrapped, sealed, now_rfc3339()],
        )?;
        Ok(())
    }

    /// Lit et déchiffre un enregistrement. Le tampon rendu s'efface à sa destruction.
    pub fn get(&self, kind: &str, id: &str) -> Result<Zeroizing<Vec<u8>>, VaultError> {
        check_kind(kind)?;
        let (wrapped, sealed): (Vec<u8>, Vec<u8>) = self
            .conn
            .query_row(
                "SELECT wrapped_dek, sealed FROM records WHERE kind = ?1 AND id = ?2",
                params![kind, id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?
            .ok_or(VaultError::RecordNotFound)?;
        let dek = unwrap_key(&self.master, &wrapped, &dek_context(kind, id))?;
        Ok(Zeroizing::new(open(&dek, &sealed, &record_aad(kind, id))?))
    }

    pub fn put_json<T: serde::Serialize>(
        &self,
        kind: &str,
        id: &str,
        value: &T,
    ) -> Result<(), VaultError> {
        let bytes = Zeroizing::new(serde_json::to_vec(value)?);
        self.put(kind, id, &bytes)
    }

    pub fn get_json<T: serde::de::DeserializeOwned>(
        &self,
        kind: &str,
        id: &str,
    ) -> Result<T, VaultError> {
        Ok(serde_json::from_slice(&self.get(kind, id)?)?)
    }

    /// Supprime un enregistrement (écrasement physique grâce à `secure_delete`).
    pub fn delete(&self, kind: &str, id: &str) -> Result<(), VaultError> {
        check_kind(kind)?;
        let n = self.conn.execute(
            "DELETE FROM records WHERE kind = ?1 AND id = ?2",
            params![kind, id],
        )?;
        if n == 0 {
            return Err(VaultError::RecordNotFound);
        }
        Ok(())
    }

    /// Identifiants des enregistrements d'un type.
    pub fn list(&self, kind: &str) -> Result<Vec<String>, VaultError> {
        check_kind(kind)?;
        let mut stmt = self
            .conn
            .prepare("SELECT id FROM records WHERE kind = ?1 ORDER BY id")?;
        let ids = stmt
            .query_map([kind], |r| r.get(0))?
            .collect::<Result<_, _>>()?;
        Ok(ids)
    }

    /// Nouvel identifiant aléatoire, sans lien avec l'identité du patient.
    pub fn new_id() -> String {
        uuid::Uuid::new_v4().to_string()
    }

    pub fn put_secret(&self, name: &str, value: &[u8]) -> Result<(), VaultError> {
        put_secret(&self.conn, &self.master, name, value)
    }

    pub fn get_secret(&self, name: &str) -> Result<Option<Zeroizing<Vec<u8>>>, VaultError> {
        get_secret(&self.conn, &self.master, name)
    }

    /// Inscrit et persiste une entrée d'audit. Si l'écriture échoue (disque plein…),
    /// l'entrée est aussi retirée de la mémoire : le journal en mémoire et celui du
    /// disque ne divergent jamais, sans quoi la chaîne paraîtrait rompue.
    pub fn audit(
        &mut self,
        actor: &str,
        action: AuditAction,
        target: Option<&str>,
    ) -> Result<(), VaultError> {
        let cp = self.audit.checkpoint();
        let entry = self.audit.append(actor, action, target).clone();
        let (conn, master) = (&self.conn, &self.master);
        let written = savepoint(conn, || {
            conn.execute(
                "INSERT INTO audit (seq, entry) VALUES (?1, ?2)",
                params![entry.seq as i64, serde_json::to_string(&entry)?],
            )?;
            let head = format!("{}:{}", entry.seq, entry.mac);
            put_secret(conn, master, AUDIT_HEAD_SECRET, head.as_bytes())
        });
        if written.is_err() {
            self.audit.rollback(cp);
        }
        written
    }

    /// Exécute `f` en tout ou rien : modifications des enregistrements **et** entrées
    /// d'audit. En cas d'erreur, rien n'est écrit et le journal en mémoire est rétabli.
    pub fn atomic<T, E: From<VaultError>>(
        &mut self,
        f: impl FnOnce(&mut Self) -> Result<T, E>,
    ) -> Result<T, E> {
        let cp = self.audit.checkpoint();
        self.conn
            .execute_batch("SAVEPOINT phonalys_atomic")
            .map_err(VaultError::from)?;
        let result = f(self).and_then(|v| {
            self.conn
                .execute_batch("RELEASE phonalys_atomic")
                .map_err(VaultError::from)?;
            Ok(v)
        });
        if result.is_err() {
            let _ = self
                .conn
                .execute_batch("ROLLBACK TO phonalys_atomic; RELEASE phonalys_atomic");
            self.audit.rollback(cp);
        }
        result
    }

    /// Altération du journal d'audit constatée à l'ouverture, le cas échéant.
    pub fn audit_alert(&self) -> Option<&str> {
        self.audit_alert.as_deref()
    }

    pub fn audit_entries(&self) -> &[AuditEntry] {
        self.audit.entries()
    }
}

/// Résultat de la vérification d'une sauvegarde.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackupReport {
    /// Enregistrements déchiffrés avec succès.
    pub records: usize,
    /// Entrées du journal d'audit.
    pub audit_entries: usize,
    /// Chaîne du journal intacte. Une sauvegarde au journal altéré reste une
    /// sauvegarde des dossiers : elle est gardée, et l'altération signalée.
    pub audit_intact: bool,
}

impl UnlockedVault {
    /// Copie cohérente du coffre (SQLite `VACUUM INTO`), même pendant son utilisation.
    /// La copie est aussi chiffrée que l'original : sans la phrase de passe (et, en mode
    /// lié, une clé ou un code de récupération), elle est illisible.
    pub fn snapshot(&self, dest: &Path) -> Result<(), VaultError> {
        if dest.exists() {
            return Err(VaultError::AlreadyExists);
        }
        if !self.conn.is_autocommit() {
            return Err(VaultError::Precondition(
                "sauvegarde impossible pendant une transaction",
            ));
        }
        let dest_str = dest
            .to_str()
            .ok_or(VaultError::Precondition("chemin de sauvegarde non UTF-8"))?;
        self.conn.execute("VACUUM INTO ?1", [dest_str])?;
        restrict_permissions(dest).map_err(|e| VaultError::Integrity(e.to_string()))?;
        Ok(())
    }

    /// Test de restauration : ouvre la sauvegarde en lecture seule, déchiffre **chaque**
    /// enregistrement et vérifie la chaîne du journal d'audit.
    pub fn verify_backup(&self, path: &Path) -> Result<BackupReport, VaultError> {
        let conn = open_read_only(path)?;
        let format: i64 =
            conn.query_row("SELECT format FROM meta WHERE id = 1", [], |r| r.get(0))?;
        if format != FORMAT_VERSION {
            return Err(VaultError::UnsupportedFormat(format));
        }
        let mut stmt = conn.prepare("SELECT kind, id, wrapped_dek, sealed FROM records")?;
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, Vec<u8>>(2)?,
                    r.get::<_, Vec<u8>>(3)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        for (kind, id, wrapped, sealed) in &rows {
            let dek = unwrap_key(&self.master, wrapped, &dek_context(kind, id))?;
            drop(Zeroizing::new(open(&dek, sealed, &record_aad(kind, id))?));
        }
        let (audit, alert) = LockedVault::load_audit(&conn, &self.master)?;
        Ok(BackupReport {
            records: rows.len(),
            audit_entries: audit.entries().len(),
            audit_intact: alert.is_none(),
        })
    }
}

fn open_read_only(path: &Path) -> Result<Connection, VaultError> {
    if !path.is_file() {
        return Err(VaultError::NotFound);
    }
    Ok(Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?)
}

impl LockedVault {
    /// Contrôle sommaire d'un fichier avant restauration : c'est bien un coffre Phonalys
    /// d'un format connu. Son contenu reste illisible sans les facteurs d'origine.
    pub fn inspect(path: &Path) -> Result<(), VaultError> {
        let conn =
            open_read_only(path).map_err(|_| VaultError::Integrity("fichier illisible".into()))?;
        let format: i64 = conn
            .query_row("SELECT format FROM meta WHERE id = 1", [], |r| r.get(0))
            .map_err(|_| VaultError::Integrity("ce fichier n'est pas un coffre Phonalys".into()))?;
        if !(1..=FORMAT_VERSION).contains(&format) {
            return Err(VaultError::UnsupportedFormat(format));
        }
        let ok: String = conn.query_row("PRAGMA integrity_check", [], |r| r.get(0))?;
        if ok != "ok" {
            return Err(VaultError::Integrity("fichier endommagé".into()));
        }
        Ok(())
    }

    /// Recharge le journal d'audit et vérifie sa chaîne et son ancrage. Un journal altéré
    /// n'empêche pas l'ouverture : l'altération est renvoyée comme alerte.
    fn load_audit(
        conn: &Connection,
        master: &SecretKey,
    ) -> Result<(AuditLog, Option<String>), VaultError> {
        let audit_key = get_secret(conn, master, AUDIT_KEY_SECRET)?
            .ok_or_else(|| VaultError::Integrity("clé d'audit absente".into()))?;
        let key_bytes: [u8; 32] = audit_key
            .as_slice()
            .try_into()
            .map_err(|_| VaultError::Integrity("clé d'audit invalide".into()))?;
        let mut stmt = conn.prepare("SELECT seq, entry FROM audit ORDER BY seq")?;
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?
            .collect::<Result<Vec<_>, _>>()?;
        let mut alerts = Vec::new();
        let mut entries = Vec::with_capacity(rows.len());
        let mut max_seq = None;
        for (seq, text) in &rows {
            max_seq = max_seq.max(Some(*seq));
            match serde_json::from_str::<AuditEntry>(text) {
                Ok(e) if e.seq as i64 == *seq => entries.push(e),
                _ => alerts.push(format!("entrée {seq} illisible")),
            }
        }
        let next_seq = max_seq.map_or(0, |s| s.saturating_add(1) as u64);
        let (log, broken) = AuditLog::recover(SecretKey::from_bytes(key_bytes), entries, next_seq);
        if let Some(e) = broken {
            alerts.push(e.to_string());
        }
        if let Some(head) = get_secret(conn, master, AUDIT_HEAD_SECRET)? {
            let last = log.entries().last().map(|e| format!("{}:{}", e.seq, e.mac));
            if last.as_deref().map(str::as_bytes) != Some(head.as_slice()) {
                alerts.push("dernières entrées supprimées ou remplacées".into());
            }
        }
        let alert = (!alerts.is_empty()).then(|| alerts.join(" ; "));
        Ok((log, alert))
    }
}

fn put_secret(
    conn: &Connection,
    master: &SecretKey,
    name: &str,
    value: &[u8],
) -> Result<(), VaultError> {
    let sealed = seal(master, value, &secret_aad(name))?;
    conn.execute(
        "INSERT INTO secrets (name, sealed) VALUES (?1, ?2)
         ON CONFLICT (name) DO UPDATE SET sealed = excluded.sealed",
        params![name, sealed],
    )?;
    Ok(())
}

fn get_secret(
    conn: &Connection,
    master: &SecretKey,
    name: &str,
) -> Result<Option<Zeroizing<Vec<u8>>>, VaultError> {
    let sealed: Option<Vec<u8>> = conn
        .query_row("SELECT sealed FROM secrets WHERE name = ?1", [name], |r| {
            r.get(0)
        })
        .optional()?;
    sealed
        .map(|s| Ok(Zeroizing::new(open(master, &s, &secret_aad(name))?)))
        .transpose()
}

fn check_kind(kind: &str) -> Result<(), VaultError> {
    let ok = !kind.is_empty()
        && kind.len() <= 32
        && kind.bytes().all(|b| b.is_ascii_lowercase() || b == b'_');
    if ok {
        Ok(())
    } else {
        Err(VaultError::InvalidKind)
    }
}

fn record_aad(kind: &str, id: &str) -> Vec<u8> {
    format!("phonalys:v1:record:{kind}:{id}").into_bytes()
}

fn dek_context(kind: &str, id: &str) -> Vec<u8> {
    format!("phonalys:v1:dek:{kind}:{id}").into_bytes()
}

fn secret_aad(name: &str) -> Vec<u8> {
    format!("phonalys:v1:secret:{name}").into_bytes()
}

fn now_rfc3339() -> String {
    OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_default()
}
