//! Tests de bout en bout de l'API, sans réseau.

use super::*;
use axum::body::{to_bytes, Body};
use axum::http::Request;
use data_encoding::BASE32_NOPAD;
use phonalys_auth::{SessionConfig, Totp};
use phonalys_crypto::KdfParams;
use serde_json::{json, Value};
use tower::ServiceExt;

const PASSPHRASE: &str = "girafe tambour lumière escalier";

struct Harness {
    _dir: tempfile::TempDir,
    state: AppState,
    setup_token: String,
}

impl Harness {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let (state, token) = AppState::new(Config {
            vault_path: dir.path().join("cabinet.phonalys-vault"),
            kdf: KdfParams::insecure_for_tests(),
            sessions: SessionConfig::default(),
        });
        Self {
            _dir: dir,
            state,
            setup_token: token.unwrap(),
        }
    }

    async fn call(
        &self,
        method: &str,
        uri: &str,
        body: Option<Value>,
        cookie: Option<&str>,
    ) -> (StatusCode, HeaderMap, Value) {
        let mut req = Request::builder()
            .method(method)
            .uri(uri)
            .header("host", "localhost:8443");
        if method != "GET" {
            req = req.header(CSRF_HEADER, "1");
        }
        if let Some(c) = cookie {
            req = req.header("cookie", c);
        }
        let req = match body {
            Some(b) => req
                .header("content-type", "application/json")
                .body(Body::from(b.to_string())),
            None => req.body(Body::empty()),
        }
        .unwrap();
        let res = app(self.state.clone()).oneshot(req).await.unwrap();
        let (status, headers) = (res.status(), res.headers().clone());
        let bytes = to_bytes(res.into_body(), usize::MAX).await.unwrap();
        (
            status,
            headers,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }

    /// Installation complète ; renvoie le TOTP et les codes de récupération.
    async fn setup(&self) -> (Totp, Vec<String>) {
        let (status, _, body) = self
            .call("POST", "/api/setup", Some(json!({ "setup_token": self.setup_token, "passphrase": PASSPHRASE, "practitioner_name": "Marie Martin" })), None)
            .await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
        let secret = BASE32_NOPAD
            .decode(body["totp_secret"].as_str().unwrap().as_bytes())
            .unwrap();
        let codes = body["recovery_codes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c.as_str().unwrap().to_owned())
            .collect();
        (Totp::from_secret(secret), codes)
    }

    async fn login(&self, totp: &Totp) -> String {
        let (status, headers, body) = self
            .call(
                "POST",
                "/api/login",
                Some(json!({ "passphrase": PASSPHRASE, "code": totp.code_at(state::now()) })),
                None,
            )
            .await;
        assert_eq!(status, StatusCode::NO_CONTENT, "{body}");
        cookie_from(&headers)
    }
}

use axum::http::HeaderMap;

fn cookie_from(headers: &HeaderMap) -> String {
    let set = headers.get("set-cookie").unwrap().to_str().unwrap();
    assert!(set.contains("HttpOnly") && set.contains("Secure") && set.contains("SameSite=Strict"));
    set.split(';').next().unwrap().to_owned()
}

#[tokio::test]
async fn health_has_security_headers() {
    let h = Harness::new();
    let (status, headers, _) = h.call("GET", "/api/health", None, None).await;
    assert_eq!(status, StatusCode::OK);
    for (name, _) in SECURITY_HEADERS {
        assert!(headers.contains_key(*name), "en-tête manquant : {name}");
    }
}

#[tokio::test]
async fn setup_requires_token_and_strong_passphrase_and_runs_once() {
    let h = Harness::new();
    let (s, _, _) = h
        .call(
            "POST",
            "/api/setup",
            Some(json!({ "setup_token": "FAUX", "passphrase": PASSPHRASE })),
            None,
        )
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, _, _) = h
        .call(
            "POST",
            "/api/setup",
            Some(json!({ "setup_token": h.setup_token, "passphrase": "motdepasse123456" })),
            None,
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, _, _) = h.call("POST", "/api/setup", Some(json!({ "setup_token": h.setup_token, "passphrase": "Martin girafe tambour lumière" , "practitioner_name": "Marie Martin" })), None).await;
    assert_eq!(
        s,
        StatusCode::BAD_REQUEST,
        "le nom du praticien est refusé dans la phrase de passe"
    );

    let (_, codes) = h.setup().await;
    assert_eq!(codes.len(), 10);
    let (s, _, _) = h
        .call(
            "POST",
            "/api/setup",
            Some(json!({ "setup_token": h.setup_token, "passphrase": PASSPHRASE })),
            None,
        )
        .await;
    assert_eq!(s, StatusCode::CONFLICT);
    let (_, _, status) = h.call("GET", "/api/status", None, None).await;
    assert_eq!(status["initialized"], true);
}

