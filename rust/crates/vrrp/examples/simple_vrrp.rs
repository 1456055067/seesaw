//! Simple VRRP example
//!
//! This example creates a VRRP node and runs the state machine.
//!
//! Usage:
//!   sudo target/release/examples/simple_vrrp
//!
//! Or with capabilities:
//!   sudo setcap cap_net_admin,cap_net_raw+ep target/release/examples/simple_vrrp
//!   target/release/examples/simple_vrrp

use std::env;
use std::sync::Arc;
use std::time::Duration;
use tokio::signal;
use tokio::time::{interval, sleep};
use vrrp::{VRRPConfig, VRRPNode};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter("vrrp=info")
        .init();

    // Parse command line arguments
    let args: Vec<String> = env::args().collect();
    let priority: u8 = if args.len() > 1 {
        args[1].parse().unwrap_or(100)
    } else {
        100
    };

    println!("╔═══════════════════════════════════════╗");
    println!("║   VRRP Example Node                   ║");
    println!("╚═══════════════════════════════════════╝");
    println!();
    println!("Configuration:");
    println!("  VRID:           1");
    println!("  Priority:       {}", priority);
    println!("  Interface:      lo (loopback)");
    println!("  Virtual IP:     127.0.10.1");
    println!("  Primary IP:     127.0.0.1");
    println!("  Advert Interval: 1 second");
    println!();

    if priority == 255 {
        println!("⚠ Priority 255 = IP Address Owner (immediate Master)");
    } else {
        println!("ℹ Starting as Backup, will become Master after timeout");
    }
    println!();

    // Create VRRP configuration
    let config = VRRPConfig {
        vrid: 1,
        priority,
        advert_interval: 100, // 1 second in centiseconds
        interface: "lo".to_string(),
        virtual_ips: vec!["127.0.10.1".parse()?],
        preempt: true,
        accept_mode: false,
    };

    // Validate configuration
    config.validate()?;

    // Create VRRP node
    let primary_ip = "127.0.0.1".parse()?;
    let node = VRRPNode::new(config, "lo", primary_ip)?;
    let node_arc = Arc::new(node);

    println!("✓ VRRP node created successfully");
    println!();

    // Start state machine in background
    let node_runner = node_arc.clone();
    let run_handle = tokio::spawn(async move {
        if let Err(e) = node_runner.run().await {
            eprintln!("VRRP error: {}", e);
        }
    });

    // Start statistics monitor
    let node_monitor = node_arc.clone();
    let monitor_handle = tokio::spawn(async move {
        let mut tick = interval(Duration::from_secs(5));
        tick.tick().await; // Skip first immediate tick

        loop {
            tick.tick().await;

            let state = node_monitor.get_state().await;
            let stats = node_monitor.get_stats().await;

            println!("╔═══════════════════════════════════════╗");
            println!("║  VRRP Status                          ║");
            println!("╠═══════════════════════════════════════╣");
            println!("║  State: {:28} ║", format!("{:?}", state));
            println!("║  Master transitions: {:16} ║", stats.master_transitions);
            println!("║  Backup transitions: {:16} ║", stats.backup_transitions);
            println!("║  Adverts sent:       {:16} ║", stats.adverts_sent);
            println!("║  Adverts received:   {:16} ║", stats.adverts_received);
            println!("║  Invalid adverts:    {:16} ║", stats.invalid_adverts);
            println!("║  Checksum errors:    {:16} ║", stats.checksum_errors);
            println!("╚═══════════════════════════════════════╝");
            println!();
        }
    });

    // Wait for Ctrl+C
    println!("Press Ctrl+C to shutdown gracefully...");
    println!();

    // Give node time to initialize
    sleep(Duration::from_millis(100)).await;

    signal::ctrl_c().await?;
    println!();
    println!("Received shutdown signal, stopping gracefully...");

    // Shutdown node gracefully
    node_arc.shutdown().await?;
    println!("✓ VRRP node shut down");

    // Cancel background tasks
    run_handle.abort();
    monitor_handle.abort();

    println!();
    println!("Final statistics:");
    let final_stats = node_arc.get_stats().await;
    println!("  Master transitions: {}", final_stats.master_transitions);
    println!("  Backup transitions: {}", final_stats.backup_transitions);
    println!("  Total adverts sent: {}", final_stats.adverts_sent);
    println!("  Total adverts received: {}", final_stats.adverts_received);

    Ok(())
}
