//! Prometheus metrics for healthcheck server.

use crate::types::State;
use prometheus_client::encoding::EncodeLabelSet;
use prometheus_client::metrics::counter::Counter;
use prometheus_client::metrics::family::Family;
use prometheus_client::metrics::gauge::Gauge;
use prometheus_client::metrics::histogram::{exponential_buckets, Histogram};
use prometheus_client::registry::Registry;
use std::time::Duration;

/// Labels for per-healthcheck metrics
#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
pub struct HealthcheckLabels {
    /// Healthcheck ID
    pub id: String,
    /// Checker type (tcp, http, dns)
    pub checker_type: String,
}

/// Labels for check result metrics
#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
pub struct CheckLabels {
    /// Healthcheck ID
    pub id: String,
    /// Checker type (tcp, http, dns)
    pub checker_type: String,
    /// Result (success, failure, timeout, error)
    pub result: String,
}

/// Labels for state transition metrics
#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
pub struct StateTransitionLabels {
    /// Healthcheck ID
    pub id: String,
    /// Checker type (tcp, http, dns)
    pub checker_type: String,
    /// From state
    pub from: String,
    /// To state
    pub to: String,
}

/// Labels for batch trigger metrics
#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
pub struct BatchTriggerLabels {
    /// Trigger reason (size_limit, time_delay)
    pub trigger: String,
}

/// Labels for state-based metrics
#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
pub struct StateLabels {
    /// State (healthy, unhealthy, unknown)
    pub state: String,
}

/// Labels for channel metrics
#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
pub struct ChannelLabels {
    /// Channel name (notification, config_update, proxy_message)
    pub channel: String,
}

/// Labels for error metrics
#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
pub struct ErrorLabels {
    /// Error type (channel, socket, parse, etc.)
    pub error_type: String,
}

/// Metrics registry with all healthcheck server metrics
pub struct MetricsRegistry {
    /// Prometheus registry
    pub registry: Registry,

    // Per-healthcheck metrics
    /// Total checks performed
    checks_total: Family<CheckLabels, Counter>,
    /// Check response time
    response_time_seconds: Family<HealthcheckLabels, Histogram>,
    /// Current health state (0=unknown, 1=healthy, 2=unhealthy)
    state: Family<HealthcheckLabels, Gauge>,
    /// Consecutive successes
    consecutive_successes: Family<HealthcheckLabels, Gauge>,
    /// Consecutive failures
    consecutive_failures: Family<HealthcheckLabels, Gauge>,
    /// State transitions
    state_transitions_total: Family<StateTransitionLabels, Counter>,

    // System-wide metrics
    /// Active monitors
    monitors_active: Gauge,
    /// Monitors by state
    monitors_by_state: Family<StateLabels, Gauge>,
    /// Notifications batched
    notifications_batched_total: Counter,
    /// Notifications sent (by trigger)
    notifications_sent_total: Family<BatchTriggerLabels, Counter>,
    /// Batch size histogram
    batch_size: Histogram,
    /// Batch delay histogram
    batch_delay_seconds: Histogram,
    /// Proxy connection state
    proxy_connected: Gauge,
    /// Config updates received
    config_updates_total: Counter,
    /// Errors by type
    errors_total: Family<ErrorLabels, Counter>,

    // Resource metrics
    /// Channel depth
    channel_depth: Family<ChannelLabels, Gauge>,
    /// Monitor task duration
    monitor_task_duration_seconds: Histogram,
}

