//! Health check types and structures.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::time::Duration;

/// Health check status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HealthStatus {
    /// Service is healthy
    Healthy,
    /// Service is unhealthy
    Unhealthy,
    /// Health check timed out
    Timeout,
    /// Health check encountered an error
    Error,
}

impl fmt::Display for HealthStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HealthStatus::Healthy => write!(f, "HEALTHY"),
            HealthStatus::Unhealthy => write!(f, "UNHEALTHY"),
            HealthStatus::Timeout => write!(f, "TIMEOUT"),
            HealthStatus::Error => write!(f, "ERROR"),
        }
    }
}

/// Health check result
#[derive(Debug, Clone)]
pub struct HealthCheckResult {
    /// Status of the health check
    pub status: HealthStatus,

    /// Duration of the health check
    pub duration: Duration,

    /// Optional error message
    pub message: Option<String>,

    /// Response code (for HTTP checks)
    pub response_code: Option<u16>,
}

impl HealthCheckResult {
    /// Create a healthy result
    pub fn healthy(duration: Duration) -> Self {
        Self {
            status: HealthStatus::Healthy,
            duration,
            message: None,
            response_code: None,
        }
    }

    /// Create an unhealthy result
    pub fn unhealthy(duration: Duration, message: impl Into<String>) -> Self {
        Self {
            status: HealthStatus::Unhealthy,
            duration,
            message: Some(message.into()),
            response_code: None,
        }
    }

    /// Create a timeout result
    pub fn timeout(duration: Duration) -> Self {
        Self {
            status: HealthStatus::Timeout,
            duration,
            message: Some("Health check timed out".to_string()),
            response_code: None,
        }
    }

    /// Create an error result
    pub fn error(duration: Duration, message: impl Into<String>) -> Self {
        Self {
            status: HealthStatus::Error,
            duration,
            message: Some(message.into()),
            response_code: None,
        }
    }

    /// Check if the result is healthy
    pub fn is_healthy(&self) -> bool {
        self.status == HealthStatus::Healthy
    }
}

/// Health check configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HealthCheckConfig {
    /// Target address (IP:port or hostname:port)
    pub target: String,

    /// Timeout for the health check
    #[serde(with = "humantime_serde")]
    pub timeout: Duration,

    /// Interval between checks
    #[serde(with = "humantime_serde")]
    pub interval: Duration,

    /// Number of consecutive successes required
    pub rise: u32,

    /// Number of consecutive failures required
    pub fall: u32,

    /// Check type
    pub check_type: CheckType,
}

impl Default for HealthCheckConfig {
    fn default() -> Self {
        Self {
            target: String::new(),
            timeout: Duration::from_secs(5),
            interval: Duration::from_secs(10),
            rise: 2,
            fall: 3,
            check_type: CheckType::Tcp,
        }
    }
}

/// Health check type
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum CheckType {
    /// TCP connection check
    Tcp,

    /// HTTP/HTTPS check
    Http {
        /// HTTP method (GET, POST, etc.)
        method: String,
        /// Request path
        path: String,
        /// Expected status codes
        expected_codes: Vec<u16>,
        /// Use HTTPS
        https: bool,
    },

    /// ICMP ping check
    Ping,

    /// DNS resolution check
    Dns {
        /// Query name
        query: String,
        /// Expected IP addresses
        expected_ips: Vec<String>,
    },
}

/// Health check statistics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HealthCheckStats {
    /// Total checks performed
    pub total_checks: u64,

    /// Successful checks
    pub successful_checks: u64,

    /// Failed checks
    pub failed_checks: u64,

    /// Timeout count
    pub timeouts: u64,

    /// Average response time (milliseconds)
    pub avg_response_time_ms: f64,

    /// Current consecutive successes
    pub consecutive_successes: u32,

    /// Current consecutive failures
    pub consecutive_failures: u32,
}

impl HealthCheckStats {
    /// Update stats with a check result
    pub fn update(&mut self, result: &HealthCheckResult) {
        self.total_checks += 1;

        match result.status {
            HealthStatus::Healthy => {
                self.successful_checks += 1;
                self.consecutive_successes += 1;
                self.consecutive_failures = 0;
            }
            HealthStatus::Unhealthy | HealthStatus::Error => {
                self.failed_checks += 1;
                self.consecutive_failures += 1;
                self.consecutive_successes = 0;
            }
            HealthStatus::Timeout => {
                self.timeouts += 1;
                self.consecutive_failures += 1;
                self.consecutive_successes = 0;
            }
        }

        // Update average response time
        let duration_ms = result.duration.as_millis() as f64;
        self.avg_response_time_ms = (self.avg_response_time_ms * (self.total_checks - 1) as f64
            + duration_ms)
            / self.total_checks as f64;
    }
}
