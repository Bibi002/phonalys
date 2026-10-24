//! Coffre documentaire : comptes rendus, prescriptions, courriers et images, chiffrés
//! dans le coffre comme le reste du dossier.
//!
//! Le contenu d'un document n'est remis qu'à une session **réauthentifiée depuis peu**
//! (phrase de passe et code, ou clé biométrique) : une session restée ouverte ne suffit
//! pas à emporter les pièces du dossier. Le type est déterminé par le serveur à partir
//! du contenu, jamais d'après ce qu'annonce le client, et seuls quelques formats sont
//! acceptés.

use crate::api::{blocking, check_totp, parse_id, session_token, unlock_with_passphrase, PATIENT};
use crate::error::ApiError;
use crate::passkey::{credentials, save_credentials};
use crate::state::{new_challenge, now, AppState, PendingReauth, Phase, ACTOR, CHALLENGE_TTL_SECS};
use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use phonalys_core::audit::AuditAction;
use phonalys_crypto::constant_time_eq;
use phonalys_vault::{UnlockedVault, VaultError};
use phonalys_webauthn::{authentication_options, finish_authentication, AuthenticationResponse};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use zeroize::Zeroizing;

/// Métadonnées (nom, type, patient) : listables sans réauthentification.
const META: &str = "document_meta";
/// Contenu chiffré.
const BLOB: &str = "document";
/// Taille maximale d'un document importé.
pub const MAX_DOCUMENT_BYTES: usize = 20 * 1024 * 1024;
/// Durée pendant laquelle une réauthentification ouvre les documents.
pub const ELEVATION_SECS: u64 = 5 * 60;
const MAX_NAME_CHARS: usize = 160;
const MAX_CATEGORY_CHARS: usize = 60;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentMeta {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub patient_id: Option<String>,
    pub name: String,
    pub mime: String,
    pub size: usize,
    #[serde(default)]
    pub category: String,
    pub created_at: u64,
}

/// Formats acceptés, reconnus à leur signature.
fn detect(bytes: &[u8], name: &str) -> Option<&'static str> {
    let ext = name
        .rsplit_once('.')
        .map(|(_, e)| e.to_ascii_lowercase())
        .unwrap_or_default();
    if bytes.starts_with(b"%PDF-") {
        return Some("application/pdf");
    }
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Some("image/png");
    }
    if bytes.starts_with(b"\xFF\xD8\xFF") {
        return Some("image/jpeg");
    }
    if bytes.starts_with(b"PK\x03\x04") {
        return match ext.as_str() {
            "docx" => {
                Some("application/vnd.openxmlformats-officedocument.wordprocessingml.document")
            }
            "xlsx" => Some("application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"),
            "odt" => Some("application/vnd.oasis.opendocument.text"),
            "ods" => Some("application/vnd.oasis.opendocument.spreadsheet"),
            _ => None,
        };
    }
    if ext == "txt" && !bytes.contains(&0) && std::str::from_utf8(bytes).is_ok() {
        return Some("text/plain; charset=utf-8");
    }
    None
}

/// Formats affichables dans le navigateur ; les autres sont toujours téléchargés.
fn displayable(mime: &str) -> bool {
    matches!(
        mime,
        "application/pdf" | "image/png" | "image/jpeg" | "text/plain; charset=utf-8"
    )
}

/// Nom de fichier sans chemin ni caractère de contrôle.
fn clean_name(name: &str) -> Result<String, ApiError> {
    let base = name.rsplit(['/', '\\']).next().unwrap_or_default();
    let cleaned: String = base
        .chars()
        .filter(|c| !c.is_control() && !matches!(c, '"' | '<' | '>' | '|' | '*' | '?' | ':'))
        .take(MAX_NAME_CHARS)
        .collect();
    let cleaned = cleaned.trim().trim_start_matches('.').to_owned();
    if cleaned.is_empty() {
        return Err(ApiError::BadRequest("nom de fichier manquant".into()));
    }
    Ok(cleaned)
}

/// En-tête `Content-Disposition` : forme ASCII de repli et forme UTF-8 (RFC 6266).
fn disposition(kind: &str, name: &str) -> String {
    let ascii: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || " .-_()".contains(c) {
                c
            } else {
                '_'
            }
        })
        .collect();
    let encoded: String = name
        .bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b".-_".contains(&b) {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect();
    format!("{kind}; filename=\"{ascii}\"; filename*=UTF-8''{encoded}")
}

fn target(id: &str) -> String {
    format!("{BLOB}/{id}")
}

