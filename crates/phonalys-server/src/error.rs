//! Erreurs de l'API. Les messages restent volontairement génériques : aucun détail
//! interne (chemins, SQL, cryptographie) n'est renvoyé au client.

use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use phonalys_vault::VaultError;

#[derive(Debug)]
pub enum ApiError {
    Unauthorized,
    Forbidden,
    BadRequest(String),
    NotFound,
    Conflict,
    NotInitialized,
    TooManyAttempts(i64),
    /// Connexion refusée ; nombre d'essais restants avant le blocage.
    LoginFailed(u32),
    /// Phrase de passe ou code de récupération faux (on ne dit pas lequel).
    RecoveryRejected,
    /// Après plusieurs échecs, la connexion en un geste est refusée.
    StepUpRequired,
    /// Opération sensible : une réauthentification récente est exigée.
    ReauthRequired,
    /// Réauthentification refusée ; essais restants si la phrase de passe est en jeu.
    /// Jamais 401 : la session reste ouverte.
    ReauthFailed(Option<u32>),
    Internal,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            ApiError::Unauthorized => (
                StatusCode::UNAUTHORIZED,
                "authentification requise ou invalide".to_owned(),
            ),
            ApiError::Forbidden => (StatusCode::FORBIDDEN, "accès refusé".to_owned()),
            ApiError::BadRequest(m) => (StatusCode::BAD_REQUEST, m.clone()),
            ApiError::NotFound => (StatusCode::NOT_FOUND, "introuvable".to_owned()),
            ApiError::Conflict => (StatusCode::CONFLICT, "déjà initialisé".to_owned()),
            ApiError::NotInitialized => {
                (StatusCode::CONFLICT, "installation non terminée".to_owned())
            }
            ApiError::TooManyAttempts(s) => (
                StatusCode::TOO_MANY_REQUESTS,
                format!(
                    "connexion bloquée après {} échecs : réessayez dans {}, ou utilisez un code \
                     de récupération pour débloquer tout de suite",
                    phonalys_vault::MAX_ATTEMPTS,
                    human_delay(*s)
                ),
            ),
            ApiError::RecoveryRejected => (
                StatusCode::UNAUTHORIZED,
                "phrase de passe ou code de récupération incorrect".to_owned(),
            ),
            ApiError::StepUpRequired => (
                StatusCode::UNAUTHORIZED,
                format!(
                    "après {} échecs, deux vérifications sont demandées : phrase de passe, puis \
                     clé biométrique ou code à 6 chiffres",
                    phonalys_vault::QUICK_MAX_FAILURES
                ),
            ),
            ApiError::LoginFailed(0) => (
                StatusCode::UNAUTHORIZED,
                "identifiants incorrects : la connexion est maintenant bloquée ; un code de \
                 récupération permet de débloquer tout de suite"
                    .to_owned(),
            ),
            ApiError::LoginFailed(n) => (
                StatusCode::UNAUTHORIZED,
                format!(
                    "identifiants incorrects : encore {n} essai{} avant le blocage",
                    if *n > 1 { "s" } else { "" }
                ),
            ),
            ApiError::ReauthRequired => (
                StatusCode::FORBIDDEN,
                "confirmez votre identité pour ouvrir les documents".to_owned(),
            ),
            ApiError::ReauthFailed(Some(0)) => (
                StatusCode::FORBIDDEN,
                "vérification refusée : la connexion est maintenant bloquée".to_owned(),
            ),
            ApiError::ReauthFailed(Some(n)) => (
                StatusCode::FORBIDDEN,
                format!(
                    "vérification refusée : encore {n} essai{} avant le blocage",
                    if *n > 1 { "s" } else { "" }
                ),
            ),
            ApiError::ReauthFailed(None) => {
                (StatusCode::FORBIDDEN, "vérification refusée".to_owned())
            }
            ApiError::Internal => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "erreur interne".to_owned(),
            ),
        };
        let mut body = serde_json::json!({ "error": message });
        match &self {
            ApiError::LoginFailed(n) => body["remaining_attempts"] = (*n).into(),
            ApiError::TooManyAttempts(s) => body["retry_after_secs"] = (*s).into(),
            ApiError::StepUpRequired => body["step_up"] = true.into(),
            ApiError::ReauthRequired => body["reauth"] = true.into(),
            ApiError::ReauthFailed(Some(n)) => body["remaining_attempts"] = (*n).into(),
            _ => {}
        }
        let mut res = (status, Json(body)).into_response();
        if let ApiError::TooManyAttempts(s) = self {
            if let Ok(v) = s.to_string().parse() {
                res.headers_mut().insert(header::RETRY_AFTER, v);
            }
        }
        res
    }
}

/// « 15 min », « 40 s »…
fn human_delay(secs: i64) -> String {
    if secs >= 60 {
        format!("{} min", (secs + 59) / 60)
    } else {
        format!("{secs} s")
    }
}

impl From<VaultError> for ApiError {
    fn from(e: VaultError) -> Self {
        match e {
            VaultError::RecordNotFound => ApiError::NotFound,
            VaultError::AlreadyExists => ApiError::Conflict,
            VaultError::NotFound => ApiError::NotInitialized,
            VaultError::WrongPassword => ApiError::Unauthorized,
            VaultError::TooManyAttempts(s) => ApiError::TooManyAttempts(s),
            VaultError::FactorRejected => ApiError::Unauthorized,
            VaultError::StepUpRequired => ApiError::StepUpRequired,
            VaultError::HardwareBound => ApiError::BadRequest(
                "coffre lié au matériel : connectez-vous avec votre clé biométrique".into(),
            ),
            VaultError::Precondition(m) => {
                ApiError::BadRequest(format!("opération impossible : {m}"))
            }
            other => {
                tracing::error!(error = %other, "erreur du coffre");
                ApiError::Internal
            }
        }
    }
}

impl From<serde_json::Error> for ApiError {
    fn from(e: serde_json::Error) -> Self {
        tracing::error!(error = %e, "erreur de sérialisation");
        ApiError::Internal
    }
}
