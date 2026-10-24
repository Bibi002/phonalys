//! Clés de sécurité et biométrie (WebAuthn / passkeys).
//!
//! Connexion en deux temps :
//! 1. `POST /api/login/passkey/start` avec la phrase de passe : un défi est renvoyé ;
//! 2. `POST /api/login/passkey/finish` avec la signature produite par l'appareil
//!    après la vérification biométrique.
//!
//! En mode **lié au matériel**, l'appareil renvoie aussi une sortie PRF : c'est elle,
//! combinée à la phrase de passe, qui ouvre cryptographiquement le coffre.

use crate::api::{blocking, conclude_login, recovery_secrets, take_vault, unlock_with_passphrase};
use crate::error::ApiError;
use crate::state::{
    new_challenge, now, AppState, PendingBind, PendingLogin, PendingRegistration, Phase, ACTOR,
    CHALLENGE_TTL_SECS,
};
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::Response;
use axum::Json;
use phonalys_auth::RecoveryCodes;
use phonalys_core::audit::AuditAction;
use phonalys_vault::{FactorKind, LockedVault, UnlockedVault, VaultError};
use phonalys_webauthn::{
    authentication_options, decode_prf_output, finish_authentication, finish_registration,
    registration_options, AuthenticationResponse, RegistrationResponse, StoredCredential,
    WebAuthnError,
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

fn save_policy(vault: &UnlockedVault, policy: SecurityPolicy) -> Result<(), ApiError> {
    Ok(vault.put_secret(POLICY, &serde_json::to_vec(&policy)?)?)
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
    /// La clé est liée cryptographiquement au coffre (extension PRF).
    bound: bool,
}

fn infos(vault: &UnlockedVault) -> Result<Vec<PasskeyInfo>, ApiError> {
    let bound = vault.factor_ids(FactorKind::Passkey)?;
    Ok(credentials(vault)?
        .iter()
        .map(|c| PasskeyInfo {
            id: c.id.clone(),
            label: c.label.clone(),
            backup_eligible: c.backup_eligible,
            aaguid: c.aaguid.clone(),
            bound: bound.contains(&c.id),
        })
        .collect())
}

fn bad(e: WebAuthnError) -> ApiError {
    ApiError::BadRequest(e.to_string())
}

fn sanitize_label(label: Option<&str>) -> String {
    label
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .unwrap_or("Clé de sécurité")
        .chars()
        .filter(|c| !c.is_control())
        .take(MAX_LABEL_CHARS)
        .collect()
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
) -> Result<(StatusCode, Json<Vec<PasskeyInfo>>), ApiError> {
    let mut inner = state.lock_inner();
    let pending = inner
        .pending_registration
        .take()
        .filter(|p| now() <= p.expires_at)
        .ok_or_else(|| ApiError::BadRequest("aucune inscription en cours ou défi expiré".into()))?;
    let cred = finish_registration(
        &state.config.relying_party,
        &pending.challenge,
        &req.response,
        &sanitize_label(req.label.as_deref()),
    )
    .map_err(bad)?;

    let Phase::Unlocked(vault) = &mut inner.phase else {
        return Err(ApiError::Unauthorized);
    };
    let mut creds = credentials(vault)?;
    if creds.iter().any(|c| c.id == cred.id) {
        return Err(ApiError::Conflict);
    }
    creds.push(cred);
    save_credentials(vault, &creds)?;
    vault.audit(ACTOR, AuditAction::PasskeyRegistered, None)?;
    Ok((StatusCode::CREATED, Json(infos(vault)?)))
}

pub async fn list(State(state): State<AppState>) -> Result<Json<Vec<PasskeyInfo>>, ApiError> {
    state.with_vault(|vault| Ok(Json(infos(vault)?)))
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
        // Refuse aussi de retirer la dernière clé liée d'un coffre lié au matériel.
        vault.remove_factor(FactorKind::Passkey, &id)?;
        save_credentials(vault, &creds)?;
        vault.audit(ACTOR, AuditAction::PasskeyRemoved, None)?;
        Ok(StatusCode::NO_CONTENT)
    })
}

// --- Lien cryptographique d'une clé (extension PRF) ---------------------------------

#[derive(Deserialize)]
pub struct BindStart {
    credential_id: String,
}

