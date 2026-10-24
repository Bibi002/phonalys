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
                format!("trop de tentatives, réessayez dans {s} s"),
            ),
            ApiError::Internal => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "erreur interne".to_owned(),
            ),
        };
        let mut res = (status, Json(serde_json::json!({ "error": message }))).into_response();
        if let ApiError::TooManyAttempts(s) = self {
            if let Ok(v) = s.to_string().parse() {
                res.headers_mut().insert(header::RETRY_AFTER, v);
            }
        }
        res
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
