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
const RP_ID: &str = "localhost";
const ORIGIN: &str = "https://localhost:8443";

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
            relying_party: phonalys_webauthn::RelyingParty {
                id: RP_ID.into(),
                name: "Phonalys".into(),
                origins: vec![ORIGIN.into()],
            },
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

// --- Clés biométriques (WebAuthn) ------------------------------------------------------

use phonalys_webauthn::testing::{Algorithm, SoftAuthenticator};

impl Harness {
    /// Inscrit une clé simulée pendant une session ouverte.
    async fn register_passkey(&self, cookie: &str, alg: Algorithm) -> SoftAuthenticator {
        let device = SoftAuthenticator::new(alg);
        let (s, _, options) = self
            .call("POST", "/api/passkeys/register/start", None, Some(cookie))
            .await;
        assert_eq!(s, StatusCode::OK, "{options}");
        assert_eq!(
            options["authenticatorSelection"]["userVerification"],
            "required"
        );
        let response = device.register(RP_ID, ORIGIN, options["challenge"].as_str().unwrap());
        let (s, _, body) = self
            .call(
                "POST",
                "/api/passkeys/register/finish",
                Some(json!({ "response": response, "label": "Windows Hello" })),
                Some(cookie),
            )
            .await;
        assert_eq!(s, StatusCode::CREATED, "{body}");
        device
    }

    /// Connexion complète par phrase de passe + clé biométrique.
    async fn passkey_login(&self, device: &mut SoftAuthenticator) -> (StatusCode, HeaderMap) {
        let (s, _, options) = self
            .call(
                "POST",
                "/api/login/passkey/start",
                Some(json!({ "passphrase": PASSPHRASE })),
                None,
            )
            .await;
        assert_eq!(s, StatusCode::OK, "{options}");
        let response = device.authenticate(RP_ID, ORIGIN, options["challenge"].as_str().unwrap());
        let (s, headers, _) = self
            .call(
                "POST",
                "/api/login/passkey/finish",
                Some(json!({ "response": response })),
                None,
            )
            .await;
        (s, headers)
    }
}

