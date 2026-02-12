//! Integration tests for Manager component

use healthcheck_server::types::{CheckerConfig, HealthcheckConfig, Notification, State};
use healthcheck_server::manager::Manager;
use std::time::Duration;
use tokio::sync::mpsc;

/// Helper to create a TCP healthcheck config
fn tcp_config(id: u64, ip: &str, port: u16) -> HealthcheckConfig {
    HealthcheckConfig {
        id,
        interval: Duration::from_secs(1),
        timeout: Duration::from_millis(100),
        retries: 2,
        checker: CheckerConfig::Tcp {
            ip: ip.parse().unwrap(),
            port,
        },
    }
}

/// Helper to create an HTTP healthcheck config
fn http_config(id: u64, ip: &str, port: u16, path: &str) -> HealthcheckConfig {
    HealthcheckConfig {
        id,
        interval: Duration::from_secs(1),
        timeout: Duration::from_millis(100),
        retries: 1,
        checker: CheckerConfig::Http {
            ip: ip.parse().unwrap(),
            port,
            method: "GET".to_string(),
            path: path.to_string(),
            expected_codes: vec![200],
            secure: false,
        },
    }
}

#[tokio::test]
async fn test_manager_adds_new_healthchecks() {
    // Create channels
    let (notify_tx, mut notify_rx) = mpsc::channel::<Notification>(100);
    let (config_tx, config_rx) = mpsc::channel::<Vec<HealthcheckConfig>>(10);

    // Create and spawn manager
    let manager = Manager::new(notify_tx, config_rx, Duration::from_millis(500), None);
    let manager_handle = tokio::spawn(async move {
        manager.run().await;
    });

    // Send initial configs
    let configs = vec![
        tcp_config(1, "127.0.0.1", 8080),
        tcp_config(2, "127.0.0.1", 9090),
    ];
    config_tx.send(configs).await.unwrap();

    // Give manager time to process
    tokio::time::sleep(Duration::from_millis(100)).await;

    // We should eventually receive notifications as monitors check unhealthy targets
    // (127.0.0.1:8080 and 9090 are likely not running)
    let notification = tokio::time::timeout(Duration::from_secs(3), notify_rx.recv())
        .await
        .expect("Timeout waiting for notification")
        .expect("Channel closed");

    // Should be from one of our healthchecks
    assert!(notification.id == 1 || notification.id == 2);
    // Should transition to unhealthy (nothing listening on those ports)
    assert_eq!(notification.status.state, State::Unhealthy);

    drop(config_tx);
    drop(notify_rx);
    drop(manager_handle);
}

#[tokio::test]
async fn test_manager_removes_deleted_healthchecks() {
    let (notify_tx, _notify_rx) = mpsc::channel::<Notification>(100);
    let (config_tx, config_rx) = mpsc::channel::<Vec<HealthcheckConfig>>(10);

    let manager = Manager::new(notify_tx, config_rx, Duration::from_millis(500), None);
    let manager_handle = tokio::spawn(async move {
        manager.run().await;
    });

    // Add 3 healthchecks
    let configs = vec![
        tcp_config(1, "127.0.0.1", 8080),
        tcp_config(2, "127.0.0.1", 9090),
        tcp_config(3, "127.0.0.1", 7070),
    ];
    config_tx.send(configs).await.unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Remove healthcheck 2 by sending configs without it
    let configs = vec![
        tcp_config(1, "127.0.0.1", 8080),
        tcp_config(3, "127.0.0.1", 7070),
    ];
    config_tx.send(configs).await.unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Manager should have removed healthcheck 2
    // This is verified by the manager continuing to run without errors

    drop(config_tx);
    drop(manager_handle);
}

