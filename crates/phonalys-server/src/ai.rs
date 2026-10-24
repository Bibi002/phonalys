//! Assistant de rédaction : un modèle **hébergé par le praticien** propose un projet de
//! conclusion et de projet thérapeutique à partir des résultats saisis.
//!
//! - Adresse validée par [`EndpointPolicy`] : IA en ligne et adresses publiques refusées.
//! - Pseudonymisation : le modèle ne reçoit ni nom, ni prénom, ni école, ni médecin,
//!   ni date de naissance (seulement l'âge) ; les jetons sont restitués dans la réponse.
//! - Le texte est un **projet** : le praticien le relit, le corrige et en reste l'auteur.
//!   Phonalys ne pose aucun diagnostic.

use crate::api::{parse_id, PATIENT};
use crate::error::ApiError;
use crate::state::{AppState, ACTOR};
use axum::extract::State;
use axum::Json;
use phonalys_ai::{
    AiError, ChatMessage, ChatRequest, EndpointPolicy, Engine, ProviderKind, Pseudonymizer, Role,
};
use phonalys_core::audit::AuditAction;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::time::Duration;

const AI_SETTINGS: &str = "ai";
const MAX_STYLE_CHARS: usize = 20_000;
const MAX_INSTRUCTIONS_CHARS: usize = 2_000;
const TEST_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AiSettings {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_kind")]
    pub kind: ProviderKind,
    #[serde(default = "default_endpoint")]
    pub endpoint: String,
    #[serde(default)]
    pub model: String,
    /// Empreinte SHA-256 du certificat du serveur d'IA (HTTPS sur le réseau local).
    #[serde(default)]
    pub cert_sha256: Option<String>,
    /// HTTP en clair vers une autre machine du réseau (déconseillé).
    #[serde(default)]
    pub allow_plaintext_lan: bool,
    /// Consignes de style du praticien : tournures, plan habituel, exemple anonymisé.
    #[serde(default)]
    pub style: String,
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
}

fn default_kind() -> ProviderKind {
    ProviderKind::Ollama
}

fn default_endpoint() -> String {
    "http://127.0.0.1:11434".into()
}

fn default_timeout() -> u64 {
    300
}

impl Default for AiSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            kind: default_kind(),
            endpoint: default_endpoint(),
            model: String::new(),
            cert_sha256: None,
            allow_plaintext_lan: false,
            style: String::new(),
            timeout_secs: default_timeout(),
        }
    }
}

impl AiSettings {
    fn engine(&self) -> Engine {
        Engine {
            kind: self.kind,
            endpoint: self.endpoint.trim().to_owned(),
            cert_sha256: self.cert_sha256.clone().filter(|p| !p.trim().is_empty()),
            policy: EndpointPolicy {
                allow_plaintext_lan: self.allow_plaintext_lan,
            },
        }
    }
}

fn ai_error(e: AiError) -> ApiError {
    tracing::warn!(error = %e, "assistant de rédaction");
    ApiError::BadRequest(e.to_string())
}

fn load(state: &AppState) -> Result<AiSettings, ApiError> {
    state.with_vault(|vault| {
        Ok(match vault.get_secret(AI_SETTINGS)? {
            Some(bytes) => serde_json::from_slice(&bytes)?,
            None => AiSettings::default(),
        })
    })
}

pub async fn get_settings(State(state): State<AppState>) -> Result<Json<AiSettings>, ApiError> {
    Ok(Json(load(&state)?))
}

pub async fn put_settings(
    State(state): State<AppState>,
    Json(mut new): Json<AiSettings>,
) -> Result<Json<AiSettings>, ApiError> {
    new.endpoint = new.endpoint.trim().to_owned();
    new.model = new.model.trim().chars().take(200).collect();
    new.cert_sha256 = new
        .cert_sha256
        .map(|p| p.trim().to_owned())
        .filter(|p| !p.is_empty());
    if new.style.chars().count() > MAX_STYLE_CHARS {
        return Err(ApiError::BadRequest(format!(
            "consignes de style trop longues (maximum {MAX_STYLE_CHARS} caractères)"
        )));
    }
    if !(30..=900).contains(&new.timeout_secs) {
        return Err(ApiError::BadRequest(
            "délai de réponse entre 30 et 900 secondes".into(),
        ));
    }
    // Contrôle immédiat de l'adresse, résolution comprise : une IA en ligne ou une
    // adresse publique n'est même pas enregistrable. Elle est revérifiée à chaque appel.
    new.engine().resolve().await.map_err(ai_error)?;
    state.with_vault(|vault| {
        vault.put_secret(AI_SETTINGS, &serde_json::to_vec(&new)?)?;
        vault.audit(ACTOR, AuditAction::SettingsChanged, Some("ia"))?;
        Ok(())
    })?;
    Ok(Json(new))
}

