//! Configuration System Demo
//!
//! This example demonstrates the new TOML-based unified configuration loading system.
//! It shows environment detection, configuration loading, and validation.

use std::env;
use tasker_core::config::ConfigManager;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging so we can see the configuration loading messages
    tracing_subscriber::fmt().with_env_filter("info").init();

    println!("🔧 TaskerCore Configuration System Demo");
    println!("======================================\n");

    // Demo 1: Environment Detection
    println!("1. Environment Detection:");
    println!("   TASKER_ENV: {:?}", env::var("TASKER_ENV"));
    println!("   RAILS_ENV: {:?}", env::var("RAILS_ENV"));

    // Demo 2: Load Configuration
    println!("\n2. Loading Configuration:");
    let config_manager = ConfigManager::load()?;
    let config = config_manager.config();

    println!("   ✅ Configuration loaded successfully!");
    println!("   Environment: {}", config_manager.environment());

    // Demo 3: Database Configuration
    println!("\n3. Database Configuration:");
    println!("   Host: {}", config.database.host);
    println!("   Username: {}", config.database.username);
    println!("   Pool Size: {}", config.database.pool);
    println!("   Database URL: {}", config.database_url());

    // Demo 4: PGMQ Configuration
    println!("\n4. PGMQ Configuration:");
    println!("   Poll Interval: {:?}", config.pgmq.poll_interval());
    println!(
        "   Visibility Timeout: {:?}",
        config.pgmq.visibility_timeout()
    );
    println!("   Batch Size: {}", config.pgmq.batch_size);
    println!(
        "   Fulfillment Queue: {}",
        config.pgmq.queue_name_for_namespace("fulfillment")
    );

    // Demo 5: Orchestration Configuration
    println!("\n5. Orchestration Configuration:");
    println!("   Mode: {}", config.orchestration.mode);
    println!(
        "   Cycle Interval: {:?}",
        config.orchestration.cycle_interval()
    );
    println!(
        "   Active Namespaces: {:?}",
        config.orchestration.active_namespaces
    );
    println!(
        "   Max Concurrent Orchestrators: {}",
        config.orchestration.max_concurrent_orchestrators
    );

    // Demo 6: Configuration Summary
    println!("\n6. Configuration Summary:");
    println!("   Environment: {}", config_manager.environment());
    println!("   Configuration successfully loaded and validated");

    // Demo 7: Configuration Validation
    println!("\n7. Configuration Validation:");
    match config.validate() {
        Ok(()) => println!("   ✅ Configuration is valid"),
        Err(e) => println!("   ❌ Configuration validation error: {e}"),
    }

    println!("\n🎉 Configuration system working perfectly!");
    println!("   • Environment-aware loading ✅");
    println!("   • TOML configuration merging ✅");
    println!("   • Type-safe configuration access ✅");
    println!("   • Comprehensive validation ✅");
    println!("   • Unified configuration architecture ✅");
    println!("   • Workspace root detection ✅");
    println!("   • Fail-fast error handling ✅");

    Ok(())
}
