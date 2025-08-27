use super::queue::{QueueConfig, QueueSettings};
use super::state::OperationalStateConfig;
use serde::{Deserialize, Serialize};
use std::time::{Duration, SystemTime};

pub mod embedded;
pub use embedded::EmbeddedOrchestratorConfig;

pub mod task_claim_step_enqueuer;
pub use task_claim_step_enqueuer::TaskClaimStepEnqueuerConfig;
pub mod step_enqueuer;
pub use step_enqueuer::StepEnqueuerConfig;
pub mod step_result_processor;
pub use step_result_processor::StepResultProcessorConfig;
pub mod task_claimer;
pub use crate::config::executor::{ExecutorConfig, ExecutorType};
pub use task_claimer::TaskClaimerConfig;

/// Orchestration system configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OrchestrationConfig {
    pub mode: String,
    pub task_requests_queue_name: String,
    pub tasks_per_cycle: u32,
    pub cycle_interval_ms: u64,
    pub task_request_polling_interval_ms: u64,
    pub task_request_visibility_timeout_seconds: u64,
    pub task_request_batch_size: u32,
    pub active_namespaces: Vec<String>,
    pub max_concurrent_orchestrators: u32,
    pub enable_performance_logging: bool,
    pub default_claim_timeout_seconds: u64,
    pub queues: QueueConfig,
    pub embedded_orchestrator: EmbeddedOrchestratorConfig,
    pub enable_heartbeat: bool,
    pub heartbeat_interval_ms: u64,
    /// TAS-37 Supplemental: Shutdown-aware monitoring configuration
    pub operational_state: OperationalStateConfig,
}

impl OrchestrationConfig {
    /// Get cycle interval as Duration
    pub fn cycle_interval(&self) -> Duration {
        Duration::from_millis(self.cycle_interval_ms)
    }

    /// Get task request polling interval as Duration
    pub fn task_request_polling_interval(&self) -> Duration {
        Duration::from_millis(self.task_request_polling_interval_ms)
    }

    /// Get heartbeat interval as Duration
    pub fn heartbeat_interval(&self) -> Duration {
        Duration::from_millis(self.heartbeat_interval_ms)
    }

    /// Get task request visibility timeout as Duration
    pub fn task_request_visibility_timeout(&self) -> Duration {
        Duration::from_secs(self.task_request_visibility_timeout_seconds)
    }

    /// Get default claim timeout as Duration
    pub fn default_claim_timeout(&self) -> Duration {
        Duration::from_secs(self.default_claim_timeout_seconds)
    }

    /// Convert to OrchestrationSystemConfig for bootstrapping the orchestration system
    pub fn to_orchestration_system_config(&self) -> OrchestrationSystemConfig {
        // Generate orchestrator ID if not provided
        let timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let orchestrator_id = format!("orchestrator-{timestamp}");

        // Create orchestration loop configuration
        let orchestration_loop_config = TaskClaimStepEnqueuerConfig {
            tasks_per_cycle: self.tasks_per_cycle as i32,
            namespace_filter: None,
            cycle_interval: Duration::from_millis(self.cycle_interval_ms),
            max_cycles: None,
            enable_performance_logging: self.enable_performance_logging,
            enable_heartbeat: self.enable_heartbeat,
            task_claimer_config: TaskClaimerConfig {
                max_batch_size: self.tasks_per_cycle as i32,
                default_claim_timeout: self.default_claim_timeout_seconds as i32,
                heartbeat_interval: Duration::from_millis(self.heartbeat_interval_ms),
                enable_heartbeat: self.enable_heartbeat,
            },
            step_enqueuer_config: StepEnqueuerConfig::default(),
            step_result_processor_config: StepResultProcessorConfig::default(),
        };

        OrchestrationSystemConfig {
            task_requests_queue_name: self.task_requests_queue_name.clone(),
            orchestrator_id,
            orchestration_loop_config,
            task_request_polling_interval_ms: self.task_request_polling_interval_ms,
            task_request_visibility_timeout_seconds: self.task_request_visibility_timeout_seconds
                as i32,
            task_request_batch_size: self.task_request_batch_size as i32,
            active_namespaces: self.active_namespaces.clone(),
            max_concurrent_orchestrators: self.max_concurrent_orchestrators as usize,
            enable_performance_logging: self.enable_performance_logging,
        }
    }
}