/// La session de la requête a-t-elle été réauthentifiée récemment ?
fn require_elevation(state: &AppState, headers: &HeaderMap) -> Result<(), ApiError> {
    let elevated =
        session_token(headers).is_some_and(|t| state.lock_inner().sessions.is_elevated(&t, now()));
    if elevated {
        Ok(())
    } else {
        Err(ApiError::ReauthRequired)
    }
}

#[derive(Deserialize)]
pub struct ListQuery {
    #[serde(default)]
    patient_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct DocumentInfo {
    id: String,
    #[serde(flatten)]
    meta: DocumentMeta,
}

pub async fn list(
    State(state): State<AppState>,
    Query(q): Query<ListQuery>,
) -> Result<Json<Vec<DocumentInfo>>, ApiError> {
    state.with_vault(|vault| {
        let mut out = Vec::new();
        for id in vault.list(META)? {
            let meta: DocumentMeta = vault.get_json(META, &id)?;
            if q.patient_id
                .as_deref()
                .is_none_or(|pid| meta.patient_id.as_deref() == Some(pid))
            {
                out.push(DocumentInfo { id, meta });
            }
        }
        out.sort_by_key(|d| std::cmp::Reverse(d.meta.created_at));
        Ok(Json(out))
    })
}

#[derive(Deserialize)]
pub struct UploadQuery {
    name: String,
    #[serde(default)]
    category: Option<String>,
    #[serde(default)]
    patient_id: Option<String>,
}

pub async fn upload(
    State(state): State<AppState>,
    Query(q): Query<UploadQuery>,
    body: Bytes,
) -> Result<(StatusCode, Json<DocumentInfo>), ApiError> {
    if body.is_empty() {
        return Err(ApiError::BadRequest("fichier vide".into()));
    }
    if body.len() > MAX_DOCUMENT_BYTES {
        return Err(ApiError::BadRequest(format!(
            "document trop volumineux (maximum {} Mo)",
            MAX_DOCUMENT_BYTES / 1024 / 1024
        )));
    }
    let name = clean_name(&q.name)?;
    let mime = detect(&body, &name).ok_or_else(|| {
        ApiError::BadRequest(
            "format non accepté : PDF, PNG, JPEG, texte (.txt), Word (.docx), \
             OpenDocument (.odt, .ods) ou Excel (.xlsx)"
                .into(),
        )
    })?;
    let category: String = q
        .category
        .unwrap_or_default()
        .trim()
        .chars()
        .filter(|c| !c.is_control())
        .take(MAX_CATEGORY_CHARS)
        .collect();
    let patient_id = q
        .patient_id
        .filter(|p| !p.is_empty())
        .map(|p| parse_id(&p).map_err(|_| ApiError::BadRequest("patient_id invalide".into())))
        .transpose()?;
    blocking(move || {
        state.with_vault(|vault| {
            if let Some(pid) = &patient_id {
                match vault.get(PATIENT, pid) {
                    Ok(_) => {}
                    Err(VaultError::RecordNotFound) => {
                        return Err(ApiError::BadRequest("patient inconnu".into()))
                    }
                    Err(e) => return Err(e.into()),
                }
            }
            let id = UnlockedVault::new_id();
            let meta = DocumentMeta {
                patient_id,
                name,
                mime: mime.to_owned(),
                size: body.len(),
                category,
                created_at: now(),
            };
            // Le contenu d'abord : des métadonnées ne désignent jamais un contenu absent.
            vault.put(BLOB, &id, &body)?;
            vault.put_json(META, &id, &meta)?;
            vault.audit(ACTOR, AuditAction::DocumentImported, Some(&target(&id)))?;
            Ok((StatusCode::CREATED, Json(DocumentInfo { id, meta })))
        })
    })
    .await
}

#[derive(Deserialize)]
pub struct ContentQuery {
    #[serde(default)]
    download: Option<u8>,
}

/// Contenu déchiffré. Les formats affichables s'ouvrent dans la visionneuse de
/// l'application (cadre de même origine) ou un onglet, sous une CSP qui interdit tout
/// script ; les autres sont téléchargés.
pub async fn content(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(q): Query<ContentQuery>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let id = parse_id(&id)?;
    require_elevation(&state, &headers)?;
    let (meta, bytes) = state.with_vault(|vault| {
        let meta: DocumentMeta = vault.get_json(META, &id)?;
        let bytes = vault.get(BLOB, &id)?;
        vault.audit(ACTOR, AuditAction::DocumentOpened, Some(&target(&id)))?;
        Ok((meta, bytes))
    })?;
    let inline = displayable(&meta.mime) && q.download != Some(1);
    let csp = if meta.mime == "application/pdf" {
        // La visionneuse PDF intégrée du navigateur refuse la directive `sandbox`.
        "default-src 'none'; img-src 'self' data:; style-src 'unsafe-inline'; \
         frame-ancestors 'self'; base-uri 'none'; form-action 'none'"
    } else {
        "sandbox; default-src 'none'; img-src 'self' data:; style-src 'unsafe-inline'; \
         frame-ancestors 'self'"
    };
    let mut res = (StatusCode::OK, bytes.to_vec()).into_response();
    let h = res.headers_mut();
    h.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(&meta.mime).map_err(|_| ApiError::Internal)?,
    );
    h.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&disposition(
            if inline { "inline" } else { "attachment" },
            &meta.name,
        ))
        .map_err(|_| ApiError::Internal)?,
    );
    h.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(csp),
    );
    h.insert(
        header::X_FRAME_OPTIONS,
        HeaderValue::from_static("SAMEORIGIN"),
    );
    Ok(res)
}