/// Prépare le lien : l'appareil devra signer un défi et calculer sa sortie PRF.
pub async fn bind_start(
    State(state): State<AppState>,
    Json(req): Json<BindStart>,
) -> Result<Json<Value>, ApiError> {
    let challenge = new_challenge();
    let options = state.with_vault(|vault| {
        if !credentials(vault)?
            .iter()
            .any(|c| c.id == req.credential_id)
        {
            return Err(ApiError::NotFound);
        }
        Ok(authentication_options(
            &state.config.relying_party,
            &challenge,
            std::slice::from_ref(&req.credential_id),
            Some(&vault.prf_salt()?),
        ))
    })?;
    state.lock_inner().pending_bind = Some(PendingBind {
        challenge,
        credential_id: req.credential_id,
        expires_at: now() + CHALLENGE_TTL_SECS,
    });
    Ok(Json(options))
}

#[derive(Deserialize)]
pub struct BindFinish {
    passphrase: Zeroizing<String>,
    response: AuthenticationResponse,
    prf_output: Zeroizing<String>,
}

pub async fn bind_finish(
    State(state): State<AppState>,
    Json(req): Json<BindFinish>,
) -> Result<Json<Vec<PasskeyInfo>>, ApiError> {
    blocking(move || {
        let mut inner = state.lock_inner();
        let pending = inner
            .pending_bind
            .take()
            .filter(|p| now() <= p.expires_at && p.credential_id == req.response.id)
            .ok_or_else(|| ApiError::BadRequest("aucun lien en cours ou défi expiré".into()))?;
        let Phase::Unlocked(vault) = &mut inner.phase else {
            return Err(ApiError::Unauthorized);
        };

        // L'appareil prouve sa présence et la biométrie, puis la phrase de passe est
        // revérifiée : lier une clé est une opération sensible.
        let mut creds = credentials(vault)?;
        finish_authentication(
            &state.config.relying_party,
            &pending.challenge,
            &req.response,
            &mut creds,
        )
        .map_err(bad)?;
        save_credentials(vault, &creds)?;
        let prf = Zeroizing::new(decode_prf_output(&req.prf_output).map_err(bad)?);
        let pk = vault.password_key(req.passphrase.as_bytes())?;
        vault.add_factor(&pk, FactorKind::Passkey, &pending.credential_id, &prf)?;
        vault.audit(ACTOR, AuditAction::FactorBound, None)?;
        Ok(Json(infos(vault)?))
    })
    .await
}

// --- Politique et mode lié au matériel ----------------------------------------------

#[derive(Debug, Serialize)]
pub struct SecurityStatus {
    require_passkey: bool,
    hardware_bound: bool,
    passkeys: usize,
    bound_passkeys: usize,
    recovery_codes_remaining: usize,
}

pub async fn status(State(state): State<AppState>) -> Result<Json<SecurityStatus>, ApiError> {
    state.with_vault(|vault| {
        Ok(Json(SecurityStatus {
            require_passkey: policy(vault)?.require_passkey,
            hardware_bound: vault.is_hardware_bound()?,
            passkeys: credentials(vault)?.len(),
            bound_passkeys: vault.factor_ids(FactorKind::Passkey)?.len(),
            recovery_codes_remaining: vault.factor_ids(FactorKind::Recovery)?.len(),
        }))
    })
}

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
        if !new.require_passkey && vault.is_hardware_bound()? {
            return Err(ApiError::BadRequest(
                "désactivez d'abord le lien matériel du coffre".into(),
            ));
        }
        save_policy(vault, new)?;
        vault.audit(ACTOR, AuditAction::SecurityPolicyChanged, None)?;
        Ok(Json(new))
    })
}

#[derive(Deserialize)]
pub struct BindingRequest {
    passphrase: Zeroizing<String>,
    enabled: bool,
}

#[derive(Debug, Serialize)]
pub struct BindingResponse {
    hardware_bound: bool,
    /// Nouveaux codes de récupération, affichés une seule fois (activation uniquement).
    #[serde(skip_serializing_if = "Option::is_none")]
    recovery_codes: Option<Vec<String>>,
}

