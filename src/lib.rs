//! Containerflare runtime crate.
//!
//! This crate exposes an Axum-friendly runtime that plugs into Cloudflare's
//! Containers platform, letting you write idiomatic Rust handlers that still have
//! access to the surrounding worker container capabilities.

pub mod config;
pub mod context;
pub mod error;
pub mod platform;
pub mod runtime;

pub use crate::config::{RuntimeConfig, RuntimeConfigBuilder};
pub use crate::context::{ContainerContext, RequestMetadata, TraceContext};
pub use crate::error::{ContainerflareError, Result};
pub use crate::platform::{CloudRunPlatform, CloudflarePlatform, RuntimePlatform};
pub use crate::runtime::{ContainerflareRuntime, run, serve};
pub use containerflare_command::{
    CommandClient, CommandEndpoint, CommandError, CommandRequest, CommandResponse,
};