#[derive(Debug, Serialize)]
pub struct ModelsResponse {
    models: Vec<String>,
}

/// Test de connexion : la liste des modèles installés sur le moteur.
pub async fn models(State(state): State<AppState>) -> Result<Json<ModelsResponse>, ApiError> {
    let settings = load(&state)?;
    let models = settings
        .engine()
        .models(TEST_TIMEOUT)
        .await
        .map_err(ai_error)?;
    Ok(Json(ModelsResponse { models }))
}

#[derive(Deserialize)]
pub struct DraftRequest {
    patient_id: String,
    /// Bilan en cours de saisie (pas forcément enregistré).
    bilan: Value,
    #[serde(default)]
    instructions: String,
}

#[derive(Debug, Serialize)]
pub struct DraftResponse {
    conclusion: String,
    projet: String,
    model: String,
}

const SYSTEM_PROMPT: &str = "Tu es l'assistant de rédaction d'un ou d'une logopède exerçant en Belgique. \
Tu rédiges, en français, un PROJET de conclusion et de projet thérapeutique pour un bilan logopédique, \
à partir UNIQUEMENT des données fournies au format JSON.

Règles impératives :
1. N'invente aucune donnée : aucun score, aucun antécédent, aucune observation. Si une information manque, écris [à compléter].
2. Ne pose pas de diagnostic et n'emploie pas de formulation définitive : tu proposes un texte que le praticien vérifiera, corrigera et validera. Il en reste seul responsable.
3. Lis chaque score selon son échelle : « z » (moyenne 0, écart type 1), « percentile », « note_standard » (moyenne 10, écart type 3), « note_composite » (moyenne 100, écart type 15). Le champ « lecture » donne la valeur convertie en z et sa position par rapport à la moyenne : fie-toi à ce champ (un z positif est au-dessus de la moyenne). Ne cite aucune norme ni aucun seuil qui ne figure pas dans les données.
4. Les jetons entre crochets ([PATIENT_1], [PRENOM_1], [ECOLE_1]…) remplacent des données personnelles : recopie-les tels quels, sans chercher à les deviner.
5. Ton professionnel, clair pour les parents, les enseignants et les médecins, sans jargon inutile.
6. Réponds exactement en deux parties, dans cet ordre, sans rien avant ni après :
### CONCLUSION
(synthèse des points forts et des difficultés, par domaine, avec les scores pertinents)
### PROJET
(propositions : axes de travail, objectifs mesurables, fréquence des séances, conseils et collaborations éventuelles)";

/// Lecture z d'un score, comme dans l'interface (conversions purement mathématiques).
fn z_of(mesure: &str, valeur: &str) -> Option<f64> {
    let v: f64 = valeur.trim().replace(',', ".").parse().ok()?;
    match mesure {
        "z" => Some(v),
        "note_standard" => Some((v - 10.0) / 3.0),
        "note_composite" => Some((v - 100.0) / 15.0),
        "percentile" if v > 0.0 && v < 100.0 => Some(inverse_normal(v / 100.0)),
        _ => None,
    }
}

