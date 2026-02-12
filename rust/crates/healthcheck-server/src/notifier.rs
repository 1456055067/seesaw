//! Notifier for batching and sending healthcheck notifications.

use crate::types::{Notification, NotificationBatch, ServerToProxyMsg};
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::{interval, Instant};
use tracing::{debug, info, warn};

/// Notifier batches notifications and sends them to the proxy
pub struct Notifier {
    /// Notification receiver
    notify_rx: mpsc::Receiver<Notification>,

    /// Proxy message sender
    proxy_tx: mpsc::Sender<ServerToProxyMsg>,

    /// Batch delay
    batch_delay: Duration,

    /// Maximum batch size
    batch_size: usize,
}

impl Notifier {
    /// Create a new notifier
    pub fn new(
        notify_rx: mpsc::Receiver<Notification>,
        proxy_tx: mpsc::Sender<ServerToProxyMsg>,
        batch_delay: Duration,
        batch_size: usize,
    ) -> Self {
        Self {
            notify_rx,
            proxy_tx,
            batch_delay,
            batch_size,
        }
    }

    /// Run the notifier task
    pub async fn run(mut self) {
        info!("Notifier task started");

        let mut batch = Vec::new();
        let mut batch_timer = interval(self.batch_delay);
        batch_timer.tick().await; // Skip first immediate tick

        let mut last_batch_time = Instant::now();

        loop {
            tokio::select! {
                // Receive notification
                Some(notification) = self.notify_rx.recv() => {
                    debug!(id = notification.id, state = ?notification.status.state, "Received notification");
                    batch.push(notification);

                    // Send batch if it reaches max size
                    if batch.len() >= self.batch_size {
                        self.send_batch(&mut batch).await;
                        last_batch_time = Instant::now();
                        batch_timer.reset();
                    }
                }

                // Batch delay elapsed
                _ = batch_timer.tick() => {
                    // Only send if we have notifications and delay has passed
                    if !batch.is_empty() && last_batch_time.elapsed() >= self.batch_delay {
                        self.send_batch(&mut batch).await;
                        last_batch_time = Instant::now();
                    }
                }
            }
        }
    }

    /// Send a batch of notifications
    async fn send_batch(&self, batch: &mut Vec<Notification>) {
        if batch.is_empty() {
            return;
        }

        info!("Sending notification batch with {} items", batch.len());

        let msg = ServerToProxyMsg::NotificationBatch {
            batch: NotificationBatch {
                notifications: batch.clone(),
            },
        };

        if let Err(e) = self.proxy_tx.send(msg).await {
            warn!(error = %e, "Failed to send batch to proxy");
        }

        batch.clear();
    }
}
