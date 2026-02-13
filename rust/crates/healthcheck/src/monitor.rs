//! Health check monitoring and management.

use crate::checkers::HealthChecker;
use crate::types::{HealthCheckConfig, HealthCheckResult, HealthCheckStats, HealthStatus};
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::interval;
use tracing::{debug, info, warn};

#[cfg(test)]
use std::time::Duration;
#[cfg(test)]
use tokio::time::sleep;

/// Health check monitor
pub struct HealthCheckMonitor {
    checker: Arc<dyn HealthChecker>,
    config: HealthCheckConfig,
    stats: Arc<RwLock<HealthCheckStats>>,
    is_up: Arc<RwLock<bool>>,
    stop_signal: Arc<tokio::sync::Notify>,
}

impl HealthCheckMonitor {
    /// Create a new health check monitor
    pub fn new(checker: Arc<dyn HealthChecker>, config: HealthCheckConfig) -> Self {
        Self {
            checker,
            config,
            stats: Arc::new(RwLock::new(HealthCheckStats::default())),
            is_up: Arc::new(RwLock::new(false)),
            stop_signal: Arc::new(tokio::sync::Notify::new()),
        }
    }

    /// Start monitoring
    pub async fn start(&self) {
        let checker = self.checker.clone();
        let config = self.config.clone();
        let stats = self.stats.clone();
        let is_up = self.is_up.clone();
        let stop_signal = self.stop_signal.clone();

        tokio::spawn(async move {
            let mut check_interval = interval(config.interval);
            check_interval.tick().await; // Skip first immediate tick

            loop {
                tokio::select! {
                    _ = check_interval.tick() => {
                        let result = checker.check().await;
                        Self::process_result(result, &config, &stats, &is_up).await;
                    }
                    _ = stop_signal.notified() => {
                        info!("Health check monitor stopping");
                        break;
                    }
                }
            }
        });
    }

    /// Stop monitoring
    pub async fn stop(&self) {
        self.stop_signal.notify_one();
    }

    /// Get current health status
    pub async fn is_healthy(&self) -> bool {
        *self.is_up.read().await
    }

    /// Get statistics
    pub async fn get_stats(&self) -> HealthCheckStats {
        *self.stats.read().await
    }

    /// Process a health check result
    async fn process_result(
        result: HealthCheckResult,
        config: &HealthCheckConfig,
        stats: &Arc<RwLock<HealthCheckStats>>,
        is_up: &Arc<RwLock<bool>>,
    ) {
        // Update statistics
        let mut stats_guard = stats.write().await;
        stats_guard.update(&result);

        let consecutive_successes = stats_guard.consecutive_successes;
        let consecutive_failures = stats_guard.consecutive_failures;
        drop(stats_guard);

        // Determine if state change is needed
        let mut is_up_guard = is_up.write().await;
        let was_up = *is_up_guard;

        if !was_up && consecutive_successes >= config.rise {
            // Transition to UP
            *is_up_guard = true;
            info!(
                target = config.target,
                rise = config.rise,
                "Service is now HEALTHY (rise threshold met)"
            );
        } else if was_up && consecutive_failures >= config.fall {
            // Transition to DOWN
            *is_up_guard = false;
            warn!(
                target = config.target,
                fall = config.fall,
                "Service is now UNHEALTHY (fall threshold met)"
            );
        }

        // Log check result
        match result.status {
            HealthStatus::Healthy => {
                debug!(
                    target = config.target,
                    duration_ms = result.duration.as_millis(),
                    consecutive = consecutive_successes,
                    "Health check passed"
                );
            }
            HealthStatus::Unhealthy => {
                warn!(
                    target = config.target,
                    message = result.message.as_deref().unwrap_or("unknown"),
                    consecutive = consecutive_failures,
                    "Health check failed"
                );
            }
            HealthStatus::Timeout => {
                warn!(
                    target = config.target,
                    consecutive = consecutive_failures,
                    "Health check timed out"
                );
            }
            HealthStatus::Error => {
                warn!(
                    target = config.target,
                    error = result.message.as_deref().unwrap_or("unknown"),
                    consecutive = consecutive_failures,
                    "Health check error"
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checkers::TcpChecker;
    use std::net::SocketAddr;

    #[tokio::test]
    async fn test_monitor_creation() {
        let checker = Arc::new(TcpChecker::new(
            "127.0.0.1:80".parse::<SocketAddr>().unwrap(),
            Duration::from_secs(1),
        ));

        let config = HealthCheckConfig {
            target: "127.0.0.1:80".to_string(),
            timeout: Duration::from_secs(1),
            interval: Duration::from_secs(5),
            rise: 2,
            fall: 3,
            check_type: crate::types::CheckType::Tcp,
        };

        let monitor = HealthCheckMonitor::new(checker, config);

        // Should start as unhealthy
        assert!(!monitor.is_healthy().await);

        let stats = monitor.get_stats().await;
        assert_eq!(stats.total_checks, 0);
    }

    #[tokio::test]
    async fn test_monitor_lifecycle() {
        let checker = Arc::new(TcpChecker::new(
            "127.0.0.1:1".parse::<SocketAddr>().unwrap(),
            Duration::from_millis(100),
        ));

        let config = HealthCheckConfig {
            target: "127.0.0.1:1".to_string(),
            timeout: Duration::from_millis(100),
            interval: Duration::from_millis(200),
            rise: 2,
            fall: 2,
            check_type: crate::types::CheckType::Tcp,
        };

        let monitor = HealthCheckMonitor::new(checker, config);
        monitor.start().await;

        // Wait for a few checks
        sleep(Duration::from_millis(500)).await;

        let stats = monitor.get_stats().await;
        assert!(stats.total_checks > 0);

        monitor.stop().await;
    }
}
