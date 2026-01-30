//! # Orchestration Configuration
//!
//! Configuration types for the orchestration system that coordinates task and step execution.
//!
//! ## Overview
//!
//! This module provides configuration for orchestration behavior including:
//! - **Step Enqueueing**: Batch sizes, delays, and timeouts for step queue operations
//! - **Result Processing**: Queue names, batch sizes, and retry policies
//! - **Batch Processing**: Configuration for batch workflow processing
//! - **Event Systems**: Orchestration-specific event system settings
//!
//! ## Structure
//!
//! ```text
//! orchestration/
//! ├── mod.rs                      # OrchestrationConfig, OrchestrationSystemConfig
//! ├── step_enqueuer.rs            # StepEnqueuerConfig
//! ├── step_result_processor.rs    # StepResultProcessorConfig
//! ├── batch_processing.rs         # BatchProcessingConfig
//! ├── event_systems.rs            # OrchestrationEventSystemConfig
//! └── task_claim_step_enqueuer.rs # TaskClaimStepEnqueuerConfig
//! ```
//!
//! ## Configuration Loading
//!
//! Orchestration configuration is loaded from `config/tasker/base/orchestration.toml`
//! and environment-specific overrides in `config/tasker/environments/{env}/orchestration.toml`.
//!
//! ## Example
//!
//! ```toml
//! [orchestration]
//! mode = "standalone"
//! enable_performance_logging = false
//!
//! [orchestration.web]
//! enabled = true
//! host = "0.0.0.0"
//! port = 3000
//! ```

use serde::{Deserialize, Serialize};

// TAS-61 Phase 6C: decision_points moved to tasker::DecisionPointsConfig
pub mod task_claim_step_enqueuer;
pub use task_claim_step_enqueuer::TaskClaimStepEnqueuerConfig;
pub mod step_enqueuer;
pub use step_enqueuer::StepEnqueuerConfig;
pub mod step_result_processor;
pub use step_result_processor::StepResultProcessorConfig;
pub mod batch_processing;
pub use batch_processing::BatchProcessingConfig;
pub mod event_systems;
pub use crate::config::web::WebConfig;
pub use event_systems::OrchestrationEventSystemConfig;

/// Orchestration system configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OrchestrationConfig {
    pub mode: String,
    pub enable_performance_logging: bool,
    // Note: Queue configuration removed - use TaskerConfig.queues for centralized queue config
    // Note: Event systems configuration moved to unified TaskerConfig.event_systems
    // Note: Heartbeat configuration removed - moved to task_claim_step_enqueuer for TAS-41
    /// Web API configuration
    pub web: WebConfig,
}

impl OrchestrationConfig {
    /// Get web configuration with fallback to defaults
    pub fn web_config(&self) -> WebConfig {
        self.web.clone()
    }

    /// Check if web API is enabled
    pub fn web_enabled(&self) -> bool {
        self.web.enabled
    }
}

impl Default for OrchestrationConfig {
    fn default() -> Self {
        Self {
            mode: "standalone".to_string(),
            enable_performance_logging: false,
            // Queue configuration now comes from centralized QueuesConfig
            // Event systems configuration now comes from unified TaskerConfig.event_systems
            // Heartbeat configuration now comes from task_claim_step_enqueuer for TAS-41
            web: WebConfig::default(),
        }
    }
}

/// Configuration for the orchestration system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestrationSystemConfig {
    /// Orchestrator instance identifier
    pub orchestrator_id: String,
    /// Orchestration loop configuration
    /// Namespaces to create queues for
    /// Enable comprehensive performance logging
    pub enable_performance_logging: bool,
}

impl Default for OrchestrationSystemConfig {
    fn default() -> Self {
        use std::time::SystemTime;

        // Generate a simple orchestrator ID using timestamp
        let timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();

        Self {
            orchestrator_id: format!("orchestrator-{timestamp}"),
            enable_performance_logging: false,
        }
    }
}

impl OrchestrationSystemConfig {
    /// Create OrchestrationSystemConfig from ConfigManager
    pub fn from_config_manager(config_manager: &crate::config::ConfigManager) -> Self {
        use std::time::SystemTime;
        let config = config_manager.config();

        // Generate a simple orchestrator ID using timestamp
        let timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();

        Self {
            orchestrator_id: format!("orchestrator-{timestamp}"),
            // TAS-61 V2: Access orchestration.enable_performance_logging (optional)
            enable_performance_logging: config
                .orchestration
                .as_ref()
                .map(|o| o.enable_performance_logging)
                .unwrap_or(false),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_orchestration_config_default_values() {
        let config = OrchestrationConfig::default();
        assert_eq!(config.mode, "standalone");
        assert!(!config.enable_performance_logging);
    }

    #[test]
    fn test_orchestration_config_web_config() {
        let config = OrchestrationConfig::default();
        let web = config.web_config();
        // web_config returns a clone of the web field
        assert_eq!(web.enabled, config.web.enabled);
    }

    #[test]
    fn test_orchestration_config_web_enabled() {
        let mut config = OrchestrationConfig::default();
        // Default web config has enabled field from WebConfig::default()
        let default_enabled = config.web_enabled();

        // Verify web_enabled reflects the web.enabled field
        config.web.enabled = !default_enabled;
        assert_eq!(config.web_enabled(), !default_enabled);
    }

    #[test]
    fn test_orchestration_system_config_default_generates_id() {
        let config = OrchestrationSystemConfig::default();
        assert!(config.orchestrator_id.starts_with("orchestrator-"));
        assert!(!config.enable_performance_logging);
    }

    #[test]
    fn test_orchestration_system_config_unique_ids() {
        let config1 = OrchestrationSystemConfig::default();
        // Small delay to ensure different timestamp
        std::thread::sleep(std::time::Duration::from_millis(1));
        let config2 = OrchestrationSystemConfig::default();
        // IDs should differ (based on timestamp)
        assert_ne!(config1.orchestrator_id, config2.orchestrator_id);
    }
}
