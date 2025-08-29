use super::state::OperationalStateConfig;
use serde::{Deserialize, Serialize};
use std::time::Duration;

pub mod task_claim_step_enqueuer;
pub use task_claim_step_enqueuer::TaskClaimStepEnqueuerConfig;
pub mod step_enqueuer;
pub use step_enqueuer::StepEnqueuerConfig;
pub mod step_result_processor;
pub use step_result_processor::StepResultProcessorConfig;
pub mod task_claimer;
pub use crate::config::executor::{ExecutorConfig, ExecutorType};
pub use task_claimer::TaskClaimerConfig;
pub mod event_systems;
pub use event_systems::OrchestrationEventSystemConfig;

/// Orchestration system configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OrchestrationConfig {
    pub mode: String,
    pub active_namespaces: Vec<String>,
    pub enable_performance_logging: bool,
    // Note: Queue configuration removed - use TaskerConfig.queues for centralized queue config
    pub event_systems: OrchestrationEventSystemConfig,
    pub enable_heartbeat: bool,
    pub heartbeat_interval_ms: u64,
    /// TAS-37 Supplemental: Shutdown-aware monitoring configuration
    pub operational_state: OperationalStateConfig,
}

impl OrchestrationConfig {
    /// Get heartbeat interval as Duration
    pub fn heartbeat_interval(&self) -> Duration {
        Duration::from_millis(self.heartbeat_interval_ms)
    }
}

impl Default for OrchestrationConfig {
    fn default() -> Self {
        Self {
            mode: "standalone".to_string(),
            active_namespaces: vec![
                "fulfillment".to_string(),
                "inventory".to_string(),
                "notifications".to_string(),
                "payments".to_string(),
                "analytics".to_string(),
            ],
            enable_performance_logging: false,
            // Queue configuration now comes from centralized QueuesConfig
            event_systems: OrchestrationEventSystemConfig::default(),
            enable_heartbeat: true,
            heartbeat_interval_ms: 5000,
            operational_state: OperationalStateConfig::default(), // TAS-37 Supplemental: Add missing field
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
    pub active_namespaces: Vec<String>,
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
            active_namespaces: vec![
                "fulfillment".to_string(),
                "inventory".to_string(),
                "notifications".to_string(),
                "payments".to_string(),
                "analytics".to_string(),
            ],
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
            active_namespaces: config.orchestration.active_namespaces.clone(),
            enable_performance_logging: config.orchestration.enable_performance_logging,
        }
    }
}