/// Quantile de la loi normale (approximation d'Acklam, erreur < 1,2e-9).
fn inverse_normal(p: f64) -> f64 {
    const A: [f64; 6] = [
        -39.696_830_286_653_76,
        220.946_098_424_520_5,
        -275.928_510_446_968_7,
        138.357_751_867_269,
        -30.664_798_066_147_16,
        2.506_628_277_459_239,
    ];
    const B: [f64; 5] = [
        -54.476_098_798_224_06,
        161.585_836_858_040_9,
        -155.698_979_859_886_6,
        66.801_311_887_719_72,
        -13.280_681_552_885_72,
    ];
    const C: [f64; 6] = [
        -0.007_784_894_002_430_293,
        -0.322_396_458_041_136_4,
        -2.400_758_277_161_838,
        -2.549_732_539_343_734,
        4.374_664_141_464_968,
        2.938_163_982_698_783,
    ];
    const D: [f64; 4] = [
        0.007_784_695_709_041_462,
        0.322_467_129_070_039_8,
        2.445_134_137_142_996,
        3.754_408_661_907_416,
    ];
    let tail = |q: f64| {
        (((((C[0] * q + C[1]) * q + C[2]) * q + C[3]) * q + C[4]) * q + C[5])
            / ((((D[0] * q + D[1]) * q + D[2]) * q + D[3]) * q + 1.0)
    };
    if p < 0.02425 {
        tail((-2.0 * p.ln()).sqrt())
    } else if p > 1.0 - 0.02425 {
        -tail((-2.0 * (1.0 - p).ln()).sqrt())
    } else {
        let q = p - 0.5;
        let r = q * q;
        (((((A[0] * r + A[1]) * r + A[2]) * r + A[3]) * r + A[4]) * r + A[5]) * q
            / (((((B[0] * r + B[1]) * r + B[2]) * r + B[3]) * r + B[4]) * r + 1.0)
    }
}

/// Position d'un score par rapport à la moyenne, en toutes lettres : de petits modèles
/// lisent mal le signe d'un nombre. Simple calcul, sans seuil clinique.
fn reading(z: f64) -> String {
    let side = if z.abs() < 0.005 {
        return "z = 0.00, exactement à la moyenne".into();
    } else if z > 0.0 {
        "au-dessus"
    } else {
        "en dessous"
    };
    format!(
        "z = {z:.2}, soit {:.2} écart type {side} de la moyenne",
        z.abs()
    )
}

/// Âge à une date, « 8 ans 3 mois », à partir de dates AAAA-MM-JJ.
fn age_at(naissance: &str, at: &str) -> Option<String> {
    let parse = |s: &str| -> Option<(i32, i32, i32)> {
        let mut it = s.split('-').map(|x| x.parse::<i32>().ok());
        Some((it.next()??, it.next()??, it.next()??))
    };
    let (by, bm, bd) = parse(naissance)?;
    let (ay, am, ad) = parse(at)?;
    let mut months = (ay - by) * 12 + (am - bm);
    if ad < bd {
        months -= 1;
    }
    if months < 0 {
        return None;
    }
    let (y, m) = (months / 12, months % 12);
    let mut s = format!("{y} an{}", if y > 1 { "s" } else { "" });
    if m > 0 {
        s.push_str(&format!(" {m} mois"));
    }
    Some(s)
}

fn text(v: &Value, key: &str) -> String {
    v.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default()
        .to_owned()
}

