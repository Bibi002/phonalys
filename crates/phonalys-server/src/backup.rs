//! Sauvegardes chiffrées et restauration.
//!
//! Une sauvegarde est une copie cohérente du coffre : elle est donc chiffrée comme lui
//! et ne s'ouvre qu'avec la phrase de passe (et, en mode lié, une clé ou un code de
//! récupération) **en vigueur au moment de la sauvegarde**. Chaque sauvegarde est
//! aussitôt soumise à un test de restauration : tous les enregistrements sont
//! déchiffrés et la chaîne du journal d'audit est vérifiée.
//!
//! - automatique : une fois par 24 h pendant l'utilisation (coffre ouvert) ;
//! - copie supplémentaire facultative vers un second dossier (clé USB, NAS) pour la
//!   règle 3-2-1 ;
//! - conservation des [`KEEP`] plus récentes dans chaque dossier, dans l'ordre où elles
//!   ont été faites (pas d'après l'horloge : une horloge déréglée ne doit jamais faire
//!   supprimer la sauvegarde la plus récente) ;
//! - un échec, une copie hors du disque absente ou une sauvegarde trop ancienne sont
//!   signalés à l'accueil : une sauvegarde qui échoue en silence n'en est pas une.

use crate::error::ApiError;
use crate::records::settings;
use crate::state::{now, AppState, Phase, ACTOR};
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use phonalys_core::audit::AuditAction;
use phonalys_crypto::constant_time_eq;
use phonalys_vault::{LockedVault, UnlockedVault};
use serde::{Deserialize, Serialize};
use std::path::{Path as FsPath, PathBuf};
use time::OffsetDateTime;

/// Sauvegardes conservées dans chaque dossier.
pub const KEEP: usize = 14;
/// Intervalle des sauvegardes automatiques.
pub const AUTO_INTERVAL_SECS: u64 = 24 * 3600;
/// Taille maximale d'un fichier restauré.
pub const MAX_RESTORE_BYTES: usize = 1024 * 1024 * 1024;
const EXTENSION: &str = "phonalys-backup";
const LOG: &str = "backup-log";
/// Dernier échec d'une sauvegarde, effacé à la suivante réussie.
const FAILURE: &str = "backup-failure";
/// Au-delà, l'absence de sauvegarde réussie est signalée.
const STALE_SECS: u64 = 3 * 24 * 3600;
/// Au-delà, l'absence de copie dans le second dossier est signalée.
const COPY_STALE_SECS: u64 = 7 * 24 * 3600;
const LOG_LEN: usize = 60;
/// En-tête portant le jeton d'installation lors d'une restauration.
pub const SETUP_TOKEN_HEADER: &str = "x-phonalys-setup-token";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupEntry {
    pub name: String,
    pub created_at: u64,
    pub size: u64,
    pub records: usize,
    pub audit_entries: usize,
    /// Le test de restauration a réussi.
    pub verified: bool,
    /// Copie déposée dans le dossier supplémentaire.
    pub copied: bool,
    #[serde(default)]
    pub copy_error: Option<String>,
}

fn log(vault: &UnlockedVault) -> Result<Vec<BackupEntry>, ApiError> {
    Ok(match vault.get_secret(LOG)? {
        Some(b) => serde_json::from_slice(&b)?,
        None => Vec::new(),
    })
}

fn save_log(vault: &UnlockedVault, entries: &[BackupEntry]) -> Result<(), ApiError> {
    let start = entries.len().saturating_sub(LOG_LEN);
    Ok(vault.put_secret(LOG, &serde_json::to_vec(&entries[start..])?)?)
}

/// Nom horodaté, sans aucune information sur le cabinet ni les patients.
fn backup_name(at: u64) -> String {
    let t = OffsetDateTime::from_unix_timestamp(at as i64).unwrap_or(OffsetDateTime::UNIX_EPOCH);
    format!(
        "phonalys-{:04}{:02}{:02}-{:02}{:02}{:02}.{EXTENSION}",
        t.year(),
        u8::from(t.month()),
        t.day(),
        t.hour(),
        t.minute(),
        t.second()
    )
}

/// Un nom reçu du client ne peut désigner qu'un fichier de sauvegarde du dossier.
fn valid_name(name: &str) -> bool {
    name.len() <= 64
        && name.starts_with("phonalys-")
        && name.ends_with(&format!(".{EXTENSION}"))
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'.')
        && !name.contains("..")
}

fn backups_in(dir: &FsPath) -> Vec<PathBuf> {
    let mut list: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(valid_name)
        })
        .collect();
    list.sort();
    list
}

