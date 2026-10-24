//! Serveur auto-hébergé de Phonalys.

mod api;
mod error;
mod passkey;
mod records;
mod state;
pub mod tls;

pub use state::{AppState, Config};

use axum::extract::Request;
use axum::http::{header, HeaderName, HeaderValue, Method, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{any, get, post};
use axum::{Json, Router};
use error::ApiError;
use serde::Serialize;
use std::sync::Arc;
use std::time::Duration;
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::set_header::SetResponseHeaderLayer;

/// Taille maximale d'une requête (documents importés compris).
pub const MAX_BODY_BYTES: usize = 25 * 1024 * 1024;

/// En-tête exigé sur toute requête qui modifie l'état. Un site tiers ne peut pas
/// l'ajouter sans requête CORS préalable, que le serveur n'autorise jamais.
pub const CSRF_HEADER: &str = "x-phonalys-request";

/// En-têtes de durcissement appliqués à chaque réponse.
const SECURITY_HEADERS: &[(&str, &str)] = &[
    (
        "content-security-policy",
        "default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self' data: blob:; \
         connect-src 'self'; font-src 'self'; object-src 'none'; base-uri 'none'; \
         frame-ancestors 'none'; form-action 'self'",
    ),
    (
        "strict-transport-security",
        "max-age=63072000; includeSubDomains",
    ),
    ("x-content-type-options", "nosniff"),
    ("x-frame-options", "DENY"),
    ("referrer-policy", "no-referrer"),
    ("cross-origin-opener-policy", "same-origin"),
    ("cross-origin-resource-policy", "same-origin"),
    (
        "permissions-policy",
        "camera=(), microphone=(self), geolocation=(), payment=(), usb=()",
    ),
    ("cache-control", "no-store"),
];

#[derive(Debug, Serialize)]
struct Health {
    status: &'static str,
    version: &'static str,
}

async fn health() -> Json<Health> {
    Json(Health {
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
    })
}

/// Adresse demandée par le client : l'en-tête `Host` en HTTP/1.1, le pseudo-en-tête
/// `:authority` (porté par l'URI) en HTTP/2, où `Host` est généralement absent.
fn authority(req: &Request) -> Option<String> {
    req.headers()
        .get(header::HOST)
        .and_then(|h| h.to_str().ok())
        .map(str::to_owned)
        .or_else(|| req.uri().authority().map(|a| a.as_str().to_owned()))
        .map(|a| a.to_ascii_lowercase())
}

/// Adresses acceptées : celles des origines publiques du serveur, avec et sans port
/// par défaut.
fn allowed_authorities(origins: &[String]) -> Vec<String> {
    origins
        .iter()
        .filter_map(|o| url::Url::parse(o).ok())
        .flat_map(|u| {
            let host = u.host_str().unwrap_or_default().to_ascii_lowercase();
            let port = u.port_or_known_default().unwrap_or_default();
            let mut list = vec![format!("{host}:{port}")];
            if u.port().is_none() {
                list.push(host);
            }
            list
        })
        .collect()
}

/// Refuse toute requête adressée à un autre nom que celui du serveur. Bloque le
/// « DNS rebinding » : un site malveillant dont le domaine pointerait vers
/// 127.0.0.1 ou vers le serveur du cabinet ne peut pas lui parler.
async fn host_guard(allowed: Arc<Vec<String>>, req: Request, next: Next) -> Response {
    match authority(&req) {
        Some(a) if allowed.contains(&a) => next.run(req).await,
        _ => {
            tracing::warn!(authority = ?authority(&req), "requête adressée à un nom non autorisé");
            let hint = allowed
                .first()
                .map_or_else(String::new, |a| format!(" : utilisez {a}"));
            (
                StatusCode::MISDIRECTED_REQUEST,
                format!("adresse non autorisée{hint}"),
            )
                .into_response()
        }
    }
}

/// Refuse les requêtes modifiantes sans en-tête anti-CSRF ou venant d'une autre origine.
async fn csrf_guard(req: Request, next: Next) -> Response {
    let safe = matches!(*req.method(), Method::GET | Method::HEAD | Method::OPTIONS);
    if !safe {
        let marked = req.headers().get(CSRF_HEADER).is_some_and(|v| v == "1");
        let same_origin = match (req.headers().get(header::ORIGIN), authority(&req)) {
            (None, _) => true,
            (Some(origin), Some(host)) => {
                let origin = origin.to_str().unwrap_or_default().to_ascii_lowercase();
                origin == format!("https://{host}") || origin == format!("http://{host}")
            }
            (Some(_), None) => false,
        };
        if !marked || !same_origin {
            return (StatusCode::FORBIDDEN, "requête refusée").into_response();
        }
    }
    next.run(req).await
}

/// Construit l'application HTTP.
pub fn app(state: AppState) -> Router {
    let protected = Router::new()
        .route("/api/session", get(api::session))
        .route("/api/logout", post(api::logout))
        .route("/api/lock", post(api::lock))
        .route(
            "/api/patients",
            get(api::list_patients).post(api::create_patient),
        )
        .route(
            "/api/patients/{id}",
            get(api::get_patient)
                .put(api::update_patient)
                .delete(api::delete_patient),
        )
        .route("/api/passkeys", get(passkey::list))
        .route(
            "/api/passkeys/register/start",
            post(passkey::register_start),
        )
        .route(
            "/api/passkeys/register/finish",
            post(passkey::register_finish),
        )
        .route("/api/passkeys/bind/start", post(passkey::bind_start))
        .route("/api/passkeys/bind/finish", post(passkey::bind_finish))
        .route("/api/passkeys/{id}", axum::routing::delete(passkey::remove))
        .route(
            "/api/passkeys/{id}/quick",
            axum::routing::delete(passkey::remove_quick),
        )
        .route(
            "/api/records/{kind}",
            get(records::list).post(records::create),
        )
        .route(
            "/api/records/{kind}/{id}",
            get(records::get)
                .put(records::update)
                .delete(records::delete),
        )
        .route(
            "/api/settings",
            get(records::get_settings).put(records::put_settings),
        )
        .route("/api/audit", get(records::audit))
        .route("/api/security", get(passkey::status))
        .route(
            "/api/security/policy",
            get(passkey::get_policy).put(passkey::put_policy),
        )
        .route("/api/security/hardware-binding", post(passkey::set_binding))
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            api::require_session,
        ));

    let web_dir = state.config.web_dir.clone();
    let allowed = Arc::new(allowed_authorities(&state.config.relying_party.origins));
    let api = Router::new()
        .route("/api/health", get(health))
        .route("/api/status", get(api::status))
        .route("/api/setup", post(api::setup))
        .route("/api/login", post(api::login))
        .route("/api/login/passkey/start", post(passkey::login_start))
        .route("/api/login/passkey/finish", post(passkey::login_finish))
        .route("/api/login/quick/start", post(passkey::quick_start))
        .route("/api/login/quick/finish", post(passkey::quick_finish))
        // Une route d'API inconnue ne doit jamais renvoyer la page de l'application.
        .route("/api/{*rest}", any(|| async { ApiError::NotFound }))
        .merge(protected)
        .with_state(state);

    // Application web : fichiers statiques, et `index.html` pour les routes de l'interface.
    let api = match web_dir {
        Some(dir) => api
            .fallback_service(ServeDir::new(&dir).fallback(ServeFile::new(dir.join("index.html")))),
        None => api,
    };

    let mut router = api
        .layer(middleware::from_fn(csrf_guard))
        .layer(middleware::from_fn(move |req, next| {
            host_guard(allowed.clone(), req, next)
        }))
        .layer(RequestBodyLimitLayer::new(MAX_BODY_BYTES));
    for (name, value) in SECURITY_HEADERS {
        router = router.layer(SetResponseHeaderLayer::overriding(
            HeaderName::from_static(name),
            HeaderValue::from_static(value),
        ));
    }
    router.layer(SetResponseHeaderLayer::overriding(
        header::SERVER,
        HeaderValue::from_static("phonalys"),
    ))
}

/// Tâche de fond : verrouille le coffre dès qu'aucune session n'est plus active.
pub fn spawn_auto_lock(state: AppState) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(15));
        loop {
            tick.tick().await;
            state.auto_lock(state::now());
        }
    })
}

#[cfg(test)]
mod tests;
