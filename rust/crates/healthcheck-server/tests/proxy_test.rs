//! Integration tests for Proxy component

use healthcheck_server::proxy::ProxyComm;
use healthcheck_server::types::{
    CheckerConfig, HealthcheckConfig, Notification, ProxyToServerMsg, ServerToProxyMsg, State,
    Status,
};
use std::time::{Duration, SystemTime};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::sync::mpsc;

/// Helper to create a test socket path
fn test_socket_path(test_name: &str) -> String {
    format!("/tmp/healthcheck-proxy-test-{}.sock", test_name)
}

/// Helper to create a TCP config for testing
fn tcp_config(id: u64) -> HealthcheckConfig {
    HealthcheckConfig {
        id,
        interval: Duration::from_secs(5),
        timeout: Duration::from_secs(1),
        retries: 2,
        checker: CheckerConfig::Tcp {
            ip: "127.0.0.1".parse().unwrap(),
            port: 8080,
        },
    }
}

#[tokio::test]
async fn test_proxy_sends_ready_message() {
    let socket_path = test_socket_path("ready");
    let (to_proxy_tx, to_proxy_rx) = mpsc::channel::<ServerToProxyMsg>(10);
    let (from_proxy_tx, _from_proxy_rx) = mpsc::channel::<ProxyToServerMsg>(10);

    // Start proxy
    let proxy = ProxyComm::new(socket_path.clone(), to_proxy_rx, from_proxy_tx);
    tokio::spawn(async move {
        let _ = proxy.run().await;
    });

    // Give proxy time to bind
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Connect as Go proxy
    let stream = UnixStream::connect(&socket_path)
        .await
        .expect("Failed to connect to proxy");

    let (reader, _writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    let mut line = String::new();

    // Read Ready message
    let bytes_read = tokio::time::timeout(Duration::from_secs(1), reader.read_line(&mut line))
        .await
        .expect("Timeout reading Ready message")
        .expect("Failed to read line");

    assert!(bytes_read > 0, "Should receive Ready message");

    let msg: ServerToProxyMsg = serde_json::from_str(line.trim())
        .expect("Failed to parse Ready message");

    match msg {
        ServerToProxyMsg::Ready => {
            // Success!
        }
        _ => panic!("Expected Ready message, got {:?}", msg),
    }

    // Cleanup
    drop(to_proxy_tx);
    let _ = std::fs::remove_file(&socket_path);
}

#[tokio::test]
async fn test_proxy_receives_config_updates() {
    let socket_path = test_socket_path("config");
    let (to_proxy_tx, to_proxy_rx) = mpsc::channel::<ServerToProxyMsg>(10);
    let (from_proxy_tx, mut from_proxy_rx) = mpsc::channel::<ProxyToServerMsg>(10);

    // Start proxy
    let proxy = ProxyComm::new(socket_path.clone(), to_proxy_rx, from_proxy_tx);
    tokio::spawn(async move {
        let _ = proxy.run().await;
    });

    tokio::time::sleep(Duration::from_millis(50)).await;

    // Connect and skip Ready message
    let stream = UnixStream::connect(&socket_path).await.unwrap();
    let (mut reader, mut writer) = stream.into_split();
    let mut buf_reader = BufReader::new(&mut reader);
    let mut line = String::new();

    // Read and discard Ready message
    buf_reader.read_line(&mut line).await.unwrap();
    line.clear();

    // Send UpdateConfigs message
    let configs = vec![tcp_config(1), tcp_config(2)];
    let msg = ProxyToServerMsg::UpdateConfigs { configs };
    let json = serde_json::to_string(&msg).unwrap();
    writer.write_all(json.as_bytes()).await.unwrap();
    writer.write_all(b"\n").await.unwrap();
    writer.flush().await.unwrap();

    // Proxy should forward to from_proxy_rx
    let received = tokio::time::timeout(Duration::from_secs(1), from_proxy_rx.recv())
        .await
        .expect("Timeout receiving message")
        .expect("Channel closed");

    match received {
        ProxyToServerMsg::UpdateConfigs { configs } => {
            assert_eq!(configs.len(), 2);
            assert_eq!(configs[0].id, 1);
            assert_eq!(configs[1].id, 2);
        }
        _ => panic!("Expected UpdateConfigs, got {:?}", received),
    }

    drop(to_proxy_tx);
    let _ = std::fs::remove_file(&socket_path);
}

#[tokio::test]
async fn test_proxy_sends_notifications() {
    let socket_path = test_socket_path("notification");
    let (to_proxy_tx, to_proxy_rx) = mpsc::channel::<ServerToProxyMsg>(10);
    let (from_proxy_tx, _from_proxy_rx) = mpsc::channel::<ProxyToServerMsg>(10);

    // Start proxy
    let proxy = ProxyComm::new(socket_path.clone(), to_proxy_rx, from_proxy_tx);
    tokio::spawn(async move {
        let _ = proxy.run().await;
    });

    tokio::time::sleep(Duration::from_millis(50)).await;

    // Connect
    let stream = UnixStream::connect(&socket_path).await.unwrap();
    let (mut reader, _writer) = stream.into_split();
    let mut buf_reader = BufReader::new(&mut reader);
    let mut line = String::new();

    // Read Ready message
    buf_reader.read_line(&mut line).await.unwrap();
    line.clear();

    // Send notification batch through channel
    let notification = Notification {
        id: 123,
        status: Status {
            last_check: Some(SystemTime::now()),
            duration: Duration::from_millis(42),
            failures: 0,
            successes: 1,
            state: State::Healthy,
            message: "All good".to_string(),
        },
    };

    let batch_msg = ServerToProxyMsg::NotificationBatch {
        batch: healthcheck_server::types::NotificationBatch {
            notifications: vec![notification],
        },
    };

    to_proxy_tx.send(batch_msg).await.unwrap();

    // Read notification from socket
    let bytes_read = tokio::time::timeout(Duration::from_secs(1), buf_reader.read_line(&mut line))
        .await
        .expect("Timeout reading notification")
        .expect("Failed to read");

    assert!(bytes_read > 0);

    let received: ServerToProxyMsg = serde_json::from_str(line.trim())
        .expect("Failed to parse notification");

    match received {
        ServerToProxyMsg::NotificationBatch { batch } => {
            assert_eq!(batch.notifications.len(), 1);
            assert_eq!(batch.notifications[0].id, 123);
            assert_eq!(batch.notifications[0].status.state, State::Healthy);
        }
        _ => panic!("Expected NotificationBatch, got {:?}", received),
    }

    drop(to_proxy_tx);
    let _ = std::fs::remove_file(&socket_path);
}

#[tokio::test]
async fn test_proxy_handles_shutdown() {
    let socket_path = test_socket_path("shutdown");
    let (to_proxy_tx, to_proxy_rx) = mpsc::channel::<ServerToProxyMsg>(10);
    let (from_proxy_tx, mut from_proxy_rx) = mpsc::channel::<ProxyToServerMsg>(10);

    // Start proxy
    let proxy = ProxyComm::new(socket_path.clone(), to_proxy_rx, from_proxy_tx);
    tokio::spawn(async move {
        let _ = proxy.run().await;
    });

    tokio::time::sleep(Duration::from_millis(50)).await;

    // Connect
    let stream = UnixStream::connect(&socket_path).await.unwrap();
    let (mut reader, mut writer) = stream.into_split();
    let mut buf_reader = BufReader::new(&mut reader);
    let mut line = String::new();

    // Read Ready
    buf_reader.read_line(&mut line).await.unwrap();

    // Send Shutdown message
    let msg = ProxyToServerMsg::Shutdown;
    let json = serde_json::to_string(&msg).unwrap();
    writer.write_all(json.as_bytes()).await.unwrap();
    writer.write_all(b"\n").await.unwrap();
    writer.flush().await.unwrap();

    // Should receive shutdown message
    let received = tokio::time::timeout(Duration::from_secs(1), from_proxy_rx.recv())
        .await
        .expect("Timeout")
        .expect("Channel closed");

    match received {
        ProxyToServerMsg::Shutdown => {
            // Success!
        }
        _ => panic!("Expected Shutdown, got {:?}", received),
    }

    drop(to_proxy_tx);
    let _ = std::fs::remove_file(&socket_path);
}

#[tokio::test]
async fn test_proxy_json_serialization() {
    // Test that various message types serialize/deserialize correctly

    // UpdateConfigs
    let configs = vec![tcp_config(1)];
    let msg = ProxyToServerMsg::UpdateConfigs { configs: configs.clone() };
    let json = serde_json::to_string(&msg).unwrap();
    let parsed: ProxyToServerMsg = serde_json::from_str(&json).unwrap();
    match parsed {
        ProxyToServerMsg::UpdateConfigs { configs: parsed_configs } => {
            assert_eq!(parsed_configs.len(), 1);
            assert_eq!(parsed_configs[0].id, 1);
        }
        _ => panic!("Deserialization failed"),
    }

    // NotificationBatch
    let notification = Notification {
        id: 42,
        status: Status {
            last_check: Some(SystemTime::now()),
            duration: Duration::from_millis(10),
            failures: 1,
            successes: 5,
            state: State::Healthy,
            message: "Test".to_string(),
        },
    };
    let msg = ServerToProxyMsg::NotificationBatch {
        batch: healthcheck_server::types::NotificationBatch {
            notifications: vec![notification],
        },
    };
    let json = serde_json::to_string(&msg).unwrap();
    let parsed: ServerToProxyMsg = serde_json::from_str(&json).unwrap();
    match parsed {
        ServerToProxyMsg::NotificationBatch { batch } => {
            assert_eq!(batch.notifications[0].id, 42);
        }
        _ => panic!("Deserialization failed"),
    }

    // Ready
    let msg = ServerToProxyMsg::Ready;
    let json = serde_json::to_string(&msg).unwrap();
    let parsed: ServerToProxyMsg = serde_json::from_str(&json).unwrap();
    matches!(parsed, ServerToProxyMsg::Ready);

    // Shutdown
    let msg = ProxyToServerMsg::Shutdown;
    let json = serde_json::to_string(&msg).unwrap();
    let parsed: ProxyToServerMsg = serde_json::from_str(&json).unwrap();
    matches!(parsed, ProxyToServerMsg::Shutdown);
}
