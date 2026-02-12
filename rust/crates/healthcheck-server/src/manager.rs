//! Manager for healthcheck monitor lifecycle.

use crate::metrics::MetricsRegistry;
use crate::types::{HealthcheckConfig, HealthcheckId, Notification, State, Status, CheckerConfig};
use dashmap::DashMap;
use healthcheck::{
    checkers::{DnsChecker, HealthChecker, HttpChecker, TcpChecker},
    monitor::HealthCheckMonitor,
};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

/// Manages the lifecycle of healthcheck monitors
pub struct Manager {
    /// Active monitors mapped by ID
    monitors: Arc<DashMap<HealthcheckId, MonitorState>>,

    /// Channel for sending notifications
    notify_tx: mpsc::Sender<Notification>,

    /// Configuration update receiver
    config_rx: mpsc::Receiver<Vec<HealthcheckConfig>>,

    /// Monitor polling interval
    monitor_interval: Duration,

    /// Metrics registry (optional)
    metrics: Option<Arc<MetricsRegistry>>,
}

/// State for a single monitor
struct MonitorState {
    config: HealthcheckConfig,
    monitor: HealthCheckMonitor,
    failed: u64,
    successes: u64,
    failures: u64,
    last_state: State,
}

impl Manager {
    /// Create a new manager
    pub fn new(
        notify_tx: mpsc::Sender<Notification>,
        config_rx: mpsc::Receiver<Vec<HealthcheckConfig>>,
        monitor_interval: Duration,
        metrics: Option<Arc<MetricsRegistry>>,
    ) -> Self {
        Self {
            monitors: Arc::new(DashMap::new()),
            notify_tx,
            config_rx,
            monitor_interval,
            metrics,
        }
    }

    /// Run the manager task
    pub async fn run(mut self) {
        info!("Manager task started");

        // Spawn monitor tasks
        let monitors_clone = self.monitors.clone();
        let notify_tx_clone = self.notify_tx.clone();
        let monitor_interval = self.monitor_interval;
        let metrics_clone = self.metrics.clone();
        tokio::spawn(async move {
            Self::monitor_task(monitors_clone, notify_tx_clone, monitor_interval, metrics_clone).await;
        });

        // Handle configuration updates
        while let Some(configs) = self.config_rx.recv().await {
            self.update_configs(configs).await;
        }

        info!("Manager task stopped");
    }

    /// Update healthcheck configurations
    async fn update_configs(&mut self, configs: Vec<HealthcheckConfig>) {
        debug!("Updating {} healthcheck configs", configs.len());

        // Build set of new config IDs
        let new_ids: std::collections::HashSet<_> = configs.iter().map(|c| c.id).collect();

        // Remove deleted healthchecks
        self.monitors.retain(|id, _| {
            if !new_ids.contains(id) {
                info!(id = *id, "Removing healthcheck");
                false
            } else {
                true
            }
        });

        // Update monitor count after removals
        if let Some(ref m) = self.metrics {
            m.update_monitor_count(self.monitors.len());
        }

        // Add or update healthchecks
        for config in configs {
            if let Some(mut entry) = self.monitors.get_mut(&config.id) {
                // Update existing monitor if config changed
                if entry.config.to_monitor_config() != config.to_monitor_config() {
                    info!(id = config.id, "Updating healthcheck config");

                    // Create new monitor with updated config
                    if let Some(new_monitor) = Self::create_monitor(&config) {
                        entry.monitor = new_monitor;
                        entry.config = config;

                        // Restart monitor
                        entry.monitor.start().await;
                    }
                }
            } else {
                    let id = config.id;

                // Create new monitor
                if let Some(monitor) = Self::create_monitor(&config) {
                    info!(id = config.id, target = %config.to_monitor_config().target, "Adding healthcheck");

                    let state = MonitorState {
                        config: config.clone(),
                        monitor,
                        failed: 0,
                        successes: 0,
                        failures: 0,
                        last_state: State::Unknown,
                    };

                    self.monitors.insert(id, state);

                    // Start the monitor
                    if let Some(entry) = self.monitors.get(&id) {
                        entry.monitor.start().await;
                    }
                }
            }
        }

        // Update monitor count after additions
        if let Some(ref m) = self.metrics {
            m.update_monitor_count(self.monitors.len());
        }
    }

