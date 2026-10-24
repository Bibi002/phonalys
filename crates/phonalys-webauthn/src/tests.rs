#![allow(clippy::unwrap_used)]

use super::testing::{Algorithm, SoftAuthenticator};
use super::*;
use rand_core::{OsRng, RngCore};

const RP_ID: &str = "phonalys.lan";
const ORIGIN: &str = "https://phonalys.lan:8443";

fn rp() -> RelyingParty {
    RelyingParty {
        id: RP_ID.into(),
        name: "Phonalys".into(),
        origins: vec![ORIGIN.into()],
    }
}

fn challenge() -> Challenge {
    let mut c = [0u8; 32];
    OsRng.fill_bytes(&mut c);
    Challenge::from_random(c)
}

fn registered(alg: Algorithm) -> (SoftAuthenticator, Vec<StoredCredential>) {
    let auth = SoftAuthenticator::new(alg);
    let c = challenge();
    let cred = finish_registration(
        &rp(),
        &c,
        &auth.register(RP_ID, ORIGIN, &c.to_base64url()),
        "Windows Hello",
    )
    .unwrap();
    (auth, vec![cred])
}

#[test]
fn full_ceremony_for_both_algorithms() {
    for alg in [Algorithm::Es256, Algorithm::Ed25519] {
        let (mut auth, mut creds) = registered(alg);
        assert_eq!(creds[0].label, "Windows Hello");
        for _ in 0..3 {
            let c = challenge();
            let resp = auth.authenticate(RP_ID, ORIGIN, &c.to_base64url());
            assert_eq!(
                finish_authentication(&rp(), &c, &resp, &mut creds),
                Ok(0),
                "{alg:?}"
            );
        }
        assert_eq!(creds[0].sign_count, 3);
    }
}

#[test]
fn credential_survives_json_storage() {
    let (_, creds) = registered(Algorithm::Es256);
    let json = serde_json::to_string(&creds).unwrap();
    assert_eq!(
        serde_json::from_str::<Vec<StoredCredential>>(&json).unwrap(),
        creds
    );
}

#[test]
fn user_verification_is_mandatory() {
    let mut auth = SoftAuthenticator::new(Algorithm::Es256);
    auth.user_verified = false;
    let c = challenge();
    assert_eq!(
        finish_registration(
            &rp(),
            &c,
            &auth.register(RP_ID, ORIGIN, &c.to_base64url()),
            "x"
        ),
        Err(WebAuthnError::UserNotVerified)
    );

    let (mut auth, mut creds) = registered(Algorithm::Es256);
    auth.user_verified = false;
    let c = challenge();
    let resp = auth.authenticate(RP_ID, ORIGIN, &c.to_base64url());
    assert_eq!(
        finish_authentication(&rp(), &c, &resp, &mut creds),
        Err(WebAuthnError::UserNotVerified)
    );
}

#[test]
fn phishing_origin_and_rp_id_are_rejected() {
    let (mut auth, mut creds) = registered(Algorithm::Es256);
    let c = challenge();
    let resp = auth.authenticate(RP_ID, "https://phonalys-login.example", &c.to_base64url());
    assert!(matches!(
        finish_authentication(&rp(), &c, &resp, &mut creds),
        Err(WebAuthnError::Origin(_))
    ));

    let c = challenge();
    let resp = auth.authenticate("autre.example", ORIGIN, &c.to_base64url());
    assert_eq!(
        finish_authentication(&rp(), &c, &resp, &mut creds),
        Err(WebAuthnError::RpId)
    );
}

#[test]
fn wrong_or_replayed_challenge_is_rejected() {
    let (mut auth, mut creds) = registered(Algorithm::Ed25519);
    let c = challenge();
    let resp = auth.authenticate(RP_ID, ORIGIN, &c.to_base64url());
    assert_eq!(
        finish_authentication(&rp(), &challenge(), &resp, &mut creds),
        Err(WebAuthnError::Challenge)
    );
}

#[test]
fn tampered_signature_or_data_is_rejected() {
    let (mut auth, mut creds) = registered(Algorithm::Es256);
    let c = challenge();
    let mut resp = auth.authenticate(RP_ID, ORIGIN, &c.to_base64url());
    let mut data = URL_SAFE_NO_PAD.decode(&resp.authenticator_data).unwrap();
    data[36] ^= 0x01; // modifie le compteur après signature
    resp.authenticator_data = URL_SAFE_NO_PAD.encode(data);
    assert_eq!(
        finish_authentication(&rp(), &c, &resp, &mut creds),
        Err(WebAuthnError::Signature)
    );
}

