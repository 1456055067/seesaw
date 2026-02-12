//! Types for healthcheck server and Go<->Rust communication.

use healthcheck::types::{CheckType, HealthCheckConfig};
use serde::{Deserialize, Serialize};
use std::net::IpAddr;
use std::time::Duration;

/// Healthcheck ID
pub type HealthcheckId = u64;

/// Server configuration
#[derive(Debug, Clone)]
pub struct ServerConfig {
    /// Batch delay for notifications
    pub batch_delay: Duration,

    /// Maximum batch size
    pub batch_size: usize,

    /// Channel buffer size
    pub channel_size: usize,

    /// Maximum notification failures before giving up
    pub max_failures: usize,

    /// Interval between status notifications
    pub notify_interval: Duration,

    /// Interval between config fetches
    pub fetch_interval: Duration,

    /// Retry delay on failures
    pub retry_delay: Duration,

    /// Socket path for Go proxy communication
    pub proxy_socket: String,

    /// Config update channel buffer size
    pub config_channel_size: usize,

    /// Proxy message channel buffer size
    pub proxy_channel_size: usize,

    /// Manager monitor polling interval
    pub manager_monitor_interval: Duration,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            batch_delay: Duration::from_millis(100),
            batch_size: 100,
            channel_size: 1000,
            max_failures: 10,
            notify_interval: Duration::from_secs(15),
            fetch_interval: Duration::from_secs(15),
            retry_delay: Duration::from_secs(2),
            proxy_socket: "/var/run/seesaw/healthcheck-proxy.sock".to_string(),
            config_channel_size: 10,
            proxy_channel_size: 10,
            manager_monitor_interval: Duration::from_millis(500),
        }
    }
}

/// Health check state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum State {
    Unknown,
    Unhealthy,
    Healthy,
}

/// Status of a healthcheck
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Status {
    pub last_check: Option<std::time::SystemTime>,
    pub duration: Duration,
    pub failures: u64,
    pub successes: u64,
    pub state: State,
    pub message: String,
}

/// Notification from healthcheck to engine
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Notification {
    pub id: HealthcheckId,
    pub status: Status,
}

/// Batch of notifications
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationBatch {
    pub notifications: Vec<Notification>,
}

/// Message from Go proxy to Rust server
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ProxyToServerMsg {
    /// Update healthcheck configurations
    UpdateConfigs { configs: Vec<HealthcheckConfig> },

    /// Request status for all healthchecks
    RequestStatus,

    /// Shutdown server
    Shutdown,
}

/// Message from Rust server to Go proxy
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerToProxyMsg {
    /// Send notification batch to engine
    NotificationBatch { batch: NotificationBatch },

    /// Response to status request
    StatusResponse { statuses: Vec<(HealthcheckId, Status)> },

    /// Server ready
    Ready,

    /// Error occurred
    Error { message: String },
}

/// Healthcheck configuration from engine
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthcheckConfig {
    pub id: HealthcheckId,

    #[serde(with = "humantime_serde")]
    pub interval: Duration,

    #[serde(with = "humantime_serde")]
    pub timeout: Duration,

    pub retries: u32,

    #[serde(flatten)]
    pub checker: CheckerConfig,
}

/// Checker-specific configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "checker_type", rename_all = "lowercase")]
pub enum CheckerConfig {
    Tcp {
        ip: IpAddr,
        port: u16,
    },
    Http {
        ip: IpAddr,
        port: u16,
        method: String,
        path: String,
        expected_codes: Vec<u16>,
        secure: bool,
    },
    Dns {
        query: String,
        expected_ips: Vec<IpAddr>,
    },
}

impl HealthcheckConfig {
    /// Convert to healthcheck crate's HealthCheckConfig
    pub fn to_monitor_config(&self) -> HealthCheckConfig {
        match &self.checker {
            CheckerConfig::Tcp { ip, port } => HealthCheckConfig {
                target: format!("{}:{}", ip, port),
                timeout: self.timeout,
                interval: self.interval,
                rise: (self.retries + 1).max(2),  // Convert retries to rise
                fall: (self.retries + 1).max(2),  // Convert retries to fall
                check_type: CheckType::Tcp,
            },
            CheckerConfig::Http {
                ip,
                port,
                method,
                path,
                expected_codes,
                secure,
            } => HealthCheckConfig {
                target: format!("{}:{}", ip, port),
                timeout: self.timeout,
                interval: self.interval,
                rise: (self.retries + 1).max(2),
                fall: (self.retries + 1).max(2),
                check_type: CheckType::Http {
                    method: method.clone(),
                    path: path.clone(),
                    expected_codes: expected_codes.clone(),
                    https: *secure,
                },
            },
            CheckerConfig::Dns {
                query,
                expected_ips,
            } => HealthCheckConfig {
                target: "0.0.0.0:53".to_string(),  // Placeholder
                timeout: self.timeout,
                interval: self.interval,
                rise: (self.retries + 1).max(2),
                fall: (self.retries + 1).max(2),
                check_type: CheckType::Dns {
                    query: query.clone(),
                    expected_ips: expected_ips.iter().map(|ip| ip.to_string()).collect(),
                },
            },
        }
    }
}
