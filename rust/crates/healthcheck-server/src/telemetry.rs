//! OpenTelemetry integration for distributed tracing
//!
//! This module provides OpenTelemetry tracing capabilities for the healthcheck server,
//! enabling distributed tracing and observability.

use opentelemetry::{trace::TracerProvider as _, KeyValue};
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::{
    runtime,
    trace::{RandomIdGenerator, Sampler, TracerProvider},
    Resource,
};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

/// OpenTelemetry tracer guard
///
/// When dropped, flushes all pending spans and shuts down the tracer
pub struct TelemetryGuard;

impl Drop for TelemetryGuard {
    fn drop(&mut self) {
        opentelemetry::global::shutdown_tracer_provider();
    }
}

/// Initialize OpenTelemetry tracing with OTLP exporter
///
/// # Arguments
///
/// * `service_name` - Name of the service (e.g., "healthcheck-server")
/// * `otlp_endpoint` - OTLP collector endpoint (e.g., "http://localhost:4317")
/// * `enabled` - Whether to enable OpenTelemetry tracing
///
/// # Returns
///
/// A `TelemetryGuard` that must be kept alive for the duration of the program.
/// Dropping it will flush pending spans and shut down the tracer.
pub async fn init_telemetry(
    service_name: &str,
    otlp_endpoint: &str,
    enabled: bool,
) -> Result<Option<TelemetryGuard>, Box<dyn std::error::Error>> {
    if !enabled {
        tracing::info!("OpenTelemetry tracing disabled");
        return Ok(None);
    }

    tracing::info!(
        service_name = service_name,
        otlp_endpoint = otlp_endpoint,
        "Initializing OpenTelemetry tracing"
    );

    // Create OTLP exporter
    let exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_tonic()
        .with_endpoint(otlp_endpoint)
        .build()?;

    // Create resource with service information
    let resource = Resource::new(vec![
        KeyValue::new("service.name", service_name.to_string()),
        KeyValue::new("service.version", env!("CARGO_PKG_VERSION").to_string()),
    ]);

    // Create tracer provider with batch span processor
    let tracer = TracerProvider::builder()
        .with_batch_exporter(exporter, runtime::Tokio)
        .with_config(
            opentelemetry_sdk::trace::Config::default()
                .with_sampler(Sampler::AlwaysOn)
                .with_id_generator(RandomIdGenerator::default())
                .with_resource(resource),
        )
        .build();

    // Set global tracer provider
    opentelemetry::global::set_tracer_provider(tracer);

    tracing::info!("OpenTelemetry tracing initialized successfully");

    Ok(Some(TelemetryGuard))
}

/// Initialize OpenTelemetry with HTTP OTLP exporter (alternative to gRPC)
///
/// Use this when the OTLP collector only supports HTTP/JSON instead of gRPC.
pub async fn init_telemetry_http(
    service_name: &str,
    otlp_http_endpoint: &str,
    enabled: bool,
) -> Result<Option<TelemetryGuard>, Box<dyn std::error::Error>> {
    if !enabled {
        tracing::info!("OpenTelemetry tracing disabled");
        return Ok(None);
    }

    tracing::info!(
        service_name = service_name,
        otlp_http_endpoint = otlp_http_endpoint,
        "Initializing OpenTelemetry tracing with HTTP exporter"
    );

    // Create HTTP OTLP exporter
    let exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_http()
        .with_endpoint(otlp_http_endpoint)
        .build()?;

    // Create resource
    let resource = Resource::new(vec![
        KeyValue::new("service.name", service_name.to_string()),
        KeyValue::new("service.version", env!("CARGO_PKG_VERSION").to_string()),
    ]);

    // Create tracer provider
    let tracer = TracerProvider::builder()
        .with_batch_exporter(exporter, runtime::Tokio)
        .with_config(
            opentelemetry_sdk::trace::Config::default()
                .with_sampler(Sampler::AlwaysOn)
                .with_id_generator(RandomIdGenerator::default())
                .with_resource(resource),
        )
        .build();

    opentelemetry::global::set_tracer_provider(tracer);

    tracing::info!("OpenTelemetry HTTP tracing initialized successfully");

    Ok(Some(TelemetryGuard))
}

/// Setup tracing-subscriber with OpenTelemetry layer
///
/// This integrates OpenTelemetry with the standard `tracing` crate,
/// allowing all `tracing::*` macros to export spans to OTLP.
pub async fn setup_tracing_with_otel(
    service_name: &str,
    otlp_endpoint: &str,
    enabled: bool,
    log_level: &str,
) -> Result<Option<TelemetryGuard>, Box<dyn std::error::Error>> {
    // Initialize OpenTelemetry first
    let guard = init_telemetry(service_name, otlp_endpoint, enabled).await?;

    // Create OpenTelemetry layer for tracing-subscriber
    if enabled {
        let telemetry_layer = tracing_opentelemetry::layer()
            .with_tracer(opentelemetry::global::tracer("healthcheck-server"));

        // Setup tracing subscriber with both stdout and OpenTelemetry
        tracing_subscriber::registry()
            .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(log_level)))
            .with(tracing_subscriber::fmt::layer())
            .with(telemetry_layer)
            .init();

        tracing::info!("Tracing initialized with OpenTelemetry integration");
    } else {
        // Just stdout logging
        tracing_subscriber::registry()
            .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(log_level)))
            .with(tracing_subscriber::fmt::layer())
            .init();

        tracing::info!("Tracing initialized without OpenTelemetry");
    }

    Ok(guard)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_telemetry_disabled() {
        let result = init_telemetry("test-service", "http://localhost:4317", false).await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_telemetry_http_disabled() {
        let result =
            init_telemetry_http("test-service", "http://localhost:4318/v1/traces", false).await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }
}
