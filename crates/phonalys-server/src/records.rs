//! Dossier clinique (séances, bilans, objectifs, résultats d'exercices), réglages du
//! cabinet et consultation du journal d'audit.
//!
//! Chaque enregistrement est un objet JSON chiffré individuellement dans le coffre.
//! Les types sont fixés par une liste blanche : un client ne peut pas créer de type
//! arbitraire. Un enregistrement rattaché à un patient (`patient_id`) disparaît avec
//! lui lors de l'effacement du dossier (droit à l'effacement, RGPD art. 17).

use crate::api::{parse_id, PATIENT};
use crate::error::ApiError;
use crate::state::{AppState, ACTOR};
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use phonalys_core::audit::AuditAction;
use phonalys_vault::{UnlockedVault, VaultError};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// Types d'enregistrements cliniques acceptés.
pub const CLINICAL_KINDS: &[&str] = &["seance", "bilan", "objectif", "exercice"];
/// Taille maximale d'un enregistrement sérialisé.
const MAX_RECORD_BYTES: usize = 2 * 1024 * 1024;
const SETTINGS: &str = "settings";
/// Délais de verrouillage automatique proposés, en minutes.
pub const IDLE_CHOICES_MINUTES: &[u64] = &[5, 10, 15, 30, 60];
const DEFAULT_IDLE_MINUTES: u64 = 15;

fn kind(k: &str) -> Result<&'static str, ApiError> {
    CLINICAL_KINDS
        .iter()
        .find(|known| **known == k)
        .copied()
        .ok_or(ApiError::NotFound)
}

fn target(kind: &str, id: &str) -> String {
    format!("{kind}/{id}")
}

/// Vérifie la forme d'un enregistrement et l'existence du patient auquel il se rattache.
fn validate(vault: &UnlockedVault, data: &Value) -> Result<(), ApiError> {
    let obj = data
        .as_object()
        .ok_or_else(|| ApiError::BadRequest("objet JSON attendu".into()))?;
    if serde_json::to_vec(data)?.len() > MAX_RECORD_BYTES {
        return Err(ApiError::BadRequest(
            "enregistrement trop volumineux".into(),
        ));
    }
    if let Some(pid) = obj.get("patient_id") {
        let pid = pid
            .as_str()
            .ok_or_else(|| ApiError::BadRequest("patient_id invalide".into()))?;
        let pid = parse_id(pid).map_err(|_| ApiError::BadRequest("patient_id invalide".into()))?;
        match vault.get(PATIENT, &pid) {
            Ok(_) => {}
            Err(VaultError::RecordNotFound) => {
                return Err(ApiError::BadRequest("patient inconnu".into()))
            }
            Err(e) => return Err(e.into()),
        }
    }
    Ok(())
}

fn with_id(mut value: Value, id: String) -> Value {
    if let Some(obj) = value.as_object_mut() {
        obj.insert("id".into(), Value::String(id));
    }
    value
}

/// L'identifiant fait partie de l'adresse, pas du contenu chiffré.
fn strip_id(mut data: Value) -> Value {
    if let Some(obj) = data.as_object_mut() {
        obj.remove("id");
    }
    data
}

#[derive(Debug, Serialize)]
pub struct Created {
    id: String,
}

#[derive(Deserialize)]
pub struct ListQuery {
    #[serde(default)]
    patient_id: Option<String>,
}

pub async fn list(
    State(state): State<AppState>,
    Path(k): Path<String>,
    Query(q): Query<ListQuery>,
) -> Result<Json<Vec<Value>>, ApiError> {
    let k = kind(&k)?;
    state.with_vault(|vault| {
        let mut out = Vec::new();
        for id in vault.list(k)? {
            let value: Value = vault.get_json(k, &id)?;
            let keep = q
                .patient_id
                .as_deref()
                .is_none_or(|pid| value.get("patient_id").and_then(Value::as_str) == Some(pid));
            if keep {
                out.push(with_id(value, id));
            }
        }
        Ok(Json(out))
    })
}

pub async fn create(
    State(state): State<AppState>,
    Path(k): Path<String>,
    Json(data): Json<Value>,
) -> Result<(StatusCode, Json<Created>), ApiError> {
    let k = kind(&k)?;
    let data = strip_id(data);
    state.with_vault(|vault| {
        validate(vault, &data)?;
        let id = UnlockedVault::new_id();
        vault.put_json(k, &id, &data)?;
        vault.audit(ACTOR, AuditAction::RecordCreated, Some(&target(k, &id)))?;
        Ok((StatusCode::CREATED, Json(Created { id })))
    })
}

