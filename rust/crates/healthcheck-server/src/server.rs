//! Main healthcheck server implementation.

use crate::manager::Manager;
use crate::notifier::Notifier;
use crate::types::{HealthcheckConfig, Notification, ProxyToServerMsg, ServerConfig, ServerToProxyMsg};
use tokio::sync::mpsc;
use tracing::info;

/// Healthcheck server
pub struct HealthcheckServer {
    config: ServerConfig,
}

impl HealthcheckServer {
    /// Create a new healthcheck server
    pub fn new(config: ServerConfig) -> Self {
        Self { config }
    }

    /// Run the server
    pub async fn run(self) -> Result<(), Box<dyn std::error::Error>> {
        info!("Starting healthcheck server");

        // Create channels
        let (notify_tx, notify_rx) = mpsc::channel::<Notification>(self.config.channel_size);
        let (config_tx, config_rx) = mpsc::channel::<Vec<HealthcheckConfig>>(10);
        let (proxy_msg_tx, proxy_msg_rx) = mpsc::channel::<ServerToProxyMsg>(self.config.channel_size);
        let (server_msg_tx, _server_msg_rx) = mpsc::channel::<ProxyToServerMsg>(10);

        // Create components
        let manager = Manager::new(notify_tx, config_rx);
        let notifier = Notifier::new(
            notify_rx,
            proxy_msg_tx.clone(),
            self.config.batch_delay,
            self.config.batch_size,
        );

        // Spawn tasks
        let manager_handle = tokio::spawn(async move {
            manager.run().await;
        });

        let notifier_handle = tokio::spawn(async move {
            notifier.run().await;
        });

        info!("All tasks spawned, server running");

        // Wait for tasks to complete (they shouldn't unless shutdown)
        tokio::select! {
            _ = manager_handle => {
                info!("Manager task completed");
            }
            _ = notifier_handle => {
                info!("Notifier task completed");
            }
        }

        info!("Healthcheck server stopped");
        Ok(())
    }
}