impl Default for OrchestrationConfig {
    fn default() -> Self {
        use std::collections::HashMap;

        Self {
            mode: "embedded".to_string(),
            task_requests_queue_name: "task_requests_queue".to_string(),
            tasks_per_cycle: 5,
            cycle_interval_ms: 250,
            task_request_polling_interval_ms: 250,
            task_request_visibility_timeout_seconds: 300,
            task_request_batch_size: 10,
            active_namespaces: vec![
                "fulfillment".to_string(),
                "inventory".to_string(),
                "notifications".to_string(),
                "payments".to_string(),
                "analytics".to_string(),
            ],
            max_concurrent_orchestrators: 3,
            enable_performance_logging: false,
            default_claim_timeout_seconds: 300,
            queues: QueueConfig {
                orchestration_owned: super::queue::OrchestrationOwnedQueues::default(),
                worker_queues: {
                    let mut queues = HashMap::new();
                    queues.insert("default".to_string(), "default_queue".to_string());
                    queues.insert("fulfillment".to_string(), "fulfillment_queue".to_string());
                    queues
                },
                settings: QueueSettings {
                    visibility_timeout_seconds: 30,
                    message_retention_seconds: 604800,
                    dead_letter_queue_enabled: true,
                    max_receive_count: 3,
                },
            },
            embedded_orchestrator: EmbeddedOrchestratorConfig {
                auto_start: false,
                namespaces: vec!["default".to_string(), "fulfillment".to_string()],
                shutdown_timeout_seconds: 30,
            },
            enable_heartbeat: true,
            heartbeat_interval_ms: 5000,
            operational_state: OperationalStateConfig::default(), // TAS-37 Supplemental: Add missing field
        }
    }
}

/// Configuration for the orchestration system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestrationSystemConfig {
    /// Queue name for task requests
    pub task_requests_queue_name: String,
    /// Orchestrator instance identifier
    pub orchestrator_id: String,
    /// Orchestration loop configuration
    pub orchestration_loop_config: TaskClaimStepEnqueuerConfig,
    /// Task request processor polling interval in milliseconds
    pub task_request_polling_interval_ms: u64,
    /// Visibility timeout for task request messages (seconds)
    pub task_request_visibility_timeout_seconds: i32,
    /// Number of task requests to process per batch
    pub task_request_batch_size: i32,
    /// Namespaces to create queues for
    pub active_namespaces: Vec<String>,
    /// Maximum concurrent orchestration loops
    pub max_concurrent_orchestrators: usize,
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
            task_requests_queue_name: "task_requests_queue".to_string(),
            orchestrator_id: format!("orchestrator-{timestamp}"),
            orchestration_loop_config: TaskClaimStepEnqueuerConfig::default(),
            task_request_polling_interval_ms: 250, // 250ms = 4x/sec default
            task_request_visibility_timeout_seconds: 300, // 5 minutes
            task_request_batch_size: 10,
            active_namespaces: vec![
                "fulfillment".to_string(),
                "inventory".to_string(),
                "notifications".to_string(),
                "payments".to_string(),
                "analytics".to_string(),
            ],
            max_concurrent_orchestrators: 3,
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
            task_requests_queue_name: config.orchestration.task_requests_queue_name.clone(),
            orchestrator_id: format!("orchestrator-{timestamp}"),
            orchestration_loop_config: TaskClaimStepEnqueuerConfig::from_config_manager(
                config_manager,
            ),

            task_request_polling_interval_ms: config.orchestration.task_request_polling_interval_ms,
            task_request_visibility_timeout_seconds: config
                .orchestration
                .task_request_visibility_timeout_seconds
                as i32,
            task_request_batch_size: config.orchestration.task_request_batch_size as i32,
            active_namespaces: config.orchestration.active_namespaces.clone(),
            max_concurrent_orchestrators: config.orchestration.max_concurrent_orchestrators
                as usize,
            enable_performance_logging: config.orchestration.enable_performance_logging,
        }
    }
}
