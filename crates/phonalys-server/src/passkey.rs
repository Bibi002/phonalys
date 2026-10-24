//! Clés de sécurité et biométrie (WebAuthn / passkeys).
//!
//! Connexion en deux temps :
//! 1. `POST /api/login/passkey/start` avec la phrase de passe : elle est vérifiée et
//!    un défi est renvoyé ;
//! 2. `POST /api/login/passkey/finish` avec la signature produite par l'appareil
//!    après la vérification biométrique.

use crate::api::{blocking, conclude_login, take_vault, unlock_with_passphrase};
use crate::error::ApiError;
use crate::state::{
    new_challenge, now, AppState, PendingLogin, PendingRegistration, Phase, ACTOR,
    CHALLENGE_TTL_SECS,
};
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::Response;
use axum::Json;
use phonalys_core::audit::AuditAction;
use phonalys_vault::UnlockedVault;
use phonalys_webauthn::{
    authentication_options, finish_authentication, finish_registration, registration_options,
    AuthenticationResponse, RegistrationResponse, StoredCredential, WebAuthnError,
};
use rand_core::{OsRng, RngCore};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use zeroize::Zeroizing;

const CREDENTIALS: &str = "webauthn-credentials";
const USER_ID: &str = "webauthn-user-id";
const POLICY: &str = "auth-policy";
const MAX_LABEL_CHARS: usize = 64;

/// Politique d'authentification, stockée chiffrée dans le coffre.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SecurityPolicy {
    /// La connexion exige une clé biométrique : le TOTP n'est plus accepté.
    /// Les codes de récupération restent utilisables en cas de perte de l'appareil.
    pub require_passkey: bool,
}

pub(crate) fn policy(vault: &UnlockedVault) -> Result<SecurityPolicy, ApiError> {
    Ok(match vault.get_secret(POLICY)? {
        Some(bytes) => serde_json::from_slice(&bytes)?,
        None => SecurityPolicy::default(),
    })
}

fn credentials(vault: &UnlockedVault) -> Result<Vec<StoredCredential>, ApiError> {
    Ok(match vault.get_secret(CREDENTIALS)? {
        Some(bytes) => serde_json::from_slice(&bytes)?,
        None => Vec::new(),
    })
}

fn save_credentials(vault: &UnlockedVault, creds: &[StoredCredential]) -> Result<(), ApiError> {
    Ok(vault.put_secret(CREDENTIALS, &serde_json::to_vec(creds)?)?)
}

/// Identifiant WebAuthn aléatoire du praticien, sans lien avec son identité.
fn user_id(vault: &UnlockedVault) -> Result<Vec<u8>, ApiError> {
    if let Some(id) = vault.get_secret(USER_ID)? {
        return Ok(id.to_vec());
    }
    let mut id = vec![0u8; 16];
    OsRng.fill_bytes(&mut id);
    vault.put_secret(USER_ID, &id)?;
    Ok(id)
}

#[derive(Debug, Serialize)]
pub struct PasskeyInfo {
    id: String,
    label: String,
    backup_eligible: bool,
    aaguid: String,
}

impl From<&StoredCredential> for PasskeyInfo {
    fn from(c: &StoredCredential) -> Self {
        Self {
            id: c.id.clone(),
            label: c.label.clone(),
            backup_eligible: c.backup_eligible,
            aaguid: c.aaguid.clone(),
        }
    }
}

fn bad(e: WebAuthnError) -> ApiError {
    ApiError::BadRequest(e.to_string())
}

// --- Inscription d'une clé (session requise) ---------------------------------------

pub async fn register_start(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    let challenge = new_challenge();
    let options = state.with_vault(|vault| {
        Ok(registration_options(
            &state.config.relying_party,
            &user_id(vault)?,
            "praticien",
            &challenge,
            &credentials(vault)?,
        ))
    })?;
    state.lock_inner().pending_registration = Some(PendingRegistration {
        challenge,
        expires_at: now() + CHALLENGE_TTL_SECS,
    });
    Ok(Json(options))
}

#[derive(Deserialize)]
pub struct RegisterFinish {
    response: RegistrationResponse,
    #[serde(default)]
    label: Option<String>,
}

pub async fn register_finish(
    State(state): State<AppState>,
    Json(req): Json<RegisterFinish>,
) -> Result<(StatusCode, Json<PasskeyInfo>), ApiError> {
    let mut inner = state.lock_inner();
    let pending = inner
        .pending_registration
        .take()
        .filter(|p| now() <= p.expires_at)
        .ok_or_else(|| ApiError::BadRequest("aucune inscription en cours ou défi expiré".into()))?;
    let label: String = req
        .label
        .as_deref()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .unwrap_or("Clé de sécurité")
        .chars()
        .filter(|c| !c.is_control())
        .take(MAX_LABEL_CHARS)
        .collect();
    let cred = finish_registration(
        &state.config.relying_party,
        &pending.challenge,
        &req.response,
        &label,
    )
    .map_err(bad)?;

    let Phase::Unlocked(vault) = &mut inner.phase else {
        return Err(ApiError::Unauthorized);
    };
    let mut creds = credentials(vault)?;
    if creds.iter().any(|c| c.id == cred.id) {
        return Err(ApiError::Conflict);
    }
    creds.push(cred.clone());
    save_credentials(vault, &creds)?;
    vault.audit(ACTOR, AuditAction::PasskeyRegistered, None)?;
    Ok((StatusCode::CREATED, Json(PasskeyInfo::from(&cred))))
}

