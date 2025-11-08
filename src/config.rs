use std::env;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::str::FromStr;

use thiserror::Error;

/// Configuration consumed by the runtime before spinning up Axum/hyper.
#[derive(Clone, Debug)]
pub struct RuntimeConfig {
    pub bind_addr: SocketAddr,
    pub command_endpoint: CommandEndpoint,
}

impl RuntimeConfig {
    pub fn from_env() -> Result<Self, ConfigError> {
        let port = env::var("CF_CONTAINER_PORT")
            .ok()
            .and_then(|value| value.parse::<u16>().ok())
            .unwrap_or(8787);

        let addr = env::var("CF_CONTAINER_ADDR")
            .ok()
            .and_then(|value| value.parse::<IpAddr>().ok())
            .unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST));

        let bind_addr = SocketAddr::new(addr, port);

        let command_endpoint = env::var("CF_CMD_ENDPOINT")
            .ok()
            .map(|value| value.parse())
            .transpose()? // convert Option<Result> -> Result<Option>
            .unwrap_or_default();

        Ok(Self {
            bind_addr,
            command_endpoint,
        })
    }

    pub fn builder() -> RuntimeConfigBuilder {
        RuntimeConfigBuilder::default()
    }
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        // Default matches the local Cloudflare containers sidecar contract.
        Self {
            bind_addr: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 8787),
            command_endpoint: CommandEndpoint::Stdio,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CommandEndpoint {
    Stdio,
    #[cfg(unix)]
    UnixSocket(PathBuf),
    Tcp(String),
}

impl Default for CommandEndpoint {
    fn default() -> Self {
        CommandEndpoint::Stdio
    }
}

impl FromStr for CommandEndpoint {
    type Err = ConfigError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let value = s.trim();
        if value.eq_ignore_ascii_case("stdio") {
            return Ok(CommandEndpoint::Stdio);
        }

        #[cfg(unix)]
        if let Some(path) = value.strip_prefix("unix://") {
            return Ok(CommandEndpoint::UnixSocket(PathBuf::from(path)));
        }

        if let Some(addr) = value.strip_prefix("tcp://") {
            return Ok(CommandEndpoint::Tcp(addr.to_owned()));
        }

        Err(ConfigError::InvalidCommandEndpoint(value.to_owned()))
    }
}

#[derive(Default, Clone, Debug)]
pub struct RuntimeConfigBuilder {
    bind_addr: Option<SocketAddr>,
    command_endpoint: Option<CommandEndpoint>,
}

impl RuntimeConfigBuilder {
    pub fn bind_addr(mut self, addr: SocketAddr) -> Self {
        self.bind_addr = Some(addr);
        self
    }

    pub fn command_endpoint(mut self, endpoint: CommandEndpoint) -> Self {
        self.command_endpoint = Some(endpoint);
        self
    }

    pub fn build(self) -> RuntimeConfig {
        RuntimeConfig {
            bind_addr: self
                .bind_addr
                .unwrap_or_else(|| SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 8787)),
            command_endpoint: self
                .command_endpoint
                .unwrap_or_else(CommandEndpoint::default),
        }
    }
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("invalid command endpoint: {0}")]
    InvalidCommandEndpoint(String),
}

#[cfg(test)]
mod tests {
    use super::*;
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
