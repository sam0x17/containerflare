use axum::Router;
use axum::extract::Extension;
use tokio::net::TcpListener;

use crate::command::CommandClient;
use crate::config::RuntimeConfig;
use crate::error::Result;

pub struct ContainerflareRuntime {
    config: RuntimeConfig,
}

impl ContainerflareRuntime {
    pub fn new(config: RuntimeConfig) -> Self {
        Self { config }
    }

    pub async fn serve(self, router: Router) -> Result<()> {
        serve(router, self.config).await
    }
}

pub async fn serve(router: Router, config: RuntimeConfig) -> Result<()> {
    let listener = TcpListener::bind(config.bind_addr).await?;
    tracing::info!(addr = %config.bind_addr, "containerflare listening");

    let command_client = CommandClient::new(config.command_endpoint.clone());
    let router = router.layer(Extension(command_client));
    let service = router.into_make_service();

    axum::serve(listener, service)
        .with_graceful_shutdown(shutdown_signal())
        .into_future()
        .await?;

    Ok(())
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
