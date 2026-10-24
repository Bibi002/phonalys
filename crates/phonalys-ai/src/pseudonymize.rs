//! Pseudonymisation des invites envoyées au modèle.
//!
//! Même un modèle local n'a pas besoin du nom de l'enfant pour rédiger un bilan :
//! les identifiants sont remplacés par des jetons avant l'envoi, puis restitués dans
//! la réponse. Le modèle ne voit jamais l'identité, ce qui protège aussi les journaux
//! et le cache du serveur d'inférence.

use std::collections::BTreeMap;

#[derive(Debug, Default, Clone)]
pub struct Pseudonymizer {
    /// valeur réelle → jeton
    forward: BTreeMap<String, String>,
}

impl Pseudonymizer {
    pub fn new() -> Self {
        Self::default()
    }

    /// Enregistre un identifiant (nom, prénom, école, adresse…) sous une catégorie.
    pub fn register(&mut self, category: &str, value: &str) -> String {
        let value = value.trim();
        if let Some(token) = self.forward.get(value) {
            return token.clone();
        }
        let index = self
            .forward
            .values()
            .filter(|t| t.starts_with(&format!("[{category}_")))
            .count()
            + 1;
        let token = format!("[{category}_{index}]");
        if !value.is_empty() {
            self.forward.insert(value.to_owned(), token.clone());
        }
        token
    }

    /// Remplace les identifiants par leurs jetons. Les valeurs les plus longues passent
    /// en premier pour qu'un prénom ne masque pas un nom complet.
    pub fn mask(&self, text: &str) -> String {
        let mut pairs: Vec<_> = self.forward.iter().collect();
        pairs.sort_by_key(|(value, _)| std::cmp::Reverse(value.chars().count()));
        pairs.iter().fold(text.to_owned(), |acc, (value, token)| {
            replace_ci(&acc, value, token)
        })
    }

    /// Restitue les identifiants dans la réponse du modèle.
    pub fn unmask(&self, text: &str) -> String {
        self.forward
            .iter()
            .fold(text.to_owned(), |acc, (value, token)| {
                acc.replace(token.as_str(), value)
            })
    }
}

/// Remplacement insensible à la casse (comparaison Unicode simple).
fn replace_ci(haystack: &str, needle: &str, replacement: &str) -> String {
    let lower_hay = haystack.to_lowercase();
    let lower_needle = needle.to_lowercase();
    // La mise en minuscules peut changer la longueur en octets : on retombe alors sur
    // un remplacement exact pour ne jamais découper un caractère.
    if lower_hay.len() != haystack.len() || lower_needle.len() != needle.len() || needle.is_empty()
    {
        return haystack.replace(needle, replacement);
    }
    let mut out = String::with_capacity(haystack.len());
    let mut last = 0;
    for (start, _) in lower_hay.match_indices(&lower_needle) {
        if start < last {
            continue;
        }
        out.push_str(&haystack[last..start]);
        out.push_str(replacement);
        last = start + needle.len();
    }
    out.push_str(&haystack[last..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masks_and_restores() {
        let mut p = Pseudonymizer::new();
        p.register("PATIENT", "Léa Dupont");
        p.register("PRENOM", "Léa");
        p.register("ECOLE", "École Saint-Joseph");
        let masked =
            p.mask("Léa Dupont (dite LÉA) est en 3e à l'École Saint-Joseph. Léa lit lentement.");
        assert!(
            !masked.contains("Léa") && !masked.contains("Dupont") && !masked.contains("Joseph"),
            "{masked}"
        );
        assert!(masked.contains("[PATIENT_1]") && masked.contains("[ECOLE_1]"));
        assert_eq!(p.unmask("[PRENOM_1] progresse."), "Léa progresse.");
    }

    #[test]
    fn same_value_same_token() {
        let mut p = Pseudonymizer::new();
        assert_eq!(p.register("X", "Tom"), p.register("X", " Tom "));
        assert_eq!(p.register("X", "Zoé"), "[X_2]");
    }

    #[test]
    fn case_insensitive_ascii() {
        let mut p = Pseudonymizer::new();
        p.register("NOM", "Martin");
        assert_eq!(p.mask("MARTIN et martin"), "[NOM_1] et [NOM_1]");
    }
}
