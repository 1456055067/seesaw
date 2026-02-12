//! Logging utilities for Seesaw Rust components.

use tracing_subscriber::{EnvFilter, fmt, prelude::*};

/// Initialize tracing with sensible defaults.
///
/// Uses the RUST_LOG environment variable to control log levels.
/// Default level is INFO.
pub fn init() {
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .init();
}

/// Initialize tracing with JSON formatting (useful for structured logging).
pub fn init_json() {
    tracing_subscriber::registry()
        .with(fmt::layer().json())
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .init();
}
