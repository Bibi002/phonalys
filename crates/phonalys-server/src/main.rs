use phonalys_auth::SessionConfig;
use phonalys_crypto::KdfParams;
use phonalys_server::{app, spawn_auto_lock, tls, AppState, Config};
use phonalys_webauthn::RelyingParty;
use std::net::SocketAddr;
use std::path::PathBuf;
use tracing_subscriber::EnvFilter;

/// Par défaut, le serveur n'écoute que sur cette machine. L'ouvrir au réseau local
/// est un choix explicite : `PHONALYS_BIND=0.0.0.0:8443`.
const DEFAULT_BIND: &str = "127.0.0.1:8443";

/// Dossier de données par défaut, propre à l'utilisateur.
fn default_data_dir() -> PathBuf {
    let base = if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA").map(PathBuf::from)
    } else {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
    };
    base.map_or_else(|| PathBuf::from("phonalys-data"), |b| b.join("Phonalys"))
}

/// Identité WebAuthn. `PHONALYS_PUBLIC_ORIGIN` donne l'adresse exacte à laquelle les
/// appareils joignent le serveur, par exemple `https://phonalys.lan:8443`.
/// WebAuthn exige un nom de domaine (pas une adresse IP) ; `localhost` est accepté.
fn relying_party(bind: SocketAddr, insecure_http: bool) -> Result<RelyingParty, tls::BoxError> {
    let scheme = if insecure_http { "http" } else { "https" };
    let origin = std::env::var("PHONALYS_PUBLIC_ORIGIN")
        .unwrap_or_else(|_| format!("{scheme}://localhost:{}", bind.port()));
    let url = url::Url::parse(&origin)?;
    let Some(url::Host::Domain(domain)) = url.host() else {
        return Err(
            "PHONALYS_PUBLIC_ORIGIN doit utiliser un nom de domaine, pas une adresse IP".into(),
        );
    };
    Ok(RelyingParty {
        id: domain.to_owned(),
        name: "Phonalys".into(),
        origins: vec![origin.trim_end_matches('/').to_owned()],
    })
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

#[tokio::main]
async fn main() -> Result<(), tls::BoxError> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let bind: SocketAddr = std::env::var("PHONALYS_BIND")
        .unwrap_or_else(|_| DEFAULT_BIND.to_owned())
        .parse()?;
    let data_dir =
        std::env::var_os("PHONALYS_DATA_DIR").map_or_else(default_data_dir, PathBuf::from);
    create_private_dir(&data_dir)?;

    let insecure_http = std::env::var("PHONALYS_INSECURE_HTTP").as_deref() == Ok("1");
    let relying_party = relying_party(bind, insecure_http)?;
    tracing::info!(rp_id = %relying_party.id, origins = ?relying_party.origins, "identité WebAuthn");

    let (state, setup_token) = AppState::new(Config {
        vault_path: data_dir.join("cabinet.phonalys-vault"),
        kdf: KdfParams::default(),
        sessions: SessionConfig::default(),
        relying_party,
    });
    if let Some(token) = setup_token {
        // Écrit sur la console uniquement, jamais dans les journaux persistants.
        eprintln!("\n  Premier démarrage — jeton d'installation (usage unique) : {token}\n");
    }
    spawn_auto_lock(state.clone());
    let router = app(state);

    if insecure_http {
        if !bind.ip().is_loopback() {
            return Err("PHONALYS_INSECURE_HTTP n'est autorisé que sur 127.0.0.1 / ::1".into());
        }
        tracing::warn!(%bind, "HTTP en clair (développement local uniquement)");
        let listener = tokio::net::TcpListener::bind(bind).await?;
        axum::serve(listener, router).await?;
        return Ok(());
    }

    let extra_names: Vec<String> = std::env::var("PHONALYS_TLS_NAMES")
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect();
    let material = tls::load_or_create(&data_dir, &extra_names)?;
    eprintln!(
        "  Empreinte du certificat (SHA-256) : {}",
        material.fingerprint
    );
    tracing::info!(%bind, data_dir = %data_dir.display(), "Phonalys à l'écoute (TLS 1.3)");

    let config = axum_server::tls_rustls::RustlsConfig::from_config(material.config);
    axum_server::bind_rustls(bind, config)
        .serve(router.into_make_service())
        .await?;
    Ok(())
}