pub async fn get(
    State(state): State<AppState>,
    Path((k, id)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    let k = kind(&k)?;
    let id = parse_id(&id)?;
    state.with_vault(|vault| {
        let value: Value = vault.get_json(k, &id)?;
        vault.audit(ACTOR, AuditAction::RecordViewed, Some(&target(k, &id)))?;
        Ok(Json(with_id(value, id)))
    })
}

pub async fn update(
    State(state): State<AppState>,
    Path((k, id)): Path<(String, String)>,
    Json(data): Json<Value>,
) -> Result<StatusCode, ApiError> {
    let k = kind(&k)?;
    let id = parse_id(&id)?;
    let data = strip_id(data);
    state.with_vault(|vault| {
        // Un identifiant inconnu ne crée pas d'enregistrement.
        let _: Value = vault.get_json(k, &id)?;
        validate(vault, &data)?;
        vault.put_json(k, &id, &data)?;
        vault.audit(ACTOR, AuditAction::RecordUpdated, Some(&target(k, &id)))?;
        Ok(StatusCode::NO_CONTENT)
    })
}

pub async fn delete(
    State(state): State<AppState>,
    Path((k, id)): Path<(String, String)>,
) -> Result<StatusCode, ApiError> {
    let k = kind(&k)?;
    let id = parse_id(&id)?;
    state.with_vault(|vault| {
        vault.delete(k, &id)?;
        vault.audit(ACTOR, AuditAction::RecordErased, Some(&target(k, &id)))?;
        Ok(StatusCode::NO_CONTENT)
    })
}

/// Efface tous les enregistrements cliniques d'un patient (avant son dossier).
pub(crate) fn erase_patient_records(
    vault: &mut UnlockedVault,
    patient_id: &str,
) -> Result<usize, ApiError> {
    let mut erased = 0;
    for k in CLINICAL_KINDS {
        for id in vault.list(k)? {
            let value: Value = vault.get_json(k, &id)?;
            if value.get("patient_id").and_then(Value::as_str) == Some(patient_id) {
                vault.delete(k, &id)?;
                vault.audit(ACTOR, AuditAction::RecordErased, Some(&target(k, &id)))?;
                erased += 1;
            }
        }
    }
    Ok(erased)
}

// --- Réglages ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    /// Verrouillage automatique après ce nombre de minutes sans activité.
    #[serde(default = "default_idle")]
    pub idle_timeout_minutes: u64,
    /// Coordonnées du cabinet, reprises sur les bilans imprimés.
    #[serde(default)]
    pub practitioner: Map<String, Value>,
    /// Sauvegarde automatique quotidienne (activée par défaut).
    #[serde(default = "yes")]
    pub auto_backup: bool,
    /// Second dossier recevant une copie de chaque sauvegarde (clé USB, NAS).
    #[serde(default)]
    pub backup_extra_dir: Option<String>,
}

fn yes() -> bool {
    true
}

fn default_idle() -> u64 {
    DEFAULT_IDLE_MINUTES
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            idle_timeout_minutes: DEFAULT_IDLE_MINUTES,
            practitioner: Map::new(),
            auto_backup: true,
            backup_extra_dir: None,
        }
    }
}

pub(crate) fn settings(vault: &UnlockedVault) -> Result<Settings, ApiError> {
    Ok(match vault.get_secret(SETTINGS)? {
        Some(bytes) => serde_json::from_slice(&bytes)?,
        None => Settings::default(),
    })
}

pub async fn get_settings(State(state): State<AppState>) -> Result<Json<Settings>, ApiError> {
    state.with_vault(|vault| Ok(Json(settings(vault)?)))
}

pub async fn put_settings(
    State(state): State<AppState>,
    Json(new): Json<Settings>,
) -> Result<Json<Settings>, ApiError> {
    if !IDLE_CHOICES_MINUTES.contains(&new.idle_timeout_minutes) {
        return Err(ApiError::BadRequest(format!(
            "délai de verrouillage non proposé : choisissez parmi {IDLE_CHOICES_MINUTES:?} minutes"
        )));
    }
    if serde_json::to_vec(&new)?.len() > 64 * 1024 {
        return Err(ApiError::BadRequest("réglages trop volumineux".into()));
    }
    let mut new = new;
    new.backup_extra_dir = new
        .backup_extra_dir
        .map(|d| d.trim().to_owned())
        .filter(|d| !d.is_empty());
    if let Some(dir) = &new.backup_extra_dir {
        let path = std::path::Path::new(dir);
        if !path.is_absolute() || !path.is_dir() {
            return Err(ApiError::BadRequest(
                "dossier de sauvegarde supplémentaire introuvable : indiquez le chemin complet \
                 d'un dossier existant (par exemple E:\\Sauvegardes)"
                    .into(),
            ));
        }
    }
    let mut inner = state.lock_inner();
    let crate::state::Phase::Unlocked(vault) = &mut inner.phase else {
        return Err(ApiError::Unauthorized);
    };
    vault.put_secret(SETTINGS, &serde_json::to_vec(&new)?)?;
    vault.audit(ACTOR, AuditAction::SettingsChanged, None)?;
    inner
        .sessions
        .set_idle_timeout(new.idle_timeout_minutes * 60);
    Ok(Json(new))
}

// --- Journal d'audit ----------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct AuditView {
    seq: u64,
    at: String,
    action: AuditAction,
    target: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct AuditPage {
    /// La chaîne HMAC a été vérifiée à l'ouverture du coffre.
    verified: bool,
    total: usize,
    entries: Vec<AuditView>,
}

/// Les 500 entrées les plus récentes, de la plus récente à la plus ancienne. Le journal
/// ne contient que des identifiants pseudonymes, jamais de nom ni de donnée clinique.
pub async fn audit(State(state): State<AppState>) -> Result<Json<AuditPage>, ApiError> {
    state.with_vault(|vault| {
        vault.audit(ACTOR, AuditAction::AuditViewed, None)?;
        let all = vault.audit_entries();
        let entries = all
            .iter()
            .rev()
            .take(500)
            .map(|e| AuditView {
                seq: e.seq,
                at: e
                    .at
                    .format(&time::format_description::well_known::Rfc3339)
                    .unwrap_or_default(),
                action: e.action,
                target: e.target.clone(),
            })
            .collect();
        Ok(Json(AuditPage {
            verified: true,
            total: all.len(),
            entries,
        }))
    })
}
