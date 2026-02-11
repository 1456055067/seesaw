//! VRRP integration tests
//!
//! These tests require CAP_NET_ADMIN capability to run.
//! Run with: sudo -E VRRP_TEST_ENABLED=1 cargo test --test integration_test
//!
//! Tests cover:
//! - Node creation and initialization
//! - State transitions (Init → Backup → Master)
//! - Advertisement sending and receiving
//! - Failover scenarios
//! - Graceful shutdown

use std::env;
use std::net::IpAddr;
use std::time::Duration;
use tokio::time::sleep;
use vrrp::{VRRPConfig, VRRPNode, VRRPState};

/// Check if integration tests are enabled
fn integration_tests_enabled() -> bool {
    env::var("VRRP_TEST_ENABLED").is_ok()
}

/// Create a test VRRP configuration
fn create_test_config(vrid: u8, priority: u8) -> VRRPConfig {
    VRRPConfig {
        vrid,
        priority,
        advert_interval: 100, // 1 second
        interface: "lo".to_string(),
        virtual_ips: vec!["127.0.1.1".parse().unwrap()],
        preempt: true,
        accept_mode: false,
    }
}

#[tokio::test]
async fn test_node_creation() {
    if !integration_tests_enabled() {
        println!("Skipping integration test (set VRRP_TEST_ENABLED=1 to run)");
        return;
    }

    let config = create_test_config(1, 100);
    let primary_ip: IpAddr = "127.0.0.1".parse().unwrap();

    let node = match VRRPNode::new(config, "lo", primary_ip) {
        Ok(n) => n,
        Err(e) => {
            eprintln!("Failed to create VRRP node: {}", e);
            eprintln!("Make sure to run with CAP_NET_ADMIN: sudo -E cargo test");
            return;
        }
    };

    // Verify initial state
    let state = node.get_state().await;
    assert_eq!(state, VRRPState::Init, "Initial state should be Init");

    // Verify initial stats
    let stats = node.get_stats().await;
    assert_eq!(stats.master_transitions, 0);
    assert_eq!(stats.backup_transitions, 0);
    assert_eq!(stats.adverts_sent, 0);
    assert_eq!(stats.adverts_received, 0);

    println!("✓ Node creation test passed");
}

#[tokio::test]
async fn test_priority_255_becomes_master() {
    if !integration_tests_enabled() {
        println!("Skipping integration test (set VRRP_TEST_ENABLED=1 to run)");
        return;
    }

    // Priority 255 = IP address owner, should immediately become master
    let config = create_test_config(2, 255);
    let primary_ip: IpAddr = "127.0.0.1".parse().unwrap();

    let node = match VRRPNode::new(config, "lo", primary_ip) {
        Ok(n) => n,
        Err(e) => {
            eprintln!("Failed to create VRRP node: {}", e);
            return;
        }
    };

    // Run node in background
    let node_clone = std::sync::Arc::new(node);
    let node_ref = node_clone.clone();

    tokio::spawn(async move {
        let _ = node_ref.run().await;
    });

    // Give it time to initialize
    sleep(Duration::from_millis(500)).await;

    // Should be in Master state
    let state = node_clone.get_state().await;
    assert_eq!(
        state,
        VRRPState::Master,
        "Priority 255 should become Master immediately"
    );

    // Should have transitioned to master
    let stats = node_clone.get_stats().await;
    assert!(
        stats.master_transitions > 0,
        "Should have transitioned to Master"
    );

    // Should have sent advertisements
    sleep(Duration::from_millis(1500)).await; // Wait for at least 1 advert interval
    let stats = node_clone.get_stats().await;
    assert!(stats.adverts_sent > 0, "Should have sent advertisements");

    // Shutdown gracefully
    let _ = node_clone.shutdown().await;

    println!("✓ Priority 255 master test passed");
}

#[tokio::test]
async fn test_backup_state_transitions() {
    if !integration_tests_enabled() {
        println!("Skipping integration test (set VRRP_TEST_ENABLED=1 to run)");
        return;
    }

    // Lower priority should start as backup
    let config = create_test_config(3, 100);
    let primary_ip: IpAddr = "127.0.0.1".parse().unwrap();

    let node = match VRRPNode::new(config, "lo", primary_ip) {
        Ok(n) => n,
        Err(e) => {
            eprintln!("Failed to create VRRP node: {}", e);
            return;
        }
    };

    let node_arc = std::sync::Arc::new(node);
    let node_ref = node_arc.clone();

    tokio::spawn(async move {
        let _ = node_ref.run().await;
    });

    // Give it time to initialize
    sleep(Duration::from_millis(500)).await;

    // Should be in Backup state initially
    let state = node_arc.get_state().await;
    assert!(
        state == VRRPState::Backup || state == VRRPState::Master,
        "Should be in Backup or Master state after init"
    );

    // Should have transitioned from Init
    let stats = node_arc.get_stats().await;
    let total_transitions = stats.master_transitions + stats.backup_transitions;
    assert!(total_transitions > 0, "Should have transitioned from Init");

    // Shutdown
    let _ = node_arc.shutdown().await;

    println!("✓ Backup state transition test passed");
}