    /// Create a healthcheck monitor from configuration
    fn create_monitor(config: &HealthcheckConfig) -> Option<HealthCheckMonitor> {
        let monitor_config = config.to_monitor_config();

        let checker: Arc<dyn HealthChecker> = match &config.checker {
            crate::types::CheckerConfig::Tcp { ip, port } => {
                let addr = format!("{}:{}", ip, port).parse::<SocketAddr>().ok()?;
                Arc::new(TcpChecker::new(addr, config.timeout))
            }
            crate::types::CheckerConfig::Http {
                ip,
                port,
                method,
                path,
                expected_codes,
                secure,
            } => {
                let protocol = if *secure { "https" } else { "http" };
                let url = format!("{}://{}:{}{}", protocol, ip, port, path);
                let req_method = match method.to_uppercase().as_str() {
                    "GET" => reqwest::Method::GET,
                    "POST" => reqwest::Method::POST,
                    "HEAD" => reqwest::Method::HEAD,
                    "PUT" => reqwest::Method::PUT,
                    "DELETE" => reqwest::Method::DELETE,
                    _ => reqwest::Method::GET,
                };

                HttpChecker::new(url, req_method, expected_codes.clone(), config.timeout).ok().map(Arc::new)?
            }
            crate::types::CheckerConfig::Dns { query, expected_ips } => {
                Arc::new(DnsChecker::new(
                    query.clone(),
                    expected_ips.clone(),
                    config.timeout,
                ))
            }
        };

        Some(HealthCheckMonitor::new(checker, monitor_config))
    }

    /// Background task to monitor health status and send notifications
    async fn monitor_task(
        monitors: Arc<DashMap<HealthcheckId, MonitorState>>,
        notify_tx: mpsc::Sender<Notification>,
        monitor_interval: Duration,
        metrics: Option<Arc<MetricsRegistry>>,
    ) {
        let mut interval = tokio::time::interval(monitor_interval);

        loop {
            let start = std::time::Instant::now();
            interval.tick().await;

            // Update active monitor count
            if let Some(ref m) = metrics {
                m.update_monitor_count(monitors.len());
            }

            // Check all monitors
            for mut entry in monitors.iter_mut() {
                let (id, state) = entry.pair_mut();

                // Get current health status
                let is_healthy = state.monitor.is_healthy().await;
                let stats = state.monitor.get_stats().await;

                // Determine checker type for metrics
                let checker_type = match &state.config.checker {
                    CheckerConfig::Tcp { .. } => "tcp",
                    CheckerConfig::Http { .. } => "http",
                    CheckerConfig::Dns { .. } => "dns",
                };

                // Record check metrics (detect new checks by comparing counts)
                if let Some(ref m) = metrics {
                    let new_successes = stats.successful_checks;
                    let new_failures = stats.failed_checks;
                    let response_time = Duration::from_millis(stats.avg_response_time_ms as u64);

                    if new_successes > state.successes {
                        m.record_check(*id, checker_type, "success", response_time);
                    }
                    if new_failures > state.failures {
                        m.record_check(*id, checker_type, "failure", response_time);
                    }

                    // Update state and consecutive count gauges
                    m.update_state(*id, checker_type, is_healthy);
                    m.update_consecutive(*id, checker_type, stats.consecutive_successes as u64, stats.consecutive_failures as u64);
                }

                // Update counts
                state.successes = stats.successful_checks;
                state.failures = stats.failed_checks;

                // Determine new state
                let new_state = if is_healthy {
                    State::Healthy
                } else {
                    State::Unhealthy
                };

                // Send notification on state change
                if new_state != state.last_state {
                    info!(
                        id = *id,
                        old_state = ?state.last_state,
                        new_state = ?new_state,
                        "Health state changed"
                    );

                    // Record state transition metric
                    if let Some(ref m) = metrics {
                        m.record_state_transition(*id, checker_type, state.last_state, new_state);
                    }

                    let notification = Notification {
                        id: *id,
                        status: Status {
                            last_check: Some(SystemTime::now()),
                            duration: Duration::from_millis(stats.avg_response_time_ms as u64),
                            failures: stats.failed_checks,
                            successes: stats.successful_checks,
                            state: new_state,
                            message: format!(
                                "{}/{} checks successful",
                                stats.successful_checks,
                                stats.total_checks
                            ),
                        },
                    };

                    if let Err(e) = notify_tx.send(notification).await {
                        warn!(error = %e, "Failed to send notification");
                    }

                    state.last_state = new_state;
                }
            }

            // Record monitor task duration
            if let Some(ref m) = metrics {
                m.record_monitor_task_duration(start.elapsed());
            }
        }
    }

    /// Get status for all monitors
    pub async fn get_statuses(&self) -> Vec<(HealthcheckId, Status)> {
        let mut statuses = Vec::new();

        for entry in self.monitors.iter() {
            let (id, state) = entry.pair();
            let stats = state.monitor.get_stats().await;

            let status = Status {
                last_check: Some(SystemTime::now()),
                duration: Duration::from_millis(stats.avg_response_time_ms as u64),
                failures: stats.failed_checks,
                successes: stats.successful_checks,
                state: state.last_state,
                message: format!(
                    "{}/{} checks successful",
                    stats.successful_checks,
                    stats.total_checks
                ),
            };

            statuses.push((*id, status));
        }

        statuses
    }
}