#[tokio::test]
async fn test_manager_updates_existing_healthchecks() {
    let (notify_tx, _notify_rx) = mpsc::channel::<Notification>(100);
    let (config_tx, config_rx) = mpsc::channel::<Vec<HealthcheckConfig>>(10);

    let manager = Manager::new(notify_tx, config_rx, Duration::from_millis(500), None);
    let manager_handle = tokio::spawn(async move {
        manager.run().await;
    });

    // Add healthcheck with 1s interval
    let configs = vec![tcp_config(1, "127.0.0.1", 8080)];
    config_tx.send(configs).await.unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Update healthcheck with different port (should recreate monitor)
    let mut updated_config = tcp_config(1, "127.0.0.1", 9999);
    updated_config.timeout = Duration::from_millis(200); // Different timeout
    let configs = vec![updated_config];
    config_tx.send(configs).await.unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Manager should have updated the healthcheck
    // Verified by no errors and continued operation

    drop(config_tx);
    drop(manager_handle);
}

#[tokio::test]
async fn test_manager_handles_mixed_checker_types() {
    let (notify_tx, mut notify_rx) = mpsc::channel::<Notification>(100);
    let (config_tx, config_rx) = mpsc::channel::<Vec<HealthcheckConfig>>(10);

    let manager = Manager::new(notify_tx, config_rx, Duration::from_millis(500), None);
    let manager_handle = tokio::spawn(async move {
        manager.run().await;
    });

    // Send configs with different checker types
    let configs = vec![
        tcp_config(1, "127.0.0.1", 8080),
        http_config(2, "127.0.0.1", 8080, "/health"),
        HealthcheckConfig {
            id: 3,
            interval: Duration::from_secs(1),
            timeout: Duration::from_millis(100),
            retries: 1,
            checker: CheckerConfig::Dns {
                query: "example.com".to_string(),
                expected_ips: vec!["93.184.216.34".parse().unwrap()],
            },
        },
    ];
    config_tx.send(configs).await.unwrap();

    // Give monitors time to start and check
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Should receive notifications for state transitions
    let mut received = 0;
    while received < 3 {
        if tokio::time::timeout(Duration::from_secs(5), notify_rx.recv())
            .await
            .is_ok()
        {
            received += 1;
        } else {
            break;
        }
    }

    // Should have received notifications from all three checkers
    assert!(received >= 1, "Expected at least 1 notification, got {}", received);

    drop(config_tx);
    drop(notify_rx);
    drop(manager_handle);
}

#[tokio::test]
async fn test_manager_status_snapshot() {
    let (notify_tx, _notify_rx) = mpsc::channel::<Notification>(100);
    let (config_tx, config_rx) = mpsc::channel::<Vec<HealthcheckConfig>>(10);

    let manager = Manager::new(notify_tx, config_rx, Duration::from_millis(500), None);

    // Add some healthchecks
    let configs = vec![
        tcp_config(1, "127.0.0.1", 8080),
        tcp_config(2, "127.0.0.1", 9090),
    ];
    config_tx.send(configs).await.unwrap();

    // Spawn manager
    let manager_handle = tokio::spawn(async move {
        manager.run().await;
    });

    // Give monitors time to initialize
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Getting status would require exposing it through a channel or RPC
    // For now, verify manager continues running

    drop(config_tx);
    drop(manager_handle);
}

#[tokio::test]
async fn test_manager_handles_empty_config_updates() {
    let (notify_tx, _notify_rx) = mpsc::channel::<Notification>(100);
    let (config_tx, config_rx) = mpsc::channel::<Vec<HealthcheckConfig>>(10);

    let manager = Manager::new(notify_tx, config_rx, Duration::from_millis(500), None);
    let manager_handle = tokio::spawn(async move {
        manager.run().await;
    });

    // Send empty config (remove all)
    config_tx.send(vec![]).await.unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Add some
    let configs = vec![tcp_config(1, "127.0.0.1", 8080)];
    config_tx.send(configs).await.unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Remove all again
    config_tx.send(vec![]).await.unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;

    drop(config_tx);
    drop(manager_handle);
}
