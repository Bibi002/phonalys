//! Mises à jour : signalées au démarrage, installées seulement si le praticien accepte.
//!
//! - La seule connexion sortante de l'application : la lecture de `latest.json` et de
//!   l'installateur sur la page publique des versions (GitHub). Aucun identifiant,
//!   aucune donnée du cabinet n'est transmis.
//! - L'installateur n'est exécuté qu'après vérification de sa signature (clé publique
//!   inscrite dans `tauri.conf.json`) et de la version signée (`requireSignedVersion`),
//!   qui empêche de faire passer une ancienne version pour une nouvelle.
//! - La vérification est faite en Rust, hors de la page web : l'interface n'a aucun
//!   moyen de déclencher une installation.
//! - Désactivation (cabinet géré par un informaticien) : fichier
//!   `mises-a-jour-desactivees` dans le dossier de données, ou variable
//!   `PHONALYS_NO_UPDATE_CHECK`.

use std::path::Path;
use std::time::Duration;
use tauri::AppHandle;
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
use tauri_plugin_updater::{Update, UpdaterExt};

/// La fenêtre et le serveur embarqué d'abord.
const CHECK_DELAY: Duration = Duration::from_secs(10);
pub const OPT_OUT_FILE: &str = "mises-a-jour-desactivees";
const MAX_NOTES_CHARS: usize = 600;

/// Faux tant que la clé publique n'est pas inscrite, ou si le cabinet a désactivé la
/// vérification.
pub fn enabled(app: &AppHandle, data_dir: Option<&Path>) -> bool {
    if std::env::var_os("PHONALYS_NO_UPDATE_CHECK").is_some() {
        return false;
    }
    if data_dir.is_some_and(|d| d.join(OPT_OUT_FILE).exists()) {
        return false;
    }
    app.config()
        .plugins
        .0
        .get("updater")
        .and_then(|u| u.get("pubkey"))
        .and_then(|k| k.as_str())
        .is_some_and(|k| !k.trim().is_empty())
}

/// Notes de version raccourcies pour la boîte de dialogue.
pub fn excerpt(notes: &str) -> String {
    let notes = notes.trim();
    if notes.chars().count() <= MAX_NOTES_CHARS {
        return notes.to_owned();
    }
    let cut: String = notes.chars().take(MAX_NOTES_CHARS).collect();
    format!("{}…", cut.trim_end())
}

pub fn message(update: &Update) -> String {
    let mut text = format!(
        "La version {} de Phonalys est disponible (vous utilisez la {}).\n\n\
         Elle est signée par l'éditeur ; la signature est vérifiée avant l'installation. \
         Phonalys va se fermer puis redémarrer : terminez d'abord la saisie en cours. \
         Vos données ne sont pas modifiées.",
        update.version, update.current_version
    );
    if let Some(notes) = update.body.as_deref().filter(|n| !n.trim().is_empty()) {
        text.push_str("\n\n");
        text.push_str(&excerpt(notes));
    }
    text
}

/// Vérifie en arrière-plan ; les erreurs (hors ligne, serveur indisponible) sont
/// seulement journalisées : elles ne doivent jamais gêner le travail.
pub fn spawn_check(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(CHECK_DELAY).await;
        let update = match app.updater() {
            Ok(updater) => updater.check().await,
            Err(e) => Err(e),
        };
        match update {
            Ok(Some(update)) => propose(app, update),
            Ok(None) => tracing::info!("Phonalys est à jour"),
            Err(e) => tracing::warn!(error = %e, "vérification des mises à jour impossible"),
        }
    });
}

fn propose(app: AppHandle, update: Update) {
    let handle = app.clone();
    app.dialog()
        .message(message(&update))
        .title("Mise à jour de Phonalys")
        .kind(MessageDialogKind::Info)
        .buttons(MessageDialogButtons::OkCancelCustom(
            "Installer maintenant".into(),
            "Plus tard".into(),
        ))
        .show(move |accepted| {
            if accepted {
                tauri::async_runtime::spawn(install(handle, update));
            }
        });
}

async fn install(app: AppHandle, update: Update) {
    tracing::info!(version = %update.version, "installation de la mise à jour");
    match update.download_and_install(|_, _| {}, || {}).await {
        Ok(()) => app.restart(),
        Err(e) => {
            tracing::error!(error = %e, "échec de la mise à jour");
            app.dialog()
                .message(format!(
                    "La mise à jour n'a pas pu être installée : {e}\n\n\
                     Rien n'a été modifié. Vous pouvez télécharger la nouvelle version sur \
                     https://github.com/Bibi002/phonalys-releases/releases/latest"
                ))
                .title("Mise à jour de Phonalys")
                .kind(MessageDialogKind::Warning)
                .show(|_| {});
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_notes_are_kept_and_long_ones_cut_on_a_character() {
        assert_eq!(excerpt("  Corrections.  "), "Corrections.");
        let long = "é".repeat(MAX_NOTES_CHARS + 50);
        let cut = excerpt(&long);
        assert_eq!(cut.chars().count(), MAX_NOTES_CHARS + 1);
        assert!(cut.ends_with('…'));
    }
}
