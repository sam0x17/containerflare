use axum::Router;
use axum::extract::Extension;
use tokio::net::TcpListener;

use crate::config::RuntimeConfig;
use crate::error::Result;
use containerflare_command::CommandClient;

/// High-level runtime that wires an Axum router into Cloudflare's container environment.
pub struct ContainerflareRuntime {
    config: RuntimeConfig,
}

impl ContainerflareRuntime {
    /// Creates a runtime with the provided configuration.
    pub fn new(config: RuntimeConfig) -> Self {
        Self { config }
    }

    /// Consumes the runtime and starts serving the supplied router.
    pub async fn serve(self, router: Router) -> Result<()> {
        serve(router, self.config).await
    }
}

/// Serves the router with the provided configuration.
pub async fn serve(router: Router, config: RuntimeConfig) -> Result<()> {
    let listener = TcpListener::bind(config.bind_addr).await?;
    tracing::info!(addr = %config.bind_addr, "containerflare listening");

    let command_client = CommandClient::connect(config.command_endpoint.clone()).await?;
    let router = router.layer(Extension(command_client));
    let service = router.into_make_service();

    axum::serve(listener, service)
        .with_graceful_shutdown(shutdown_signal())
        .into_future()
        .await?;

    Ok(())
}

/// Loads [`RuntimeConfig`] from the environment and starts serving the router.
pub async fn run(router: Router) -> Result<()> {
    let config = RuntimeConfig::from_env()?;
    serve(router, config).await
}

async fn shutdown_signal() {
    #[cfg(unix)]
    {
        let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler");

        tokio::select! {
            _ = tokio::signal::ctrl_c() => {},
            _ = sigterm.recv() => {},
        }
    }

    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}
