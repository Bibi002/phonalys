//! Tables de normes et cotation d'un score brut.

use crate::age::Age;
use crate::distribution::{normal_cdf, normal_quantile};
use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum NormError {
    #[error("la table « {0} » ne contient aucune tranche d'âge")]
    Empty(String),
    #[error("tranche {index} : bornes d'âge invalides")]
    InvalidAgeRange { index: usize },
    #[error("tranches {a} et {b} : les âges se chevauchent")]
    OverlappingBands { a: usize, b: usize },
    #[error("tranche {index} : moyenne ou écart-type invalide")]
    InvalidMoments { index: usize },
    #[error("tranche {index} : fidélité hors de ]0, 1[")]
    InvalidReliability { index: usize },
    #[error("tranche {index} : table de percentiles invalide")]
    InvalidPercentiles { index: usize },
    #[error("aucune norme pour l'âge {0}")]
    NoBandForAge(Age),
    #[error("score brut invalide")]
    InvalidRawScore,
}

/// Sens de l'échelle : un score élevé est-il une bonne performance ?
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    /// Réponses correctes, empan…
    HigherIsBetter,
    /// Erreurs, temps de lecture…
    LowerIsBetter,
}

/// Normes pour une tranche d'âge (bornes incluses, en mois).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NormBand {
    pub min_months: u32,
    pub max_months: u32,
    pub mean: f64,
    pub sd: f64,
    /// Effectif de l'échantillon normatif.
    #[serde(default)]
    pub n: Option<u32>,
    /// Coefficient de fidélité (test-retest ou alpha), pour l'intervalle de confiance.
    #[serde(default)]
    pub reliability: Option<f64>,
    /// Percentiles empiriques `(score brut, percentile)`, triés par score brut croissant.
    /// Prioritaires sur l'approximation normale quand ils sont fournis.
    #[serde(default)]
    pub percentiles: Vec<(f64, f64)>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NormTable {
    pub id: String,
    pub label: String,
    pub direction: Direction,
    pub bands: Vec<NormBand>,
}

/// Seuils d'interprétation, exprimés en écarts-types (z orienté : négatif = difficulté).
/// Configurables, car ils varient selon les épreuves et les critères en vigueur.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct InterpretationThresholds {
    pub deficit: f64,
    pub fragile: f64,
    pub low_average: f64,
    pub above_average: f64,
}

impl Default for InterpretationThresholds {
    fn default() -> Self {
        Self {
            deficit: -2.0,
            fragile: -1.5,
            low_average: -1.0,
            above_average: 1.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Zone {
    Deficit,
    Fragile,
    LowAverage,
    Average,
    AboveAverage,
}

impl Zone {
    pub fn label_fr(&self) -> &'static str {
        match self {
            Zone::Deficit => "Déficitaire",
            Zone::Fragile => "Fragile",
            Zone::LowAverage => "Moyenne faible",
            Zone::Average => "Dans la moyenne",
            Zone::AboveAverage => "Supérieure à la moyenne",
        }
    }
}

/// Méthode utilisée pour obtenir le percentile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PercentileMethod {
    Empirical,
    NormalApproximation,
}

/// Score brut à coter.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Score {
    pub raw: f64,
    pub age: Age,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScoredResult {
    pub raw: f64,
    /// Score z orienté : négatif = performance inférieure à la norme, quel que soit le sens.
    pub z: f64,
    pub percentile: f64,
    pub percentile_method: PercentileMethod,
    /// Note standard (moyenne 100, écart-type 15).
    pub standard_score: f64,
    /// Note d'échelle (moyenne 10, écart-type 3), bornée à [1, 19].
    pub scaled_score: u8,
    /// Note T (moyenne 50, écart-type 10).
    pub t_score: f64,
    pub zone: Zone,
    /// Intervalle de confiance à 95 % du z, si la fidélité est connue.
    pub z_ci95: Option<(f64, f64)>,
    pub band_min_months: u32,
    pub band_max_months: u32,
}

const PERCENTILE_FLOOR: f64 = 0.1;
const PERCENTILE_CEIL: f64 = 99.9;

impl NormTable {
    /// Vérifie la cohérence de la table. À appeler à chaque import.
    pub fn validate(&self) -> Result<(), NormError> {
        if self.bands.is_empty() {
            return Err(NormError::Empty(self.id.clone()));
        }
        for (index, band) in self.bands.iter().enumerate() {
            if band.min_months > band.max_months {
                return Err(NormError::InvalidAgeRange { index });
            }
            if !band.mean.is_finite() || !band.sd.is_finite() || band.sd <= 0.0 {
                return Err(NormError::InvalidMoments { index });
            }
            if let Some(r) = band.reliability {
                if !(r > 0.0 && r < 1.0) {
                    return Err(NormError::InvalidReliability { index });
                }
            }
            let ordered = band
                .percentiles
                .windows(2)
                .all(|w| w[0].0 < w[1].0 && w[0].1 <= w[1].1);
            let in_range = band
                .percentiles
                .iter()
                .all(|(raw, p)| raw.is_finite() && (0.0..=100.0).contains(p));
            let monotonic = match self.direction {
                Direction::HigherIsBetter => ordered,
                Direction::LowerIsBetter => band
                    .percentiles
                    .windows(2)
                    .all(|w| w[0].0 < w[1].0 && w[0].1 >= w[1].1),
            };
            if !in_range || !monotonic || band.percentiles.len() == 1 {
                return Err(NormError::InvalidPercentiles { index });
            }
        }
        for a in 0..self.bands.len() {
            for b in (a + 1)..self.bands.len() {
                let (x, y) = (&self.bands[a], &self.bands[b]);
                if x.min_months <= y.max_months && y.min_months <= x.max_months {
                    return Err(NormError::OverlappingBands { a, b });
                }
            }
        }
        Ok(())
    }