/// Active ou désactive le mode lié au matériel. À l'activation, de nouveaux codes de
/// récupération sont générés : ce sont eux qui permettront de récupérer le coffre si
/// toutes les clés biométriques sont perdues.
pub async fn set_binding(
    State(state): State<AppState>,
    Json(req): Json<BindingRequest>,
) -> Result<Json<BindingResponse>, ApiError> {
    blocking(move || {
        state.with_vault(|vault| {
            let pk = vault.password_key(req.passphrase.as_bytes())?;
            vault.audit(ACTOR, AuditAction::HardwareBindingChanged, None)?;
            if req.enabled {
                if vault.factor_ids(FactorKind::Passkey)?.is_empty() {
                    return Err(ApiError::BadRequest(
                        "liez d'abord au moins une clé biométrique (extension PRF)".into(),
                    ));
                }
                let (_, codes) = RecoveryCodes::generate();
                vault.replace_recovery_factors(&pk, &recovery_secrets(&codes))?;
                vault.set_hardware_bound(&pk, true)?;
                save_policy(
                    vault,
                    SecurityPolicy {
                        require_passkey: true,
                    },
                )?;
                Ok(Json(BindingResponse {
                    hardware_bound: true,
                    recovery_codes: Some(codes),
                }))
            } else {
                vault.set_hardware_bound(&pk, false)?;
                Ok(Json(BindingResponse {
                    hardware_bound: false,
                    recovery_codes: None,
                }))
            }
        })
    })
    .await
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
        let challenge = new_challenge();
        let locked = LockedVault::open(&state.config.vault_path)?;

        let (options, pending) = if locked.is_hardware_bound()? {
            // La phrase de passe ne peut pas être vérifiée seule : sa clé est gardée
            // jusqu'à la réponse de l'appareil, qui fournira l'autre moitié.
            let password_key = locked.password_key_at(req.passphrase.as_bytes(), now as i64)?;
            let ids = locked.factor_ids(FactorKind::Passkey)?;
            let options = authentication_options(
                &state.config.relying_party,
                &challenge,
                &ids,
                Some(&locked.prf_salt()?),
            );
            (options, (None, Some(password_key)))
        } else {
            let fresh = unlock_with_passphrase(&state, req.passphrase.as_bytes(), now)?;
            let creds = match &inner.phase {
                Phase::Unlocked(existing) => credentials(existing)?,
                Phase::Locked => credentials(&fresh)?,
            };
            if creds.is_empty() {
                return Err(ApiError::BadRequest(
                    "aucune clé biométrique enregistrée".into(),
                ));
            }
            let ids: Vec<String> = creds.into_iter().map(|c| c.id).collect();
            let options =
                authentication_options(&state.config.relying_party, &challenge, &ids, None);
            let already_open = matches!(inner.phase, Phase::Unlocked(_));
            (options, ((!already_open).then_some(fresh), None))
        };

        let (fresh, password_key) = pending;
        inner.pending_login = Some(PendingLogin {
            challenge,
            expires_at: now + CHALLENGE_TTL_SECS,
            fresh,
            password_key,
        });
        Ok(Json(options))
    })
    .await
}

#[derive(Deserialize)]
pub struct LoginFinish {
    response: AuthenticationResponse,
    /// Sortie PRF de l'appareil (base64url), exigée pour un coffre lié au matériel.
    #[serde(default)]
    prf_output: Option<Zeroizing<String>>,
}

pub async fn login_finish(
    State(state): State<AppState>,
    Json(req): Json<LoginFinish>,
) -> Result<Response, ApiError> {
    blocking(move || {
        let mut inner = state.lock_inner();
        let now = now();
        let pending = inner
            .pending_login
            .take()
            .filter(|p| now <= p.expires_at)
            .ok_or(ApiError::Unauthorized)?;

        let fresh = match &pending.password_key {
            Some(password_key) => {
                let prf = req
                    .prf_output
                    .as_deref()
                    .ok_or_else(|| {
                        ApiError::BadRequest(
                            "cet appareil ne fournit pas l'extension PRF requise".into(),
                        )
                    })
                    .and_then(|p| decode_prf_output(p).map_err(bad))
                    .map(Zeroizing::new)?;
                let locked = LockedVault::open(&state.config.vault_path)?;
                let id = req.response.id.trim_end_matches('=');
                match locked.unlock_with_factor_at(
                    password_key,
                    FactorKind::Passkey,
                    id,
                    &prf,
                    now as i64,
                ) {
                    Ok(v) => Some(v),
                    Err((locked, e)) => {
                        tracing::warn!(error = %e, "échec d'ouverture par clé liée");
                        return Err(match e {
                            VaultError::FactorRejected => {
                                ApiError::LoginFailed(locked.remaining_attempts()?)
                            }
                            e => e.into(),
                        });
                    }
                }
            }
            None => pending.fresh,
        };
        let (vault, was_unlocked) = take_vault(&mut inner, fresh)?;

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
    })
    .await
}