impl MetricsRegistry {
    /// Create a new metrics registry with custom histogram buckets
    /// Note: Custom buckets will be fully implemented in Phase 6.2
    pub fn new(
        _response_time_buckets: &[f64],
        batch_delay_buckets: &[f64],
        batch_size_buckets: &[f64],
    ) -> Self {
        let mut registry = Registry::default();

        // Per-healthcheck metrics
        let checks_total = Family::<CheckLabels, Counter>::default();
        registry.register(
            "healthcheck_checks_total",
            "Total health checks performed",
            checks_total.clone(),
        );

        // For now, use exponential buckets to avoid closure type complexity
        // Custom buckets will be handled in Phase 6.2 when integrating with config
        let response_time_seconds = Family::<HealthcheckLabels, Histogram>::new_with_constructor(|| {
            // Exponential buckets from 1ms to ~10s
            Histogram::new(exponential_buckets(0.001, 2.0, 14))
        });
        registry.register(
            "healthcheck_response_time_seconds",
            "Health check response time in seconds",
            response_time_seconds.clone(),
        );

        let state = Family::<HealthcheckLabels, Gauge>::default();
        registry.register(
            "healthcheck_state",
            "Current health state (0=unknown, 1=healthy, 2=unhealthy)",
            state.clone(),
        );

        let consecutive_successes = Family::<HealthcheckLabels, Gauge>::default();
        registry.register(
            "healthcheck_consecutive_successes",
            "Current consecutive success count",
            consecutive_successes.clone(),
        );

        let consecutive_failures = Family::<HealthcheckLabels, Gauge>::default();
        registry.register(
            "healthcheck_consecutive_failures",
            "Current consecutive failure count",
            consecutive_failures.clone(),
        );

        let state_transitions_total = Family::<StateTransitionLabels, Counter>::default();
        registry.register(
            "healthcheck_state_transitions_total",
            "Total health state transitions",
            state_transitions_total.clone(),
        );

        // System-wide metrics
        let monitors_active = Gauge::default();
        registry.register(
            "healthcheck_monitors_active",
            "Number of active monitors",
            monitors_active.clone(),
        );

        let monitors_by_state = Family::<StateLabels, Gauge>::default();
        registry.register(
            "healthcheck_monitors_by_state",
            "Monitors by health state",
            monitors_by_state.clone(),
        );

        let notifications_batched_total = Counter::default();
        registry.register(
            "healthcheck_notifications_batched_total",
            "Total notifications batched",
            notifications_batched_total.clone(),
        );

        let notifications_sent_total = Family::<BatchTriggerLabels, Counter>::default();
        registry.register(
            "healthcheck_notifications_sent_total",
            "Total notification batches sent",
            notifications_sent_total.clone(),
        );

        let batch_size = Histogram::new(batch_size_buckets.iter().copied());
        registry.register(
            "healthcheck_batch_size",
            "Notification batch size",
            batch_size.clone(),
        );

        let batch_delay_seconds = Histogram::new(batch_delay_buckets.iter().copied());
        registry.register(
            "healthcheck_batch_delay_seconds",
            "Actual batch delay in seconds",
            batch_delay_seconds.clone(),
        );

        let proxy_connected = Gauge::default();
        registry.register(
            "healthcheck_proxy_connected",
            "Proxy connection state (1=connected, 0=disconnected)",
            proxy_connected.clone(),
        );

        let config_updates_total = Counter::default();
        registry.register(
            "healthcheck_config_updates_total",
            "Total config updates received",
            config_updates_total.clone(),
        );

        let errors_total = Family::<ErrorLabels, Counter>::default();
        registry.register(
            "healthcheck_errors_total",
            "Total errors by type",
            errors_total.clone(),
        );

        // Resource metrics
        let channel_depth = Family::<ChannelLabels, Gauge>::default();
        registry.register(
            "healthcheck_channel_depth",
            "Channel queue depth",
            channel_depth.clone(),
        );

        let monitor_task_duration_seconds = Histogram::new(exponential_buckets(0.001, 2.0, 10));
        registry.register(
            "healthcheck_monitor_task_duration_seconds",
            "Monitor task iteration duration",
            monitor_task_duration_seconds.clone(),
        );

        Self {
            registry,
            checks_total,
            response_time_seconds,
            state,
            consecutive_successes,
            consecutive_failures,
            state_transitions_total,
            monitors_active,
            monitors_by_state,
            notifications_batched_total,
            notifications_sent_total,
            batch_size,
            batch_delay_seconds,
            proxy_connected,
            config_updates_total,
            errors_total,
            channel_depth,
            monitor_task_duration_seconds,
        }
    }

    /// Record a health check result
    pub fn record_check(
        &self,
        id: u64,
        checker_type: &str,
        result: &str,
        response_time: Duration,
    ) {
        // Increment check counter
        self.checks_total
            .get_or_create(&CheckLabels {
                id: id.to_string(),
                checker_type: checker_type.to_string(),
                result: result.to_string(),
            })
            .inc();

        // Record response time
        self.response_time_seconds
            .get_or_create(&HealthcheckLabels {
                id: id.to_string(),
                checker_type: checker_type.to_string(),
            })
            .observe(response_time.as_secs_f64());
    }

