//! Containerflare runtime crate.
//!
//! This crate exposes an Axum-friendly runtime that plugs into Cloudflare's
//! Containers platform, letting you write idiomatic Rust handlers that still have
//! access to the surrounding worker container capabilities.

pub mod command;
pub mod config;
pub mod context;
pub mod error;
pub mod runtime;

pub use crate::command::{CommandClient, CommandError, CommandRequest, CommandResponse};
pub use crate::config::{CommandEndpoint, RuntimeConfig, RuntimeConfigBuilder};
pub use crate::context::{ContainerContext, RequestMetadata};
pub use crate::error::{ContainerflareError, Result};
pub use crate::runtime::{ContainerflareRuntime, serve};
