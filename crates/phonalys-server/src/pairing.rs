//! Appairage des téléphones et tablettes : installation de l'autorité locale du
//! cabinet (voir [`crate::tls`]).
//!
//! Le certificat de l'autorité n'a rien de secret : il est servi sans connexion, pour
//! que l'appareil puisse l'installer avant même de faire confiance au serveur. Le
//! praticien compare l'empreinte affichée sur l'appareil avec celle de l'écran
//! « Sécurité » : c'est ce contrôle qui écarte un intermédiaire malveillant.

use crate::api::qr_svg;
use crate::error::ApiError;
use crate::state::AppState;
use axum::extract::State;
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use data_encoding::BASE64;
use serde::Serialize;
use sha2::{Digest, Sha256};

#[derive(Debug, Serialize)]
pub struct PairingInfo {
    /// Faux pour l'application de bureau (boucle locale, sans TLS).
    available: bool,
    ca_fingerprint: Option<String>,
    origin: Option<String>,
}

pub async fn info(State(state): State<AppState>) -> Json<PairingInfo> {
    let ca = state.config.authority.as_ref();
    Json(PairingInfo {
        available: ca.is_some(),
        ca_fingerprint: ca.map(|c| c.fingerprint.clone()),
        origin: state.config.relying_party.origins.first().cloned(),
    })
}

fn download(body: Vec<u8>, mime: &'static str, name: &'static str) -> Response {
    let mut res = (StatusCode::OK, body).into_response();
    let h = res.headers_mut();
    h.insert(header::CONTENT_TYPE, HeaderValue::from_static(mime));
    h.insert(header::CONTENT_DISPOSITION, HeaderValue::from_static(name));
    res
}

/// Certificat de l'autorité, au format PEM (Android, Windows, Linux).
pub async fn certificate(State(state): State<AppState>) -> Result<Response, ApiError> {
    let ca = state.config.authority.as_ref().ok_or(ApiError::NotFound)?;
    Ok(download(
        pem(&ca.der).into_bytes(),
        "application/x-x509-ca-cert",
        "attachment; filename=\"phonalys-cabinet.crt\"",
    ))
}

/// Profil de configuration iOS / iPadOS / macOS contenant l'autorité.
pub async fn mobileconfig(State(state): State<AppState>) -> Result<Response, ApiError> {
    let ca = state.config.authority.as_ref().ok_or(ApiError::NotFound)?;
    Ok(download(
        profile(&ca.der).into_bytes(),
        "application/x-apple-aspen-config",
        "attachment; filename=\"phonalys-cabinet.mobileconfig\"",
    ))
}

#[derive(Debug, Serialize)]
pub struct PairingQr {
    url: String,
    qr_svg: String,
    ca_fingerprint: String,
}

/// QR code à scanner avec l'appareil photo du téléphone (session requise).
pub async fn qr(State(state): State<AppState>) -> Result<Json<PairingQr>, ApiError> {
    let ca = state.config.authority.as_ref().ok_or(ApiError::NotFound)?;
    let origin = state
        .config
        .relying_party
        .origins
        .first()
        .ok_or(ApiError::NotFound)?;
    let url = format!("{origin}/#appairage");
    Ok(Json(PairingQr {
        qr_svg: qr_svg(&url)?,
        url,
        ca_fingerprint: ca.fingerprint.clone(),
    }))
}

pub(crate) fn pem(der: &[u8]) -> String {
    let b64 = BASE64.encode(der);
    let mut out = String::from("-----BEGIN CERTIFICATE-----\n");
    for chunk in b64.as_bytes().chunks(64) {
        out.push_str(std::str::from_utf8(chunk).unwrap_or_default());
        out.push('\n');
    }
    out.push_str("-----END CERTIFICATE-----\n");
    out
}

/// UUID stable dérivé de l'autorité : réinstaller le profil remplace l'ancien.
fn uuid_from(der: &[u8], salt: &str) -> String {
    let mut h = Sha256::new();
    h.update(salt.as_bytes());
    h.update(der);
    let mut b: [u8; 16] = h.finalize()[..16].try_into().unwrap_or_default();
    b[6] = (b[6] & 0x0f) | 0x50;
    b[8] = (b[8] & 0x3f) | 0x80;
    uuid::Uuid::from_bytes(b).to_string().to_uppercase()
}

pub(crate) fn profile(der: &[u8]) -> String {
    let (inner, outer) = (uuid_from(der, "ca"), uuid_from(der, "profile"));
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>PayloadContent</key>
  <array>
    <dict>
      <key>PayloadCertificateFileName</key>
      <string>phonalys-cabinet.cer</string>
      <key>PayloadContent</key>
      <data>{data}</data>
      <key>PayloadDescription</key>
      <string>Autorité locale du cabinet, limitée aux adresses du réseau local.</string>
      <key>PayloadDisplayName</key>
      <string>Phonalys - autorité locale du cabinet</string>
      <key>PayloadIdentifier</key>
      <string>be.phonalys.ca.{inner}</string>
      <key>PayloadType</key>
      <string>com.apple.security.root</string>
      <key>PayloadUUID</key>
      <string>{inner}</string>
      <key>PayloadVersion</key>
      <integer>1</integer>
    </dict>
  </array>
  <key>PayloadDescription</key>
  <string>Permet à cet appareil de se connecter en toute sécurité au serveur Phonalys du cabinet.</string>
  <key>PayloadDisplayName</key>
  <string>Phonalys - connexion au cabinet</string>
  <key>PayloadIdentifier</key>
  <string>be.phonalys.profile.{outer}</string>
  <key>PayloadRemovalDisallowed</key>
  <false/>
  <key>PayloadType</key>
  <string>Configuration</string>
  <key>PayloadUUID</key>
  <string>{outer}</string>
  <key>PayloadVersion</key>
  <integer>1</integer>
</dict>
</plist>
"#,
        data = BASE64.encode(der)
    )
}
