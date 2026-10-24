//! Points d'accès de l'API.

use crate::error::ApiError;
use crate::state::{now, AppState, Phase, ACTOR};
use axum::extract::{Path, Request, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::Json;
use phonalys_auth::{check_passphrase, RecoveryCodes, Totp};
use phonalys_core::audit::AuditAction;
use phonalys_crypto::constant_time_eq;
use phonalys_vault::{LockedVault, UnlockedVault, VaultError};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use zeroize::Zeroizing;

/// Le préfixe `__Host-` impose au navigateur : Secure, Path=/ et aucun Domain.
pub const SESSION_COOKIE: &str = "__Host-phonalys_session";
const PATIENT: &str = "patient";
const TOTP_SECRET: &str = "totp";
const TOTP_LAST_STEP: &str = "totp-last-step";
const RECOVERY: &str = "recovery-codes";

/// Exécute une tâche coûteuse (Argon2id) hors du fil asynchrone.
async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, ApiError> + Send + 'static,
) -> Result<T, ApiError> {
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|_| ApiError::Internal)?
}

fn session_token(headers: &HeaderMap) -> Option<String> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .filter_map(|pair| pair.trim().split_once('='))
        .find(|(name, _)| *name == SESSION_COOKIE)
        .map(|(_, value)| value.to_owned())
}

pub async fn require_session(State(state): State<AppState>, req: Request, next: Next) -> Response {
    let ok = session_token(req.headers()).is_some_and(|token| {
        let mut inner = state.lock_inner();
        matches!(inner.phase, Phase::Unlocked(_)) && inner.sessions.touch(&token, now())
    });
    if ok {
        next.run(req).await
    } else {
        ApiError::Unauthorized.into_response()
    }
}

#[derive(Debug, Serialize)]
pub struct Status {
    initialized: bool,
}

pub async fn status(State(state): State<AppState>) -> Json<Status> {
    Json(Status {
        initialized: state.lock_inner().setup_token.is_none(),
    })
}

