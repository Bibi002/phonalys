//! Serveur auto-hébergé de Phonalys.

mod api;
mod error;
mod state;
pub mod tls;

pub use state::{AppState, Config};

use axum::extract::Request;
use axum::http::{header, HeaderName, HeaderValue, Method, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Serialize;
use std::time::Duration;
use tower_http::limit::RequestBodyLimitLayer;
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

/// Refuse les requêtes modifiantes sans en-tête anti-CSRF ou venant d'une autre origine.
async fn csrf_guard(req: Request, next: Next) -> Response {
    let safe = matches!(*req.method(), Method::GET | Method::HEAD | Method::OPTIONS);
    if !safe {
        let headers = req.headers();
        let marked = headers.get(CSRF_HEADER).is_some_and(|v| v == "1");
        let same_origin = match (headers.get(header::ORIGIN), headers.get(header::HOST)) {
            (None, _) => true,
            (Some(origin), Some(host)) => {
                let origin = origin.to_str().unwrap_or_default();
                let host = host.to_str().unwrap_or_default();
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
            get(api::get_patient).delete(api::delete_patient),
        )
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            api::require_session,
        ));

    let mut router = Router::new()
        .route("/api/health", get(health))
        .route("/api/status", get(api::status))
        .route("/api/setup", post(api::setup))
        .route("/api/login", post(api::login))
        .merge(protected)
        .with_state(state)
        .layer(middleware::from_fn(csrf_guard))
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
