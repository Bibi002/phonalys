//! Moteur psychométrique de Phonalys.
//!
//! Convertit un score brut en résultats normés à partir d'une table de normes
//! (moyenne et écart-type par tranche d'âge, ou percentiles empiriques), puis
//! interprète l'écart à la norme et l'évolution entre deux passations.
//!
//! Ce module ne contient **aucune** norme : elles proviennent soit des manuels dont
//! le praticien détient la licence, soit des études normatives de Phonalys.

pub mod age;
pub mod distribution;
pub mod norms;
pub mod progress;

pub use age::{age_in_months, Age};
pub use norms::{
    zone_for, Direction, InterpretationThresholds, NormBand, NormError, NormTable,
    PercentileMethod, Score, ScoredResult, Zone,
};
pub use progress::{reliable_change, ReliableChange};
