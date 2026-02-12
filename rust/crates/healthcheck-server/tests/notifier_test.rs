//! Integration tests for Notifier component

use healthcheck_server::notifier::Notifier;
use healthcheck_server::types::{Notification, ServerToProxyMsg, State, Status};
use std::time::{Duration, SystemTime};
use tokio::sync::mpsc;

/// Helper to create a notification
fn notification(id: u64, state: State) -> Notification {
    Notification {
        id,
        status: Status {
            last_check: Some(SystemTime::now()),
            duration: Duration::from_millis(10),
            failures: if state == State::Unhealthy { 1 } else { 0 },
            successes: if state == State::Healthy { 1 } else { 0 },
            state,
            message: format!("Test message for {}", id),
        },
    }
}

#[tokio::test]
async fn test_notifier_batches_notifications() {
    let (notify_tx, notify_rx) = mpsc::channel::<Notification>(100);
    let (proxy_tx, mut proxy_rx) = mpsc::channel::<ServerToProxyMsg>(100);

    // Create notifier with 10ms delay and batch size of 5
    let notifier = Notifier::new(
        notify_rx,
        proxy_tx,
        Duration::from_millis(10),
        5,
    );

    tokio::spawn(async move {
        notifier.run().await;
    });

    // Send 3 notifications quickly (should batch)
    for i in 1..=3 {
        notify_tx.send(notification(i, State::Healthy)).await.unwrap();
    }

    // Should receive a batch after delay
    let msg = tokio::time::timeout(Duration::from_millis(50), proxy_rx.recv())
        .await
        .expect("Timeout waiting for batch")
        .expect("Channel closed");

    match msg {
        ServerToProxyMsg::NotificationBatch { batch } => {
            assert_eq!(batch.notifications.len(), 3, "Should batch all 3 notifications");
        }
        _ => panic!("Expected NotificationBatch, got {:?}", msg),
    }
}

#[tokio::test]
async fn test_notifier_sends_full_batch_immediately() {
    let (notify_tx, notify_rx) = mpsc::channel::<Notification>(100);
    let (proxy_tx, mut proxy_rx) = mpsc::channel::<ServerToProxyMsg>(100);

    // Create notifier with 1s delay but batch size of 3
    let notifier = Notifier::new(
        notify_rx,
        proxy_tx,
        Duration::from_secs(1),  // Long delay
        3,                        // Small batch size
    );

    tokio::spawn(async move {
        notifier.run().await;
    });

    // Send exactly batch_size notifications
    for i in 1..=3 {
        notify_tx.send(notification(i, State::Healthy)).await.unwrap();
    }

    // Should receive batch immediately (before delay expires)
    let msg = tokio::time::timeout(Duration::from_millis(100), proxy_rx.recv())
        .await
        .expect("Timeout - batch should be sent immediately when full")
        .expect("Channel closed");

    match msg {
        ServerToProxyMsg::NotificationBatch { batch } => {
            assert_eq!(batch.notifications.len(), 3, "Should send full batch");
        }
        _ => panic!("Expected NotificationBatch, got {:?}", msg),
    }
}

#[tokio::test]
async fn test_notifier_handles_multiple_batches() {
    let (notify_tx, notify_rx) = mpsc::channel::<Notification>(100);
    let (proxy_tx, mut proxy_rx) = mpsc::channel::<ServerToProxyMsg>(100);

    let notifier = Notifier::new(
        notify_rx,
        proxy_tx,
        Duration::from_millis(20),
        3,
    );

    tokio::spawn(async move {
        notifier.run().await;
    });

    // Send first batch worth
    for i in 1..=3 {
        notify_tx.send(notification(i, State::Healthy)).await.unwrap();
    }

    // Receive first batch
    let msg1 = tokio::time::timeout(Duration::from_millis(50), proxy_rx.recv())
        .await
        .expect("Timeout on first batch")
        .expect("Channel closed");

    // Send second batch worth
    for i in 4..=6 {
        notify_tx.send(notification(i, State::Unhealthy)).await.unwrap();
    }

    // Receive second batch
    let msg2 = tokio::time::timeout(Duration::from_millis(50), proxy_rx.recv())
        .await
        .expect("Timeout on second batch")
        .expect("Channel closed");

    // Verify both are batches
    match (msg1, msg2) {
        (
            ServerToProxyMsg::NotificationBatch { batch: batch1 },
            ServerToProxyMsg::NotificationBatch { batch: batch2 },
        ) => {
            assert_eq!(batch1.notifications.len(), 3);
            assert_eq!(batch2.notifications.len(), 3);
        }
        _ => panic!("Expected two NotificationBatch messages"),
    }
}

