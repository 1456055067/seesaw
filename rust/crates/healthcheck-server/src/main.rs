//! Seesaw Healthcheck Server binary

use healthcheck_server::{HealthcheckServer, ServerConfig};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "healthcheck_server=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    tracing::info!("Seesaw Healthcheck Server starting");

    // Create server with default config
    let config = ServerConfig::default();
    let server = HealthcheckServer::new(config);

    // Run server
    server.run().await?;

    Ok(())
}
