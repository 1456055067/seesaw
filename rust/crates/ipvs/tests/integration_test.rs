//! Integration tests for IPVS operations.
//!
//! These tests require:
//! - Root privileges (CAP_NET_ADMIN)
//! - IPVS kernel module loaded (modprobe ip_vs)
//! - Set IPVS_TEST_ENABLED=1 environment variable to run
//!
//! Run with: sudo -E cargo test --test integration_test -- --nocapture

use ipvs::{
    Destination, DestinationFlags, DestinationStats, IPVSManager, Protocol, Scheduler, Service,
    ServiceFlags, ServiceStats,
};
use std::net::{IpAddr, Ipv4Addr};

/// Helper to check if tests should run
fn should_run_tests() -> bool {
    std::env::var("IPVS_TEST_ENABLED").is_ok()
}

/// Helper to skip test if not enabled
macro_rules! skip_unless_enabled {
    () => {
        if !should_run_tests() {
            eprintln!("Skipping test (set IPVS_TEST_ENABLED=1 to enable)");
            return;
        }
    };
}

#[test]
fn test_ipvs_manager_creation() {
    skip_unless_enabled!();

    let result = IPVSManager::new();
    match result {
        Ok(manager) => {
            println!("✓ IPVSManager created successfully");
            println!("  Family ID: {}", manager.family_id());
        }
        Err(e) => {
            panic!("Failed to create IPVSManager: {}", e);
        }
    }
}

#[test]
fn test_ipvs_version() {
    skip_unless_enabled!();

    let mut manager = IPVSManager::new().expect("Failed to create manager");
    match manager.version() {
        Ok(version) => {
            println!("✓ IPVS version: {}", version);
            assert!(version.major > 0, "Version major should be > 0");
        }
        Err(e) => {
            panic!("Failed to get IPVS version: {}", e);
        }
    }
}

#[test]
fn test_service_lifecycle() {
    skip_unless_enabled!();

    let mut manager = IPVSManager::new().expect("Failed to create manager");

    // Clean slate
    println!("Flushing all IPVS services...");
    manager.flush().expect("Failed to flush");
    println!("✓ Flush successful");

    // Create a test service
    let service = Service {
        address: IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1)),
        protocol: Protocol::TCP,
        port: 80,
        fwmark: 0,
        scheduler: Scheduler::RoundRobin,
        flags: ServiceFlags::default(),
        timeout: 0,
        persistence_engine: None,
        statistics: ServiceStats::default(),
    };

    // Add service
    println!("\nAdding service: {}", service);
    manager
        .add_service(&service)
        .expect("Failed to add service");
    println!("✓ Service added successfully");

    // Update service (change scheduler)
    let mut updated_service = service.clone();
    updated_service.scheduler = Scheduler::WeightedRoundRobin;
    println!("\nUpdating service to scheduler: {}", updated_service.scheduler);
    manager
        .update_service(&updated_service)
        .expect("Failed to update service");
    println!("✓ Service updated successfully");

    // Add a destination
    let dest = Destination {
        address: IpAddr::V4(Ipv4Addr::new(192, 168, 1, 10)),
        port: 8080,
        weight: 100,
        flags: DestinationFlags::Route,
        lower_threshold: 0,
        upper_threshold: 0,
        statistics: DestinationStats::default(),
    };

    println!("\nAdding destination: {}", dest);
    manager
        .add_destination(&updated_service, &dest)
        .expect("Failed to add destination");
    println!("✓ Destination added successfully");

    // Update destination (change weight)
    let mut updated_dest = dest.clone();
    updated_dest.weight = 200;
    println!("\nUpdating destination weight to: {}", updated_dest.weight);
    manager
        .update_destination(&updated_service, &updated_dest)
        .expect("Failed to update destination");
    println!("✓ Destination updated successfully");

    // Delete destination
    println!("\nDeleting destination...");
    manager
        .delete_destination(&updated_service, &updated_dest)
        .expect("Failed to delete destination");
    println!("✓ Destination deleted successfully");

    // Delete service
    println!("\nDeleting service...");
    manager
        .delete_service(&updated_service)
        .expect("Failed to delete service");
    println!("✓ Service deleted successfully");

    println!("\n✓ Full service lifecycle test passed!");
}

#[test]
fn test_firewall_mark_service() {
    skip_unless_enabled!();

    let mut manager = IPVSManager::new().expect("Failed to create manager");

    // Clean slate
    manager.flush().expect("Failed to flush");

    // Create a firewall mark based service
    let service = Service {
        address: IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)),
        protocol: Protocol::TCP,
        port: 0,
        fwmark: 100, // Firewall mark
        scheduler: Scheduler::LeastConnection,
        flags: ServiceFlags::default(),
        timeout: 0,
        persistence_engine: None,
        statistics: ServiceStats::default(),
    };

    println!("Adding firewall mark service (fwmark=100)...");
    manager
        .add_service(&service)
        .expect("Failed to add fwmark service");
    println!("✓ Firewall mark service added");

    // Clean up
    manager
        .delete_service(&service)
        .expect("Failed to delete service");
    println!("✓ Firewall mark service deleted");
}

#[test]
fn test_udp_service() {
    skip_unless_enabled!();

    let mut manager = IPVSManager::new().expect("Failed to create manager");
    manager.flush().expect("Failed to flush");

    let service = Service {
        address: IpAddr::V4(Ipv4Addr::new(10, 0, 0, 2)),
        protocol: Protocol::UDP,
        port: 53,
        fwmark: 0,
        scheduler: Scheduler::SourceHashing,
        flags: ServiceFlags::default(),
        timeout: 0,
        persistence_engine: None,
        statistics: ServiceStats::default(),
    };

    println!("Adding UDP service on port 53...");
    manager.add_service(&service).expect("Failed to add UDP service");
    println!("✓ UDP service added");

    manager.delete_service(&service).expect("Failed to delete service");
    println!("✓ UDP service deleted");
}

#[test]
fn test_multiple_destinations() {
    skip_unless_enabled!();

    let mut manager = IPVSManager::new().expect("Failed to create manager");
    manager.flush().expect("Failed to flush");

    let service = Service {
        address: IpAddr::V4(Ipv4Addr::new(10, 0, 0, 3)),
        protocol: Protocol::TCP,
        port: 443,
        fwmark: 0,
        scheduler: Scheduler::WeightedLeastConnection,
        flags: ServiceFlags::default(),
        timeout: 0,
        persistence_engine: None,
        statistics: ServiceStats::default(),
    };

    manager.add_service(&service).expect("Failed to add service");
    println!("✓ Service added");

    // Add multiple destinations
    for i in 1..=3 {
        let dest = Destination {
            address: IpAddr::V4(Ipv4Addr::new(192, 168, 1, 10 + i)),
            port: 8443,
            weight: 100 * i as u32,
            flags: DestinationFlags::Route,
            lower_threshold: 0,
            upper_threshold: 0,
            statistics: DestinationStats::default(),
        };

        println!("Adding destination {}: {} (weight={})", i, dest.address, dest.weight);
        manager
            .add_destination(&service, &dest)
            .expect(&format!("Failed to add destination {}", i));
    }

    println!("✓ All 3 destinations added");

    // Clean up
    manager.delete_service(&service).expect("Failed to delete service");
    println!("✓ Service and all destinations deleted");
}
