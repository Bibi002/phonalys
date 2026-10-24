use std::net::SocketAddr;
use tracing_subscriber::EnvFilter;

/// Par défaut, le serveur n'écoute que sur cette machine. L'ouvrir au réseau local
/// est un choix explicite (`PHONALYS_BIND=0.0.0.0:8443`), à faire derrière TLS.
const DEFAULT_BIND: &str = "127.0.0.1:8443";

#[tokio::main]
async fn main() -> std::io::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let bind: SocketAddr = std::env::var("PHONALYS_BIND")
        .unwrap_or_else(|_| DEFAULT_BIND.to_owned())
        .parse()
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e))?;

    let listener = tokio::net::TcpListener::bind(bind).await?;
    tracing::info!(%bind, "Phonalys à l'écoute");
    axum::serve(listener, phonalys_server::app())
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await
}
