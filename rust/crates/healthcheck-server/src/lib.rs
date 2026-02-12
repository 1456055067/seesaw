//! Seesaw Healthcheck Server - Rust implementation
//!
//! High-performance healthcheck server that manages health checking of
//! backend servers for Seesaw load balancer.
//!
//! # Architecture
//!
//! The server uses a hybrid approach:
//! - Rust server handles all health checking logic (this crate)
//! - Thin Go proxy handles RPC communication with Seesaw Engine
//! - JSON over Unix socket for Go<->Rust communication
//!
//! # Components
//!
//! - **Manager**: Manages lifecycle of healthcheck monitors
//! - **Notifier**: Batches and sends notifications to engine
//! - **Proxy**: Communicates with Go proxy via Unix socket
//!
//! # Performance
//!
//! Expected to be 5-6x faster than FFI-based approach:
//! - Pure Rust checks: ~42µs per check
//! - No FFI overhead
//! - Efficient async concurrency with tokio

pub mod config;
pub mod http_server;
pub mod manager;
pub mod metrics;
pub mod notifier;
pub mod proxy;
pub mod server;
pub mod telemetry;
pub mod types;

pub use config::{Config, ConfigError};
pub use http_server::MetricsServer;
pub use metrics::MetricsRegistry;
pub use server::HealthcheckServer;
pub use telemetry::{init_telemetry, init_telemetry_http, setup_tracing_with_otel, TelemetryGuard};
pub use types::ServerConfig;
