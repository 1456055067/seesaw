//! HTTP server for Prometheus metrics endpoint.

use crate::metrics::MetricsRegistry;
use axum::{
    Router,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
};
use prometheus_client::encoding::text::encode;
use std::sync::Arc;
use tokio::net::TcpListener;
use tower::ServiceBuilder;
use tower_http::trace::TraceLayer;
use tracing::{info, warn};

/// HTTP server for metrics endpoint
pub struct MetricsServer {
    /// Metrics registry
    registry: Arc<MetricsRegistry>,
    /// Listen address
    listen_addr: String,
}

impl MetricsServer {
    /// Create a new metrics server
    pub fn new(registry: Arc<MetricsRegistry>, listen_addr: String) -> Self {
        Self {
            registry,
            listen_addr,
        }
    }

    /// Run the HTTP server
    pub async fn run(self) -> Result<(), Box<dyn std::error::Error>> {
        info!(listen_addr = %self.listen_addr, "Starting metrics HTTP server");

        // Create router
        let app = Router::new()
            .route("/metrics", get(metrics_handler))
            .layer(ServiceBuilder::new().layer(TraceLayer::new_for_http()))
            .with_state(self.registry);

        // Bind to address
        let listener = TcpListener::bind(&self.listen_addr).await?;
        info!(listen_addr = %self.listen_addr, "Metrics server listening");

        // Run server
        axum::serve(listener, app).await?;

        Ok(())
    }
}

/// Handler for /metrics endpoint
async fn metrics_handler(State(registry): State<Arc<MetricsRegistry>>) -> Response {
    // Encode metrics to Prometheus text format
    let mut buffer = String::new();
    if let Err(e) = encode(&mut buffer, &registry.registry) {
        warn!(error = %e, "Failed to encode metrics");
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to encode metrics: {}", e),
        )
            .into_response();
    }

    // Return with correct content type
    (
        StatusCode::OK,
        [("content-type", "text/plain; version=0.0.4")],
        buffer,
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn test_metrics_handler() {
        // Create registry with test data
        let registry = Arc::new(MetricsRegistry::new(
            &[0.001, 0.01, 0.1, 1.0],
            &[0.01, 0.1, 1.0],
            &[1.0, 10.0, 100.0],
        ));

        // Record some test metrics
        registry.record_check(1, "tcp", "success", Duration::from_millis(50));
        registry.update_monitor_count(5);
        registry.set_proxy_connected(true);

        // Call handler
        let _response = metrics_handler(State(registry)).await;

        // Verify response (would need to parse body in real test)
        // For now, just verify it doesn't panic
    }

    #[tokio::test]
    async fn test_metrics_server_creation() {
        let registry = Arc::new(MetricsRegistry::new(
            &[0.001, 0.01, 0.1, 1.0],
            &[0.01, 0.1, 1.0],
            &[1.0, 10.0, 100.0],
        ));

        let server = MetricsServer::new(registry, "127.0.0.1:0".to_string());
        assert_eq!(server.listen_addr, "127.0.0.1:0");
    }
}