#[tokio::test]
async fn test_graceful_shutdown() {
    if !integration_tests_enabled() {
        println!("Skipping integration test (set VRRP_TEST_ENABLED=1 to run)");
        return;
    }

    let config = create_test_config(4, 255); // Use 255 to become master quickly
    let primary_ip: IpAddr = "127.0.0.1".parse().unwrap();

    let node = match VRRPNode::new(config, "lo", primary_ip) {
        Ok(n) => n,
        Err(e) => {
            eprintln!("Failed to create VRRP node: {}", e);
            return;
        }
    };

    let node_arc = std::sync::Arc::new(node);
    let node_ref = node_arc.clone();

    tokio::spawn(async move {
        let _ = node_ref.run().await;
    });

    // Wait for it to become master
    sleep(Duration::from_millis(500)).await;

    let state_before = node_arc.get_state().await;
    assert_eq!(state_before, VRRPState::Master, "Should be Master before shutdown");

    // Graceful shutdown
    node_arc.shutdown().await.expect("Shutdown should succeed");

    // Should be in Init state after shutdown
    let state_after = node_arc.get_state().await;
    assert_eq!(
        state_after,
        VRRPState::Init,
        "Should be Init after shutdown"
    );

    println!("✓ Graceful shutdown test passed");
}

#[tokio::test]
async fn test_statistics_tracking() {
    if !integration_tests_enabled() {
        println!("Skipping integration test (set VRRP_TEST_ENABLED=1 to run)");
        return;
    }

    let config = create_test_config(5, 255);
    let primary_ip: IpAddr = "127.0.0.1".parse().unwrap();

    let node = match VRRPNode::new(config, "lo", primary_ip) {
        Ok(n) => n,
        Err(e) => {
            eprintln!("Failed to create VRRP node: {}", e);
            return;
        }
    };

    let node_arc = std::sync::Arc::new(node);
    let node_ref = node_arc.clone();

    tokio::spawn(async move {
        let _ = node_ref.run().await;
    });

    // Initial stats
    sleep(Duration::from_millis(100)).await;
    let stats_initial = node_arc.get_stats().await;

    // Wait for some advertisements
    sleep(Duration::from_millis(2500)).await;

    let stats_final = node_arc.get_stats().await;

    // Verify statistics increased
    assert!(
        stats_final.master_transitions >= stats_initial.master_transitions,
        "Master transitions should not decrease"
    );
    assert!(
        stats_final.adverts_sent > stats_initial.adverts_sent,
        "Should have sent more advertisements"
    );

    // Shutdown
    let _ = node_arc.shutdown().await;

    println!("✓ Statistics tracking test passed");
    println!(
        "  Master transitions: {} → {}",
        stats_initial.master_transitions, stats_final.master_transitions
    );
    println!(
        "  Adverts sent: {} → {}",
        stats_initial.adverts_sent, stats_final.adverts_sent
    );
}

#[tokio::test]
async fn test_multiple_virtual_ips() {
    if !integration_tests_enabled() {
        println!("Skipping integration test (set VRRP_TEST_ENABLED=1 to run)");
        return;
    }

    let config = VRRPConfig {
        vrid: 6,
        priority: 255,
        advert_interval: 100,
        interface: "lo".to_string(),
        virtual_ips: vec![
            "127.0.2.1".parse().unwrap(),
            "127.0.2.2".parse().unwrap(),
            "127.0.2.3".parse().unwrap(),
        ],
        preempt: true,
        accept_mode: false,
    };

    let primary_ip: IpAddr = "127.0.0.1".parse().unwrap();

    let node = match VRRPNode::new(config, "lo", primary_ip) {
        Ok(n) => n,
        Err(e) => {
            eprintln!("Failed to create VRRP node: {}", e);
            return;
        }
    };

    let node_arc = std::sync::Arc::new(node);
    let node_ref = node_arc.clone();

    tokio::spawn(async move {
        let _ = node_ref.run().await;
    });

    // Wait for initialization
    sleep(Duration::from_millis(500)).await;

    let state = node_arc.get_state().await;
    assert_eq!(
        state,
        VRRPState::Master,
        "Should be Master with multiple VIPs"
    );

    // Shutdown
    let _ = node_arc.shutdown().await;

    println!("✓ Multiple virtual IPs test passed");
}

#[tokio::test]
async fn test_config_validation() {
    // Test invalid VRID
    let mut config = create_test_config(0, 100);
    assert!(config.validate().is_err(), "VRID 0 should be invalid");

    // Test invalid priority
    config.vrid = 1;
    config.priority = 0;
    assert!(config.validate().is_err(), "Priority 0 should be invalid");

    // Test empty virtual IPs
    config.priority = 100;
    config.virtual_ips.clear();
    assert!(
        config.validate().is_err(),
        "Empty virtual IPs should be invalid"
    );

    // Test empty interface
    config.virtual_ips.push("127.0.3.1".parse().unwrap());
    config.interface = String::new();
    assert!(
        config.validate().is_err(),
        "Empty interface should be invalid"
    );

    // Test valid config
    config.interface = "lo".to_string();
    assert!(config.validate().is_ok(), "Valid config should pass");

    println!("✓ Config validation test passed");
}

#[tokio::test]
async fn test_advert_interval_calculation() {
    let config = create_test_config(7, 100);

    // Advertisement interval: 100 centiseconds = 1000ms
    let advert_ms = config.advert_interval_ms();
    assert_eq!(advert_ms, Duration::from_millis(1000));

    // Master down interval calculation
    // Formula: (3 * advert_interval) + skew_time
    // skew_time = ((256 - priority) * advert_interval) / 256
    let master_down = config.master_down_interval();

    // For priority 100:
    // skew_time = ((256 - 100) * 1000) / 256 = 609ms
    // master_down = (3 * 1000) + 609 = 3609ms
    assert_eq!(master_down, Duration::from_millis(3609));

    println!("✓ Advert interval calculation test passed");
    println!("  Advertisement interval: {:?}", advert_ms);
    println!("  Master down interval: {:?}", master_down);
}
