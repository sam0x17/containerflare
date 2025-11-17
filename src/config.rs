use std::env;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::str::FromStr;

use containerflare_command::CommandEndpoint;
use dotenvy::Error as DotenvError;
use thiserror::Error;

/// Configuration consumed by the runtime before spinning up Axum/hyper.
#[derive(Clone, Debug)]
pub struct RuntimeConfig {
    pub bind_addr: SocketAddr,
    pub command_endpoint: CommandEndpoint,
}

impl RuntimeConfig {
    /// Loads configuration from Cloudflare-supplied `CF_*` environment variables.
    ///
    /// Values from a local `.env` file (parsed via [`dotenvy::dotenv_override`]) override whatever is already set in
    /// the process environment, which makes local development workflows predictable.
    pub fn from_env() -> Result<Self, ConfigError> {
        load_env_overrides()?;

        let port = env::var("CF_CONTAINER_PORT")
            .ok()
            .and_then(|value| value.parse::<u16>().ok())
            .unwrap_or(8787);

        let addr = env::var("CF_CONTAINER_ADDR")
            .ok()
            .and_then(|value| value.parse::<IpAddr>().ok())
            .unwrap_or(IpAddr::V4(Ipv4Addr::UNSPECIFIED));

        let bind_addr = SocketAddr::new(addr, port);

        let command_endpoint = env::var("CF_CMD_ENDPOINT")
            .ok()
            .map(|value| {
                CommandEndpoint::from_str(&value)
                    .map_err(|_| ConfigError::InvalidCommandEndpoint(value))
            })
            .transpose()? // convert Option<Result> -> Result<Option>
            .unwrap_or_default();

        Ok(Self {
            bind_addr,
            command_endpoint,
        })
    }

    /// Returns a builder for programmatic overrides.
    pub fn builder() -> RuntimeConfigBuilder {
        RuntimeConfigBuilder::default()
    }
}

impl Default for RuntimeConfig {
    /// Binds to `0.0.0.0:8787` and talks to the host over stdio.
    fn default() -> Self {
        // Default matches the local Cloudflare containers sidecar contract.
        Self {
            bind_addr: SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 8787),
            command_endpoint: CommandEndpoint::Stdio,
        }
    }
}

/// Builder type for [`RuntimeConfig`].
#[derive(Default, Clone, Debug)]
pub struct RuntimeConfigBuilder {
    bind_addr: Option<SocketAddr>,
    command_endpoint: Option<CommandEndpoint>,
}

impl RuntimeConfigBuilder {
    /// Sets the address for the embedded Axum listener.
    pub fn bind_addr(mut self, addr: SocketAddr) -> Self {
        self.bind_addr = Some(addr);
        self
    }

    /// Sets the host command endpoint transport.
    pub fn command_endpoint(mut self, endpoint: CommandEndpoint) -> Self {
        self.command_endpoint = Some(endpoint);
        self
    }

    /// Builds the final configuration.
    pub fn build(self) -> RuntimeConfig {
        RuntimeConfig {
            bind_addr: self
                .bind_addr
                .unwrap_or_else(|| SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 8787)),
            command_endpoint: self.command_endpoint.unwrap_or_default(),
        }
    }
}

/// Errors that can occur while building [`RuntimeConfig`].
#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("invalid command endpoint: {0}")]
    InvalidCommandEndpoint(String),
    #[error("failed to load .env overrides: {0}")]
    Dotenv(#[from] DotenvError),
}

fn load_env_overrides() -> Result<(), ConfigError> {
    match dotenvy::dotenv_override() {
        Ok(_) => Ok(()),
        Err(err) if err.not_found() => Ok(()),
        Err(err) => Err(ConfigError::Dotenv(err)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use containerflare_command::CommandEndpoint;
    #[cfg(unix)]
    use std::path::PathBuf;

    #[test]
    fn builder_overrides_defaults() {
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 8)), 9999);
        let config = RuntimeConfig::builder()
            .bind_addr(addr)
            .command_endpoint(CommandEndpoint::Tcp("127.0.0.1:9998".into()))
            .build();

        assert_eq!(config.bind_addr, addr);
        assert!(matches!(config.command_endpoint, CommandEndpoint::Tcp(_)));
    }

    #[test]
    fn parses_command_endpoint_strings() {
        assert!(matches!(
            "stdio".parse::<CommandEndpoint>(),
            Ok(CommandEndpoint::Stdio)
        ));
        assert!(matches!(
            "tcp://127.0.0.1:1111".parse::<CommandEndpoint>(),
            Ok(CommandEndpoint::Tcp(addr)) if addr == "127.0.0.1:1111"
        ));

        #[cfg(unix)]
        {
            let endpoint = "unix:///tmp/socket".parse::<CommandEndpoint>();
            assert!(
                matches!(endpoint, Ok(CommandEndpoint::UnixSocket(path)) if path == PathBuf::from("/tmp/socket"))
            );
        }
    }

    #[test]
    fn reads_env_configuration() {
        unsafe {
            std::env::set_var("CF_CONTAINER_PORT", "9000");
            std::env::set_var("CF_CONTAINER_ADDR", "127.0.0.2");
            std::env::set_var("CF_CMD_ENDPOINT", "tcp://127.0.0.1:7878");
        }

        let config = RuntimeConfig::from_env().expect("config");
        assert_eq!(
            config.bind_addr,
            SocketAddr::new("127.0.0.2".parse().unwrap(), 9000)
        );
        assert!(matches!(config.command_endpoint, CommandEndpoint::Tcp(_)));

        unsafe {
            std::env::remove_var("CF_CONTAINER_PORT");
            std::env::remove_var("CF_CONTAINER_ADDR");
            std::env::remove_var("CF_CMD_ENDPOINT");
        }
    }
}