/// Données envoyées au modèle : identité remplacée par des jetons, âge au lieu de la
/// date de naissance, aucun identifiant interne.
pub(crate) fn build_prompt(
    patient: &Value,
    bilan: &Value,
    objectifs: &[Value],
) -> (Value, Pseudonymizer) {
    let mut p = Pseudonymizer::new();
    let (nom, prenom) = (text(patient, "nom"), text(patient, "prenom"));
    if !nom.is_empty() && !prenom.is_empty() {
        p.register("PATIENT", &format!("{prenom} {nom}"));
        p.register("PATIENT", &format!("{nom} {prenom}"));
    }
    if !prenom.is_empty() {
        p.register("PRENOM", &prenom);
    }
    if !nom.is_empty() {
        p.register("NOM", &nom);
    }
    for (key, cat) in [("ecole", "ECOLE"), ("medecin", "MEDECIN")] {
        let v = text(patient, key);
        if !v.is_empty() {
            p.register(cat, &v);
        }
    }
    for c in patient
        .get("contacts")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let v = text(c, "nom");
        if !v.is_empty() {
            p.register("CONTACT", &v);
        }
    }

    let m = |s: String| Value::String(p.mask(&s));
    let date = text(bilan, "date");
    let resultats: Vec<Value> = bilan
        .get("resultats")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|r| !text(r, "epreuve").is_empty())
        .map(|r| {
            let mesure = text(r, "mesure");
            let valeur = text(r, "valeur");
            let mut o = Map::new();
            o.insert("epreuve".into(), m(text(r, "epreuve")));
            for k in ["domaine", "brut", "commentaire"] {
                let v = text(r, k);
                if !v.is_empty() {
                    o.insert(k.into(), m(v));
                }
            }
            o.insert("echelle".into(), Value::String(mesure.clone()));
            if !valeur.is_empty() {
                o.insert("valeur".into(), Value::String(valeur.clone()));
            }
            if let Some(z) = z_of(&mesure, &valeur) {
                o.insert("lecture".into(), json!(reading(z)));
            }
            Value::Object(o)
        })
        .collect();
    let objectifs: Vec<Value> = objectifs
        .iter()
        .map(|o| {
            json!({
                "domaine": m(text(o, "domaine")),
                "intitule": m(text(o, "intitule")),
                "statut": text(o, "statut"),
                "progression": o.get("progression").cloned().unwrap_or(Value::Null),
            })
        })
        .collect();
    let data = json!({
        "patient": {
            "age": age_at(&text(patient, "naissance"), &date),
            "sexe": text(patient, "sexe"),
            "classe": text(patient, "classe"),
            "langues": m(text(patient, "langues")),
            "motif": m(text(patient, "motif")),
            "anamnese": m(text(patient, "anamnese")),
        },
        "bilan": {
            "type": text(bilan, "type"),
            "date": date,
            "batterie": m(text(bilan, "batterie_nom")),
            "demande": m(text(bilan, "demande")),
            "observations": m(text(bilan, "observations")),
            "resultats": resultats,
            "conclusion_existante": m(text(bilan, "conclusion")),
            "projet_existant": m(text(bilan, "projet")),
        },
        "objectifs_en_cours": objectifs,
    });
    (data, p)
}

/// Sépare la réponse en conclusion et projet ; tout va dans la conclusion si le modèle
/// n'a pas respecté le format.
fn split_sections(answer: &str) -> (String, String) {
    let lower = answer.to_lowercase();
    match (lower.find("### conclusion"), lower.find("### projet")) {
        (Some(c), Some(p)) if c < p => {
            let conclusion = answer[c + "### conclusion".len()..p].trim().to_owned();
            let projet = answer[p + "### projet".len()..].trim().to_owned();
            (conclusion, projet)
        }
        _ => (answer.trim().to_owned(), String::new()),
    }
}

pub async fn draft(
    State(state): State<AppState>,
    Json(req): Json<DraftRequest>,
) -> Result<Json<DraftResponse>, ApiError> {
    let pid = parse_id(&req.patient_id)
        .map_err(|_| ApiError::BadRequest("patient_id invalide".into()))?;
    if req.instructions.chars().count() > MAX_INSTRUCTIONS_CHARS {
        return Err(ApiError::BadRequest("demande trop longue".into()));
    }
    if serde_json::to_vec(&req.bilan)?.len() > 512 * 1024 {
        return Err(ApiError::BadRequest("bilan trop volumineux".into()));
    }
    let settings = load(&state)?;
    if !settings.enabled || settings.model.is_empty() {
        return Err(ApiError::BadRequest(
            "assistant de rédaction non configuré : voir Réglages › Assistant de rédaction".into(),
        ));
    }
    // Lecture du dossier sous verrou, puis appel au modèle sans le verrou.
    let (data, pseudo) = state.with_vault(|vault| {
        let patient: Value = vault.get_json(PATIENT, &pid)?;
        let mut objectifs = Vec::new();
        for id in vault.list("objectif")? {
            let o: Value = vault.get_json("objectif", &id)?;
            if o.get("patient_id").and_then(Value::as_str) == Some(pid.as_str())
                && o.get("statut").and_then(Value::as_str) == Some("en_cours")
            {
                objectifs.push(o);
            }
        }
        Ok(build_prompt(&patient, &req.bilan, &objectifs))
    })?;
    let mut user = format!(
        "Données du bilan (JSON) :\n{}",
        serde_json::to_string_pretty(&data)?
    );
    if !settings.style.trim().is_empty() {
        user.push_str(&format!(
            "\n\nConsignes de style du praticien :\n{}",
            pseudo.mask(settings.style.trim())
        ));
    }
    if !req.instructions.trim().is_empty() {
        user.push_str(&format!(
            "\n\nDemande particulière :\n{}",
            pseudo.mask(req.instructions.trim())
        ));
    }
    let chat = ChatRequest {
        model: settings.model.clone(),
        messages: vec![
            ChatMessage {
                role: Role::System,
                content: SYSTEM_PROMPT.into(),
            },
            ChatMessage {
                role: Role::User,
                content: user,
            },
        ],
        temperature: Some(0.3),
    };
    let target = format!("patient/{pid}");
    let answer = settings
        .engine()
        .chat(&chat, Duration::from_secs(settings.timeout_secs))
        .await;
    // Chaque requête est journalisée, qu'elle aboutisse ou non.
    state.with_vault(|vault| {
        vault.audit(ACTOR, AuditAction::AiRequest, Some(&target))?;
        Ok(())
    })?;
    let answer = answer.map_err(ai_error)?;
    let (conclusion, projet) = split_sections(&pseudo.unmask(&answer));
    Ok(Json(DraftResponse {
        conclusion,
        projet,
        model: settings.model,
    }))
}

