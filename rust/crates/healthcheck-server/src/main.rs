//! Seesaw Healthcheck Server binary

use healthcheck_server::{Config, HealthcheckServer, setup_tracing_with_otel};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load configuration first (needed for telemetry settings)
    let yaml_config = match Config::load() {
        Ok(cfg) => {
            // Can't use tracing yet - not initialized
            Some(cfg)
        }
        Err(e) => {
            eprintln!("Configuration error: {}", e);
            eprintln!("Using default configuration");
            None
        }
    };

    // Get telemetry settings
    let (telemetry_enabled, service_name, otlp_endpoint, log_level) =
        if let Some(ref cfg) = yaml_config {
            (
                cfg.telemetry.enabled,
                cfg.telemetry.service_name.clone(),
                cfg.telemetry.otlp_endpoint.clone(),
                cfg.logging
                    .level
                    .as_deref()
                    .unwrap_or("info")
                    .to_string(),
            )
        } else {
            (
                false,
                "healthcheck-server".into(),
                "http://localhost:4317".into(),
                "info".into(),
            )
        };

    // Initialize tracing with OpenTelemetry (if enabled)
    let _telemetry_guard =
        setup_tracing_with_otel(&service_name, &otlp_endpoint, telemetry_enabled, &log_level)
            .await?;

    tracing::info!("Seesaw Healthcheck Server starting");

    // Convert to ServerConfig
    let server_config = yaml_config
        .map(|cfg| {
            tracing::info!("Configuration loaded successfully");
            cfg.to_server_config()
        })
        .unwrap_or_else(|| {
            tracing::warn!("Using default configuration");
            healthcheck_server::ServerConfig::default()
        });

    let server = HealthcheckServer::new(server_config);

    // Run server
    server.run().await?;

    // Telemetry guard will flush spans on drop

    Ok(())
}