    /// Update health state gauge
    pub fn update_state(&self, id: u64, checker_type: &str, is_healthy: bool) {
        let state_value = if is_healthy { 1 } else { 2 };

        self.state
            .get_or_create(&HealthcheckLabels {
                id: id.to_string(),
                checker_type: checker_type.to_string(),
            })
            .set(state_value);
    }

    /// Update consecutive success/failure counts
    pub fn update_consecutive(
        &self,
        id: u64,
        checker_type: &str,
        successes: u64,
        failures: u64,
    ) {
        let labels = HealthcheckLabels {
            id: id.to_string(),
            checker_type: checker_type.to_string(),
        };

        self.consecutive_successes
            .get_or_create(&labels)
            .set(successes as i64);

        self.consecutive_failures
            .get_or_create(&labels)
            .set(failures as i64);
    }

    /// Record a state transition
    pub fn record_state_transition(
        &self,
        id: u64,
        checker_type: &str,
        from_state: State,
        to_state: State,
    ) {
        self.state_transitions_total
            .get_or_create(&StateTransitionLabels {
                id: id.to_string(),
                checker_type: checker_type.to_string(),
                from: state_to_string(from_state),
                to: state_to_string(to_state),
            })
            .inc();
    }

    /// Update active monitor count
    pub fn update_monitor_count(&self, count: usize) {
        self.monitors_active.set(count as i64);
    }

    /// Update monitors by state count
    pub fn update_monitors_by_state(&self, healthy: usize, unhealthy: usize, unknown: usize) {
        self.monitors_by_state
            .get_or_create(&StateLabels {
                state: "healthy".to_string(),
            })
            .set(healthy as i64);

        self.monitors_by_state
            .get_or_create(&StateLabels {
                state: "unhealthy".to_string(),
            })
            .set(unhealthy as i64);

        self.monitors_by_state
            .get_or_create(&StateLabels {
                state: "unknown".to_string(),
            })
            .set(unknown as i64);
    }

    /// Record notification batched
    pub fn record_notification_batched(&self) {
        self.notifications_batched_total.inc();
    }

    /// Record batch sent
    pub fn record_batch_sent(&self, size: usize, trigger: &str, delay: Duration) {
        self.notifications_sent_total
            .get_or_create(&BatchTriggerLabels {
                trigger: trigger.to_string(),
            })
            .inc();

        self.batch_size.observe(size as f64);
        self.batch_delay_seconds.observe(delay.as_secs_f64());
    }

    /// Set proxy connected state
    pub fn set_proxy_connected(&self, connected: bool) {
        self.proxy_connected.set(if connected { 1 } else { 0 });
    }

    /// Record config update received
    pub fn record_config_update(&self) {
        self.config_updates_total.inc();
    }

    /// Record error by type
    pub fn record_error(&self, error_type: &str) {
        self.errors_total
            .get_or_create(&ErrorLabels {
                error_type: error_type.to_string(),
            })
            .inc();
    }

    /// Update channel depth
    pub fn update_channel_depth(&self, channel: &str, depth: usize) {
        self.channel_depth
            .get_or_create(&ChannelLabels {
                channel: channel.to_string(),
            })
            .set(depth as i64);
    }

    /// Record monitor task duration
    pub fn record_monitor_task_duration(&self, duration: Duration) {
        self.monitor_task_duration_seconds
            .observe(duration.as_secs_f64());
    }
}

