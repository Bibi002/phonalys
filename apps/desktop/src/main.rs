//! Application de bureau Phonalys.
//!
//! Le serveur Phonalys tourne **dans** l'application, à l'écoute de la seule boucle
//! locale (`127.0.0.1`, port aléatoire) : aucun autre appareil ne peut le joindre. La
//! fenêtre affiche l'interface web servie par ce serveur ; elle ne peut naviguer vers
//! aucune autre adresse et ne dispose d'aucun accès aux commandes natives de Tauri.
//!
//! Pour connecter des téléphones, le praticien lance le serveur autonome
//! (`phonalys-server`) en TLS sur le réseau du cabinet.

// Pas de console sous Windows en version publiée.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use phonalys_auth::SessionConfig;
use phonalys_crypto::KdfParams;
use phonalys_server::{app, spawn_auto_lock, AppState, Config};
use phonalys_webauthn::RelyingParty;
use std::path::PathBuf;
use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};
use tracing_subscriber::EnvFilter;
use url::Url;

type BoxError = Box<dyn std::error::Error>;

/// Même dossier que le serveur autonome : un seul coffre par praticien.
fn data_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("PHONALYS_DATA_DIR") {
        return Some(PathBuf::from(dir));
    }
    let base = if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA").map(PathBuf::from)
    } else {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
    };
    base.map(|b| b.join("Phonalys"))
}

fn create_private_dir(dir: &PathBuf) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

/// Interface web : ressources de l'installateur, sinon `web/dist` du dépôt (développement).
fn web_dir(app: &tauri::App) -> Option<PathBuf> {
    let bundled = app.path().resource_dir().ok().map(|d| d.join("web"));
    let dev = Some(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../web/dist"));
    [bundled, dev]
        .into_iter()
        .flatten()
        .find(|d| d.join("index.html").is_file())
}

/// Démarre le serveur embarqué et renvoie l'adresse de la page à ouvrir.
async fn start_server(web_dir: Option<PathBuf>) -> Result<Url, BoxError> {
    let data_dir = data_dir().ok_or("dossier de données introuvable")?;
    create_private_dir(&data_dir)?;

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();
    // `localhost` (et non 127.0.0.1) : WebAuthn exige un nom de domaine, et les
    // navigateurs considèrent http://localhost comme un contexte sécurisé.
    let origin = format!("http://localhost:{port}");

    let (state, setup_token) = AppState::new(Config {
        vault_path: data_dir.join("cabinet.phonalys-vault"),
        kdf: KdfParams::default(),
        sessions: SessionConfig::default(),
        relying_party: RelyingParty {
            id: "localhost".into(),
            name: "Phonalys".into(),
            origins: vec![origin.clone()],
        },
        web_dir,
        backup_dir: data_dir.join("sauvegardes"),
        // Boucle locale sans TLS : rien à installer sur un autre appareil.
        authority: None,
    });
    spawn_auto_lock(state.clone());
    let router = app(state);
    tauri::async_runtime::spawn(async move {
        if let Err(e) = axum::serve(listener, router).await {
            tracing::error!(error = %e, "arrêt du serveur embarqué");
        }
    });

    let mut url = Url::parse(&origin)?;
    // Le jeton d'installation passe dans le fragment, qui n'est jamais transmis sur
    // le réseau ; l'interface l'efface dès sa lecture.
    if let Some(token) = setup_token {
        url.set_fragment(Some(&format!("setup={token}")));
    }
    tracing::info!(%port, "serveur embarqué à l'écoute de la boucle locale");
    Ok(url)
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let result = tauri::Builder::default()
        .setup(|app| {
            let web = web_dir(app);
            if web.is_none() {
                tracing::error!("interface web introuvable : compilez `web/` (npm run build)");
            }
            let url = tauri::async_runtime::block_on(start_server(web))?;
            let allowed = url.origin();

            WebviewWindowBuilder::new(app, "main", WebviewUrl::External(url))
                .title("Phonalys")
                .inner_size(1200.0, 820.0)
                .min_inner_size(420.0, 600.0)
                // Aucune navigation hors du serveur embarqué (liens, redirections).
                .on_navigation(move |target| target.origin() == allowed)
                .build()?;
            Ok(())
        })
        .run(tauri::generate_context!());

    if let Err(e) = result {
        tracing::error!(error = %e, "Phonalys n'a pas pu démarrer");
        std::process::exit(1);
    }
}
