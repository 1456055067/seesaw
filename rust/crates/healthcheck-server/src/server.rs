//! Main healthcheck server implementation.

use crate::manager::Manager;
use crate::notifier::Notifier;
use crate::proxy::ProxyComm;
use crate::types::{HealthcheckConfig, Notification, ProxyToServerMsg, ServerConfig, ServerToProxyMsg};
use tokio::sync::mpsc;
use tracing::{info, warn};

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
        let (config_tx, config_rx) = mpsc::channel::<Vec<HealthcheckConfig>>(self.config.config_channel_size);
        let (to_proxy_tx, to_proxy_rx) = mpsc::channel::<ServerToProxyMsg>(self.config.channel_size);
        let (from_proxy_tx, from_proxy_rx) = mpsc::channel::<ProxyToServerMsg>(self.config.proxy_channel_size);

        // Create components
        let manager = Manager::new(notify_tx, config_rx, self.config.manager_monitor_interval);
        let notifier = Notifier::new(
            notify_rx,
            to_proxy_tx.clone(),
            self.config.batch_delay,
            self.config.batch_size,
        );
        let proxy = ProxyComm::new(
            self.config.proxy_socket.clone(),
            to_proxy_rx,
            from_proxy_tx,
        );

        // Spawn proxy task
        let proxy_handle = tokio::spawn(async move {
            if let Err(e) = proxy.run().await {
                warn!(error = %e, "Proxy task error");
            }
        });

        // Spawn message handler task (handles messages from Go proxy)
        let message_handler_handle = tokio::spawn(async move {
            Self::handle_proxy_messages(from_proxy_rx, config_tx).await;
        });

        // Spawn manager task
        let manager_handle = tokio::spawn(async move {
            manager.run().await;
        });

        // Spawn notifier task
        let notifier_handle = tokio::spawn(async move {
            notifier.run().await;
        });

        info!("All tasks spawned, server running");

        // Wait for tasks to complete (they shouldn't unless shutdown)
        tokio::select! {
            _ = proxy_handle => {
                info!("Proxy task completed");
            }
            _ = message_handler_handle => {
                info!("Message handler task completed");
            }
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

    /// Handle messages received from Go proxy
    async fn handle_proxy_messages(
        mut from_proxy_rx: mpsc::Receiver<ProxyToServerMsg>,
        config_tx: mpsc::Sender<Vec<HealthcheckConfig>>,
    ) {
        while let Some(msg) = from_proxy_rx.recv().await {
            match msg {
                ProxyToServerMsg::UpdateConfigs { configs } => {
                    info!("Received {} healthcheck configs from proxy", configs.len());
                    if let Err(e) = config_tx.send(configs).await {
                        warn!(error = %e, "Failed to send configs to manager");
                    }
                }
                ProxyToServerMsg::RequestStatus => {
                    // TODO: Implement status request handling
                    info!("Received status request from proxy");
                }
                ProxyToServerMsg::Shutdown => {
                    info!("Received shutdown request from proxy");
                    break;
                }
            }
        }
    }
}
