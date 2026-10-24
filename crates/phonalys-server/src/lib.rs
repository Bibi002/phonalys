//! Serveur auto-hébergé de Phonalys.

use axum::http::{header, HeaderName, HeaderValue};
use axum::{routing::get, Json, Router};
use serde::Serialize;
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::set_header::SetResponseHeaderLayer;

/// Taille maximale d'une requête (documents importés compris).
pub const MAX_BODY_BYTES: usize = 25 * 1024 * 1024;

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

/// Construit l'application HTTP.
pub fn app() -> Router {
    let mut router = Router::new()
        .route("/api/health", get(health))
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

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    #[tokio::test]
    async fn health_has_security_headers() {
        let res = app()
            .oneshot(Request::get("/api/health").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        for (name, _) in SECURITY_HEADERS {
            assert!(
                res.headers().contains_key(*name),
                "en-tête manquant : {name}"
            );
        }
    }

    #[tokio::test]
    async fn unknown_routes_are_404_with_headers() {
        let res = app()
            .oneshot(Request::get("/admin").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
        assert!(res.headers().contains_key("content-security-policy"));
    }
}