#[derive(Deserialize)]
pub struct UpdateMeta {
    name: String,
    #[serde(default)]
    category: String,
}

pub async fn rename(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<UpdateMeta>,
) -> Result<Json<DocumentInfo>, ApiError> {
    let id = parse_id(&id)?;
    let name = clean_name(&req.name)?;
    let category: String = req
        .category
        .trim()
        .chars()
        .filter(|c| !c.is_control())
        .take(MAX_CATEGORY_CHARS)
        .collect();
    state.with_vault(|vault| {
        let mut meta: DocumentMeta = vault.get_json(META, &id)?;
        meta.name = name;
        meta.category = category;
        vault.put_json(META, &id, &meta)?;
        vault.audit(ACTOR, AuditAction::RecordUpdated, Some(&target(&id)))?;
        Ok(Json(DocumentInfo { id, meta }))
    })
}

fn erase(vault: &mut UnlockedVault, id: &str) -> Result<(), ApiError> {
    vault.delete(META, id)?;
    match vault.delete(BLOB, id) {
        Ok(()) | Err(VaultError::RecordNotFound) => {}
        Err(e) => return Err(e.into()),
    }
    vault.audit(ACTOR, AuditAction::RecordErased, Some(&target(id)))?;
    Ok(())
}

/// Suppression : réauthentification exigée, comme pour l'ouverture.
pub async fn delete(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Result<StatusCode, ApiError> {
    let id = parse_id(&id)?;
    require_elevation(&state, &headers)?;
    state.with_vault(|vault| {
        erase(vault, &id)?;
        Ok(StatusCode::NO_CONTENT)
    })
}

/// Efface les documents d'un patient, avec son dossier.
pub(crate) fn erase_patient_documents(
    vault: &mut UnlockedVault,
    patient_id: &str,
) -> Result<usize, ApiError> {
    let mut erased = 0;
    for id in vault.list(META)? {
        let meta: DocumentMeta = vault.get_json(META, &id)?;
        if meta.patient_id.as_deref() == Some(patient_id) {
            erase(vault, &id)?;
            erased += 1;
        }
    }
    Ok(erased)
}

// --- Réauthentification -------------------------------------------------------------

fn elevate(state: &AppState, headers: &HeaderMap, now: u64) -> Result<Response, ApiError> {
    let token = session_token(headers).ok_or(ApiError::Unauthorized)?;
    let mut inner = state.lock_inner();
    if !inner.sessions.elevate(&token, now + ELEVATION_SECS) {
        return Err(ApiError::Unauthorized);
    }
    if let Phase::Unlocked(vault) = &mut inner.phase {
        vault.audit(ACTOR, AuditAction::Reauthenticated, None)?;
    }
    Ok(Json(serde_json::json!({ "elevated_secs": ELEVATION_SECS })).into_response())
}

#[derive(Deserialize)]
pub struct ReauthRequest {
    passphrase: Zeroizing<String>,
    code: String,
}

/// Réauthentification par phrase de passe et code à 6 chiffres. Les échecs comptent
/// dans le blocage de la connexion, comme à l'écran de connexion.
pub async fn reauth(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<ReauthRequest>,
) -> Result<Response, ApiError> {
    let s = state.clone();
    let now = now();
    blocking(move || {
        let fresh = match unlock_with_passphrase(&s, req.passphrase.as_bytes(), now) {
            Ok(v) => v,
            // Un 401 fermerait la session de l'interface : l'échec est signalé autrement.
            Err(ApiError::LoginFailed(n)) => return Err(ApiError::ReauthFailed(Some(n))),
            Err(e) => return Err(e),
        };
        drop(fresh);
        s.with_vault(|vault| {
            if crate::passkey::policy(vault)?.require_passkey {
                return Err(ApiError::BadRequest(
                    "clé biométrique exigée : confirmez avec votre clé".into(),
                ));
            }
            if check_totp(vault, &req.code, now)?.is_some() {
                vault.reset_lockout()?;
                return Ok(());
            }
            vault.register_failure(now as i64)?;
            vault.audit(ACTOR, AuditAction::LoginFailed, None)?;
            Err(ApiError::ReauthFailed(Some(vault.remaining_attempts()?)))
        })
    })
    .await?;
    elevate(&state, &headers, now)
}

pub async fn reauth_passkey_start(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    let token = session_token(&headers).ok_or(ApiError::Unauthorized)?;
    let ids: Vec<String> =
        state.with_vault(|vault| Ok(credentials(vault)?.into_iter().map(|c| c.id).collect()))?;
    if ids.is_empty() {
        return Err(ApiError::BadRequest(
            "aucune clé biométrique enregistrée".into(),
        ));
    }
    let challenge = new_challenge();
    let options = authentication_options(&state.config.relying_party, &challenge, &ids, None);
    state.lock_inner().pending_reauth = Some(PendingReauth {
        challenge,
        session: Zeroizing::new(token),
        expires_at: now() + CHALLENGE_TTL_SECS,
    });
    Ok(Json(options))
}

#[derive(Deserialize)]
pub struct ReauthPasskeyFinish {
    response: AuthenticationResponse,
}

pub async fn reauth_passkey_finish(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<ReauthPasskeyFinish>,
) -> Result<Response, ApiError> {
    let token = session_token(&headers).ok_or(ApiError::Unauthorized)?;
    let now = now();
    {
        let mut inner = state.lock_inner();
        let pending = inner
            .pending_reauth
            .take()
            .filter(|p| now <= p.expires_at)
            .filter(|p| constant_time_eq(p.session.as_bytes(), token.as_bytes()))
            .ok_or(ApiError::ReauthFailed(None))?;
        let Phase::Unlocked(vault) = &mut inner.phase else {
            return Err(ApiError::Unauthorized);
        };
        let mut creds = credentials(vault)?;
        match finish_authentication(
            &state.config.relying_party,
            &pending.challenge,
            &req.response,
            &mut creds,
        ) {
            Ok(_) => save_credentials(vault, &creds)?,
            Err(e) => {
                tracing::warn!(error = %e, "réauthentification par clé refusée");
                vault.audit(ACTOR, AuditAction::LoginFailed, None)?;
                return Err(ApiError::ReauthFailed(None));
            }
        }
    }
    elevate(&state, &headers, now)
}

#[cfg(test)]
mod unit {
    use super::*;

    #[test]
    fn detects_by_content_not_by_name() {
        assert_eq!(detect(b"%PDF-1.7 ...", "x.jpg"), Some("application/pdf"));
        assert_eq!(detect(b"\x89PNG\r\n\x1a\n...", "a"), Some("image/png"));
        assert_eq!(detect(b"\xFF\xD8\xFF\xE0", "a.pdf"), Some("image/jpeg"));
        assert_eq!(detect(b"<html><script>", "a.pdf"), None);
        assert_eq!(detect(b"<svg onload=alert(1)>", "a.svg"), None);
        assert_eq!(detect(b"PK\x03\x04", "a.zip"), None);
        assert!(detect(b"PK\x03\x04", "Compte rendu.DOCX").is_some());
        assert_eq!(
            detect(b"bonjour", "notes.txt"),
            Some("text/plain; charset=utf-8")
        );
        assert_eq!(detect(b"bon\0jour", "notes.txt"), None);
        assert_eq!(detect(b"MZ\x90\0", "setup.exe"), None);
    }

    #[test]
    fn names_lose_paths_and_dangerous_characters() {
        assert_eq!(clean_name("C:\\Users\\x\\bilan.pdf").unwrap(), "bilan.pdf");
        assert_eq!(clean_name("../../etc/passwd").unwrap(), "passwd");
        assert_eq!(clean_name("a\"b<c>\r\n.pdf").unwrap(), "abc.pdf");
        assert_eq!(clean_name("..hidden").unwrap(), "hidden");
        assert!(clean_name("   ").is_err());
        assert!(clean_name("dir/").is_err());
    }

    #[test]
    fn disposition_is_header_safe() {
        let d = disposition("inline", "Bilan élève \"Léa\".pdf");
        assert!(d.is_ascii());
        assert!(HeaderValue::from_str(&d).is_ok());
        assert!(d.contains("filename*=UTF-8''Bilan%20%C3%A9l%C3%A8ve"));
    }
}
