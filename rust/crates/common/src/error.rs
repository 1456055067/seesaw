//! Common error types for Seesaw Rust components.

use std::fmt;

/// A specialized Result type for Seesaw operations.
pub type Result<T> = std::result::Result<T, Error>;

/// Common error type for Seesaw operations.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Netlink error: {0}")]
    Netlink(String),

    #[error("IPVS error: {0}")]
    IPVS(String),

    #[error("VRRP error: {0}")]
    VRRP(String),

    #[error("Healthcheck error: {0}")]
    Healthcheck(String),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Unknown error: {0}")]
    Other(String),
}

impl Error {
    /// Create a new netlink error.
    pub fn netlink(msg: impl fmt::Display) -> Self {
        Error::Netlink(msg.to_string())
    }

    /// Create a new IPVS error.
    pub fn ipvs(msg: impl fmt::Display) -> Self {
        Error::IPVS(msg.to_string())
    }

    /// Create a new VRRP error.
    pub fn vrrp(msg: impl fmt::Display) -> Self {
        Error::VRRP(msg.to_string())
    }

    /// Create a new healthcheck error.
    pub fn healthcheck(msg: impl fmt::Display) -> Self {
        Error::Healthcheck(msg.to_string())
    }

    /// Create a new configuration error.
    pub fn config(msg: impl fmt::Display) -> Self {
        Error::Config(msg.to_string())
    }

    /// Create a new other error.
    pub fn other(msg: impl fmt::Display) -> Self {
        Error::Other(msg.to_string())
    }
}