#[tokio::test]
async fn passkey_registration_requires_session() {
    let h = Harness::new();
    h.setup().await;
    let (s, _, _) = h
        .call("POST", "/api/passkeys/register/start", None, None)
        .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn biometric_login_end_to_end() {
    for alg in [Algorithm::Es256, Algorithm::Ed25519] {
        let h = Harness::new();
        let (totp, _) = h.setup().await;
        let cookie = h.login(&totp).await;
        let mut device = h.register_passkey(&cookie, alg).await;
        let (_, _, list) = h.call("GET", "/api/passkeys", None, Some(&cookie)).await;
        assert_eq!(list[0]["label"], "Windows Hello");

        h.call("POST", "/api/lock", None, Some(&cookie)).await;
        assert!(!h.state.is_unlocked());

        let (s, headers) = h.passkey_login(&mut device).await;
        assert_eq!(s, StatusCode::NO_CONTENT, "{alg:?}");
        let cookie = cookie_from(&headers);
        let (s, _, _) = h.call("GET", "/api/patients", None, Some(&cookie)).await;
        assert_eq!(s, StatusCode::OK);
    }
}

#[tokio::test]
async fn passkey_login_needs_the_passphrase_too() {
    let h = Harness::new();
    let (totp, _) = h.setup().await;
    let cookie = h.login(&totp).await;
    h.register_passkey(&cookie, Algorithm::Es256).await;
    h.call("POST", "/api/lock", None, Some(&cookie)).await;

    let (s, _, _) = h
        .call(
            "POST",
            "/api/login/passkey/start",
            Some(json!({ "passphrase": "mauvaise phrase de passe" })),
            None,
        )
        .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    // Sans défi en cours, une signature seule ne sert à rien.
    let mut device = SoftAuthenticator::new(Algorithm::Es256);
    let forged = device.authenticate(RP_ID, ORIGIN, "AAAA");
    let (s, _, _) = h
        .call(
            "POST",
            "/api/login/passkey/finish",
            Some(json!({ "response": forged })),
            None,
        )
        .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    assert!(!h.state.is_unlocked());
}

#[tokio::test]
async fn device_without_biometrics_is_refused_and_counted() {
    let h = Harness::new();
    let (totp, _) = h.setup().await;
    let cookie = h.login(&totp).await;
    let mut device = h.register_passkey(&cookie, Algorithm::Es256).await;
    h.call("POST", "/api/lock", None, Some(&cookie)).await;

    device.user_verified = false;
    let (s, _) = h.passkey_login(&mut device).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    assert!(!h.state.is_unlocked());

    // Avec la biométrie, une nouvelle tentative (nouveau défi) réussit.
    device.user_verified = true;
    let (s, headers) = h.passkey_login(&mut device).await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let cookie = cookie_from(&headers);
    let (_, _, list) = h.call("GET", "/api/passkeys", None, Some(&cookie)).await;
    assert_eq!(list.as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn phishing_site_signature_is_refused() {
    let h = Harness::new();
    let (totp, _) = h.setup().await;
    let cookie = h.login(&totp).await;
    let mut device = h.register_passkey(&cookie, Algorithm::Es256).await;
    h.call("POST", "/api/lock", None, Some(&cookie)).await;

    let (_, _, options) = h
        .call(
            "POST",
            "/api/login/passkey/start",
            Some(json!({ "passphrase": PASSPHRASE })),
            None,
        )
        .await;
    let response = device.authenticate(
        RP_ID,
        "https://phonalys-connexion.example",
        options["challenge"].as_str().unwrap(),
    );
    let (s, _, _) = h
        .call(
            "POST",
            "/api/login/passkey/finish",
            Some(json!({ "response": response })),
            None,
        )
        .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn mandatory_passkey_policy() {
    let h = Harness::new();
    let (totp, codes) = h.setup().await;
    let cookie = h.login(&totp).await;

    // Impossible d'exiger une clé tant qu'aucune n'est enregistrée.
    let (s, _, _) = h
        .call(
            "PUT",
            "/api/security/policy",
            Some(json!({ "require_passkey": true })),
            Some(&cookie),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);

    let mut device = h.register_passkey(&cookie, Algorithm::Es256).await;
    let (s, _, policy) = h
        .call(
            "PUT",
            "/api/security/policy",
            Some(json!({ "require_passkey": true })),
            Some(&cookie),
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(policy["require_passkey"], true);

    // La dernière clé obligatoire ne peut pas être retirée.
    let (_, _, list) = h.call("GET", "/api/passkeys", None, Some(&cookie)).await;
    let id = list[0]["id"].as_str().unwrap().to_owned();
    let (s, _, _) = h
        .call(
            "DELETE",
            &format!("/api/passkeys/{id}"),
            None,
            Some(&cookie),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);

    h.call("POST", "/api/lock", None, Some(&cookie)).await;

    // Le TOTP n'est plus accepté…
    let (s, _, _) = h
        .call(
            "POST",
            "/api/login",
            Some(json!({ "passphrase": PASSPHRASE, "code": totp.code_at(state::now() + 30) })),
            None,
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    // …la clé biométrique, si…
    let (s, _) = h.passkey_login(&mut device).await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    // …et les codes de récupération aussi, en cas de perte de l'appareil.
    let (s, _, _) = h
        .call(
            "POST",
            "/api/login",
            Some(json!({ "passphrase": PASSPHRASE, "recovery_code": codes[0] })),
            None,
        )
        .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn expired_passkey_challenge_is_purged() {
    let h = Harness::new();
    let (totp, _) = h.setup().await;
    let cookie = h.login(&totp).await;
    let mut device = h.register_passkey(&cookie, Algorithm::Es256).await;
    h.call("POST", "/api/lock", None, Some(&cookie)).await;

    let (_, _, options) = h
        .call(
            "POST",
            "/api/login/passkey/start",
            Some(json!({ "passphrase": PASSPHRASE })),
            None,
        )
        .await;
    h.state
        .auto_lock(state::now() + state::CHALLENGE_TTL_SECS + 1);
    let response = device.authenticate(RP_ID, ORIGIN, options["challenge"].as_str().unwrap());
    let (s, _, _) = h
        .call(
            "POST",
            "/api/login/passkey/finish",
            Some(json!({ "response": response })),
            None,
        )
        .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
}