#[tokio::test]
async fn test_notifier_preserves_notification_order() {
    let (notify_tx, notify_rx) = mpsc::channel::<Notification>(100);
    let (proxy_tx, mut proxy_rx) = mpsc::channel::<ServerToProxyMsg>(100);

    let notifier = Notifier::new(
        notify_rx,
        proxy_tx,
        Duration::from_millis(10),
        10,
    );

    tokio::spawn(async move {
        notifier.run().await;
    });

    // Send notifications with specific IDs
    let ids = vec![5, 3, 7, 1, 9];
    for &id in &ids {
        notify_tx.send(notification(id, State::Healthy)).await.unwrap();
    }

    // Receive batch
    let msg = tokio::time::timeout(Duration::from_millis(50), proxy_rx.recv())
        .await
        .expect("Timeout")
        .expect("Channel closed");

    match msg {
        ServerToProxyMsg::NotificationBatch { batch } => {
            let received_ids: Vec<u64> = batch.notifications.iter()
                .map(|n| n.id)
                .collect();
            assert_eq!(received_ids, ids, "Notification order should be preserved");
        }
        _ => panic!("Expected NotificationBatch"),
    }
}

#[tokio::test]
async fn test_notifier_handles_rapid_notifications() {
    let (notify_tx, notify_rx) = mpsc::channel::<Notification>(1000);
    let (proxy_tx, mut proxy_rx) = mpsc::channel::<ServerToProxyMsg>(100);

    let notifier = Notifier::new(
        notify_rx,
        proxy_tx,
        Duration::from_millis(5),
        50,
    );

    tokio::spawn(async move {
        notifier.run().await;
    });

    // Send 100 notifications rapidly
    for i in 1..=100 {
        notify_tx.send(notification(i, State::Healthy)).await.unwrap();
    }

    // Collect all batches
    let mut total_received = 0;
    let timeout_duration = Duration::from_millis(200);

    while total_received < 100 {
        match tokio::time::timeout(timeout_duration, proxy_rx.recv()).await {
            Ok(Some(ServerToProxyMsg::NotificationBatch { batch })) => {
                total_received += batch.notifications.len();
            }
            Ok(Some(_)) => panic!("Unexpected message type"),
            Ok(None) => break,
            Err(_) => break,
        }
    }

    assert_eq!(total_received, 100, "Should receive all 100 notifications");
}

#[tokio::test]
async fn test_notifier_with_mixed_states() {
    let (notify_tx, notify_rx) = mpsc::channel::<Notification>(100);
    let (proxy_tx, mut proxy_rx) = mpsc::channel::<ServerToProxyMsg>(100);

    let notifier = Notifier::new(
        notify_rx,
        proxy_tx,
        Duration::from_millis(10),
        10,
    );

    tokio::spawn(async move {
        notifier.run().await;
    });

    // Send notifications with different states
    notify_tx.send(notification(1, State::Healthy)).await.unwrap();
    notify_tx.send(notification(2, State::Unhealthy)).await.unwrap();
    notify_tx.send(notification(3, State::Unknown)).await.unwrap();
    notify_tx.send(notification(4, State::Healthy)).await.unwrap();

    // Receive batch
    let msg = tokio::time::timeout(Duration::from_millis(50), proxy_rx.recv())
        .await
        .expect("Timeout")
        .expect("Channel closed");

    match msg {
        ServerToProxyMsg::NotificationBatch { batch } => {
            assert_eq!(batch.notifications.len(), 4);
            assert_eq!(batch.notifications[0].status.state, State::Healthy);
            assert_eq!(batch.notifications[1].status.state, State::Unhealthy);
            assert_eq!(batch.notifications[2].status.state, State::Unknown);
            assert_eq!(batch.notifications[3].status.state, State::Healthy);
        }
        _ => panic!("Expected NotificationBatch"),
    }
}
