//! Proxy communication layer for Go<->Rust messages.

use crate::types::{ProxyToServerMsg, ServerToProxyMsg};
use std::path::Path;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

/// Proxy communication handler
pub struct ProxyComm {
    socket_path: String,
    to_proxy_rx: mpsc::Receiver<ServerToProxyMsg>,
    from_proxy_tx: mpsc::Sender<ProxyToServerMsg>,
}

impl ProxyComm {
    /// Create a new proxy communicator
    pub fn new(
        socket_path: String,
        to_proxy_rx: mpsc::Receiver<ServerToProxyMsg>,
        from_proxy_tx: mpsc::Sender<ProxyToServerMsg>,
    ) -> Self {
        Self {
            socket_path,
            to_proxy_rx,
            from_proxy_tx,
        }
    }

    /// Run the proxy communication task
    pub async fn run(mut self) -> Result<(), Box<dyn std::error::Error>> {
        // Remove old socket if it exists
        if Path::new(&self.socket_path).exists() {
            std::fs::remove_file(&self.socket_path)?;
        }

        let listener = UnixListener::bind(&self.socket_path)?;
        info!(socket = %self.socket_path, "Proxy listener started");

        // Wait for Go proxy to connect
        let (stream, _) = listener.accept().await?;
        info!("Go proxy connected");

        // Handle communication (will send Ready message as first message)
        self.handle_connection(stream).await?;

        Ok(())
    }

    /// Handle a single connection
    async fn handle_connection(&mut self, stream: UnixStream) -> Result<(), Box<dyn std::error::Error>> {
        let (reader, mut writer) = stream.into_split();
        let mut reader = BufReader::new(reader);

        // Send Ready message to Go proxy
        let ready_msg = crate::types::ServerToProxyMsg::Ready;
        let json = serde_json::to_string(&ready_msg)?;
        writer.write_all(json.as_bytes()).await?;
        writer.write_all(b"\n").await?;
        writer.flush().await?;
        info!("Sent Ready message to Go proxy");

        let mut line = String::new();

        loop {
            tokio::select! {
                // Read from Go proxy
                result = reader.read_line(&mut line) => {
                    match result {
                        Ok(0) => {
                            info!("Go proxy disconnected");
                            break;
                        }
                        Ok(_) => {
                            // Parse message from Go proxy
                            match serde_json::from_str::<ProxyToServerMsg>(line.trim()) {
                                Ok(msg) => {
                                    debug!("Received message from Go proxy: {:?}", msg);
                                    // Forward to server for handling
                                    if let Err(e) = self.from_proxy_tx.send(msg).await {
                                        error!(error = %e, "Failed to forward message from proxy");
                                    }
                                }
                                Err(e) => {
                                    warn!(error = %e, line = %line.trim(), "Failed to parse proxy message");
                                }
                            }
                            line.clear();
                        }
                        Err(e) => {
                            error!(error = %e, "Failed to read from proxy");
                            break;
                        }
                    }
                }

                // Write to Go proxy (notifications, status, etc.)
                Some(msg) = self.to_proxy_rx.recv() => {
                    let json = serde_json::to_string(&msg)?;
                    debug!("Sending message to Go proxy: {}", json);

                    writer.write_all(json.as_bytes()).await?;
                    writer.write_all(b"\n").await?;
                    writer.flush().await?;
                }
            }
        }

        Ok(())
    }
}
