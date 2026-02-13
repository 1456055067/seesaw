//! Configuration loading and validation for healthcheck server

use crate::types::ServerConfig;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Duration;
use thiserror::Error;
use validator::{Validate, ValidationError};

/// Configuration error types
#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("Configuration file not found in search paths")]
    FileNotFound,

    #[error("Failed to read configuration file: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Failed to parse YAML: {0}")]
    ParseError(#[from] serde_yaml::Error),

    #[error("Invalid configuration: {0}")]
    ValidationError(#[from] validator::ValidationErrors),
}

/// Main configuration structure
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub server: ServerSettings,

    #[serde(default)]
    pub batching: BatchingSettings,

    #[serde(default)]
    pub channels: ChannelSettings,

    #[serde(default)]
    pub manager: ManagerSettings,

    #[serde(default)]
    pub advanced: AdvancedSettings,

    #[serde(default)]
    pub logging: LoggingSettings,

    #[serde(default)]
    pub metrics: MetricsSettings,

    #[serde(default)]
    pub telemetry: TelemetrySettings,
}

impl Validate for Config {
    fn validate(&self) -> Result<(), validator::ValidationErrors> {
        self.server.validate()?;
        self.batching.validate()?;
        self.channels.validate()?;
        self.manager.validate()?;
        self.metrics.validate()?;
        self.telemetry.validate()?;
        Ok(())
    }
}

/// Server-level settings
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ServerSettings {
    #[validate(length(min = 1), custom(function = validate_socket_path))]
    pub proxy_socket: String,
}

/// Notification batching settings
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct BatchingSettings {
    #[serde(with = "humantime_serde")]
    #[validate(custom(function = validate_batch_delay))]
    pub delay: Duration,

    #[validate(range(min = 1, max = 10000))]
    pub max_size: usize,
}

/// Channel buffer size settings
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ChannelSettings {
    #[validate(range(min = 10, max = 100000))]
    pub notification: usize,

    #[validate(range(min = 1, max = 1000))]
    pub config_update: usize,

    #[validate(range(min = 1, max = 1000))]
    pub proxy_message: usize,
}

/// Manager-specific settings
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ManagerSettings {
    #[serde(with = "humantime_serde")]
    #[validate(custom(function = validate_monitor_interval))]
    pub monitor_interval: Duration,
}

/// Advanced settings (currently unused, reserved for future)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdvancedSettings {
    pub max_failures: usize,

    #[serde(with = "humantime_serde")]
    pub notify_interval: Duration,

    #[serde(with = "humantime_serde")]
    pub fetch_interval: Duration,

    #[serde(with = "humantime_serde")]
    pub retry_delay: Duration,
}

/// Logging settings
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LoggingSettings {
    pub level: Option<String>,
    pub format: Option<String>,
}

/// Metrics settings
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(default)]
pub struct MetricsSettings {
    /// Enable metrics HTTP endpoint
    pub enabled: bool,

    /// Metrics HTTP server listen address
    #[validate(length(min = 1))]
    pub listen_addr: String,

    /// Histogram buckets for response times (in seconds)
    pub response_time_buckets: Vec<f64>,

    /// Histogram buckets for batch delay (in seconds)
    pub batch_delay_buckets: Vec<f64>,

    /// Histogram buckets for batch size
    pub batch_size_buckets: Vec<f64>,
}

/// OpenTelemetry settings
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(default)]
pub struct TelemetrySettings {
    /// Enable OpenTelemetry tracing
    pub enabled: bool,

    /// Service name for OpenTelemetry
    #[validate(length(min = 1))]
    pub service_name: String,

    /// OTLP exporter endpoint (gRPC)
    /// Example: "http://localhost:4317"
    #[validate(length(min = 1))]
    pub otlp_endpoint: String,

    /// Use HTTP instead of gRPC for OTLP export
    pub use_http: bool,

    /// Sampling rate (0.0 to 1.0)
    /// 1.0 = sample all traces, 0.1 = sample 10%
    #[validate(range(min = 0.0, max = 1.0))]
    pub sampling_rate: f64,
}

// Default implementations

impl Default for ServerSettings {
    fn default() -> Self {
        Self {
            proxy_socket: "/var/run/seesaw/healthcheck-proxy.sock".to_string(),
        }
    }
}

impl Default for BatchingSettings {
    fn default() -> Self {
        Self {
            delay: Duration::from_millis(100),
            max_size: 100,
        }
    }
}

impl Default for ChannelSettings {
    fn default() -> Self {
        Self {
            notification: 1000,
            config_update: 10,
            proxy_message: 10,
        }
    }
}

