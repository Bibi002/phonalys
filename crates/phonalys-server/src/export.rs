//! Export complet du dossier d'un patient : droit d'accès et de portabilité (RGPD
//! art. 15 et 20), droit à la copie du dossier (loi du 22 août 2002, art. 9).
//!
//! Le fichier contient tout ce que le coffre conserve sur le patient : identité,
//! dossier clinique et documents. Comme l'ouverture d'un document, il exige une
//! session réauthentifiée depuis peu, et chaque export est inscrit au journal.

use crate::api::{parse_id, PATIENT};
use crate::documents::{patient_documents, require_elevation};
use crate::error::ApiError;
use crate::records::{settings, CLINICAL_KINDS};
use crate::state::{now, AppState, ACTOR};
use axum::extract::{Path, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use data_encoding::BASE64;
use phonalys_core::audit::AuditAction;
use serde_json::{json, Map, Value};
use time::OffsetDateTime;

/// Identifie le format pour une future réimportation.
pub const FORMAT: &str = "phonalys-dossier-patient";
pub const FORMAT_VERSION: u32 = 1;

pub async fn patient(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let id = parse_id(&id)?;
    require_elevation(&state, &headers)?;
    let at = now();
    let body = state.with_vault(|vault| {
        let patient: Value = vault.get_json(PATIENT, &id)?;
        let mut records = Map::new();
        for k in CLINICAL_KINDS {
            let mut items = Vec::new();
            for rid in vault.list(k)? {
                let mut value: Value = vault.get_json(k, &rid)?;
                if value.get("patient_id").and_then(Value::as_str) == Some(id.as_str()) {
                    if let Some(obj) = value.as_object_mut() {
                        obj.insert("id".into(), Value::String(rid));
                    }
                    items.push(value);
                }
            }
            records.insert((*k).to_owned(), Value::Array(items));
        }
        let documents: Vec<Value> = patient_documents(vault, &id)?
            .into_iter()
            .map(|d| {
                json!({
                    "id": d.id,
                    "name": d.meta.name,
                    "mime": d.meta.mime,
                    "category": d.meta.category,
                    "size": d.meta.size,
                    "created_at": d.meta.created_at,
                    "content_base64": BASE64.encode(&d.bytes),
                })
            })
            .collect();
        let doc = json!({
            "format": FORMAT,
            "format_version": FORMAT_VERSION,
            "exported_at": at,
            "practitioner": settings(vault)?.practitioner,
            "patient": patient,
            "patient_id": id,
            "records": records,
            "documents": documents,
        });
        vault.audit(ACTOR, AuditAction::Export, Some(&format!("{PATIENT}/{id}")))?;
        Ok(serde_json::to_vec_pretty(&doc)?)
    })?;

    let mut res = (StatusCode::OK, body).into_response();
    let h = res.headers_mut();
    h.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    // Aucune donnée d'identité dans le nom du fichier.
    h.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!(
            "attachment; filename=\"dossier-patient-{}.json\"",
            day(at)
        ))
        .map_err(|_| ApiError::Internal)?,
    );
    Ok(res)
}

fn day(at: u64) -> String {
    let t = OffsetDateTime::from_unix_timestamp(at as i64).unwrap_or(OffsetDateTime::UNIX_EPOCH);
    format!("{:04}{:02}{:02}", t.year(), u8::from(t.month()), t.day())
}
