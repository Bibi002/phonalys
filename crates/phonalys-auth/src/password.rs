//! Politique de phrase de passe : longueur minimale et robustesse estimée hors ligne
//! (zxcvbn : dictionnaires, motifs clavier, dates, répétitions).

/// Longueur minimale, en caractères.
pub const MIN_CHARS: usize = 14;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum PassphraseError {
    #[error("la phrase de passe doit compter au moins {MIN_CHARS} caractères")]
    TooShort,
    #[error("phrase de passe trop prévisible{}", .0.as_deref().map(|s| format!(" : {s}")).unwrap_or_default())]
    TooWeak(Option<String>),
    #[error("la phrase de passe ne doit pas contenir vos informations personnelles")]
    ContainsPersonalInfo,
}

/// Vérifie une phrase de passe. `personal` : nom, prénom, e-mail, n° INAMI… à exclure.
pub fn check_passphrase(passphrase: &str, personal: &[&str]) -> Result<(), PassphraseError> {
    if passphrase.chars().count() < MIN_CHARS {
        return Err(PassphraseError::TooShort);
    }
    let lower = passphrase.to_lowercase();
    if personal
        .iter()
        .map(|p| p.trim().to_lowercase())
        .any(|p| p.chars().count() >= 3 && lower.contains(&p))
    {
        return Err(PassphraseError::ContainsPersonalInfo);
    }
    let mut inputs: Vec<&str> = personal.to_vec();
    inputs.extend([
        "phonalys",
        "logopede",
        "logopedie",
        "logopède",
        "logopédie",
        "bilan",
    ]);
    let estimate = zxcvbn::zxcvbn(passphrase, &inputs);
    if (estimate.score() as u8) < 4 {
        let hint = estimate
            .feedback()
            .and_then(|f| f.warning())
            .map(|w| w.to_string());
        return Err(PassphraseError::TooWeak(hint));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_short_and_weak() {
        assert_eq!(
            check_passphrase("Court1!", &[]),
            Err(PassphraseError::TooShort)
        );
        assert!(matches!(
            check_passphrase("motdepasse123456", &[]),
            Err(PassphraseError::TooWeak(_))
        ));
        assert!(matches!(
            check_passphrase("qwertyuiopasdfgh", &[]),
            Err(PassphraseError::TooWeak(_))
        ));
        assert!(matches!(
            check_passphrase("phonalysphonalys", &[]),
            Err(PassphraseError::TooWeak(_))
        ));
    }

    #[test]
    fn rejects_personal_info() {
        assert_eq!(
            check_passphrase("Dupont-girafe-violon-nuage-42", &["Marie Dupont", "Dupont"]),
            Err(PassphraseError::ContainsPersonalInfo)
        );
    }

    #[test]
    fn accepts_random_word_passphrase() {
        assert_eq!(
            check_passphrase("girafe tambour lumière escalier", &[]),
            Ok(())
        );
    }
}
