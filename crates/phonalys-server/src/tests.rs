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
        let web = dir.path().join("web");
        std::fs::create_dir_all(web.join("assets")).unwrap();
        std::fs::write(
            web.join("index.html"),
            "<!doctype html><title>Phonalys</title>",
        )
        .unwrap();
        std::fs::write(web.join("assets/app.js"), "console.log(1)").unwrap();
        let (state, token) = AppState::new(Config {
            web_dir: Some(web),
            backup_dir: dir.path().join("sauvegardes"),
            authority: None,
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
        self.login_at(totp, state::now()).await
    }

    /// Connexion avec le code TOTP d'un instant donné (évite l'anti-rejeu).
    async fn login_at(&self, totp: &Totp, at: u64) -> String {
        let (status, headers, body) = self
            .call(
                "POST",
                "/api/login",
                Some(json!({ "passphrase": PASSPHRASE, "code": totp.code_at(at) })),
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
    // Délai par défaut des réglages du cabinet : 15 minutes.
    h.state.auto_lock(state::now() + 15 * 60 - 10);
    assert!(h.state.is_unlocked());
    h.state.auto_lock(state::now() + 15 * 60 + 1);
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
        // Comme un navigateur : la sortie PRF n'est calculée que si le serveur fournit un sel.
        let mut body = json!({ "response": response });
        if let Some(salt) = prf_salt(&options) {
            body["prf_output"] = json!(device.prf_b64(&salt));
        }
        let (s, headers, _) = self
            .call("POST", "/api/login/passkey/finish", Some(body), None)
            .await;
        (s, headers)
    }

    /// Lie une clé déjà inscrite au coffre (extension PRF).
    async fn bind_passkey(&self, cookie: &str, device: &mut SoftAuthenticator) -> StatusCode {
        let id = BASE64URL_NOPAD.encode(&device.credential_id);
        let (s, _, options) = self
            .call(
                "POST",
                "/api/passkeys/bind/start",
                Some(json!({ "credential_id": id })),
                Some(cookie),
            )
            .await;
        assert_eq!(s, StatusCode::OK, "{options}");
        let salt = prf_salt(&options).expect("le lien demande un sel PRF");
        let response = device.authenticate(RP_ID, ORIGIN, options["challenge"].as_str().unwrap());
        let (s, _, _) = self
            .call(
                "POST",
                "/api/passkeys/bind/finish",
                Some(json!({ "passphrase": PASSPHRASE, "response": response, "prf_output": device.prf_b64(&salt) })),
                Some(cookie),
            )
            .await;
        s
    }

    /// Active le mode lié ; renvoie les nouveaux codes de récupération.
    async fn enable_binding(&self, cookie: &str) -> Vec<String> {
        let (s, _, body) = self
            .call(
                "POST",
                "/api/security/hardware-binding",
                Some(json!({ "passphrase": PASSPHRASE, "enabled": true })),
                Some(cookie),
            )
            .await;
        assert_eq!(s, StatusCode::OK, "{body}");
        assert_eq!(body["hardware_bound"], true);
        body["recovery_codes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c.as_str().unwrap().to_owned())
            .collect()
    }
}

use data_encoding::BASE64URL_NOPAD;

fn prf_salt(options: &Value) -> Option<Vec<u8>> {
    options["extensions"]["prf"]["eval"]["first"]
        .as_str()
        .map(|s| BASE64URL_NOPAD.decode(s.as_bytes()).unwrap())
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

// --- Mode lié au matériel (extension PRF) -----------------------------------------------

#[tokio::test]
async fn hardware_binding_end_to_end() {
    let h = Harness::new();
    let (totp, old_codes) = h.setup().await;
    let cookie = h.login(&totp).await;
    let mut device = h.register_passkey(&cookie, Algorithm::Es256).await;

    // Impossible d'activer le mode lié sans clé liée.
    let (s, _, _) = h
        .call(
            "POST",
            "/api/security/hardware-binding",
            Some(json!({ "passphrase": PASSPHRASE, "enabled": true })),
            Some(&cookie),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);

    assert_eq!(h.bind_passkey(&cookie, &mut device).await, StatusCode::OK);
    let (_, _, list) = h.call("GET", "/api/passkeys", None, Some(&cookie)).await;
    assert_eq!(list[0]["bound"], true);

    let codes = h.enable_binding(&cookie).await;
    assert_eq!(codes.len(), 10);
    let (_, _, status) = h.call("GET", "/api/security", None, Some(&cookie)).await;
    assert_eq!(status["hardware_bound"], true);
    assert_eq!(status["require_passkey"], true);
    assert_eq!(status["bound_passkeys"], 1);
    assert_eq!(status["recovery_codes_remaining"], 10);

    // La politique ne peut plus être assouplie tant que le coffre est lié.
    let (s, _, _) = h
        .call(
            "PUT",
            "/api/security/policy",
            Some(json!({ "require_passkey": false })),
            Some(&cookie),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    h.call("POST", "/api/lock", None, Some(&cookie)).await;

    // La phrase de passe seule (même avec le TOTP) n'ouvre plus le coffre.
    let (s, _, _) = h
        .call(
            "POST",
            "/api/login",
            Some(json!({ "passphrase": PASSPHRASE, "code": totp.code_at(state::now() + 30) })),
            None,
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    assert!(!h.state.is_unlocked());

    // Les anciens codes de récupération ont été révoqués.
    let (s, _, _) = h
        .call(
            "POST",
            "/api/login",
            Some(json!({ "passphrase": PASSPHRASE, "recovery_code": old_codes[0] })),
            None,
        )
        .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);

    // La clé liée ouvre le coffre.
    let (s, headers) = h.passkey_login(&mut device).await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let cookie = cookie_from(&headers);
    let (s, _, _) = h.call("GET", "/api/patients", None, Some(&cookie)).await;
    assert_eq!(s, StatusCode::OK);
    h.call("POST", "/api/lock", None, Some(&cookie)).await;

    // Un nouveau code de récupération fonctionne, une seule fois.
    let login_with = |code: String| {
        let h = &h;
        async move {
            h.call(
                "POST",
                "/api/login",
                Some(json!({ "passphrase": PASSPHRASE, "recovery_code": code })),
                None,
            )
            .await
            .0
        }
    };
    assert_eq!(login_with(codes[0].clone()).await, StatusCode::NO_CONTENT);
    h.state.auto_lock(state::now() + 10 * 3600);
    assert!(!h.state.is_unlocked());
    assert_eq!(login_with(codes[0].clone()).await, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn bound_vault_rejects_wrong_prf_and_missing_prf() {
    let h = Harness::new();
    let (totp, _) = h.setup().await;
    let cookie = h.login(&totp).await;
    let mut device = h.register_passkey(&cookie, Algorithm::Es256).await;
    assert_eq!(h.bind_passkey(&cookie, &mut device).await, StatusCode::OK);
    h.enable_binding(&cookie).await;
    h.call("POST", "/api/lock", None, Some(&cookie)).await;

    let start = || async {
        let (s, _, options) = h
            .call(
                "POST",
                "/api/login/passkey/start",
                Some(json!({ "passphrase": PASSPHRASE })),
                None,
            )
            .await;
        assert_eq!(s, StatusCode::OK);
        options
    };

    // Appareil sans PRF : refus explicite.
    let options = start().await;
    let response = device.authenticate(RP_ID, ORIGIN, options["challenge"].as_str().unwrap());
    let (s, _, _) = h
        .call(
            "POST",
            "/api/login/passkey/finish",
            Some(json!({ "response": response })),
            None,
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);

    // Sortie PRF d'un autre appareil (ou falsifiée) : refus.
    let options = start().await;
    let response = device.authenticate(RP_ID, ORIGIN, options["challenge"].as_str().unwrap());
    let other = SoftAuthenticator::new(Algorithm::Es256);
    let (s, _, _) = h
        .call(
            "POST",
            "/api/login/passkey/finish",
            Some(json!({ "response": response, "prf_output": other.prf_b64(&prf_salt(&options).unwrap()) })),
            None,
        )
        .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    assert!(!h.state.is_unlocked());

    // Mauvaise phrase de passe : rien n'est révélé au démarrage (pas d'oracle sans
    // l'appareil), mais même le bon appareil ne peut pas ouvrir le coffre.
    let (s, _, options) = h
        .call(
            "POST",
            "/api/login/passkey/start",
            Some(json!({ "passphrase": "mauvaise phrase de passe" })),
            None,
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    let response = device.authenticate(RP_ID, ORIGIN, options["challenge"].as_str().unwrap());
    let (s, _, _) = h
        .call(
            "POST",
            "/api/login/passkey/finish",
            Some(json!({ "response": response, "prf_output": device.prf_b64(&prf_salt(&options).unwrap()) })),
            None,
        )
        .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    assert!(!h.state.is_unlocked());
}

#[tokio::test]
async fn last_bound_key_cannot_be_removed_and_binding_can_be_disabled() {
    let h = Harness::new();
    let (totp, _) = h.setup().await;
    let cookie = h.login(&totp).await;
    let mut device = h.register_passkey(&cookie, Algorithm::Es256).await;
    assert_eq!(h.bind_passkey(&cookie, &mut device).await, StatusCode::OK);
    let mut second = h.register_passkey(&cookie, Algorithm::Ed25519).await;
    assert_eq!(h.bind_passkey(&cookie, &mut second).await, StatusCode::OK);
    h.enable_binding(&cookie).await;

    let id = BASE64URL_NOPAD.encode(&device.credential_id);
    let id2 = BASE64URL_NOPAD.encode(&second.credential_id);
    let (s, _, _) = h
        .call(
            "DELETE",
            &format!("/api/passkeys/{id}"),
            None,
            Some(&cookie),
        )
        .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (s, _, _) = h
        .call(
            "DELETE",
            &format!("/api/passkeys/{id2}"),
            None,
            Some(&cookie),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "dernière clé liée");

    // La clé retirée n'ouvre plus le coffre.
    h.call("POST", "/api/lock", None, Some(&cookie)).await;
    let (s, _) = h.passkey_login(&mut device).await;
    assert_ne!(s, StatusCode::NO_CONTENT);
    let (s, headers) = h.passkey_login(&mut second).await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let cookie = cookie_from(&headers);

    // Désactivation : la phrase de passe redevient suffisante (avec le second facteur).
    let (s, _, body) = h
        .call(
            "POST",
            "/api/security/hardware-binding",
            Some(json!({ "passphrase": PASSPHRASE, "enabled": false })),
            Some(&cookie),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{body}");
    let (s, _, _) = h
        .call(
            "PUT",
            "/api/security/policy",
            Some(json!({ "require_passkey": false })),
            Some(&cookie),
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    h.call("POST", "/api/lock", None, Some(&cookie)).await;
    h.login_at(&totp, state::now() + 30).await;
}

#[tokio::test]
async fn binding_requires_the_passphrase() {
    let h = Harness::new();
    let (totp, _) = h.setup().await;
    let cookie = h.login(&totp).await;
    let mut device = h.register_passkey(&cookie, Algorithm::Es256).await;
    let id = BASE64URL_NOPAD.encode(&device.credential_id);
    let (_, _, options) = h
        .call(
            "POST",
            "/api/passkeys/bind/start",
            Some(json!({ "credential_id": id })),
            Some(&cookie),
        )
        .await;
    let salt = prf_salt(&options).unwrap();
    let response = device.authenticate(RP_ID, ORIGIN, options["challenge"].as_str().unwrap());
    let (s, _, _) = h
        .call(
            "POST",
            "/api/passkeys/bind/finish",
            Some(json!({ "passphrase": "mauvaise phrase de passe", "response": response, "prf_output": device.prf_b64(&salt) })),
            Some(&cookie),
        )
        .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    let (_, _, list) = h.call("GET", "/api/passkeys", None, Some(&cookie)).await;
    assert_eq!(list[0]["bound"], false);
}

// --- Application web ---------------------------------------------------------------------

async fn raw_get(h: &Harness, uri: &str) -> (StatusCode, HeaderMap, String) {
    let req = Request::builder()
        .uri(uri)
        .header("host", "localhost:8443")
        .body(Body::empty())
        .unwrap();
    let res = app(h.state.clone()).oneshot(req).await.unwrap();
    let (status, headers) = (res.status(), res.headers().clone());
    let bytes = to_bytes(res.into_body(), usize::MAX).await.unwrap();
    (
        status,
        headers,
        String::from_utf8_lossy(&bytes).into_owned(),
    )
}

#[tokio::test]
async fn web_app_is_served_with_spa_fallback_and_security_headers() {
    let h = Harness::new();
    let (s, headers, body) = raw_get(&h, "/").await;
    assert_eq!(s, StatusCode::OK);
    assert!(body.contains("Phonalys"));
    assert!(headers["content-security-policy"]
        .to_str()
        .unwrap()
        .contains("script-src 'self'"));

    let (s, headers, body) = raw_get(&h, "/assets/app.js").await;
    assert_eq!(s, StatusCode::OK);
    assert!(headers["content-type"]
        .to_str()
        .unwrap()
        .contains("javascript"));
    assert_eq!(body, "console.log(1)");

    // Route de l'interface : la page de l'application.
    let (s, _, body) = raw_get(&h, "/patients/123").await;
    assert_eq!(s, StatusCode::OK);
    assert!(body.contains("Phonalys"));

    // Route d'API inconnue : 404 JSON, jamais la page.
    let (s, _, body) = raw_get(&h, "/api/inexistant").await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    assert!(!body.contains("doctype"));

    // Pas de sortie du dossier web.
    let (s, _, body) = raw_get(&h, "/../cabinet.phonalys-vault").await;
    assert!(s == StatusCode::OK || s == StatusCode::NOT_FOUND || s == StatusCode::BAD_REQUEST);
    assert!(!body.contains("SQLite"));
}

#[tokio::test]
async fn setup_returns_a_server_side_qr_code() {
    let h = Harness::new();
    let (status, _, body) = h
        .call(
            "POST",
            "/api/setup",
            Some(json!({ "setup_token": h.setup_token, "passphrase": PASSPHRASE })),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    let svg = body["totp_qr_svg"].as_str().unwrap();
    assert!(svg.contains("<svg") && !svg.contains("<script"));
}

#[tokio::test]
async fn patient_can_be_updated_but_not_created_by_put() {
    let h = Harness::new();
    let (totp, _) = h.setup().await;
    let cookie = h.login(&totp).await;
    let (_, _, created) = h
        .call(
            "POST",
            "/api/patients",
            Some(json!({ "nom": "Durand" })),
            Some(&cookie),
        )
        .await;
    let id = created["id"].as_str().unwrap();
    let (s, _, _) = h
        .call(
            "PUT",
            &format!("/api/patients/{id}"),
            Some(json!({ "nom": "Dupont" })),
            Some(&cookie),
        )
        .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (_, _, p) = h
        .call("GET", &format!("/api/patients/{id}"), None, Some(&cookie))
        .await;
    assert_eq!(p["nom"], "Dupont");

    let unknown = "00000000-0000-4000-8000-000000000000";
    let (s, _, _) = h
        .call(
            "PUT",
            &format!("/api/patients/{unknown}"),
            Some(json!({ "nom": "X" })),
            Some(&cookie),
        )
        .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let (s, _, _) = h
        .call(
            "PUT",
            &format!("/api/patients/{id}"),
            Some(json!({ "nom": "X" })),
            None,
        )
        .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
}

// --- Contrôle de l'adresse (DNS rebinding) et HTTP/2 -------------------------------------

async fn send(h: &Harness, req: Request<Body>) -> StatusCode {
    app(h.state.clone()).oneshot(req).await.unwrap().status()
}

#[tokio::test]
async fn foreign_host_is_refused() {
    let h = Harness::new();
    for host in [
        "evil.example",
        "localhost:9999",
        "127.0.0.1:8443",
        "phonalys.lan:8443",
    ] {
        let req = Request::builder()
            .uri("/api/status")
            .header("host", host)
            .body(Body::empty())
            .unwrap();
        assert_eq!(
            send(&h, req).await,
            StatusCode::MISDIRECTED_REQUEST,
            "{host}"
        );
    }
    let req = Request::builder()
        .uri("/api/status")
        .header("host", "LOCALHOST:8443")
        .body(Body::empty())
        .unwrap();
    assert_eq!(send(&h, req).await, StatusCode::OK, "casse ignorée");
}

#[tokio::test]
async fn http2_requests_without_host_header_work() {
    // En HTTP/2, le navigateur n'envoie que :authority, porté par l'URI.
    let h = Harness::new();
    let req = Request::builder()
        .method("POST")
        .uri("https://localhost:8443/api/setup")
        .header("origin", ORIGIN)
        .header(CSRF_HEADER, "1")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({ "setup_token": h.setup_token, "passphrase": PASSPHRASE }).to_string(),
        ))
        .unwrap();
    assert_eq!(send(&h, req).await, StatusCode::CREATED);

    // Une origine étrangère reste refusée.
    let req = Request::builder()
        .method("POST")
        .uri("https://localhost:8443/api/login")
        .header("origin", "https://evil.example")
        .header(CSRF_HEADER, "1")
        .header("content-type", "application/json")
        .body(Body::from(json!({ "passphrase": PASSPHRASE }).to_string()))
        .unwrap();
    assert_eq!(send(&h, req).await, StatusCode::FORBIDDEN);

    // Ni Host ni :authority : refus.
    let req = Request::builder()
        .uri("/api/status")
        .body(Body::empty())
        .unwrap();
    assert_eq!(send(&h, req).await, StatusCode::MISDIRECTED_REQUEST);
}

// --- Politique de blocage : 4 essais, puis code de récupération -------------------------

#[tokio::test]
async fn four_attempts_then_blocked_and_recovery_code_unblocks() {
    let h = Harness::new();
    let (totp, codes) = h.setup().await;
    let wrong = || json!({ "passphrase": "phrase de passe erronée", "code": "000000" });

    for left in [3, 2, 1, 0] {
        let (s, _, body) = h.call("POST", "/api/login", Some(wrong()), None).await;
        assert_eq!(s, StatusCode::UNAUTHORIZED, "{body}");
        assert_eq!(body["remaining_attempts"], left, "{body}");
    }
    // Bloqué, même avec les bons identifiants…
    let (s, headers, body) = h
        .call(
            "POST",
            "/api/login",
            Some(json!({ "passphrase": PASSPHRASE, "code": totp.code_at(state::now()) })),
            None,
        )
        .await;
    assert_eq!(s, StatusCode::TOO_MANY_REQUESTS);
    assert!(body["error"]
        .as_str()
        .unwrap()
        .contains("code de récupération"));
    assert!(body["retry_after_secs"].as_i64().unwrap() > 800);
    assert!(headers.contains_key("retry-after"));

    // …mais le code de récupération débloque tout de suite.
    let (s, _, body) = h
        .call(
            "POST",
            "/api/login",
            Some(json!({ "passphrase": PASSPHRASE, "recovery_code": codes[0] })),
            None,
        )
        .await;
    assert_eq!(s, StatusCode::NO_CONTENT, "{body}");
    h.call("POST", "/api/lock", None, None).await;
    h.state.auto_lock(state::now() + 10 * 3600);

    // Le compteur est remis à zéro : la connexion quotidienne refonctionne.
    h.login_at(&totp, state::now() + 30).await;
}

#[tokio::test]
async fn recovery_failure_does_not_reveal_which_part_is_wrong() {
    let h = Harness::new();
    let (_, codes) = h.setup().await;
    let (s1, _, b1) = h
        .call(
            "POST",
            "/api/login",
            Some(json!({ "passphrase": "fausse phrase", "recovery_code": codes[0] })),
            None,
        )
        .await;
    let (s2, _, b2) = h
        .call(
            "POST",
            "/api/login",
            Some(json!({ "passphrase": PASSPHRASE, "recovery_code": "AAAA-BBBB-CCCC-DDDD" })),
            None,
        )
        .await;
    assert_eq!(
        (s1, s2),
        (StatusCode::UNAUTHORIZED, StatusCode::UNAUTHORIZED)
    );
    assert_eq!(b1, b2);
    // Le code n'a pas été consommé par l'essai avec la mauvaise phrase de passe.
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

// --- Connexion en un geste ------------------------------------------------------------------

impl Harness {
    /// Active la connexion en un geste pour une clé inscrite.
    async fn enable_quick(
        &self,
        cookie: &str,
        device: &mut SoftAuthenticator,
        passphrase: &str,
    ) -> StatusCode {
        let id = BASE64URL_NOPAD.encode(&device.credential_id);
        let (s, _, options) = self
            .call(
                "POST",
                "/api/passkeys/bind/start",
                Some(json!({ "credential_id": id, "purpose": "quick" })),
                Some(cookie),
            )
            .await;
        assert_eq!(s, StatusCode::OK, "{options}");
        let salt = prf_salt(&options).expect("sel PRF");
        let response = device.authenticate(RP_ID, ORIGIN, options["challenge"].as_str().unwrap());
        let (s, _, _) = self
            .call(
                "POST",
                "/api/passkeys/bind/finish",
                Some(json!({ "passphrase": passphrase, "response": response, "prf_output": device.prf_b64(&salt) })),
                Some(cookie),
            )
            .await;
        s
    }

    /// Connexion en un geste ; `prf_from` permet de simuler une autre clé.
    async fn quick_login(
        &self,
        device: &mut SoftAuthenticator,
        prf_from: Option<&SoftAuthenticator>,
    ) -> (StatusCode, HeaderMap, Value) {
        let (s, _, options) = self
            .call("POST", "/api/login/quick/start", None, None)
            .await;
        if s != StatusCode::OK {
            return (s, HeaderMap::new(), options);
        }
        let salt = prf_salt(&options).expect("sel PRF");
        let response = device.authenticate(RP_ID, ORIGIN, options["challenge"].as_str().unwrap());
        let prf = prf_from.unwrap_or(device).prf_b64(&salt);
        self.call(
            "POST",
            "/api/login/quick/finish",
            Some(json!({ "response": response, "prf_output": prf })),
            None,
        )
        .await
    }
}

#[tokio::test]
async fn quick_unlock_end_to_end() {
    let h = Harness::new();
    let (totp, _) = h.setup().await;
    let cookie = h.login(&totp).await;
    let mut device = h.register_passkey(&cookie, Algorithm::Es256).await;

    // Pas encore activée : refus clair, et l'état public le dit.
    let (_, _, st) = h.call("GET", "/api/status", None, None).await;
    assert_eq!(st["quick_unlock"], false);
    let (s, _, _) = h.call("POST", "/api/login/quick/start", None, None).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);

    // Activer exige la phrase de passe.
    assert_eq!(
        h.enable_quick(&cookie, &mut device, "mauvaise phrase de passe")
            .await,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        h.enable_quick(&cookie, &mut device, PASSPHRASE).await,
        StatusCode::OK
    );
    let (_, _, list) = h.call("GET", "/api/passkeys", None, Some(&cookie)).await;
    assert_eq!(list[0]["quick"], true);
    h.call("POST", "/api/lock", None, Some(&cookie)).await;

    let (_, _, st) = h.call("GET", "/api/status", None, None).await;
    assert_eq!(st["quick_unlock"], true);
    assert_eq!(st["step_up"], false);

    // Un seul geste : pas de phrase de passe.
    let (s, headers, body) = h.quick_login(&mut device, None).await;
    assert_eq!(s, StatusCode::NO_CONTENT, "{body}");
    let cookie = cookie_from(&headers);
    let (s, _, _) = h.call("GET", "/api/patients", None, Some(&cookie)).await;
    assert_eq!(s, StatusCode::OK);

    // Retirer le raccourci : la clé reste, mais le geste seul ne suffit plus.
    let id = list[0]["id"].as_str().unwrap().to_owned();
    let (s, _, _) = h
        .call(
            "DELETE",
            &format!("/api/passkeys/{id}/quick"),
            None,
            Some(&cookie),
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    h.call("POST", "/api/lock", None, Some(&cookie)).await;
    let (s, _, _) = h.call("POST", "/api/login/quick/start", None, None).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, _) = h.passkey_login(&mut device).await;
    assert_eq!(s, StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn quick_unlock_needs_a_live_signature_and_the_right_device() {
    let h = Harness::new();
    let (totp, _) = h.setup().await;
    let cookie = h.login(&totp).await;
    let mut device = h.register_passkey(&cookie, Algorithm::Es256).await;
    assert_eq!(
        h.enable_quick(&cookie, &mut device, PASSPHRASE).await,
        StatusCode::OK
    );
    h.call("POST", "/api/lock", None, Some(&cookie)).await;

    // Une sortie PRF d'un autre appareil n'ouvre rien.
    let other = SoftAuthenticator::new(Algorithm::Es256);
    let (s, _, body) = h.quick_login(&mut device, Some(&other)).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED, "{body}");
    assert_eq!(body["remaining_attempts"], 3);

    // Une bonne sortie PRF avec une signature sans biométrie est refusée aussi.
    device.user_verified = false;
    let (s, _, _) = h.quick_login(&mut device, None).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    assert!(!h.state.is_unlocked());
}

#[tokio::test]
async fn three_failures_then_two_verifications_are_required() {
    let h = Harness::new();
    let (totp, _) = h.setup().await;
    let cookie = h.login(&totp).await;
    let mut device = h.register_passkey(&cookie, Algorithm::Es256).await;
    assert_eq!(
        h.enable_quick(&cookie, &mut device, PASSPHRASE).await,
        StatusCode::OK
    );
    h.call("POST", "/api/lock", None, Some(&cookie)).await;

    let other = SoftAuthenticator::new(Algorithm::Es256);
    for _ in 0..2 {
        let (s, _, _) = h.quick_login(&mut device, Some(&other)).await;
        assert_eq!(s, StatusCode::UNAUTHORIZED);
    }
    let (s, _, body) = h.quick_login(&mut device, Some(&other)).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    assert_eq!(body["step_up"], true, "{body}");

    let (_, _, st) = h.call("GET", "/api/status", None, None).await;
    assert_eq!(st["step_up"], true);
    // Même avec le bon appareil, le geste seul est refusé…
    let (s, _, body) = h.quick_login(&mut device, None).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    assert_eq!(body["step_up"], true);

    // …phrase de passe + clé biométrique : accepté, et le raccourci revient.
    let (s, headers) = h.passkey_login(&mut device).await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    h.call("POST", "/api/lock", None, Some(&cookie_from(&headers)))
        .await;
    let (_, _, st) = h.call("GET", "/api/status", None, None).await;
    assert_eq!(st["step_up"], false);
    let (s, _, _) = h.quick_login(&mut device, None).await;
    assert_eq!(s, StatusCode::NO_CONTENT);
}

// --- Dossier clinique, réglages, audit -----------------------------------------------------

#[tokio::test]
async fn clinical_records_are_typed_linked_and_erased_with_the_patient() {
    let h = Harness::new();
    let (totp, _) = h.setup().await;
    let cookie = h.login(&totp).await;
    let c = Some(cookie.as_str());

    let (_, _, p) = h
        .call("POST", "/api/patients", Some(json!({ "nom": "Test" })), c)
        .await;
    let pid = p["id"].as_str().unwrap().to_owned();
    let (_, _, p2) = h
        .call("POST", "/api/patients", Some(json!({ "nom": "Autre" })), c)
        .await;
    let pid2 = p2["id"].as_str().unwrap().to_owned();

    // Type inconnu : refusé.
    let (s, _, _) = h
        .call("POST", "/api/records/n_importe_quoi", Some(json!({})), c)
        .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    // Patient inconnu : refusé.
    let fake = uuid::Uuid::new_v4().to_string();
    let (s, _, _) = h
        .call(
            "POST",
            "/api/records/seance",
            Some(json!({ "patient_id": fake })),
            c,
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);

    let (s, _, seance) = h
        .call(
            "POST",
            "/api/records/seance",
            Some(json!({ "patient_id": pid, "date": "2026-10-24", "notes": "lecture" })),
            c,
        )
        .await;
    assert_eq!(s, StatusCode::CREATED);
    let sid = seance["id"].as_str().unwrap().to_owned();
    h.call(
        "POST",
        "/api/records/bilan",
        Some(json!({ "patient_id": pid, "type": "initial" })),
        c,
    )
    .await;
    h.call(
        "POST",
        "/api/records/seance",
        Some(json!({ "patient_id": pid2 })),
        c,
    )
    .await;

    let (_, _, mine) = h
        .call(
            "GET",
            &format!("/api/records/seance?patient_id={pid}"),
            None,
            c,
        )
        .await;
    assert_eq!(mine.as_array().unwrap().len(), 1);
    assert_eq!(mine[0]["notes"], "lecture");
    assert_eq!(mine[0]["id"], sid.as_str());

    let (s, _, _) = h
        .call(
            "PUT",
            &format!("/api/records/seance/{sid}"),
            Some(json!({ "patient_id": pid, "notes": "lecture + rimes" })),
            c,
        )
        .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (_, _, one) = h
        .call("GET", &format!("/api/records/seance/{sid}"), None, c)
        .await;
    assert_eq!(one["notes"], "lecture + rimes");
    let (s, _, _) = h
        .call(
            "PUT",
            &format!("/api/records/seance/{fake}"),
            Some(json!({})),
            c,
        )
        .await;
    assert_eq!(s, StatusCode::NOT_FOUND, "PUT ne crée pas");

    // Effacer le patient efface son dossier clinique, pas celui des autres.
    let (s, _, _) = h
        .call("DELETE", &format!("/api/patients/{pid}"), None, c)
        .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (_, _, all) = h.call("GET", "/api/records/seance", None, c).await;
    assert_eq!(all.as_array().unwrap().len(), 1);
    assert_eq!(all[0]["patient_id"], pid2.as_str());
    let (_, _, bilans) = h.call("GET", "/api/records/bilan", None, c).await;
    assert_eq!(bilans.as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn idle_timeout_setting_is_validated_and_applied() {
    let h = Harness::new();
    let (totp, _) = h.setup().await;
    let cookie = h.login(&totp).await;
    let c = Some(cookie.as_str());

    let (_, _, s0) = h.call("GET", "/api/settings", None, c).await;
    assert_eq!(s0["idle_timeout_minutes"], 15);
    let (s, _, _) = h
        .call(
            "PUT",
            "/api/settings",
            Some(json!({ "idle_timeout_minutes": 600 })),
            c,
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, _, _) = h
        .call(
            "PUT",
            "/api/settings",
            Some(json!({ "idle_timeout_minutes": 30, "practitioner": { "nom": "Cabinet" } })),
            c,
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    let (_, _, info) = h.call("GET", "/api/session", None, c).await;
    assert_eq!(info["idle_timeout_secs"], 1800);

    h.state.auto_lock(state::now() + 20 * 60);
    assert!(h.state.is_unlocked(), "30 min choisies");
    h.state.auto_lock(state::now() + 31 * 60);
    assert!(!h.state.is_unlocked());
}

#[tokio::test]
async fn audit_log_is_readable_and_pseudonymous() {
    let h = Harness::new();
    let (totp, _) = h.setup().await;
    let cookie = h.login(&totp).await;
    let c = Some(cookie.as_str());
    h.call(
        "POST",
        "/api/patients",
        Some(json!({ "nom": "Dupont", "prenom": "Léa" })),
        c,
    )
    .await;
    let (s, _, page) = h.call("GET", "/api/audit", None, c).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(page["verified"], true);
    assert_eq!(page["entries"][0]["action"], "audit_viewed");
    assert!(
        !page.to_string().contains("Dupont"),
        "aucun nom dans l'audit"
    );
    let (s, _, _) = h.call("GET", "/api/audit", None, None).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
}

// --- Sauvegardes -----------------------------------------------------------------------------

async fn raw(
    h: &Harness,
    method: &str,
    uri: &str,
    headers: &[(&str, &str)],
    body: Vec<u8>,
    cookie: Option<&str>,
) -> (StatusCode, Vec<u8>) {
    let mut req = Request::builder()
        .method(method)
        .uri(uri)
        .header("host", "localhost:8443")
        .header(CSRF_HEADER, "1");
    for (k, v) in headers {
        req = req.header(*k, *v);
    }
    if let Some(c) = cookie {
        req = req.header("cookie", c);
    }
    let res = app(h.state.clone())
        .oneshot(req.body(Body::from(body)).unwrap())
        .await
        .unwrap();
    let status = res.status();
    (
        status,
        to_bytes(res.into_body(), usize::MAX)
            .await
            .unwrap()
            .to_vec(),
    )
}

#[tokio::test]
async fn backup_verify_download_and_restore_on_a_new_install() {
    let h = Harness::new();
    let (totp, _) = h.setup().await;
    let cookie = h.login(&totp).await;
    let c = Some(cookie.as_str());
    let (_, _, p) = h
        .call(
            "POST",
            "/api/patients",
            Some(json!({ "nom": "Sauvegarde" })),
            c,
        )
        .await;
    let pid = p["id"].as_str().unwrap().to_owned();
    h.call(
        "POST",
        "/api/records/seance",
        Some(json!({ "patient_id": pid, "date": "2026-10-25" })),
        c,
    )
    .await;

    let (s, _, entry) = h.call("POST", "/api/backups", None, c).await;
    assert_eq!(s, StatusCode::OK, "{entry}");
    assert_eq!(entry["verified"], true);
    assert_eq!(entry["records"], 2);
    let name = entry["name"].as_str().unwrap().to_owned();

    let (_, _, st) = h.call("GET", "/api/backups", None, c).await;
    assert_eq!(st["files"][0]["name"], name.as_str());
    assert_eq!(st["auto"], true);
    let (s, _, _) = h
        .call("POST", &format!("/api/backups/{name}/verify"), None, c)
        .await;
    assert_eq!(s, StatusCode::OK);
    // Aucun autre fichier ne peut être désigné.
    let (s, _, _) = h
        .call("GET", "/api/backups/..%2Fcabinet.phonalys-vault", None, c)
        .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let (s, _) = raw(
        &h,
        "GET",
        &format!("/api/backups/{name}"),
        &[],
        vec![],
        None,
    )
    .await;
    assert_eq!(
        s,
        StatusCode::UNAUTHORIZED,
        "téléchargement réservé à une session"
    );
    let (s, file) = raw(&h, "GET", &format!("/api/backups/{name}"), &[], vec![], c).await;
    assert_eq!(s, StatusCode::OK);
    assert!(
        !String::from_utf8_lossy(&file).contains("Sauvegarde"),
        "la sauvegarde est chiffrée"
    );

    // Nouvelle installation : restauration avec le jeton d'installation.
    let fresh = Harness::new();
    let (s, _) = raw(
        &fresh,
        "POST",
        "/api/restore",
        &[("x-phonalys-setup-token", "FAUX")],
        file.clone(),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let token = fresh.setup_token.clone();
    let (s, _) = raw(
        &fresh,
        "POST",
        "/api/restore",
        &[("x-phonalys-setup-token", &token)],
        b"pas un coffre".to_vec(),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, _) = raw(
        &fresh,
        "POST",
        "/api/restore",
        &[("x-phonalys-setup-token", &token)],
        file.clone(),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (s, _) = raw(
        &fresh,
        "POST",
        "/api/restore",
        &[("x-phonalys-setup-token", &token)],
        file,
        None,
    )
    .await;
    assert_eq!(s, StatusCode::CONFLICT, "une seule restauration");

    let cookie = fresh.login_at(&totp, state::now() + 30).await;
    let (_, _, list) = fresh
        .call("GET", "/api/patients", None, Some(&cookie))
        .await;
    assert_eq!(list[0]["nom"], "Sauvegarde");
    let (_, _, seances) = fresh
        .call("GET", "/api/records/seance", None, Some(&cookie))
        .await;
    assert_eq!(seances.as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn extra_backup_dir_and_daily_automatic_backup() {
    let h = Harness::new();
    let (totp, _) = h.setup().await;
    let cookie = h.login(&totp).await;
    let c = Some(cookie.as_str());

    let (s, _, _) = h
        .call(
            "PUT",
            "/api/settings",
            Some(json!({ "idle_timeout_minutes": 15, "backup_extra_dir": "pas/absolu" })),
            c,
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let usb = tempfile::tempdir().unwrap();
    let (s, _, _) = h
        .call("PUT", "/api/settings", Some(json!({ "idle_timeout_minutes": 15, "backup_extra_dir": usb.path().to_str().unwrap() })), c)
        .await;
    assert_eq!(s, StatusCode::OK);

    let t0 = state::now();
    backup::auto_backup(&h.state, t0);
    backup::auto_backup(&h.state, t0 + 3600);
    let (_, _, st) = h.call("GET", "/api/backups", None, c).await;
    assert_eq!(
        st["files"].as_array().unwrap().len(),
        1,
        "une seule sauvegarde par 24 h"
    );
    assert_eq!(st["last"]["copied"], true);
    assert_eq!(std::fs::read_dir(usb.path()).unwrap().count(), 1);

    backup::auto_backup(&h.state, t0 + backup::AUTO_INTERVAL_SECS + 1);
    let (_, _, st) = h.call("GET", "/api/backups", None, c).await;
    assert_eq!(st["files"].as_array().unwrap().len(), 2);

    // Coffre verrouillé : rien n'est fait.
    h.call("POST", "/api/lock", None, c).await;
    backup::auto_backup(&h.state, t0 + 3 * backup::AUTO_INTERVAL_SECS);
    assert_eq!(std::fs::read_dir(usb.path()).unwrap().count(), 2);
}

async fn get_full(h: &Harness, uri: &str, cookie: &str) -> (StatusCode, HeaderMap, Vec<u8>) {
    let req = Request::builder()
        .uri(uri)
        .header("host", "localhost:8443")
        .header("cookie", cookie)
        .body(Body::empty())
        .unwrap();
    let res = app(h.state.clone()).oneshot(req).await.unwrap();
    let (status, headers) = (res.status(), res.headers().clone());
    let bytes = to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap()
        .to_vec();
    (status, headers, bytes)
}

#[tokio::test]
async fn documents_are_encrypted_typed_and_need_a_fresh_reauthentication() {
    let h = Harness::new();
    let (totp, _) = h.setup().await;
    // Trois pas TOTP consécutifs : connexion, réauthentification, second appareil.
    let now = state::now() - 30;
    let cookie = h.login_at(&totp, now).await;
    let c = Some(cookie.as_str());
    let (_, _, p) = h
        .call("POST", "/api/patients", Some(json!({ "nom": "Docs" })), c)
        .await;
    let pid = p["id"].as_str().unwrap().to_owned();

    let pdf = b"%PDF-1.7\nCompte rendu ORL".to_vec();
    let (s, body) = raw(
        &h,
        "POST",
        &format!("/api/documents?name=Compte%20rendu%20ORL.pdf&category=Courrier&patient_id={pid}"),
        &[("content-type", "application/octet-stream")],
        pdf.clone(),
        c,
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "{}", String::from_utf8_lossy(&body));
    let doc: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(doc["mime"], "application/pdf");
    assert_eq!(doc["size"], pdf.len());
    let id = doc["id"].as_str().unwrap().to_owned();

    // Un fichier HTML déguisé en PDF, ou un exécutable, est refusé.
    for (name, bytes) in [
        ("faux.pdf", b"<html><script>".to_vec()),
        ("a.exe", b"MZ\x90\0".to_vec()),
    ] {
        let (s, _) = raw(
            &h,
            "POST",
            &format!("/api/documents?name={name}"),
            &[],
            bytes,
            c,
        )
        .await;
        assert_eq!(s, StatusCode::BAD_REQUEST);
    }
    // Patient inexistant : refusé.
    let (s, _) = raw(
        &h,
        "POST",
        "/api/documents?name=a.pdf&patient_id=00000000-0000-4000-8000-000000000000",
        &[],
        pdf.clone(),
        c,
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);

    let (_, _, list) = h
        .call("GET", &format!("/api/documents?patient_id={pid}"), None, c)
        .await;
    assert_eq!(list.as_array().unwrap().len(), 1);
    assert_eq!(list[0]["name"], "Compte rendu ORL.pdf");

    // Le contenu n'apparaît pas en clair dans le fichier du coffre.
    let raw_vault = std::fs::read(&h.state.config.vault_path).unwrap();
    assert!(!raw_vault
        .windows(10)
        .any(|w| w == b"rendu ORL\0" || w == b"Compte ren"));

    // Session ouverte mais pas de réauthentification récente : refus, sans fermer la session.
    let content = format!("/api/documents/{id}/content");
    let (s, _, body) = get_full(&h, &content, &cookie).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    assert_eq!(
        serde_json::from_slice::<Value>(&body).unwrap()["reauth"],
        true
    );
    let (s, _, _) = h
        .call("DELETE", &format!("/api/documents/{id}"), None, c)
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN);

    // Mauvais code : refus compté, sans 401 (l'interface resterait connectée).
    let (s, _, body) = h
        .call(
            "POST",
            "/api/reauth",
            Some(json!({ "passphrase": PASSPHRASE, "code": "000000" })),
            c,
        )
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["remaining_attempts"], 3);
    let (s, _, body) = h
        .call(
            "POST",
            "/api/reauth",
            Some(json!({ "passphrase": "mauvaise phrase", "code": totp.code_at(now + 30) })),
            c,
        )
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN, "{body}");

    let (s, _, body) = h
        .call(
            "POST",
            "/api/reauth",
            Some(json!({ "passphrase": PASSPHRASE, "code": totp.code_at(now + 30) })),
            c,
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{body}");
    let (_, _, info) = h.call("GET", "/api/session", None, c).await;
    assert_eq!(info["elevated"], true);

    let (s, headers, bytes) = get_full(&h, &content, &cookie).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(bytes, pdf);
    assert_eq!(headers["content-type"], "application/pdf");
    assert!(headers["content-disposition"]
        .to_str()
        .unwrap()
        .starts_with("inline;"));
    let csp = headers["content-security-policy"].to_str().unwrap();
    assert!(csp.contains("default-src 'none'"), "{csp}");
    assert!(csp.contains("frame-ancestors 'self'"), "{csp}");
    assert_eq!(headers["x-frame-options"], "SAMEORIGIN");
    assert_eq!(headers["x-content-type-options"], "nosniff");
    assert_eq!(headers["cache-control"], "no-store");
    let (_, headers, _) = get_full(&h, &format!("{content}?download=1"), &cookie).await;
    assert!(headers["content-disposition"]
        .to_str()
        .unwrap()
        .starts_with("attachment;"));
    // Les autres réponses gardent la CSP de l'application.
    let (_, headers, _) = get_full(&h, "/api/documents", &cookie).await;
    assert!(headers["content-security-policy"]
        .to_str()
        .unwrap()
        .contains("script-src 'self'"));
    assert_eq!(headers["x-frame-options"], "DENY");

    // L'élévation est propre à la session : un autre appareil doit se réauthentifier.
    let other = h.login_at(&totp, now + 60).await;
    let (s, _, _) = get_full(&h, &content, &other).await;
    assert_eq!(s, StatusCode::FORBIDDEN);

    // Effacer le patient efface ses documents.
    let (s, _, _) = h
        .call("DELETE", &format!("/api/patients/{pid}"), None, c)
        .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (_, _, list) = h.call("GET", "/api/documents", None, c).await;
    assert_eq!(list, json!([]));
    let (s, _, _) = get_full(&h, &content, &cookie).await;
    assert_eq!(s, StatusCode::NOT_FOUND);

    let (_, _, audit) = h.call("GET", "/api/audit", None, c).await;
    let actions: Vec<&str> = audit["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["action"].as_str().unwrap())
        .collect();
    for a in [
        "document_imported",
        "document_opened",
        "reauthenticated",
        "record_erased",
    ] {
        assert!(actions.contains(&a), "{a} absent de {actions:?}");
    }
}

#[tokio::test]
async fn patient_export_contains_the_whole_file_and_needs_a_reauthentication() {
    let h = Harness::new();
    let (totp, _) = h.setup().await;
    let now = state::now() - 30;
    let cookie = h.login_at(&totp, now).await;
    let c = Some(cookie.as_str());
    let (_, _, p) = h
        .call(
            "POST",
            "/api/patients",
            Some(json!({ "nom": "Export", "prenom": "Léa" })),
            c,
        )
        .await;
    let pid = p["id"].as_str().unwrap().to_owned();
    let (_, _, other) = h
        .call("POST", "/api/patients", Some(json!({ "nom": "Autre" })), c)
        .await;
    for (kind, pid) in [
        ("seance", &pid),
        ("bilan", &pid),
        ("seance", &other["id"].as_str().unwrap().to_owned()),
    ] {
        let (s, _, _) = h
            .call(
                "POST",
                &format!("/api/records/{kind}"),
                Some(json!({ "patient_id": pid, "notes": "texte" })),
                c,
            )
            .await;
        assert_eq!(s, StatusCode::CREATED);
    }
    let pdf = b"%PDF-1.7\nCourrier".to_vec();
    let (s, _) = raw(
        &h,
        "POST",
        &format!("/api/documents?name=Courrier.pdf&patient_id={pid}"),
        &[],
        pdf.clone(),
        c,
    )
    .await;
    assert_eq!(s, StatusCode::CREATED);

    // Le registre des violations est propre au cabinet et n'entre pas dans l'export.
    let (s, _, _) = h
        .call(
            "POST",
            "/api/records/violation",
            Some(json!({ "date_constat": "2026-10-20T10:00", "description": "perte" })),
            c,
        )
        .await;
    assert_eq!(s, StatusCode::CREATED);

    let url = format!("/api/patients/{pid}/export");
    let (s, _, _) = get_full(&h, &url, &cookie).await;
    assert_eq!(s, StatusCode::FORBIDDEN);

    let (s, _, _) = h
        .call(
            "POST",
            "/api/reauth",
            Some(json!({ "passphrase": PASSPHRASE, "code": totp.code_at(now + 30) })),
            c,
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    let (s, headers, body) = get_full(&h, &url, &cookie).await;
    assert_eq!(s, StatusCode::OK);
    let disposition = headers["content-disposition"].to_str().unwrap();
    assert!(
        disposition.starts_with("attachment; filename=\"dossier-patient-"),
        "{disposition}"
    );
    assert!(
        !disposition.contains("Export"),
        "aucune identité dans le nom du fichier"
    );
    assert_eq!(headers["cache-control"], "no-store");

    let doc: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(doc["format"], "phonalys-dossier-patient");
    assert_eq!(doc["patient"]["prenom"], "Léa");
    assert_eq!(doc["records"]["seance"].as_array().unwrap().len(), 1);
    assert_eq!(doc["records"]["bilan"].as_array().unwrap().len(), 1);
    assert_eq!(doc["records"]["objectif"], json!([]));
    assert!(doc["records"].get("violation").is_none());
    let docs = doc["documents"].as_array().unwrap();
    assert_eq!(docs.len(), 1);
    assert_eq!(
        data_encoding::BASE64
            .decode(docs[0]["content_base64"].as_str().unwrap().as_bytes())
            .unwrap(),
        pdf
    );

    let (s, _, _) = get_full(
        &h,
        "/api/patients/00000000-0000-4000-8000-000000000000/export",
        &cookie,
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND);

    let (_, _, audit) = h.call("GET", "/api/audit", None, c).await;
    assert!(audit["entries"]
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["action"] == "export" && e["target"] == format!("patient/{pid}")));
}

#[tokio::test]
async fn reauthentication_with_a_biometric_key() {
    let h = Harness::new();
    let (totp, _) = h.setup().await;
    let cookie = h.login(&totp).await;
    let c = Some(cookie.as_str());
    let mut device = h.register_passkey(&cookie, Algorithm::Es256).await;

    let (s, _, options) = h.call("POST", "/api/reauth/passkey/start", None, c).await;
    assert_eq!(s, StatusCode::OK, "{options}");
    assert_eq!(options["userVerification"], "required");
    // Signature d'un autre site : refusée, la session reste ouverte.
    let bad = device.authenticate(
        RP_ID,
        "https://phishing.example",
        options["challenge"].as_str().unwrap(),
    );
    let (s, _, _) = h
        .call(
            "POST",
            "/api/reauth/passkey/finish",
            Some(json!({ "response": bad })),
            c,
        )
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN);

    let (_, _, options) = h.call("POST", "/api/reauth/passkey/start", None, c).await;
    let good = device.authenticate(RP_ID, ORIGIN, options["challenge"].as_str().unwrap());
    // Le défi d'une session ne sert pas à une autre.
    let other = h.login_at(&totp, state::now() + 30).await;
    let (s, _, _) = h
        .call(
            "POST",
            "/api/reauth/passkey/finish",
            Some(json!({ "response": good })),
            Some(other.as_str()),
        )
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN);

    let (_, _, options) = h.call("POST", "/api/reauth/passkey/start", None, c).await;
    let good = device.authenticate(RP_ID, ORIGIN, options["challenge"].as_str().unwrap());
    let (s, _, body) = h
        .call(
            "POST",
            "/api/reauth/passkey/finish",
            Some(json!({ "response": good })),
            c,
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{body}");
    let (_, _, info) = h.call("GET", "/api/session", None, c).await;
    assert_eq!(info["elevated"], true);
    let (_, _, info) = h
        .call("GET", "/api/session", None, Some(other.as_str()))
        .await;
    assert_eq!(info["elevated"], false);
}

#[tokio::test]
async fn background_lock_setting_and_wake_from_sleep() {
    let h = Harness::new();
    let (totp, _) = h.setup().await;
    let cookie = h.login(&totp).await;
    let c = Some(cookie.as_str());
    let (_, _, info) = h.call("GET", "/api/session", None, c).await;
    assert_eq!(info["hidden_lock_secs"], 300);
    let (_, _, mut s) = h.call("GET", "/api/settings", None, c).await;
    s["hidden_lock_minutes"] = json!(7);
    let (status, _, _) = h.call("PUT", "/api/settings", Some(s.clone()), c).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    s["hidden_lock_minutes"] = json!(1);
    let (status, _, _) = h.call("PUT", "/api/settings", Some(s), c).await;
    assert_eq!(status, StatusCode::OK);
    let (_, _, info) = h.call("GET", "/api/session", None, c).await;
    assert_eq!(info["hidden_lock_secs"], 60);

    // Un tic de 15 s est normal ; deux minutes sans tic signalent une veille.
    assert!(!resumed_from_sleep(1000, 1015));
    assert!(!resumed_from_sleep(1000, 1060));
    assert!(resumed_from_sleep(1000, 1000 + SUSPEND_GAP_SECS + 1));
    assert!(
        !resumed_from_sleep(1000, 900),
        "horloge reculée : pas de faux réveil"
    );

    h.state
        .lock_all(phonalys_core::audit::AuditAction::AutoLock);
    assert!(!h.state.is_unlocked());
    let (status, _, _) = h.call("GET", "/api/session", None, c).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

/// Faux moteur Ollama : répond `answer` à chaque requête et transmet le corps reçu.
async fn fake_ollama(
    answer: &'static str,
) -> (String, tokio::sync::mpsc::UnboundedReceiver<String>) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    tokio::spawn(async move {
        while let Ok((mut s, _)) = listener.accept().await {
            let mut buf = Vec::new();
            let mut chunk = [0u8; 8192];
            // Lit l'en-tête puis le corps annoncé par Content-Length.
            loop {
                let n = s.read(&mut chunk).await.unwrap();
                buf.extend_from_slice(&chunk[..n]);
                let text = String::from_utf8_lossy(&buf).to_string();
                if let Some(end) = text.find("\r\n\r\n") {
                    let len = text[..end]
                        .lines()
                        .find_map(|l| l.strip_prefix("Content-Length: "))
                        .and_then(|v| v.trim().parse::<usize>().ok())
                        .unwrap_or(0);
                    if buf.len() >= end + 4 + len || n == 0 {
                        let _ = tx.send(text[end + 4..].to_owned());
                        break;
                    }
                }
                if n == 0 {
                    break;
                }
            }
            let body = if String::from_utf8_lossy(&buf).starts_with("GET /api/tags") {
                r#"{"models":[{"name":"mistral:7b"}]}"#.to_owned()
            } else {
                json!({ "message": { "role": "assistant", "content": answer } }).to_string()
            };
            let res = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{body}",
                body.len()
            );
            let _ = s.write_all(res.as_bytes()).await;
            let _ = s.shutdown().await;
        }
    });
    (format!("http://127.0.0.1:{port}"), rx)
}

#[tokio::test]
async fn local_ai_drafts_a_pseudonymized_report() {
    let h = Harness::new();
    let (totp, _) = h.setup().await;
    let cookie = h.login(&totp).await;
    let c = Some(cookie.as_str());

    // Une IA en ligne ou une adresse publique ne peut pas être enregistrée.
    for endpoint in [
        "https://api.openai.com/v1",
        "http://8.8.8.8:11434",
        "https://192.168.1.5:443",
    ] {
        let (s, _, body) = h
            .call(
                "PUT",
                "/api/ai/settings",
                Some(json!({ "enabled": true, "endpoint": endpoint, "model": "x" })),
                c,
            )
            .await;
        assert_eq!(s, StatusCode::BAD_REQUEST, "{endpoint} : {body}");
    }

    let (endpoint, mut seen) = fake_ollama(
        "### CONCLUSION\n[PRENOM_1] présente des difficultés en répétition de mots (z = -1,80).\n### PROJET\nDeux séances par semaine pour [PRENOM_1].",
    )
    .await;
    let (s, _, body) = h
        .call("PUT", "/api/ai/settings", Some(json!({ "enabled": true, "endpoint": endpoint, "model": "mistral:7b", "style": "Phrases courtes." })), c)
        .await;
    assert_eq!(s, StatusCode::OK, "{body}");
    let (s, _, models) = h.call("POST", "/api/ai/models", None, c).await;
    assert_eq!(s, StatusCode::OK, "{models}");
    assert_eq!(models["models"][0], "mistral:7b");
    seen.recv().await.unwrap();

    let (_, _, p) = h
        .call("POST", "/api/patients", Some(json!({ "nom": "Dupont", "prenom": "Léa", "naissance": "2018-03-15", "ecole": "École Saint-Joseph" })), c)
        .await;
    let pid = p["id"].as_str().unwrap().to_owned();
    let bilan = json!({
        "patient_id": pid, "date": "2026-10-25", "type": "initial",
        "observations": "Léa Dupont coopère bien.",
        "resultats": [{ "epreuve": "Répétition de mots", "mesure": "z", "valeur": "-1.8" }],
    });
    let (s, _, draft) = h
        .call("POST", "/api/ai/draft", Some(json!({ "patient_id": pid, "bilan": bilan, "instructions": "Insister sur l'école de Léa." })), c)
        .await;
    assert_eq!(s, StatusCode::OK, "{draft}");
    assert_eq!(
        draft["conclusion"],
        "Léa présente des difficultés en répétition de mots (z = -1,80)."
    );
    assert_eq!(draft["projet"], "Deux séances par semaine pour Léa.");

    let sent = seen.recv().await.unwrap();
    assert!(sent.contains("\"model\":\"mistral:7b\""), "{sent}");
    assert!(sent.contains("Phrases courtes."));
    assert!(sent.contains("8 ans 7 mois"));
    for secret in [
        "Dupont",
        "Léa",
        "L\\u00e9a",
        "Saint-Joseph",
        "2018-03-15",
        &pid,
    ] {
        assert!(!sent.contains(secret), "{secret} envoyé au modèle");
    }

    let (_, _, audit) = h.call("GET", "/api/audit", None, c).await;
    assert!(audit["entries"]
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["action"] == "ai_request"));
}

#[tokio::test]
async fn pairing_serves_the_local_authority_without_login() {
    let dir = tempfile::tempdir().unwrap();
    let material = tls::load_or_create(dir.path(), &["phonalys.lan".into()]).unwrap();
    let h = Harness::new();
    let mut config = (*h.state.config).clone();
    config.authority = Some(material.ca.clone());
    let (state, _) = AppState::new(config);
    let h2 = Harness { state, ..h };

    let (s, _, info) = h2.call("GET", "/api/pairing/info", None, None).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(info["available"], true);
    assert_eq!(info["ca_fingerprint"], material.ca.fingerprint.as_str());

    let (s, headers, body) = get_full(&h2, "/api/pairing/ca.crt", "").await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(headers["content-type"], "application/x-x509-ca-cert");
    assert!(String::from_utf8(body)
        .unwrap()
        .starts_with("-----BEGIN CERTIFICATE-----\n"));

    let (s, headers, body) = get_full(&h2, "/api/pairing/phonalys.mobileconfig", "").await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(headers["content-type"], "application/x-apple-aspen-config");
    let profile = String::from_utf8(body).unwrap();
    assert!(profile.contains("com.apple.security.root"));
    assert!(profile.contains(&data_encoding::BASE64.encode(&material.ca.der)));

    // Le QR code exige une session ; sans autorité (bureau), tout répond 404.
    let (s, _, _) = h2.call("GET", "/api/pairing/qr", None, None).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    let plain = Harness::new();
    let (_, _, info) = plain.call("GET", "/api/pairing/info", None, None).await;
    assert_eq!(info["available"], false);
    let (s, _, _) = get_full(&plain, "/api/pairing/ca.crt", "").await;
    assert_eq!(s, StatusCode::NOT_FOUND);
}