/// Convert State enum to string for labels
fn state_to_string(state: State) -> String {
    match state {
        State::Unknown => "unknown".to_string(),
        State::Healthy => "healthy".to_string(),
        State::Unhealthy => "unhealthy".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_registry_creation() {
        let _registry = MetricsRegistry::new(
            &[0.001, 0.01, 0.1, 1.0],
            &[0.01, 0.1, 1.0],
            &[1.0, 10.0, 100.0],
        );

        // Verify registry was created successfully
        // Just verify we can create it without panicking
    }

    #[test]
    fn test_record_check() {
        let registry = MetricsRegistry::new(
            &[0.001, 0.01, 0.1, 1.0],
            &[0.01, 0.1, 1.0],
            &[1.0, 10.0, 100.0],
        );

        // Record successful check
        registry.record_check(1, "tcp", "success", Duration::from_millis(50));

        // Record failed check
        registry.record_check(1, "tcp", "failure", Duration::from_millis(100));

        // Metrics should be updated (can't easily verify counter values without encoding)
    }

    #[test]
    fn test_update_state() {
        let registry = MetricsRegistry::new(
            &[0.001, 0.01, 0.1, 1.0],
            &[0.01, 0.1, 1.0],
            &[1.0, 10.0, 100.0],
        );

        // Update to healthy
        registry.update_state(1, "tcp", true);

        // Update to unhealthy
        registry.update_state(1, "tcp", false);
    }

    #[test]
    fn test_update_consecutive() {
        let registry = MetricsRegistry::new(
            &[0.001, 0.01, 0.1, 1.0],
            &[0.01, 0.1, 1.0],
            &[1.0, 10.0, 100.0],
        );

        registry.update_consecutive(1, "tcp", 5, 0);
        registry.update_consecutive(1, "tcp", 0, 3);
    }

    #[test]
    fn test_record_state_transition() {
        let registry = MetricsRegistry::new(
            &[0.001, 0.01, 0.1, 1.0],
            &[0.01, 0.1, 1.0],
            &[1.0, 10.0, 100.0],
        );

        registry.record_state_transition(1, "tcp", State::Unknown, State::Healthy);
        registry.record_state_transition(1, "tcp", State::Healthy, State::Unhealthy);
    }

    #[test]
    fn test_update_monitor_count() {
        let registry = MetricsRegistry::new(
            &[0.001, 0.01, 0.1, 1.0],
            &[0.01, 0.1, 1.0],
            &[1.0, 10.0, 100.0],
        );

        registry.update_monitor_count(10);
        registry.update_monitor_count(5);
    }

    #[test]
    fn test_batch_metrics() {
        let registry = MetricsRegistry::new(
            &[0.001, 0.01, 0.1, 1.0],
            &[0.01, 0.1, 1.0],
            &[1.0, 10.0, 100.0],
        );

        registry.record_notification_batched();
        registry.record_batch_sent(50, "size_limit", Duration::from_millis(100));
        registry.record_batch_sent(10, "time_delay", Duration::from_millis(50));
    }

    #[test]
    fn test_proxy_connection() {
        let registry = MetricsRegistry::new(
            &[0.001, 0.01, 0.1, 1.0],
            &[0.01, 0.1, 1.0],
            &[1.0, 10.0, 100.0],
        );

        registry.set_proxy_connected(true);
        registry.set_proxy_connected(false);
    }

    #[test]
    fn test_config_updates() {
        let registry = MetricsRegistry::new(
            &[0.001, 0.01, 0.1, 1.0],
            &[0.01, 0.1, 1.0],
            &[1.0, 10.0, 100.0],
        );

        registry.record_config_update();
        registry.record_config_update();
    }

    #[test]
    fn test_errors() {
        let registry = MetricsRegistry::new(
            &[0.001, 0.01, 0.1, 1.0],
            &[0.01, 0.1, 1.0],
            &[1.0, 10.0, 100.0],
        );

        registry.record_error("socket_io");
        registry.record_error("channel_send");
        registry.record_error("parse_error");
    }

    #[test]
    fn test_channel_depth() {
        let registry = MetricsRegistry::new(
            &[0.001, 0.01, 0.1, 1.0],
            &[0.01, 0.1, 1.0],
            &[1.0, 10.0, 100.0],
        );

        registry.update_channel_depth("notification", 50);
        registry.update_channel_depth("config_update", 2);
        registry.update_channel_depth("proxy_message", 0);
    }

    #[test]
    fn test_monitor_task_duration() {
        let registry = MetricsRegistry::new(
            &[0.001, 0.01, 0.1, 1.0],
            &[0.01, 0.1, 1.0],
            &[1.0, 10.0, 100.0],
        );

        registry.record_monitor_task_duration(Duration::from_millis(5));
        registry.record_monitor_task_duration(Duration::from_millis(10));
    }

    #[test]
    fn test_state_to_string() {
        assert_eq!(state_to_string(State::Unknown), "unknown");
        assert_eq!(state_to_string(State::Healthy), "healthy");
        assert_eq!(state_to_string(State::Unhealthy), "unhealthy");
    }
}
