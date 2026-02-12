//! High-performance health checking for Seesaw load balancer.
//!
//! This crate provides efficient health checking capabilities for backend servers
//! with support for multiple check types:
//! - TCP connection checks
//! - HTTP/HTTPS checks
//! - DNS resolution checks
//! - ICMP ping checks (future)
//!
//! # Features
//!
//! - Async/await based for high concurrency
//! - Configurable rise/fall thresholds
//! - Comprehensive statistics tracking
//! - Sub-millisecond check latency
//! - Automatic retry with exponential backoff
//!
//! # Example
//!
//! ```no_run
//! use healthcheck::{HealthCheckConfig, HealthCheckMonitor, checkers::TcpChecker, types::CheckType};
//! use std::time::Duration;
//! use std::sync::Arc;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let config = HealthCheckConfig {
//!     target: "192.168.1.100:80".to_string(),
//!     timeout: Duration::from_secs(2),
//!     interval: Duration::from_secs(5),
//!     rise: 2,
//!     fall: 3,
//!     check_type: CheckType::Tcp,
//! };
//!
//! let checker = Arc::new(TcpChecker::new(
//!     "192.168.1.100:80".parse()?,
//!     config.timeout,
//! ));
//!
//! let monitor = HealthCheckMonitor::new(checker, config);
//! monitor.start().await;
//!
//! // Check health status
//! let is_healthy = monitor.is_healthy().await;
//! let stats = monitor.get_stats().await;
//! # Ok(())
//! # }
//! ```

pub mod checkers;
pub mod monitor;
pub mod types;

pub use checkers::{DnsChecker, HealthChecker, HttpChecker, TcpChecker};
pub use monitor::HealthCheckMonitor;
pub use types::{CheckType, HealthCheckConfig, HealthCheckResult, HealthCheckStats, HealthStatus};

// Phase 3.1: Core health checkers (DONE - TCP, HTTP, DNS)
// Phase 3.2: Monitor and state management (DONE - rise/fall, stats)
// Phase 3.3: FFI bridge (DONE - healthcheck-ffi crate + one-shot API)
// Phase 3.4: Go integration (DONE - rust bindings package)
// Phase 3.5: Testing and benchmarks (DONE - criterion + comparative analysis)
// Phase 3.6: Seesaw integration (DONE - adapter checkers)

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_health_status_display() {
        assert_eq!(HealthStatus::Healthy.to_string(), "HEALTHY");
        assert_eq!(HealthStatus::Unhealthy.to_string(), "UNHEALTHY");
        assert_eq!(HealthStatus::Timeout.to_string(), "TIMEOUT");
        assert_eq!(HealthStatus::Error.to_string(), "ERROR");
    }

    #[test]
    fn test_health_check_result() {
        let result = HealthCheckResult::healthy(std::time::Duration::from_millis(100));
        assert!(result.is_healthy());
        assert_eq!(result.status, HealthStatus::Healthy);
        assert!(result.message.is_none());
    }

    #[test]
    fn test_stats_update() {
        let mut stats = HealthCheckStats::default();

        let result = HealthCheckResult::healthy(std::time::Duration::from_millis(100));
        stats.update(&result);

        assert_eq!(stats.total_checks, 1);
        assert_eq!(stats.successful_checks, 1);
        assert_eq!(stats.consecutive_successes, 1);
        assert_eq!(stats.consecutive_failures, 0);
    }
}
