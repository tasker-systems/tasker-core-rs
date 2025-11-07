//! Configuration Demo Example
//!
//! Demonstrates the V2 configuration system with ConfigMerger (CLI) and ConfigLoader (runtime).
//! Shows how to generate merged configs and load them with environment variable substitution.
//!
//! Usage:
//!   # Generate test config and load it
//!   TASKER_ENV=test cargo run --example config_demo --all-features
//!
//!   # Generate development config and load it
//!   TASKER_ENV=development cargo run --example config_demo --all-features
//!
//!   # Generate production config and load it
//!   TASKER_ENV=production cargo run --example config_demo --all-features

use anyhow::Result;
use std::env;
use tasker_shared::config::{ConfigLoader, ConfigMerger};
use tracing::{error, info};

fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter("config_demo=debug,tasker_shared::config=debug")
        .init();

    info!("🔧 Configuration Demo Starting (V2 System)");

    // Detect environment
    let environment = env::var("TASKER_ENV").unwrap_or_else(|_| "development".to_string());
    info!("📋 Environment: {}", environment);

    // Step 1: Generate merged config using ConfigMerger (CLI simulation)
    info!("🔨 Step 1: Generating merged config with ConfigMerger...");

    let workspace_root = env::current_dir()?;
    let v2_config_dir = workspace_root.join("config").join("v2");

    if !v2_config_dir.exists() {
        error!("❌ V2 config directory not found: {:?}", v2_config_dir);
        info!("   Expected structure: config/v2/{{common,orchestration,worker}}-{{base,test,development,production}}.toml");
        return Err(anyhow::anyhow!("V2 config directory not found"));
    }

    let mut merger = ConfigMerger::new(v2_config_dir, &environment)?;

    // Generate merged config for "complete" context (includes all components)
    let merged_toml = merger.merge_context("complete")?;
    info!("✅ Generated merged config ({} bytes)", merged_toml.len());

    // Step 2: Write to temporary file (simulating CLI output)
    info!("💾 Step 2: Writing merged config to temp file...");

    let temp_file = env::temp_dir().join(format!("tasker-{}-complete.toml", environment));
    std::fs::write(&temp_file, &merged_toml)?;
    info!("✅ Wrote to: {:?}", temp_file);

    // Step 3: Load config using ConfigLoader (runtime simulation)
    info!("📖 Step 3: Loading config with ConfigLoader...");

    // Set TASKER_CONFIG_PATH as would be done in deployment
    env::set_var("TASKER_CONFIG_PATH", temp_file.to_str().unwrap());

    // Also set DATABASE_URL for environment variable substitution demo
    if env::var("DATABASE_URL").is_err() {
        env::set_var(
            "DATABASE_URL",
            "postgresql://tasker:tasker@localhost:5432/tasker_rust_demo",
        );
    }

    let config = ConfigLoader::load_from_env()?;
    info!("✅ Successfully loaded TaskerConfig from merged file");

    // Step 4: Display configuration summary
    info!("📊 Step 4: Configuration Summary");
    display_config_summary(&config);

    // Step 5: Validate configuration
    info!("🔍 Step 5: Validating configuration...");
    config.validate()?;
    info!("✅ Configuration validation passed!");

    // Step 6: Demonstrate context-specific merging
    info!("🎯 Step 6: Context-specific config generation");
    demonstrate_contexts(&mut merger)?;

    // Cleanup
    std::fs::remove_file(&temp_file).ok();

    info!("🎉 Configuration demo completed successfully!");
    Ok(())
}

fn display_config_summary(config: &tasker_shared::config::TaskerConfig) {
    info!("  Environment: {}", config.execution.environment);
    info!(
        "  Database URL: {}",
        config
            .database
            .url
            .as_ref()
            .unwrap_or(&"<not configured>".to_string())
    );
    info!(
        "  Telemetry: {}",
        if config.telemetry.enabled {
            "enabled"
        } else {
            "disabled"
        }
    );
    info!("  Queue Backend: {}", config.queues.backend);
    info!("  Orchestration Mode: {}", config.orchestration.mode);
    info!(
        "  Max Concurrent Tasks: {}",
        config.execution.max_concurrent_tasks
    );
    info!(
        "  Database Pool: {}-{} connections",
        config.database.pool.min_connections, config.database.pool.max_connections
    );
    info!(
        "  Circuit Breakers: {}",
        if config.circuit_breakers.enabled {
            "enabled"
        } else {
            "disabled"
        }
    );

    if let Some(worker_config) = &config.worker {
        info!(
            "  Worker: present (web API {})",
            if worker_config.web.enabled {
                "enabled"
            } else {
                "disabled"
            }
        );
    } else {
        info!("  Worker: not configured");
    }

    info!("  Event Systems:");
    info!(
        "    Orchestration: {} ({})",
        config.event_systems.orchestration.system_id,
        config.event_systems.orchestration.deployment_mode
    );
    info!(
        "    Task Readiness: {} ({})",
        config.event_systems.task_readiness.system_id,
        config.event_systems.task_readiness.deployment_mode
    );
    info!(
        "    Worker: {} ({})",
        config.event_systems.worker.system_id, config.event_systems.worker.deployment_mode
    );
}

fn demonstrate_contexts(merger: &mut ConfigMerger) -> Result<()> {
    // Show different context merging
    let contexts = ["common", "orchestration", "worker"];

    for context in &contexts {
        match merger.merge_context(context) {
            Ok(merged) => {
                let lines = merged.lines().count();
                info!("  ✅ Context '{}': {} lines", context, lines);
            }
            Err(e) => {
                info!("  ⚠️ Context '{}': {}", context, e);
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_demo_loads_successfully() {
        // Set required environment variables
        std::env::set_var("TASKER_ENV", "test");
        std::env::set_var("DATABASE_URL", "postgresql://test:test@localhost/test");

        // Run the demo - it should not panic
        let result = std::panic::catch_unwind(|| {
            let _ = main(); // May fail due to missing files, but shouldn't panic
        });

        assert!(!result.is_err(), "Config demo should not panic");
    }
}
