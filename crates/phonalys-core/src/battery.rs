//! Définition déclarative des batteries et des épreuves.
//!
//! Une batterie est un document JSON versionné : le moteur ne contient aucun contenu
//! de test. Le praticien peut ainsi encoder les épreuves dont il détient la licence,
//! et Phonalys livre ses propres épreuves originales dans ce même format.

use phonalys_psychometrics::{
    Age, InterpretationThresholds, NormError, NormTable, Score, ScoredResult,
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// Domaines cliniques couverts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClinicalDomain {
    OralPhonology,
    OralLexicon,
    OralMorphosyntax,
    OralPragmatics,
    Metaphonology,
    Reading,
    Spelling,
    WrittenProduction,
    Memory,
    Attention,
    ExecutiveFunctions,
    Fluency,
    Voice,
    Oromyofunctional,
    Numeracy,
}

/// Provenance du contenu, affichée dans les rapports pour la traçabilité.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ContentOrigin {
    /// Épreuve originale Phonalys.
    Phonalys,
    /// Épreuve tierce encodée par le praticien à partir de son matériel sous licence.
    LicensedByPractitioner {
        publisher: String,
        reference: String,
    },
    /// Épreuve maison du praticien (non normée sauf normes locales).
    PractitionerCustom,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ScoringRule {
    /// Somme des points des items (0/1 ou pondérés).
    SumOfItems,
    /// Nombre d'erreurs.
    ErrorCount,
    /// Temps en secondes.
    TimeSeconds,
    /// Efficience : réponses correctes par minute.
    CorrectPerMinute,
    /// Score brut saisi directement.
    ManualRaw,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Item {
    pub id: String,
    pub prompt: String,
    #[serde(default)]
    pub expected: Option<String>,
    #[serde(default = "one")]
    pub max_points: f64,
}

fn one() -> f64 {
    1.0
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Subtest {
    pub id: String,
    pub name: String,
    pub domain: ClinicalDomain,
    pub instructions: String,
    pub min_age_months: u32,
    pub max_age_months: u32,
    pub scoring: ScoringRule,
    #[serde(default)]
    pub items: Vec<Item>,
    /// Référence à une table de [`Battery::norms`].
    #[serde(default)]
    pub norm_table: Option<String>,
    #[serde(default)]
    pub thresholds: Option<InterpretationThresholds>,
    /// Durée indicative, en minutes.
    #[serde(default)]
    pub duration_minutes: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Battery {
    pub id: String,
    pub name: String,
    pub version: String,
    pub origin: ContentOrigin,
    pub subtests: Vec<Subtest>,
    #[serde(default)]
    pub norms: Vec<NormTable>,
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum BatteryError {
    #[error("identifiant en double : {0}")]
    DuplicateId(String),
    #[error("l'épreuve {subtest} référence une table de normes inconnue : {table}")]
    UnknownNormTable { subtest: String, table: String },
    #[error("épreuve {0} : tranche d'âge invalide")]
    InvalidAgeRange(String),
    #[error("épreuve inconnue : {0}")]
    UnknownSubtest(String),
    #[error("l'épreuve {0} n'est pas normée")]
    NotNormed(String),
    #[error("normes invalides : {0}")]
    Norms(#[from] NormError),
}

impl Battery {
    /// Valide la structure complète d'une batterie importée.
    pub fn validate(&self) -> Result<(), BatteryError> {
        let mut ids = HashSet::new();
        for table in &self.norms {
            table.validate()?;
            if !ids.insert(format!("norm:{}", table.id)) {
                return Err(BatteryError::DuplicateId(table.id.clone()));
            }
        }
        let norm_ids: HashSet<_> = self.norms.iter().map(|n| n.id.as_str()).collect();
        for subtest in &self.subtests {
            if !ids.insert(format!("subtest:{}", subtest.id)) {
                return Err(BatteryError::DuplicateId(subtest.id.clone()));
            }
            if subtest.min_age_months > subtest.max_age_months {
                return Err(BatteryError::InvalidAgeRange(subtest.id.clone()));
            }
            if let Some(table) = &subtest.norm_table {
                if !norm_ids.contains(table.as_str()) {
                    return Err(BatteryError::UnknownNormTable {
                        subtest: subtest.id.clone(),
                        table: table.clone(),
                    });
                }
            }
            let mut item_ids = HashSet::new();
            for item in &subtest.items {
                if !item_ids.insert(item.id.as_str()) {
                    return Err(BatteryError::DuplicateId(format!(
                        "{}/{}",
                        subtest.id, item.id
                    )));
                }
            }
        }
        Ok(())
    }

    /// Épreuves adaptées à l'âge du patient.
    pub fn subtests_for_age(&self, age: Age) -> impl Iterator<Item = &Subtest> {
        self.subtests
            .iter()
            .filter(move |s| (s.min_age_months..=s.max_age_months).contains(&age.total_months))
    }

    /// Cote un score brut pour une épreuve.
    pub fn score(
        &self,
        subtest_id: &str,
        raw: f64,
        age: Age,
    ) -> Result<ScoredResult, BatteryError> {
        let subtest = self
            .subtests
            .iter()
            .find(|s| s.id == subtest_id)
            .ok_or_else(|| BatteryError::UnknownSubtest(subtest_id.to_owned()))?;
        let table_id = subtest
            .norm_table
            .as_deref()
            .ok_or_else(|| BatteryError::NotNormed(subtest_id.to_owned()))?;
        let tables: HashMap<_, _> = self.norms.iter().map(|n| (n.id.as_str(), n)).collect();
        let table = tables
            .get(table_id)
            .ok_or_else(|| BatteryError::UnknownNormTable {
                subtest: subtest_id.to_owned(),
                table: table_id.to_owned(),
            })?;
        Ok(table.score(Score { raw, age }, &subtest.thresholds.unwrap_or_default())?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use phonalys_psychometrics::Zone;

    const SAMPLE: &str = r#"{
      "id": "phonalys-demo",
      "name": "Démo Phonalys",
      "version": "0.1.0",
      "origin": { "kind": "phonalys" },
      "subtests": [{
        "id": "rep-pseudomots",
        "name": "Répétition de pseudo-mots",
        "domain": "oral_phonology",
        "instructions": "Répète exactement ce que tu entends.",
        "min_age_months": 72, "max_age_months": 119,
        "scoring": { "kind": "sum_of_items" },
        "items": [{ "id": "i1", "prompt": "[audio]" }, { "id": "i2", "prompt": "[audio]" }],
        "norm_table": "rep-pm"
      }],
      "norms": [{
        "id": "rep-pm", "label": "Répétition pseudo-mots", "direction": "higher_is_better",
        "bands": [{ "min_months": 72, "max_months": 95, "mean": 18, "sd": 3 },
                  { "min_months": 96, "max_months": 119, "mean": 21, "sd": 2.5 }]
      }]
    }"#;

    fn sample() -> Battery {
        serde_json::from_str(SAMPLE).unwrap()
    }

    #[test]
    fn sample_battery_is_valid_and_scores() {
        let b = sample();
        b.validate().unwrap();
        let r = b
            .score("rep-pseudomots", 12.0, Age { total_months: 80 })
            .unwrap();
        assert_eq!(r.zone, Zone::Deficit);
        assert_eq!(b.subtests_for_age(Age { total_months: 130 }).count(), 0);
    }

    #[test]
    fn unknown_norm_reference_is_rejected() {
        let mut b = sample();
        b.subtests[0].norm_table = Some("absente".into());
        assert!(matches!(
            b.validate(),
            Err(BatteryError::UnknownNormTable { .. })
        ));
    }

    #[test]
    fn duplicate_items_are_rejected() {
        let mut b = sample();
        b.subtests[0].items[1].id = "i1".into();
        assert!(matches!(b.validate(), Err(BatteryError::DuplicateId(_))));
    }

    #[test]
    fn unnormed_subtest_cannot_be_scored() {
        let mut b = sample();
        b.subtests[0].norm_table = None;
        assert_eq!(
            b.score("rep-pseudomots", 1.0, Age { total_months: 80 }),
            Err(BatteryError::NotNormed("rep-pseudomots".into()))
        );
    }
}