#[cfg(test)]
mod unit {
    use super::*;

    #[test]
    fn identity_never_reaches_the_model() {
        let patient = json!({
            "nom": "Dupont", "prenom": "Léa", "naissance": "2018-03-15", "ecole": "École Saint-Joseph",
            "medecin": "Dr Martin", "anamnese": "Léa Dupont est suivie par le Dr Martin. LÉA aime lire.",
            "contacts": [{ "nom": "Sophie Dupont", "telephone": "0470 00 00 00" }],
        });
        let bilan = json!({
            "date": "2026-10-25", "type": "initial",
            "observations": "Léa est attentive ; maman (Sophie Dupont) présente.",
            "resultats": [
                { "epreuve": "Répétition de mots", "mesure": "z", "valeur": "-1,8" },
                { "epreuve": "Lexique", "mesure": "note_standard", "valeur": "7", "commentaire": "Léa hésite" },
                { "epreuve": "", "mesure": "z", "valeur": "3" },
            ],
        });
        let (data, p) = build_prompt(&patient, &bilan, &[]);
        let sent = data.to_string();
        for secret in [
            "Dupont",
            "Léa",
            "LÉA",
            "Saint-Joseph",
            "Martin",
            "2018",
            "0470",
            "Sophie",
        ] {
            assert!(!sent.contains(secret), "{secret} transmis : {sent}");
        }
        assert_eq!(data["patient"]["age"], "8 ans 7 mois");
        assert_eq!(data["bilan"]["resultats"].as_array().unwrap().len(), 2);
        assert_eq!(
            data["bilan"]["resultats"][0]["lecture"],
            "z = -1.80, soit 1.80 écart type en dessous de la moyenne"
        );
        assert_eq!(
            data["bilan"]["resultats"][1]["lecture"],
            "z = -1.00, soit 1.00 écart type en dessous de la moyenne"
        );
        assert_eq!(
            reading(0.13),
            "z = 0.13, soit 0.13 écart type au-dessus de la moyenne"
        );
        assert_eq!(p.unmask("[PRENOM_1] progresse."), "Léa progresse.");
    }

    #[test]
    fn answer_is_split_into_conclusion_and_project() {
        let (c, p) = split_sections(
            "### CONCLUSION\nDifficultés en lecture.\n### PROJET\nDeux séances par semaine.",
        );
        assert_eq!(c, "Difficultés en lecture.");
        assert_eq!(p, "Deux séances par semaine.");
        let (c, p) = split_sections("Texte libre");
        assert_eq!((c.as_str(), p.as_str()), ("Texte libre", ""));
    }

    #[test]
    fn score_conversions() {
        assert!((z_of("percentile", "50").unwrap()).abs() < 1e-6);
        assert!((z_of("percentile", "2,5").unwrap() + 1.96).abs() < 1e-3);
        assert!((z_of("note_composite", "85").unwrap() + 1.0).abs() < 1e-9);
        assert_eq!(z_of("aucune", "3"), None);
        assert_eq!(z_of("percentile", "100"), None);
        assert_eq!(
            age_at("2018-03-15", "2026-03-14").as_deref(),
            Some("7 ans 11 mois")
        );
        assert_eq!(age_at("2026-01-01", "2025-01-01"), None);
    }
}
