//! Évolution entre deux passations : indice de changement fiable (Jacobson & Truax, 1991).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ReliableChange {
    pub rci: f64,
    /// |RCI| > 1.96 : changement significatif au seuil de 5 %.
    pub significant: bool,
}

/// Calcule le RCI entre deux scores bruts sur la même échelle.
/// `sd` : écart-type normatif ; `reliability` : fidélité test-retest dans ]0, 1[.
/// Le signe suit le score brut : interpréter selon le sens de l'épreuve.
pub fn reliable_change(
    before: f64,
    after: f64,
    sd: f64,
    reliability: f64,
) -> Option<ReliableChange> {
    if !(sd > 0.0 && reliability > 0.0 && reliability < 1.0)
        || !before.is_finite()
        || !after.is_finite()
    {
        return None;
    }
    let s_diff = (2.0 * (sd * (1.0 - reliability).sqrt()).powi(2)).sqrt();
    let rci = (after - before) / s_diff;
    Some(ReliableChange {
        rci,
        significant: rci.abs() > 1.96,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_value() {
        // SEM = 10·√0.2 ≈ 4.472 ; Sdiff ≈ 6.325 ; RCI = 15 / 6.325 ≈ 2.372
        let r = reliable_change(40.0, 55.0, 10.0, 0.8).unwrap();
        assert!((r.rci - 2.371_708).abs() < 1e-5);
        assert!(r.significant);
        assert!(!reliable_change(40.0, 45.0, 10.0, 0.8).unwrap().significant);
    }

    #[test]
    fn rejects_invalid_parameters() {
        assert!(reliable_change(1.0, 2.0, 0.0, 0.8).is_none());
        assert!(reliable_change(1.0, 2.0, 1.0, 1.0).is_none());
        assert!(reliable_change(f64::NAN, 2.0, 1.0, 0.5).is_none());
    }
}
