//! Points d'accès de l'API.

use crate::error::ApiError;
use crate::state::{now, AppState, Inner, Phase, ACTOR};
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

/// Exécute une tâche coûteuse (Argon2id) hors du fil asynchrone.
pub(crate) async fn blocking<T: Send + 'static>(
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
    /// QR code du `totp_uri`, en SVG, généré sur le serveur : le secret ne transite par
    /// aucun service tiers.
    totp_qr_svg: String,
    recovery_codes: Vec<String>,
}

fn qr_svg(data: &str) -> Result<String, ApiError> {
    let code = qrcode::QrCode::with_error_correction_level(data, qrcode::EcLevel::M)
        .map_err(|_| ApiError::Internal)?;
    Ok(code
        .render::<qrcode::render::svg::Color<'_>>()
        .min_dimensions(240, 240)
        .quiet_zone(true)
        .build())
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
        // Les codes de récupération sont des clés : chacun (avec la phrase de passe)
        // ouvre une enveloppe de la clé maîtresse, détruite après usage.
        let (_, plain_codes) = RecoveryCodes::generate();
        let pk = vault.password_key(req.passphrase.as_bytes())?;
        vault.replace_recovery_factors(&pk, &recovery_secrets(&plain_codes))?;
        // Le coffre est refermé : la première connexion prouvera que la 2FA est configurée.
        drop(vault.lock()?);
        inner.setup_token = None;
        tracing::info!("installation terminée");

        let totp_uri = totp.provisioning_uri("Phonalys", name.unwrap_or("cabinet"));
        Ok((
            StatusCode::CREATED,
            Json(SetupResponse {
                totp_qr_svg: qr_svg(&totp_uri)?,
                totp_uri,
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

/// Secrets des enveloppes de récupération : forme canonique des codes.
pub(crate) fn recovery_secrets(codes: &[String]) -> Vec<Vec<u8>> {
    codes
        .iter()
        .map(|c| phonalys_auth::recovery::normalize(c).into_bytes())
        .collect()
}

/// Vérifie un code TOTP ; renvoie l'action d'audit si valide.
fn check_totp(
    vault: &UnlockedVault,
    code: &str,
    now: u64,
) -> Result<Option<AuditAction>, ApiError> {
    let secret = vault.get_secret(TOTP_SECRET)?.ok_or(ApiError::Internal)?;
    let last = vault
        .get_secret(TOTP_LAST_STEP)?
        .and_then(|b| <[u8; 8]>::try_from(b.as_slice()).ok())
        .map(u64::from_be_bytes);
    match Totp::from_secret(secret.to_vec()).verify(code, now, last) {
        Some(step) => {
            vault.put_secret(TOTP_LAST_STEP, &step.to_be_bytes())?;
            Ok(Some(AuditAction::Login))
        }
        None => Ok(None),
    }
}

/// Connexion par code de récupération : c'est l'enveloppe cryptographique du code qui
/// ouvre le coffre (fonctionne aussi en mode lié au matériel). Le code est consommé.
fn login_with_recovery(
    state: &AppState,
    inner: &mut Inner,
    passphrase: &[u8],
    code: &str,
    now: u64,
) -> Result<Response, ApiError> {
    let locked = LockedVault::open(&state.config.vault_path)?;
    let pk = locked.password_key_at(passphrase, now as i64)?;
    let secret = phonalys_auth::recovery::normalize(code);
    let fresh = match locked.unlock_with_recovery_at(&pk, secret.as_bytes(), now as i64) {
        Ok(v) => v,
        Err((_, e)) => {
            tracing::warn!(error = %e, "échec de connexion par code de récupération");
            return Err(e.into());
        }
    };
    let (vault, was_unlocked) = take_vault(inner, Some(fresh))?;
    conclude_login(
        inner,
        vault,
        was_unlocked,
        Ok(Some(AuditAction::RecoveryCodeUsed)),
        now,
    )
}

/// Vérifie la phrase de passe (Argon2id) et renvoie une instance fraîche du coffre.
pub(crate) fn unlock_with_passphrase(
    state: &AppState,
    passphrase: &[u8],
    now: u64,
) -> Result<UnlockedVault, ApiError> {
    match LockedVault::open(&state.config.vault_path)?.unlock_at(passphrase, now as i64) {
        Ok(v) => Ok(v),
        Err((_, VaultError::WrongPassword)) => {
            tracing::warn!("échec de connexion : phrase de passe incorrecte");
            Err(ApiError::Unauthorized)
        }
        Err((_, e)) => Err(e.into()),
    }
}

/// Instance du coffre à utiliser pour terminer une connexion. Si le coffre est déjà
/// ouvert (autre appareil), on garde cette instance pour que le journal d'audit en
/// mémoire reste le seul à écrire.
pub(crate) fn take_vault(
    inner: &mut Inner,
    fresh: Option<UnlockedVault>,
) -> Result<(UnlockedVault, bool), ApiError> {
    match (std::mem::replace(&mut inner.phase, Phase::Locked), fresh) {
        (Phase::Unlocked(existing), _) => Ok((existing, true)),
        (Phase::Locked, Some(fresh)) => Ok((fresh, false)),
        (Phase::Locked, None) => Err(ApiError::Unauthorized),
    }
}

/// Conclut une tentative de connexion : session et cookie si le second facteur est
/// valide, échec compté et audité sinon.
pub(crate) fn conclude_login(
    inner: &mut Inner,
    mut vault: UnlockedVault,
    was_unlocked: bool,
    outcome: Result<Option<AuditAction>, ApiError>,
    now: u64,
) -> Result<Response, ApiError> {
    let restore = |inner: &mut Inner, vault: UnlockedVault| {
        if was_unlocked {
            inner.phase = Phase::Unlocked(vault);
        }
    };
    match outcome {
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
            let failure = vault
                .register_failure(now as i64)
                .and_then(|()| vault.audit(ACTOR, AuditAction::LoginFailed, None));
            restore(inner, vault);
            failure?;
            tracing::warn!("échec de connexion : second facteur invalide");
            Err(ApiError::Unauthorized)
        }
        Err(e) => {
            restore(inner, vault);
            Err(e)
        }
    }
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
        if let Some(code) = &req.recovery_code {
            return login_with_recovery(&state, &mut inner, req.passphrase.as_bytes(), code, now);
        }
        let fresh = unlock_with_passphrase(&state, req.passphrase.as_bytes(), now)?;
        let (vault, was_unlocked) = take_vault(&mut inner, Some(fresh))?;

        let outcome = match (crate::passkey::policy(&vault), &req.code) {
            (Ok(policy), Some(_)) if policy.require_passkey => Err(ApiError::BadRequest(
                "clé biométrique exigée : utilisez la connexion par clé de sécurité".into(),
            )),
            (Ok(_), Some(code)) => check_totp(&vault, code, now),
            (Ok(_), None) => Ok(None),
            (Err(e), _) => Err(e),
        };
        conclude_login(&mut inner, vault, was_unlocked, outcome, now)
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

pub async fn update_patient(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(data): Json<Value>,
) -> Result<StatusCode, ApiError> {
    let id = parse_id(&id)?;
    if !data.is_object() {
        return Err(ApiError::BadRequest("objet JSON attendu".into()));
    }
    state.with_vault(|vault| {
        // Un identifiant inconnu ne crée pas de dossier.
        let _: Value = vault.get_json(PATIENT, &id)?;
        vault.put_json(PATIENT, &id, &data)?;
        vault.audit(ACTOR, AuditAction::PatientUpdated, Some(&id))?;
        Ok(StatusCode::NO_CONTENT)
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
