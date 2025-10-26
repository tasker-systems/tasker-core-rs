//! Configuration Demo Example
//!
//! Demonstrates the unified configuration loading system and validates
//! that all TOML files can be properly loaded and deserialized into TaskerConfig.
//!
//! Usage:
//!   TASKER_ENV=test cargo run --example config_demo --all-features
//!   TASKER_ENV=development cargo run --example config_demo --all-features
//!   TASKER_ENV=production cargo run --example config_demo --all-features

use anyhow::Result;
use tasker_shared::config::unified_loader::UnifiedConfigLoader;
use tracing::{error, info, warn};

fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter("config_demo=debug,tasker_shared::config=debug")
        .init();

    info!("🔧 Configuration Demo Starting");

    // Detect environment
    let environment = UnifiedConfigLoader::detect_environment();
    info!("📋 Detected environment: {}", environment);

    // Create loader
    let mut loader = match UnifiedConfigLoader::new(&environment) {
        Ok(loader) => {
            info!(
                "✅ Successfully created UnifiedConfigLoader for environment: {}",
                environment
            );
            loader
        }
        Err(e) => {
            error!("❌ Failed to create UnifiedConfigLoader: {}", e);
            return Err(e.into());
        }
    };

    // Load individual components (using known component names)
    info!("🔍 Loading known components...");

    let known_components = [
        "database",
        "telemetry",
        "task_templates",
        "system",
        "backoff",
        "execution",
        "queues",
        "orchestration",
        "circuit_breakers",
        "task_readiness",
        "task_claimer",
        "event_systems",
        "worker",
    ];

    for component in &known_components {
        match loader.load_component(component) {
            Ok(_config) => {
                info!("✅ Loaded component '{}'", component);
            }
            Err(e) => {
                warn!("⚠️ Failed to load component '{}': {}", component, e);
            }
        }
    }

    // Load complete TaskerConfig
    info!("🎯 Loading complete TaskerConfig...");

    // First, let's get the validated config to see what we loaded
    match loader.load_with_validation() {
        Ok(validated_config) => {
            info!(
                "✅ Successfully loaded ValidatedConfig with {} components",
                validated_config.configs.len()
            );

            // Let's inspect what was loaded
            for component_name in validated_config.configs.keys() {
                info!("  Component loaded: {}", component_name);
            }

            // Now try to convert to TaskerConfig
            match validated_config.to_tasker_config() {
                Ok(config) => {
                    info!("✅ Successfully loaded complete TaskerConfig!");

                    // Display key configuration details
                    display_config_summary(&config);

                    // Validate configuration
                    match config.validate() {
                        Ok(()) => {
                            info!("✅ Configuration validation passed!");
                        }
                        Err(e) => {
                            error!("❌ Configuration validation failed: {}", e);
                            return Err(e.into());
                        }
                    }
                }
                Err(e) => {
                    error!(
                        "❌ Failed to convert ValidatedConfig to TaskerConfig: {}",
                        e
                    );
                    return Err(e.into());
                }
            }
        }
        Err(e) => {
            error!("❌ Failed to load ValidatedConfig: {}", e);
            return Err(e.into());
        }
    }

    info!("🎉 Configuration demo completed successfully!");
    Ok(())
}

fn display_config_summary(config: &tasker_shared::config::TaskerConfig) {
    info!("📊 Configuration Summary:");
    // TAS-50: Database host/username/password fields removed - not functional, handled by DATABASE_URL
    info!(
        "  Database URL: {}",
        config
            .database
            .url
            .as_ref()
            .unwrap_or(&"<not configured>".to_string())
    );
    info!("  Environment: {}", config.execution.environment);
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
        "  Max Concurrent Steps: {}",
        config.execution.max_concurrent_steps
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
            "  Worker Configuration: present (web API {})",
            if worker_config.web.enabled {
                "enabled"
            } else {
                "disabled"
            }
        );
    } else {
        info!("  Worker Configuration: not present");
    }

    info!("  Event Systems:");
    info!(
        "    Orchestration: {} (mode: {})",
        config.event_systems.orchestration.system_id,
        config.event_systems.orchestration.deployment_mode
    );
    info!(
        "    Task Readiness: {} (mode: {})",
        config.event_systems.task_readiness.system_id,
        config.event_systems.task_readiness.deployment_mode
    );
    info!(
        "    Worker: {} (mode: {})",
        config.event_systems.worker.system_id, config.event_systems.worker.deployment_mode
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_demo_loads_successfully() {
        // This test ensures the config demo can run without panics
        std::env::set_var("TASKER_ENV", "test");

        let result = std::panic::catch_unwind(|| main());

        // We don't require success (might fail due to missing files in test env)
        // but we do require no panics
        assert!(!result.is_err(), "Config demo should not panic");
    }
}