#[test]
fn signature_from_another_key_is_rejected() {
    let (_, mut creds) = registered(Algorithm::Es256);
    let mut impostor = SoftAuthenticator::new(Algorithm::Es256);
    impostor.credential_id = URL_SAFE_NO_PAD.decode(&creds[0].id).unwrap();
    let c = challenge();
    let resp = impostor.authenticate(RP_ID, ORIGIN, &c.to_base64url());
    assert_eq!(
        finish_authentication(&rp(), &c, &resp, &mut creds),
        Err(WebAuthnError::Signature)
    );
}

#[test]
fn cloned_authenticator_is_detected() {
    let (mut auth, mut creds) = registered(Algorithm::Es256);
    let c = challenge();
    finish_authentication(
        &rp(),
        &c,
        &auth.authenticate(RP_ID, ORIGIN, &c.to_base64url()),
        &mut creds,
    )
    .unwrap();
    auth.counter = 0; // un clone repart d'un compteur antérieur
    let c = challenge();
    let resp = auth.authenticate(RP_ID, ORIGIN, &c.to_base64url());
    assert_eq!(
        finish_authentication(&rp(), &c, &resp, &mut creds),
        Err(WebAuthnError::CloneDetected)
    );
}

#[test]
fn unknown_credential_and_wrong_ceremony_type() {
    let (mut auth, mut creds) = registered(Algorithm::Es256);
    let other = SoftAuthenticator::new(Algorithm::Es256);
    let c = challenge();
    let mut resp = auth.authenticate(RP_ID, ORIGIN, &c.to_base64url());
    resp.id = URL_SAFE_NO_PAD.encode(&other.credential_id);
    assert_eq!(
        finish_authentication(&rp(), &c, &resp, &mut creds),
        Err(WebAuthnError::UnknownCredential)
    );

    // Une réponse d'inscription ne peut pas servir de connexion, et inversement.
    let c = challenge();
    let reg = auth.register(RP_ID, ORIGIN, &c.to_base64url());
    let forged = AuthenticationResponse {
        id: reg.id.clone(),
        client_data_json: reg.client_data_json.clone(),
        authenticator_data: resp.authenticator_data.clone(),
        signature: resp.signature.clone(),
    };
    assert_eq!(
        finish_authentication(&rp(), &c, &forged, &mut creds),
        Err(WebAuthnError::WrongType)
    );
}

#[test]
fn registration_id_must_match_attested_id() {
    let auth = SoftAuthenticator::new(Algorithm::Es256);
    let c = challenge();
    let mut reg = auth.register(RP_ID, ORIGIN, &c.to_base64url());
    reg.id = URL_SAFE_NO_PAD.encode(b"un autre identifiant");
    assert_eq!(
        finish_registration(&rp(), &c, &reg, "x"),
        Err(WebAuthnError::CredentialMismatch)
    );
}

#[test]
fn options_require_user_verification() {
    let (_, creds) = registered(Algorithm::Es256);
    let c = challenge();
    let reg = registration_options(&rp(), b"user", "cabinet", &c, &creds);
    assert_eq!(
        reg["authenticatorSelection"]["userVerification"],
        "required"
    );
    assert_eq!(reg["excludeCredentials"][0]["id"], creds[0].id.as_str());
    let ids: Vec<String> = creds.iter().map(|c| c.id.clone()).collect();
    let auth = authentication_options(&rp(), &c, &ids, None);
    assert_eq!(auth["userVerification"], "required");
    assert_eq!(auth["rpId"], RP_ID);
    assert!(auth.get("extensions").is_none());
    let with_prf = authentication_options(&rp(), &c, &ids, Some(&[7u8; 32]));
    assert_eq!(
        with_prf["extensions"]["prf"]["eval"]["first"],
        URL_SAFE_NO_PAD.encode([7u8; 32])
    );
}

#[test]
fn prf_output_is_per_device_and_per_salt() {
    let a = SoftAuthenticator::new(Algorithm::Es256);
    let b = SoftAuthenticator::new(Algorithm::Es256);
    assert_eq!(a.prf(&[1; 32]), a.prf(&[1; 32]));
    assert_ne!(a.prf(&[1; 32]), a.prf(&[2; 32]));
    assert_ne!(a.prf(&[1; 32]), b.prf(&[1; 32]));
    assert_eq!(decode_prf_output(&a.prf_b64(&[1; 32])).unwrap().len(), 32);
    assert!(decode_prf_output("AAAA").is_err());
}

#[test]
fn garbage_input_never_panics() {
    let c = challenge();
    for junk in ["", "!!!", "AAAA", &URL_SAFE_NO_PAD.encode([0xffu8; 200])] {
        let reg = RegistrationResponse {
            id: junk.into(),
            client_data_json: junk.into(),
            attestation_object: junk.into(),
        };
        assert!(finish_registration(&rp(), &c, &reg, "x").is_err());
    }
}