#[tokio::test]
async fn login_requires_both_factors() {
    let h = Harness::new();
    let (totp, _) = h.setup().await;
    let now = state::now();

    let (s, _, _) = h
        .call(
            "POST",
            "/api/login",
            Some(json!({ "passphrase": "mauvaise phrase de passe", "code": totp.code_at(now) })),
            None,
        )
        .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    let (s, _, _) = h
        .call(
            "POST",
            "/api/login",
            Some(json!({ "passphrase": PASSPHRASE })),
            None,
        )
        .await;
    assert_eq!(
        s,
        StatusCode::UNAUTHORIZED,
        "phrase de passe seule : refusé"
    );
    let (s, _, _) = h
        .call(
            "POST",
            "/api/login",
            Some(json!({ "passphrase": PASSPHRASE, "code": "000000" })),
            None,
        )
        .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    assert!(!h.state.is_unlocked());

    // Au-delà de 3 échecs, un délai s'impose, même avec les bons facteurs. On en
    // provoque 6 pour obtenir 4 s de délai et un test non fragile.
    for code in ["111111", "222222", "333333"] {
        let (s, _, _) = h
            .call(
                "POST",
                "/api/login",
                Some(json!({ "passphrase": PASSPHRASE, "code": code })),
                None,
            )
            .await;
        assert!(matches!(
            s,
            StatusCode::UNAUTHORIZED | StatusCode::TOO_MANY_REQUESTS
        ));
    }
    let (s, headers, _) = h
        .call(
            "POST",
            "/api/login",
            Some(json!({ "passphrase": PASSPHRASE, "code": totp.code_at(now) })),
            None,
        )
        .await;
    assert_eq!(s, StatusCode::TOO_MANY_REQUESTS);
    assert!(headers.contains_key("retry-after"));
}

#[tokio::test]
async fn totp_code_cannot_be_replayed() {
    let h = Harness::new();
    let (totp, _) = h.setup().await;
    let code = totp.code_at(state::now());
    let (s, _, _) = h
        .call(
            "POST",
            "/api/login",
            Some(json!({ "passphrase": PASSPHRASE, "code": code })),
            None,
        )
        .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (s, _, _) = h
        .call(
            "POST",
            "/api/login",
            Some(json!({ "passphrase": PASSPHRASE, "code": code })),
            None,
        )
        .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn recovery_code_works_once() {
    let h = Harness::new();
    let (_, codes) = h.setup().await;
    let body = json!({ "passphrase": PASSPHRASE, "recovery_code": codes[0] });
    let (s, _, _) = h.call("POST", "/api/login", Some(body.clone()), None).await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (s, _, _) = h.call("POST", "/api/login", Some(body), None).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn patient_crud_requires_session() {
    let h = Harness::new();
    let (totp, _) = h.setup().await;

    let (s, _, _) = h.call("GET", "/api/patients", None, None).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);

    let cookie = h.login(&totp).await;
    let patient = json!({ "first_name": "Léa", "last_name": "Dupont", "birth_date": "2018-03-15" });
    let (s, _, created) = h
        .call(
            "POST",
            "/api/patients",
            Some(patient.clone()),
            Some(&cookie),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED);
    let id = created["id"].as_str().unwrap().to_owned();

    let (s, _, got) = h
        .call("GET", &format!("/api/patients/{id}"), None, Some(&cookie))
        .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(got, patient);

    let (_, _, list) = h.call("GET", "/api/patients", None, Some(&cookie)).await;
    assert_eq!(list.as_array().unwrap().len(), 1);
    assert_eq!(list[0]["id"], id.as_str());

    let (s, _, _) = h
        .call("GET", "/api/patients/../../etc", None, Some(&cookie))
        .await;
    assert_ne!(s, StatusCode::OK);
    let (s, _, _) = h
        .call("POST", "/api/patients", Some(json!([1, 2])), Some(&cookie))
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);

    let (s, _, _) = h
        .call(
            "DELETE",
            &format!("/api/patients/{id}"),
            None,
            Some(&cookie),
        )
        .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (s, _, _) = h
        .call("GET", &format!("/api/patients/{id}"), None, Some(&cookie))
        .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn csrf_header_and_origin_are_enforced() {
    let h = Harness::new();
    let (totp, _) = h.setup().await;
    let cookie = h.login(&totp).await;

    let without_header = Request::post("/api/patients")
        .header("host", "localhost:8443")
        .header("cookie", &cookie)
        .header("content-type", "application/json")
        .body(Body::from("{}"))
        .unwrap();
    assert_eq!(
        app(h.state.clone())
            .oneshot(without_header)
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );

    let foreign_origin = Request::post("/api/patients")
        .header("host", "localhost:8443")
        .header("origin", "https://site-malveillant.example")
        .header(CSRF_HEADER, "1")
        .header("cookie", &cookie)
        .header("content-type", "application/json")
        .body(Body::from("{}"))
        .unwrap();
    assert_eq!(
        app(h.state.clone())
            .oneshot(foreign_origin)
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn lock_and_logout_close_the_vault() {
    let h = Harness::new();
    let (totp, codes) = h.setup().await;
    let cookie = h.login(&totp).await;
    assert!(h.state.is_unlocked());

    let (s, _, _) = h.call("POST", "/api/lock", None, Some(&cookie)).await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    assert!(!h.state.is_unlocked());
    let (s, _, _) = h.call("GET", "/api/patients", None, Some(&cookie)).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED, "l'ancien jeton est révoqué");

    // Le code TOTP vient d'être consommé : on se reconnecte avec un code de récupération.
    let (s, headers, _) = h
        .call(
            "POST",
            "/api/login",
            Some(json!({ "passphrase": PASSPHRASE, "recovery_code": codes[1] })),
            None,
        )
        .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let cookie = cookie_from(&headers);
    let (s, _, _) = h.call("POST", "/api/logout", None, Some(&cookie)).await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    assert!(
        !h.state.is_unlocked(),
        "dernière session fermée : coffre verrouillé"
    );
}

#[tokio::test]
async fn auto_lock_after_idle_timeout() {
    let h = Harness::new();
    let (totp, _) = h.setup().await;
    let _cookie = h.login(&totp).await;
    h.state.auto_lock(state::now());
    assert!(h.state.is_unlocked(), "session encore active");
    h.state
        .auto_lock(state::now() + SessionConfig::default().idle_timeout_secs + 1);
    assert!(!h.state.is_unlocked());
}