pub async fn list(State(state): State<AppState>) -> Result<Json<Vec<PasskeyInfo>>, ApiError> {
    state.with_vault(|vault| {
        Ok(Json(
            credentials(vault)?.iter().map(PasskeyInfo::from).collect(),
        ))
    })
}

pub async fn remove(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    state.with_vault(|vault| {
        let mut creds = credentials(vault)?;
        let before = creds.len();
        creds.retain(|c| c.id != id);
        if creds.len() == before {
            return Err(ApiError::NotFound);
        }
        if creds.is_empty() && policy(vault)?.require_passkey {
            return Err(ApiError::BadRequest(
                "impossible de retirer la dernière clé tant qu'elle est obligatoire".into(),
            ));
        }
        save_credentials(vault, &creds)?;
        vault.audit(ACTOR, AuditAction::PasskeyRemoved, None)?;
        Ok(StatusCode::NO_CONTENT)
    })
}

// --- Politique -----------------------------------------------------------------------

pub async fn get_policy(State(state): State<AppState>) -> Result<Json<SecurityPolicy>, ApiError> {
    state.with_vault(|vault| Ok(Json(policy(vault)?)))
}

pub async fn put_policy(
    State(state): State<AppState>,
    Json(new): Json<SecurityPolicy>,
) -> Result<Json<SecurityPolicy>, ApiError> {
    state.with_vault(|vault| {
        if new.require_passkey && credentials(vault)?.is_empty() {
            return Err(ApiError::BadRequest(
                "enregistrez d'abord au moins une clé biométrique".into(),
            ));
        }
        vault.put_secret(POLICY, &serde_json::to_vec(&new)?)?;
        vault.audit(ACTOR, AuditAction::SecurityPolicyChanged, None)?;
        Ok(Json(new))
    })
}

// --- Connexion par clé ---------------------------------------------------------------

#[derive(Deserialize)]
pub struct LoginStart {
    passphrase: Zeroizing<String>,
}

pub async fn login_start(
    State(state): State<AppState>,
    Json(req): Json<LoginStart>,
) -> Result<Json<Value>, ApiError> {
    blocking(move || {
        let mut inner = state.lock_inner();
        if inner.setup_token.is_some() {
            return Err(ApiError::NotInitialized);
        }
        let now = now();
        let fresh = unlock_with_passphrase(&state, req.passphrase.as_bytes(), now)?;
        let already_open = matches!(inner.phase, Phase::Unlocked(_));
        let creds = match &inner.phase {
            Phase::Unlocked(existing) => credentials(existing)?,
            Phase::Locked => credentials(&fresh)?,
        };
        if creds.is_empty() {
            return Err(ApiError::BadRequest(
                "aucune clé biométrique enregistrée".into(),
            ));
        }
        let challenge = new_challenge();
        let options = authentication_options(&state.config.relying_party, &challenge, &creds);
        inner.pending_login = Some(PendingLogin {
            challenge,
            expires_at: now + CHALLENGE_TTL_SECS,
            fresh: (!already_open).then_some(fresh),
        });
        Ok(Json(options))
    })
    .await
}

#[derive(Deserialize)]
pub struct LoginFinish {
    response: AuthenticationResponse,
}

pub async fn login_finish(
    State(state): State<AppState>,
    Json(req): Json<LoginFinish>,
) -> Result<Response, ApiError> {
    let mut inner = state.lock_inner();
    let now = now();
    let pending = inner
        .pending_login
        .take()
        .filter(|p| now <= p.expires_at)
        .ok_or(ApiError::Unauthorized)?;
    let (vault, was_unlocked) = take_vault(&mut inner, pending.fresh)?;

    let outcome = credentials(&vault).and_then(|mut creds| {
        match finish_authentication(
            &state.config.relying_party,
            &pending.challenge,
            &req.response,
            &mut creds,
        ) {
            Ok(_) => {
                save_credentials(&vault, &creds)?;
                Ok(Some(AuditAction::PasskeyLogin))
            }
            Err(WebAuthnError::CloneDetected) => {
                tracing::error!("ALERTE : authentificateur cloné détecté");
                Ok(None)
            }
            Err(e) => {
                tracing::warn!(error = %e, "échec de connexion par clé");
                Ok(None)
            }
        }
    });
    conclude_login(&mut inner, vault, was_unlocked, outcome, now)
}
