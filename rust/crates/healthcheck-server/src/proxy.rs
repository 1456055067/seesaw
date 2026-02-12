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
    server_tx: mpsc::Sender<ServerToProxyMsg>,
    proxy_rx: mpsc::Receiver<ProxyToServerMsg>,
}

impl ProxyComm {
    /// Create a new proxy communicator
    pub fn new(
        socket_path: String,
        server_tx: mpsc::Sender<ServerToProxyMsg>,
        proxy_rx: mpsc::Receiver<ProxyToServerMsg>,
    ) -> Self {
        Self {
            socket_path,
            server_tx,
            proxy_rx,
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

        // Send ready message
        let _ = self.server_tx.send(ServerToProxyMsg::Ready).await;

        // Handle communication
        self.handle_connection(stream).await?;

        Ok(())
    }

    /// Handle a single connection
    async fn handle_connection(&mut self, stream: UnixStream) -> Result<(), Box<dyn std::error::Error>> {
        let (reader, mut writer) = stream.into_split();
        let mut reader = BufReader::new(reader);

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
                            // Parse message
                            match serde_json::from_str::<ProxyToServerMsg>(line.trim()) {
                                Ok(msg) => {
                                    debug!("Received message from proxy: {:?}", msg);
                                    // Handle message (would send to appropriate channel)
                                    // For now, just log it
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

                // Write to Go proxy
                Some(msg) = self.proxy_rx.recv() => {
                    let json = serde_json::to_string(&msg)?;
                    debug!("Sending message to proxy: {}", json);

                    writer.write_all(json.as_bytes()).await?;
                    writer.write_all(b"\n").await?;
                    writer.flush().await?;
                }
            }
        }

        Ok(())
    }
}