#[derive(Deserialize)]
pub struct SetupRequest {
    setup_token: String,
    passphrase: Zeroizing<String>,
    #[serde(default)]
    practitioner_name: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SetupResponse {
    totp_uri: String,
    totp_secret: String,
    recovery_codes: Vec<String>,
}

pub async fn setup(
    State(state): State<AppState>,
    Json(req): Json<SetupRequest>,
) -> Result<(StatusCode, Json<SetupResponse>), ApiError> {
    blocking(move || {
        let mut inner = state.lock_inner();
        let expected = inner.setup_token.as_deref().ok_or(ApiError::Conflict)?;
        if !constant_time_eq(expected.as_bytes(), req.setup_token.trim().as_bytes()) {
            tracing::warn!("tentative d'installation avec un jeton invalide");
            return Err(ApiError::Forbidden);
        }
        let name = req
            .practitioner_name
            .as_deref()
            .map(str::trim)
            .filter(|n| !n.is_empty());
        let personal: Vec<&str> = name
            .into_iter()
            .flat_map(|n| n.split_whitespace())
            .collect();
        check_passphrase(&req.passphrase, &personal)
            .map_err(|e| ApiError::BadRequest(e.to_string()))?;

        let vault = LockedVault::create(
            &state.config.vault_path,
            req.passphrase.as_bytes(),
            state.config.kdf,
        )?;
        let totp = Totp::generate();
        vault.put_secret(TOTP_SECRET, totp.secret())?;
        let (codes, plain_codes) = RecoveryCodes::generate();
        vault.put_secret(RECOVERY, &serde_json::to_vec(&codes)?)?;
        // Le coffre est refermé : la première connexion prouvera que la 2FA est configurée.
        drop(vault.lock()?);
        inner.setup_token = None;
        tracing::info!("installation terminée");

        Ok((
            StatusCode::CREATED,
            Json(SetupResponse {
                totp_uri: totp.provisioning_uri("Phonalys", name.unwrap_or("cabinet")),
                totp_secret: totp.secret_base32(),
                recovery_codes: plain_codes,
            }),
        ))
    })
    .await
}

#[derive(Deserialize)]
pub struct LoginRequest {
    passphrase: Zeroizing<String>,
    #[serde(default)]
    code: Option<String>,
    #[serde(default)]
    recovery_code: Option<Zeroizing<String>>,
}

/// Vérifie le second facteur ; renvoie l'action d'audit correspondante si valide.
fn second_factor(
    vault: &UnlockedVault,
    req: &LoginRequest,
    now: u64,
) -> Result<Option<AuditAction>, ApiError> {
    if let Some(code) = &req.code {
        let secret = vault.get_secret(TOTP_SECRET)?.ok_or(ApiError::Internal)?;
        let last = vault
            .get_secret(TOTP_LAST_STEP)?
            .and_then(|b| <[u8; 8]>::try_from(b.as_slice()).ok())
            .map(u64::from_be_bytes);
        if let Some(step) = Totp::from_secret(secret.to_vec()).verify(code, now, last) {
            vault.put_secret(TOTP_LAST_STEP, &step.to_be_bytes())?;
            return Ok(Some(AuditAction::Login));
        }
    } else if let Some(code) = &req.recovery_code {
        let stored = vault.get_secret(RECOVERY)?.ok_or(ApiError::Internal)?;
        let mut codes: RecoveryCodes = serde_json::from_slice(&stored)?;
        if codes.redeem(code) {
            vault.put_secret(RECOVERY, &serde_json::to_vec(&codes)?)?;
            return Ok(Some(AuditAction::RecoveryCodeUsed));
        }
    }
    Ok(None)
}

pub async fn login(
    State(state): State<AppState>,
    Json(req): Json<LoginRequest>,
) -> Result<Response, ApiError> {
    blocking(move || {
        // Le verrou est tenu pendant Argon2id : les essais sont sérialisés, ce qui
        // borne aussi la mémoire consommée par des connexions simultanées.
        let mut inner = state.lock_inner();
        if inner.setup_token.is_some() {
            return Err(ApiError::NotInitialized);
        }
        let now = now();
        let fresh = match LockedVault::open(&state.config.vault_path)?
            .unlock_at(req.passphrase.as_bytes(), now as i64)
        {
            Ok(v) => v,
            Err((_, VaultError::WrongPassword)) => {
                tracing::warn!("échec de connexion : phrase de passe incorrecte");
                return Err(ApiError::Unauthorized);
            }
            Err((_, e)) => return Err(e.into()),
        };

        // Si le coffre est déjà ouvert (autre appareil), on garde cette instance pour
        // que le journal d'audit en mémoire reste le seul à écrire.
        let (mut vault, was_unlocked) = match std::mem::replace(&mut inner.phase, Phase::Locked) {
            Phase::Unlocked(existing) => (existing, true),
            Phase::Locked => (fresh, false),
        };

        match second_factor(&vault, &req, now) {
            Ok(Some(action)) => {
                vault.reset_lockout()?;
                vault.audit(ACTOR, action, None)?;
                let token = inner.sessions.create(now);
                inner.phase = Phase::Unlocked(vault);
                let cookie =
                    format!("{SESSION_COOKIE}={token}; Path=/; Secure; HttpOnly; SameSite=Strict");
                let mut res = StatusCode::NO_CONTENT.into_response();
                res.headers_mut().insert(
                    header::SET_COOKIE,
                    HeaderValue::from_str(&cookie).map_err(|_| ApiError::Internal)?,
                );
                Ok(res)
            }
            Ok(None) => {
                vault.register_failure(now as i64)?;
                vault.audit(ACTOR, AuditAction::LoginFailed, None)?;
                if was_unlocked {
                    inner.phase = Phase::Unlocked(vault);
                }
                tracing::warn!("échec de connexion : second facteur invalide");
                Err(ApiError::Unauthorized)
            }
            Err(e) => {
                if was_unlocked {
                    inner.phase = Phase::Unlocked(vault);
                }
                Err(e)
            }
        }
    })
    .await
}

fn expire_cookie(mut res: Response) -> Response {
    res.headers_mut().insert(
        header::SET_COOKIE,
        HeaderValue::from_static(
            "__Host-phonalys_session=; Path=/; Secure; HttpOnly; SameSite=Strict; Max-Age=0",
        ),
    );
    res
}

#[derive(Debug, Serialize)]
pub struct SessionInfo {
    idle_timeout_secs: u64,
}

pub async fn session(State(state): State<AppState>) -> Json<SessionInfo> {
    Json(SessionInfo {
        idle_timeout_secs: state.lock_inner().sessions.config().idle_timeout_secs,
    })
}

/// Ferme la session courante ; le coffre se verrouille si c'était la dernière.
pub async fn logout(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Some(token) = session_token(&headers) {
        state.lock_inner().sessions.revoke(&token);
    }
    state.auto_lock(now());
    expire_cookie(StatusCode::NO_CONTENT.into_response())
}

/// Verrouillage immédiat : toutes les sessions, sur tous les appareils.
pub async fn lock(State(state): State<AppState>) -> Result<Response, ApiError> {
    let mut inner = state.lock_inner();
    inner.sessions.revoke_all();
    if let Phase::Unlocked(mut vault) = std::mem::replace(&mut inner.phase, Phase::Locked) {
        vault.audit(ACTOR, AuditAction::Lock, None)?;
    }
    Ok(expire_cookie(StatusCode::NO_CONTENT.into_response()))
}

fn parse_id(id: &str) -> Result<String, ApiError> {
    uuid::Uuid::parse_str(id)
        .map(|u| u.to_string())
        .map_err(|_| ApiError::NotFound)
}

#[derive(Debug, Serialize)]
pub struct Created {
    id: String,
}

pub async fn create_patient(
    State(state): State<AppState>,
    Json(data): Json<Value>,
) -> Result<(StatusCode, Json<Created>), ApiError> {
    if !data.is_object() {
        return Err(ApiError::BadRequest("objet JSON attendu".into()));
    }
    state.with_vault(|vault| {
        let id = UnlockedVault::new_id();
        vault.put_json(PATIENT, &id, &data)?;
        vault.audit(ACTOR, AuditAction::PatientCreated, Some(&id))?;
        Ok((StatusCode::CREATED, Json(Created { id })))
    })
}

pub async fn list_patients(State(state): State<AppState>) -> Result<Json<Vec<Value>>, ApiError> {
    state.with_vault(|vault| {
        vault
            .list(PATIENT)?
            .into_iter()
            .map(|id| {
                let mut value: Value = vault.get_json(PATIENT, &id)?;
                if let Some(obj) = value.as_object_mut() {
                    obj.insert("id".into(), Value::String(id));
                }
                Ok(value)
            })
            .collect::<Result<Vec<_>, ApiError>>()
            .map(Json)
    })
}

pub async fn get_patient(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let id = parse_id(&id)?;
    state.with_vault(|vault| {
        let value: Value = vault.get_json(PATIENT, &id)?;
        vault.audit(ACTOR, AuditAction::PatientViewed, Some(&id))?;
        Ok(Json(value))
    })
}

pub async fn delete_patient(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let id = parse_id(&id)?;
    state.with_vault(|vault| {
        vault.delete(PATIENT, &id)?;
        vault.audit(ACTOR, AuditAction::PatientErased, Some(&id))?;
        Ok(StatusCode::NO_CONTENT)
    })
}
