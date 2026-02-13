//! Manager for healthcheck monitor lifecycle.

use crate::metrics::MetricsRegistry;
use crate::types::{CheckerConfig, HealthcheckConfig, HealthcheckId, Notification, State, Status};
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
            Self::monitor_task(
                monitors_clone,
                notify_tx_clone,
                monitor_interval,
                metrics_clone,
            )
            .await;
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

                HttpChecker::new(url, req_method, expected_codes.clone(), config.timeout)
                    .ok()
                    .map(Arc::new)?
            }
            crate::types::CheckerConfig::Dns {
                query,
                expected_ips,
            } => Arc::new(DnsChecker::new(
                query.clone(),
                expected_ips.clone(),
                config.timeout,
            )),
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

            // Collect monitor snapshots without holding DashMap locks across awaits.
            // This avoids blocking other tasks that need to access monitors.
            struct MonitorSnapshot {
                id: HealthcheckId,
                is_healthy: bool,
                stats: healthcheck::types::HealthCheckStats,
                checker_type: &'static str,
                prev_successes: u64,
                prev_failures: u64,
                last_state: State,
            }

            let mut snapshots = Vec::with_capacity(monitors.len());
            for entry in monitors.iter() {
                let (id, state) = entry.pair();
                let is_healthy = state.monitor.is_healthy().await;
                let stats = state.monitor.get_stats().await;
                let checker_type = match &state.config.checker {
                    CheckerConfig::Tcp { .. } => "tcp",
                    CheckerConfig::Http { .. } => "http",
                    CheckerConfig::Dns { .. } => "dns",
                };
                snapshots.push(MonitorSnapshot {
                    id: *id,
                    is_healthy,
                    stats,
                    checker_type,
                    prev_successes: state.successes,
                    prev_failures: state.failures,
                    last_state: state.last_state,
                });
            }

            // Process snapshots without holding any DashMap locks
            for snap in &snapshots {
                // Format ID once, reuse across all metric calls
                let id_str = snap.id.to_string();
                let response_time = Duration::from_millis(snap.stats.avg_response_time_ms as u64);

                // Record check metrics
                if let Some(ref m) = metrics {
                    if snap.stats.successful_checks > snap.prev_successes {
                        m.record_check_with_id(&id_str, snap.checker_type, "success", response_time);
                    }
                    if snap.stats.failed_checks > snap.prev_failures {
                        m.record_check_with_id(&id_str, snap.checker_type, "failure", response_time);
                    }
                    m.update_state_with_id(&id_str, snap.checker_type, snap.is_healthy);
                    m.update_consecutive_with_id(
                        &id_str,
                        snap.checker_type,
                        snap.stats.consecutive_successes as u64,
                        snap.stats.consecutive_failures as u64,
                    );
                }

                // Determine new state
                let new_state = if snap.is_healthy {
                    State::Healthy
                } else {
                    State::Unhealthy
                };

                // Update DashMap entry (brief lock)
                if let Some(mut entry) = monitors.get_mut(&snap.id) {
                    entry.successes = snap.stats.successful_checks;
                    entry.failures = snap.stats.failed_checks;

                    if new_state != snap.last_state {
                        entry.last_state = new_state;
                    }
                }

                // Send notification on state change (outside DashMap lock)
                if new_state != snap.last_state {
                    info!(
                        id = snap.id,
                        old_state = ?snap.last_state,
                        new_state = ?new_state,
                        "Health state changed"
                    );

                    if let Some(ref m) = metrics {
                        m.record_state_transition(snap.id, snap.checker_type, snap.last_state, new_state);
                    }

                    let notification = Notification {
                        id: snap.id,
                        status: Status {
                            last_check: Some(SystemTime::now()),
                            duration: response_time,
                            failures: snap.stats.failed_checks,
                            successes: snap.stats.successful_checks,
                            state: new_state,
                            message: format!(
                                "{}/{} checks successful",
                                snap.stats.successful_checks, snap.stats.total_checks
                            ),
                        },
                    };

                    if let Err(e) = notify_tx.send(notification).await {
                        warn!(error = %e, "Failed to send notification");
                    }
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
                    stats.successful_checks, stats.total_checks
                ),
            };

            statuses.push((*id, status));
        }

        statuses
    }
}