impl Default for ManagerSettings {
    fn default() -> Self {
        Self {
            monitor_interval: Duration::from_millis(500),
        }
    }
}

impl Default for AdvancedSettings {
    fn default() -> Self {
        Self {
            max_failures: 10,
            notify_interval: Duration::from_secs(15),
            fetch_interval: Duration::from_secs(15),
            retry_delay: Duration::from_secs(2),
        }
    }
}

impl Default for MetricsSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            listen_addr: "127.0.0.1:9090".to_string(),
            response_time_buckets: vec![
                0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0,
            ],
            batch_delay_buckets: vec![0.01, 0.025, 0.05, 0.075, 0.1, 0.15, 0.2, 0.25, 0.5, 1.0],
            batch_size_buckets: vec![
                1.0, 5.0, 10.0, 25.0, 50.0, 100.0, 250.0, 500.0, 1000.0, 5000.0,
            ],
        }
    }
}

impl Default for TelemetrySettings {
    fn default() -> Self {
        Self {
            enabled: false,
            service_name: "healthcheck-server".to_string(),
            otlp_endpoint: "http://localhost:4317".to_string(),
            use_http: false,
            sampling_rate: 1.0,
        }
    }
}

// Custom validators

fn validate_socket_path(path: &str) -> Result<(), ValidationError> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err(ValidationError::new("socket_path_empty"));
    }

    // Must be absolute path or relative (starting with ./)
    if !trimmed.starts_with('/') && !trimmed.starts_with("./") {
        return Err(ValidationError::new("socket_path_invalid_format"));
    }

    Ok(())
}

fn validate_batch_delay(delay: &Duration) -> Result<(), ValidationError> {
    let millis = delay.as_millis();
    if !(1..=10_000).contains(&millis) {
        return Err(ValidationError::new("batch_delay_out_of_range"));
    }
    Ok(())
}

fn validate_monitor_interval(interval: &Duration) -> Result<(), ValidationError> {
    let millis = interval.as_millis();
    if !(10..=60_000).contains(&millis) {
        return Err(ValidationError::new("monitor_interval_out_of_range"));
    }
    Ok(())
}

// Configuration loading implementation

impl Config {
    /// Load configuration from default search paths
    pub fn load() -> Result<Self, ConfigError> {
        match Self::find_config_file() {
            Some(path) => {
                tracing::info!("Loading configuration from: {}", path.display());
                Self::load_from_file(&path)
            }
            None => {
                tracing::info!("No configuration file found, using defaults");
                Ok(Self::default())
            }
        }
    }

    /// Load configuration from a specific file
    pub fn load_from_file(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let contents = std::fs::read_to_string(path.as_ref())?;
        let config: Config = serde_yaml::from_str(&contents)?;
        config.validate()?;
        Ok(config)
    }

    /// Find configuration file in standard locations
    fn find_config_file() -> Option<PathBuf> {
        let mut paths = vec![PathBuf::from("/etc/seesaw/healthcheck-server.yaml")];

        if let Some(home_path) = Self::home_config_path() {
            paths.push(home_path);
        }

        paths.push(PathBuf::from("./healthcheck-server.yaml"));

        paths
            .into_iter()
            .find(|p: &PathBuf| p.exists() && p.is_file())
    }

    /// Get home directory config path
    fn home_config_path() -> Option<PathBuf> {
        dirs::home_dir().map(|home| home.join(".config/seesaw/healthcheck-server.yaml"))
    }