/// Ne garde que les [`KEEP`] dernières sauvegardes faites par ce coffre, dans l'ordre du
/// journal des sauvegardes. Les fichiers que ce journal ne connaît pas (autre coffre,
/// copie manuelle) ne sont jamais supprimés.
fn prune(dir: &FsPath, log: &[BackupEntry]) {
    let keep: Vec<&str> = log
        .iter()
        .rev()
        .take(KEEP)
        .map(|e| e.name.as_str())
        .collect();
    for path in backups_in(dir) {
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if keep.contains(&name) || !log.iter().any(|e| e.name == name) {
            continue;
        }
        if let Err(e) = std::fs::remove_file(&path) {
            tracing::warn!(error = %e, "suppression d'une ancienne sauvegarde");
        }
    }
}

/// Force l'écriture sur le support (clé USB retirée juste après, coupure de courant).
fn sync_file(path: &FsPath) -> std::io::Result<()> {
    // Ouvert en écriture : Windows (FlushFileBuffers) l'exige.
    std::fs::OpenOptions::new()
        .write(true)
        .open(path)?
        .sync_all()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupFailure {
    pub at: u64,
    pub message: String,
}

fn record_failure(vault: &UnlockedVault, at: u64, message: &str) {
    let failure = BackupFailure {
        at,
        message: message.to_owned(),
    };
    // Au mieux : si le disque est plein, même cette écriture peut échouer.
    if let Ok(bytes) = serde_json::to_vec(&failure) {
        if let Err(e) = vault.put_secret(FAILURE, &bytes) {
            tracing::error!(error = %e, "enregistrement de l'échec de sauvegarde");
        }
    }
}

fn last_failure(vault: &UnlockedVault) -> Option<BackupFailure> {
    vault
        .get_secret(FAILURE)
        .ok()
        .flatten()
        .and_then(|b| serde_json::from_slice(&b).ok())
}

fn create_private_dir(dir: &FsPath) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

/// Sauvegarde, test de restauration, copie supplémentaire, rotation. Un échec est
/// enregistré pour être signalé à l'accueil.
pub(crate) fn run_backup(
    vault: &mut UnlockedVault,
    dir: &FsPath,
    at: u64,
) -> Result<BackupEntry, ApiError> {
    let result = try_backup(vault, dir, at);
    match &result {
        Ok(_) => {
            if last_failure(vault).is_some() {
                // « null » ne se relit pas comme un échec : l'alerte disparaît.
                let _ = vault.put_secret(FAILURE, b"null");
            }
        }
        Err(e) => record_failure(vault, at, &failure_message(e)),
    }
    result
}

fn failure_message(e: &ApiError) -> String {
    match e {
        ApiError::BadRequest(m) => m.clone(),
        _ => "la sauvegarde ou son test de restauration a échoué (disque plein ?)".into(),
    }
}

fn try_backup(vault: &mut UnlockedVault, dir: &FsPath, at: u64) -> Result<BackupEntry, ApiError> {
    create_private_dir(dir).map_err(|e| {
        tracing::error!(error = %e, "dossier de sauvegarde");
        ApiError::BadRequest("dossier de sauvegarde inaccessible".into())
    })?;
    let mut name = backup_name(at);
    let mut n = 1;
    while dir.join(&name).exists() {
        name = backup_name(at).replace(&format!(".{EXTENSION}"), &format!("-{n}.{EXTENSION}"));
        n += 1;
    }
    let path = dir.join(&name);
    vault.snapshot(&path)?;
    if let Err(e) = sync_file(&path) {
        let _ = std::fs::remove_file(&path);
        tracing::error!(error = %e, "écriture de la sauvegarde sur le disque");
        return Err(ApiError::BadRequest(
            "la sauvegarde n'a pas pu être écrite sur le disque".into(),
        ));
    }
    let report = match vault.verify_backup(&path) {
        Ok(r) => r,
        Err(e) => {
            tracing::error!(error = %e, "ÉCHEC du test de restauration : sauvegarde supprimée");
            let _ = std::fs::remove_file(&path);
            return Err(ApiError::Internal);
        }
    };
    let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
    if !report.audit_intact {
        tracing::error!("sauvegarde faite, mais le journal d'audit du coffre est altéré");
    }

    let mut entry = BackupEntry {
        name: name.clone(),
        created_at: at,
        size,
        records: report.records,
        audit_entries: report.audit_entries,
        verified: true,
        copied: false,
        copy_error: None,
    };
    let mut entries = log(vault)?;
    let extra_dir = settings(vault)?.backup_extra_dir.map(PathBuf::from);
    if let Some(extra) = &extra_dir {
        let copy = extra.join(&name);
        match std::fs::copy(&path, &copy).and_then(|_| sync_file(&copy)) {
            Ok(()) => entry.copied = true,
            Err(e) => {
                tracing::warn!(error = %e, "copie vers le dossier supplémentaire impossible");
                entry.copy_error =
                    Some("dossier supplémentaire inaccessible (clé USB débranchée ?)".into());
            }
        }
    }
    entries.push(entry.clone());
    save_log(vault, &entries)?;
    prune(dir, &entries);
    if let (Some(extra), true) = (&extra_dir, entry.copied) {
        let copied: Vec<BackupEntry> = entries.iter().filter(|e| e.copied).cloned().collect();
        prune(extra, &copied);
    }
    vault.audit(ACTOR, AuditAction::Export, Some(&name))?;
    Ok(entry)
}

/// Appelée périodiquement : sauvegarde si le coffre est ouvert, la sauvegarde
/// automatique activée et la dernière sauvegarde a plus de 24 h.
pub fn auto_backup(state: &AppState, at: u64) {
    let mut inner = state.lock_inner();
    let Phase::Unlocked(vault) = &mut inner.phase else {
        return;
    };
    let due = settings(vault).is_ok_and(|s| s.auto_backup)
        && log(vault).is_ok_and(|l| {
            l.iter()
                .rev()
                .find(|e| e.verified)
                // Horloge revenue en arrière : on sauvegarde plutôt que d'attendre.
                .is_none_or(|e| e.created_at > at || at - e.created_at >= AUTO_INTERVAL_SECS)
        });
    if due {
        match run_backup(vault, &state.config.backup_dir, at) {
            Ok(e) => {
                tracing::info!(name = %e.name, records = e.records, "sauvegarde automatique vérifiée")
            }
            Err(e) => tracing::error!(error = ?e, "sauvegarde automatique impossible"),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct BackupStatus {
    dir: String,
    extra_dir: Option<String>,
    auto: bool,
    last: Option<BackupEntry>,
    /// Fichiers présents dans le dossier principal, du plus récent au plus ancien.
    files: Vec<BackupEntry>,
    /// Dernier échec, s'il n'a pas été suivi d'une sauvegarde réussie.
    failure: Option<BackupFailure>,
    /// Ce qui menace les données, à afficher à l'accueil.
    warnings: Vec<String>,
}

/// Risques de perte de données liés aux sauvegardes, en clair pour le praticien.
pub(crate) fn warnings(
    entries: &[BackupEntry],
    extra_dir: Option<&str>,
    failure: Option<&BackupFailure>,
    now: u64,
) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(f) = failure {
        out.push(format!("La dernière sauvegarde a échoué : {}.", f.message));
    }
    let last_ok = entries.iter().rev().find(|e| e.verified);
    match last_ok {
        None => out.push("Aucune sauvegarde n'a encore été faite.".into()),
        Some(e) if now.saturating_sub(e.created_at) > STALE_SECS => {
            out.push("Aucune sauvegarde réussie depuis plus de 3 jours.".into())
        }
        Some(_) => {}
    }
    match extra_dir {
        None => out.push(
            "Les sauvegardes sont sur le même disque que le coffre : si ce disque tombe en \
             panne ou si l'ordinateur est volé, tout est perdu. Indiquez un second dossier \
             (clé USB, disque externe, NAS)."
                .into(),
        ),
        Some(_) => {
            let last_copy = entries.iter().rev().find(|e| e.copied);
            if last_copy.is_none_or(|e| now.saturating_sub(e.created_at) > COPY_STALE_SECS) {
                out.push(
                    "Aucune copie dans le second dossier depuis plus de 7 jours (clé USB \
                     débranchée ?)."
                        .into(),
                );
            }
        }
    }
    out
}

pub async fn status(State(state): State<AppState>) -> Result<Json<BackupStatus>, ApiError> {
    state.with_vault(|vault| {
        let entries = log(vault)?;
        let s = settings(vault)?;
        let files = backups_in(&state.config.backup_dir)
            .into_iter()
            .rev()
            .filter_map(|p| {
                let name = p.file_name()?.to_str()?.to_owned();
                let size = std::fs::metadata(&p).map(|m| m.len()).unwrap_or(0);
                Some(
                    entries
                        .iter()
                        .rev()
                        .find(|e| e.name == name)
                        .cloned()
                        .unwrap_or(BackupEntry {
                            name,
                            created_at: 0,
                            size,
                            records: 0,
                            audit_entries: 0,
                            verified: false,
                            copied: false,
                            copy_error: None,
                        }),
                )
            })
            .collect();
        let failure = last_failure(vault);
        Ok(Json(BackupStatus {
            dir: state.config.backup_dir.display().to_string(),
            warnings: warnings(
                &entries,
                s.backup_extra_dir.as_deref(),
                failure.as_ref(),
                now(),
            ),
            extra_dir: s.backup_extra_dir,
            auto: s.auto_backup,
            last: entries.last().cloned(),
            files,
            failure,
        }))
    })
}

pub async fn create(State(state): State<AppState>) -> Result<Json<BackupEntry>, ApiError> {
    crate::api::blocking(move || {
        state.with_vault(|vault| run_backup(vault, &state.config.backup_dir, now()).map(Json))
    })
    .await
}

/// Refait le test de restauration d'une sauvegarde existante.
pub async fn verify(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<BackupEntry>, ApiError> {
    if !valid_name(&name) {
        return Err(ApiError::NotFound);
    }
    crate::api::blocking(move || {
        state.with_vault(|vault| {
            let path = state.config.backup_dir.join(&name);
            if !path.is_file() {
                return Err(ApiError::NotFound);
            }
            let report = vault.verify_backup(&path).map_err(|e| {
                tracing::warn!(error = %e, "test de restauration en échec");
                ApiError::BadRequest(
                    "test de restauration en échec : sauvegarde endommagée, ou faite avec une \
                     autre phrase de passe"
                        .into(),
                )
            })?;
            let mut entries = log(vault)?;
            let entry = BackupEntry {
                name: name.clone(),
                created_at: entries
                    .iter()
                    .find(|e| e.name == name)
                    .map_or(0, |e| e.created_at),
                size: std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0),
                records: report.records,
                audit_entries: report.audit_entries,
                verified: true,
                copied: entries.iter().any(|e| e.name == name && e.copied),
                copy_error: None,
            };
            match entries.iter_mut().find(|e| e.name == name) {
                Some(known) => *known = entry.clone(),
                None => entries.insert(0, entry.clone()),
            }
            save_log(vault, &entries)?;
            Ok(Json(entry))
        })
    })
    .await
}

/// Téléchargement d'une sauvegarde (pour la copier ailleurs). Le fichier est chiffré.
pub async fn download(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Response, ApiError> {
    if !valid_name(&name) {
        return Err(ApiError::NotFound);
    }
    let path = state.config.backup_dir.join(&name);
    state.with_vault(|vault| {
        vault.audit(ACTOR, AuditAction::Export, Some(&name))?;
        Ok(())
    })?;
    let bytes = tokio::fs::read(&path)
        .await
        .map_err(|_| ApiError::NotFound)?;
    let mut res = (StatusCode::OK, bytes).into_response();
    let h = res.headers_mut();
    h.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/octet-stream"),
    );
    h.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!("attachment; filename=\"{name}\""))
            .map_err(|_| ApiError::Internal)?,
    );
    Ok(res)
}

/// Restauration sur une installation vierge : le fichier remplace le coffre absent.
/// Le jeton d'installation est exigé, comme pour une installation normale.
pub async fn restore(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<StatusCode, ApiError> {
    crate::api::blocking(move || {
        let mut inner = state.lock_inner();
        let expected = inner.setup_token.as_deref().ok_or(ApiError::Conflict)?;
        let given = headers
            .get(SETUP_TOKEN_HEADER)
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .trim();
        if !constant_time_eq(expected.as_bytes(), given.as_bytes()) {
            tracing::warn!("tentative de restauration avec un jeton invalide");
            return Err(ApiError::Forbidden);
        }
        let vault_path = &state.config.vault_path;
        let tmp = vault_path.with_extension("restore-tmp");
        std::fs::write(&tmp, &body).map_err(|_| ApiError::Internal)?;
        if let Err(e) = LockedVault::inspect(&tmp) {
            let _ = std::fs::remove_file(&tmp);
            tracing::warn!(error = %e, "fichier de restauration refusé");
            return Err(ApiError::BadRequest(
                "ce fichier n'est pas une sauvegarde Phonalys valide".into(),
            ));
        }
        std::fs::rename(&tmp, vault_path).map_err(|_| ApiError::Internal)?;
        inner.setup_token = None;
        tracing::info!("coffre restauré depuis une sauvegarde");
        Ok(StatusCode::NO_CONTENT)
    })
    .await
}

/// Restauration en ligne de commande (`phonalys-server restore <fichier>`).
pub fn restore_file(src: &FsPath, vault_path: &FsPath) -> Result<(), String> {
    if vault_path.exists() {
        return Err(format!(
            "un coffre existe déjà ({}). Déplacez-le avant de restaurer.",
            vault_path.display()
        ));
    }
    LockedVault::inspect(src).map_err(|e| format!("sauvegarde refusée : {e}"))?;
    std::fs::copy(src, vault_path).map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_strict() {
        assert!(valid_name(&backup_name(1_761_350_400)));
        assert_eq!(
            backup_name(1_761_350_400),
            "phonalys-20251025-000000.phonalys-backup"
        );
        for bad in [
            "../cabinet.phonalys-vault",
            "phonalys-../../x.phonalys-backup",
            "phonalys-a/b.phonalys-backup",
            "autre.phonalys-backup",
            "phonalys-1.txt",
        ] {
            assert!(!valid_name(bad), "{bad}");
        }
    }
}
