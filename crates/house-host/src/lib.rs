mod config;
mod insula;
mod policy;
mod receipt;
mod server;
mod store;
mod viewport;

pub use config::{DEFAULT_BIND, DEFAULT_WS_PATH, HostConfig, KNOCK_AUTONOMY_ENV, KnockAutonomy};
pub use server::Host;

use tokio::net::TcpListener;

pub async fn run(config: HostConfig) -> Result<(), String> {
    let bind = config.bind;
    let host = Host::new(config)?;
    let listener = TcpListener::bind(bind)
        .await
        .map_err(|error| format!("cannot bind Host to {bind}: {error}"))?;
    tracing::info!(address = %bind, "Athanor Host listening");
    host.serve(listener, async {
        if let Err(error) = tokio::signal::ctrl_c().await {
            tracing::error!(%error, "failed to install shutdown signal");
        }
    })
    .await
}
