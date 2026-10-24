//! Calcul de l'âge chronologique à la date de passation.

use serde::{Deserialize, Serialize};
use time::Date;

/// Âge exprimé en années et mois révolus.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Age {
    pub total_months: u32,
}

impl Age {
    pub fn years(&self) -> u32 {
        self.total_months / 12
    }

    pub fn months(&self) -> u32 {
        self.total_months % 12
    }
}

impl std::fmt::Display for Age {
    /// Notation clinique usuelle : `8;03` pour 8 ans et 3 mois.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{};{:02}", self.years(), self.months())
    }
}

/// Âge en mois révolus entre la naissance et la passation.
/// Renvoie `None` si la passation précède la naissance.
pub fn age_in_months(birth: Date, test: Date) -> Option<Age> {
    if test < birth {
        return None;
    }
    let mut months =
        (test.year() - birth.year()) * 12 + (test.month() as i32 - birth.month() as i32);
    if test.day() < birth.day() {
        months -= 1;
    }
    u32::try_from(months)
        .ok()
        .map(|total_months| Age { total_months })
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::date;

    #[test]
    fn counts_completed_months_only() {
        assert_eq!(
            age_in_months(date!(2018 - 03 - 15), date!(2026 - 06 - 14))
                .unwrap()
                .to_string(),
            "8;02"
        );
        assert_eq!(
            age_in_months(date!(2018 - 03 - 15), date!(2026 - 06 - 15))
                .unwrap()
                .to_string(),
            "8;03"
        );
    }

    #[test]
    fn same_day_is_zero() {
        assert_eq!(
            age_in_months(date!(2020 - 01 - 31), date!(2020 - 01 - 31))
                .unwrap()
                .total_months,
            0
        );
    }

    #[test]
    fn end_of_month_birthdays() {
        // Né le 31 janvier : le 29 février n'est pas encore un mois révolu.
        assert_eq!(
            age_in_months(date!(2020 - 01 - 31), date!(2020 - 02 - 29))
                .unwrap()
                .total_months,
            0
        );
        assert_eq!(
            age_in_months(date!(2020 - 01 - 31), date!(2020 - 03 - 31))
                .unwrap()
                .total_months,
            2
        );
    }

    #[test]
    fn test_before_birth_is_rejected() {
        assert!(age_in_months(date!(2020 - 01 - 01), date!(2019 - 12 - 31)).is_none());
    }
}