    pub fn band_for(&self, age: Age) -> Option<&NormBand> {
        self.bands
            .iter()
            .find(|b| (b.min_months..=b.max_months).contains(&age.total_months))
    }

    /// Cote un score brut.
    pub fn score(
        &self,
        score: Score,
        thresholds: &InterpretationThresholds,
    ) -> Result<ScoredResult, NormError> {
        if !score.raw.is_finite() {
            return Err(NormError::InvalidRawScore);
        }
        let band = self
            .band_for(score.age)
            .ok_or(NormError::NoBandForAge(score.age))?;

        let deviation = (score.raw - band.mean) / band.sd;
        let z_normal = match self.direction {
            Direction::HigherIsBetter => deviation,
            Direction::LowerIsBetter => -deviation,
        };

        let (percentile, method, z) = match interpolate(&band.percentiles, score.raw) {
            Some(p) => {
                let p = p.clamp(PERCENTILE_FLOOR, PERCENTILE_CEIL);
                let z = normal_quantile(p / 100.0).unwrap_or(z_normal);
                (p, PercentileMethod::Empirical, z)
            }
            None => (
                (normal_cdf(z_normal) * 100.0).clamp(PERCENTILE_FLOOR, PERCENTILE_CEIL),
                PercentileMethod::NormalApproximation,
                z_normal,
            ),
        };

        let z_ci95 = band.reliability.map(|r| {
            let margin = 1.96 * (1.0 - r).sqrt();
            (z - margin, z + margin)
        });

        Ok(ScoredResult {
            raw: score.raw,
            z,
            percentile,
            percentile_method: method,
            standard_score: 100.0 + 15.0 * z,
            scaled_score: (10.0 + 3.0 * z).round().clamp(1.0, 19.0) as u8,
            t_score: 50.0 + 10.0 * z,
            zone: zone_for(z, thresholds),
            z_ci95,
            band_min_months: band.min_months,
            band_max_months: band.max_months,
        })
    }
}

/// Zone d'interprétation d'un z orienté.
pub fn zone_for(z: f64, t: &InterpretationThresholds) -> Zone {
    if z <= t.deficit {
        Zone::Deficit
    } else if z <= t.fragile {
        Zone::Fragile
    } else if z <= t.low_average {
        Zone::LowAverage
    } else if z < t.above_average {
        Zone::Average
    } else {
        Zone::AboveAverage
    }
}

/// Interpolation linéaire du percentile, bornée aux extrémités de la table.
fn interpolate(table: &[(f64, f64)], raw: f64) -> Option<f64> {
    let (first, last) = (table.first()?, table.last()?);
    if raw <= first.0 {
        return Some(first.1);
    }
    if raw >= last.0 {
        return Some(last.1);
    }
    table.windows(2).find_map(|w| {
        let ((x0, y0), (x1, y1)) = (w[0], w[1]);
        (raw >= x0 && raw <= x1).then(|| y0 + (y1 - y0) * (raw - x0) / (x1 - x0))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn band(min: u32, max: u32, mean: f64, sd: f64) -> NormBand {
        NormBand {
            min_months: min,
            max_months: max,
            mean,
            sd,
            n: Some(100),
            reliability: None,
            percentiles: vec![],
        }
    }

    fn table(direction: Direction, bands: Vec<NormBand>) -> NormTable {
        NormTable {
            id: "t".into(),
            label: "Test".into(),
            direction,
            bands,
        }
    }

    fn age(m: u32) -> Age {
        Age { total_months: m }
    }

    #[test]
    fn normal_scoring_higher_is_better() {
        let t = table(Direction::HigherIsBetter, vec![band(72, 83, 20.0, 4.0)]);
        let r = t
            .score(
                Score {
                    raw: 12.0,
                    age: age(75),
                },
                &Default::default(),
            )
            .unwrap();
        assert!((r.z + 2.0).abs() < 1e-12);
        assert_eq!(r.zone, Zone::Deficit);
        assert!((r.standard_score - 70.0).abs() < 1e-9);
        assert_eq!(r.scaled_score, 4);
        assert!((r.percentile - 2.275).abs() < 1e-3);
    }

    #[test]
    fn lower_is_better_flips_sign() {
        let t = table(Direction::LowerIsBetter, vec![band(72, 83, 5.0, 2.0)]);
        let r = t
            .score(
                Score {
                    raw: 9.0,
                    age: age(80),
                },
                &Default::default(),
            )
            .unwrap();
        assert!((r.z + 2.0).abs() < 1e-12, "beaucoup d'erreurs = z négatif");
    }

    #[test]
    fn empirical_percentiles_take_priority() {
        let mut b = band(72, 83, 20.0, 4.0);
        b.percentiles = vec![(10.0, 5.0), (20.0, 50.0), (30.0, 95.0)];
        let t = table(Direction::HigherIsBetter, vec![b]);
        let r = t
            .score(
                Score {
                    raw: 15.0,
                    age: age(72),
                },
                &Default::default(),
            )
            .unwrap();
        assert_eq!(r.percentile_method, PercentileMethod::Empirical);
        assert!((r.percentile - 27.5).abs() < 1e-9);
    }

    #[test]
    fn confidence_interval_uses_reliability() {
        let mut b = band(0, 200, 0.0, 1.0);
        b.reliability = Some(0.91);
        let r = table(Direction::HigherIsBetter, vec![b])
            .score(
                Score {
                    raw: 0.0,
                    age: age(100),
                },
                &Default::default(),
            )
            .unwrap();
        let (lo, hi) = r.z_ci95.unwrap();
        assert!((hi - 0.588).abs() < 1e-9 && (lo + 0.588).abs() < 1e-9);
    }

    #[test]
    fn missing_band_and_bad_input() {
        let t = table(Direction::HigherIsBetter, vec![band(72, 83, 20.0, 4.0)]);
        assert_eq!(
            t.score(
                Score {
                    raw: 1.0,
                    age: age(84)
                },
                &Default::default()
            ),
            Err(NormError::NoBandForAge(age(84)))
        );
        assert_eq!(
            t.score(
                Score {
                    raw: f64::NAN,
                    age: age(72)
                },
                &Default::default()
            ),
            Err(NormError::InvalidRawScore)
        );
    }

    #[test]
    fn validation_catches_errors() {
        assert!(table(Direction::HigherIsBetter, vec![]).validate().is_err());
        assert_eq!(
            table(
                Direction::HigherIsBetter,
                vec![band(72, 83, 1.0, 1.0), band(83, 95, 1.0, 1.0)]
            )
            .validate(),
            Err(NormError::OverlappingBands { a: 0, b: 1 })
        );
        assert_eq!(
            table(Direction::HigherIsBetter, vec![band(72, 83, 1.0, 0.0)]).validate(),
            Err(NormError::InvalidMoments { index: 0 })
        );
        let mut b = band(72, 83, 1.0, 1.0);
        b.percentiles = vec![(1.0, 50.0), (2.0, 40.0)];
        assert_eq!(
            table(Direction::HigherIsBetter, vec![b]).validate(),
            Err(NormError::InvalidPercentiles { index: 0 })
        );
        assert!(table(
            Direction::HigherIsBetter,
            vec![band(72, 83, 1.0, 1.0), band(84, 95, 1.0, 1.0)]
        )
        .validate()
        .is_ok());
    }

    #[test]
    fn json_roundtrip() {
        let t = table(Direction::LowerIsBetter, vec![band(72, 83, 5.0, 2.0)]);
        let json = serde_json::to_string(&t).unwrap();
        assert!(json.contains("lower_is_better"));
        assert_eq!(serde_json::from_str::<NormTable>(&json).unwrap(), t);
    }

    #[test]
    fn zone_boundaries() {
        let t = InterpretationThresholds::default();
        assert_eq!(zone_for(-2.0, &t), Zone::Deficit);
        assert_eq!(zone_for(-1.99, &t), Zone::Fragile);
        assert_eq!(zone_for(-1.5, &t), Zone::Fragile);
        assert_eq!(zone_for(-1.0, &t), Zone::LowAverage);
        assert_eq!(zone_for(0.0, &t), Zone::Average);
        assert_eq!(zone_for(1.0, &t), Zone::AboveAverage);
    }

    proptest! {
        #[test]
        fn better_raw_never_scores_worse(mean in -50.0f64..50.0, sd in 0.1f64..20.0,
                                         a in -100.0f64..100.0, b in -100.0f64..100.0) {
            let t = table(Direction::HigherIsBetter, vec![band(0, 300, mean, sd)]);
            let th = InterpretationThresholds::default();
            let (lo, hi) = if a < b { (a, b) } else { (b, a) };
            let rl = t.score(Score { raw: lo, age: age(10) }, &th).unwrap();
            let rh = t.score(Score { raw: hi, age: age(10) }, &th).unwrap();
            prop_assert!(rl.z <= rh.z);
            prop_assert!(rl.percentile <= rh.percentile);
            prop_assert!((PERCENTILE_FLOOR..=PERCENTILE_CEIL).contains(&rl.percentile));
            prop_assert!((1..=19).contains(&rl.scaled_score));
        }
    }
}
