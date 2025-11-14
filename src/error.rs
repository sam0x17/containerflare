use thiserror::Error;

use crate::config::ConfigError;
use containerflare_command::CommandError;

pub type Result<T> = std::result::Result<T, ContainerflareError>;

#[derive(Debug, Error)]
pub enum ContainerflareError {
    #[error(transparent)]
    Config(#[from] ConfigError),
    #[error(transparent)]
    Command(#[from] CommandError),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("server error: {0}")]
    Hyper(#[from] hyper::Error),
}