    /// Convert to ServerConfig (existing internal type)
    pub fn to_server_config(&self) -> ServerConfig {
        ServerConfig {
            batch_delay: self.batching.delay,
            batch_size: self.batching.max_size,
            channel_size: self.channels.notification,
            max_failures: self.advanced.max_failures,
            notify_interval: self.advanced.notify_interval,
            fetch_interval: self.advanced.fetch_interval,
            retry_delay: self.advanced.retry_delay,
            proxy_socket: self.server.proxy_socket.clone(),
            config_channel_size: self.channels.config_update,
            proxy_channel_size: self.channels.proxy_message,
            manager_monitor_interval: self.manager.monitor_interval,
            metrics_enabled: self.metrics.enabled,
            metrics_listen_addr: self.metrics.listen_addr.clone(),
            metrics_response_time_buckets: self.metrics.response_time_buckets.clone(),
            metrics_batch_delay_buckets: self.metrics.batch_delay_buckets.clone(),
            metrics_batch_size_buckets: self.metrics.batch_size_buckets.clone(),
            telemetry_enabled: self.telemetry.enabled,
            telemetry_service_name: self.telemetry.service_name.clone(),
            telemetry_otlp_endpoint: self.telemetry.otlp_endpoint.clone(),
            telemetry_use_http: self.telemetry.use_http,
            telemetry_sampling_rate: self.telemetry.sampling_rate,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config_is_valid() {
        let config = Config::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_valid_yaml_parsing() {
        let yaml = r#"
server:
  proxy_socket: "/tmp/test.sock"

batching:
  delay: 100ms
  max_size: 100

channels:
  notification: 1000
  config_update: 10
  proxy_message: 10

manager:
  monitor_interval: 500ms
"#;

        let config: Config = serde_yaml::from_str(yaml).unwrap();
        assert!(config.validate().is_ok());
        assert_eq!(config.server.proxy_socket, "/tmp/test.sock");
        assert_eq!(config.batching.max_size, 100);
        assert_eq!(config.channels.notification, 1000);
    }

    #[test]
    fn test_minimal_yaml_uses_defaults() {
        let yaml = r#"
server:
  proxy_socket: "/tmp/test.sock"
"#;

        let config: Config = serde_yaml::from_str(yaml).unwrap();
        assert!(config.validate().is_ok());
        // Should use default values
        assert_eq!(config.batching.delay, Duration::from_millis(100));
        assert_eq!(config.batching.max_size, 100);
    }

    #[test]
    fn test_invalid_batch_delay_too_large() {
        let yaml = r#"
server:
  proxy_socket: "/tmp/test.sock"

batching:
  delay: 15s  # Invalid: > 10s
  max_size: 100
"#;

        let config: Config = serde_yaml::from_str(yaml).unwrap();
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_invalid_batch_delay_too_small() {
        let yaml = r#"
batching:
  delay: 0ms  # Invalid: < 1ms
  max_size: 100
"#;

        let config: Config = serde_yaml::from_str(yaml).unwrap();
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_invalid_batch_size_too_large() {
        let yaml = r#"
batching:
  delay: 100ms
  max_size: 50000  # Invalid: > 10000
"#;

        let config: Config = serde_yaml::from_str(yaml).unwrap();
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_invalid_batch_size_too_small() {
        let yaml = r#"
batching:
  delay: 100ms
  max_size: 0  # Invalid: < 1
"#;

        let config: Config = serde_yaml::from_str(yaml).unwrap();
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_socket_path_validation() {
        // Valid paths
        assert!(validate_socket_path("/tmp/test.sock").is_ok());
        assert!(validate_socket_path("/var/run/seesaw/healthcheck.sock").is_ok());
        assert!(validate_socket_path("./test.sock").is_ok());

        // Invalid paths
        assert!(validate_socket_path("").is_err());
        assert!(validate_socket_path("   ").is_err());
        assert!(validate_socket_path("relative/path.sock").is_err()); // Must start with / or ./
    }

    #[test]
    fn test_humantime_serde_parsing() {
        let yaml = r#"
batching:
  delay: 250ms
  max_size: 100

manager:
  monitor_interval: 1s
"#;

        let config: Config = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.batching.delay, Duration::from_millis(250));
        assert_eq!(config.manager.monitor_interval, Duration::from_secs(1));
    }

    #[test]
    fn test_config_to_server_config_conversion() {
        let config = Config::default();
        let server_config = config.to_server_config();

        assert_eq!(server_config.batch_delay, Duration::from_millis(100));
        assert_eq!(server_config.batch_size, 100);
        assert_eq!(server_config.channel_size, 1000);
        assert_eq!(server_config.config_channel_size, 10);
        assert_eq!(server_config.proxy_channel_size, 10);
        assert_eq!(
            server_config.manager_monitor_interval,
            Duration::from_millis(500)
        );
    }

    #[test]
    fn test_invalid_channel_sizes() {
        // notification channel too small
        let yaml = r#"
channels:
  notification: 5  # Invalid: < 10
  config_update: 10
  proxy_message: 10
"#;
        let config: Config = serde_yaml::from_str(yaml).unwrap();
        assert!(config.validate().is_err());

        // config_update channel too large
        let yaml = r#"
channels:
  notification: 1000
  config_update: 5000  # Invalid: > 1000
  proxy_message: 10
"#;
        let config: Config = serde_yaml::from_str(yaml).unwrap();
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_invalid_monitor_interval() {
        // Too small
        let yaml = r#"
manager:
  monitor_interval: 5ms  # Invalid: < 10ms
"#;
        let config: Config = serde_yaml::from_str(yaml).unwrap();
        assert!(config.validate().is_err());

        // Too large
        let yaml = r#"
manager:
  monitor_interval: 2m  # Invalid: > 60s
"#;
        let config: Config = serde_yaml::from_str(yaml).unwrap();
        assert!(config.validate().is_err());
    }
}
