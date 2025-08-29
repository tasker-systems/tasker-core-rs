use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Queue configuration for orchestration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct QueueConfig {
    /// Orchestration-owned queue configurations for message classification
    pub orchestration_owned: OrchestrationOwnedQueues,
    /// Worker namespace queues (namespace -> queue_name)
    pub worker_queues: HashMap<String, String>,
    /// Queue settings for visibility timeout, retention, etc.
    pub settings: QueueSettings,
    pub orchestration_namespace: String,
    pub worker_namespace: String,
}

/// Configuration for orchestration-owned queues used in message classification
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OrchestrationOwnedQueues {
    pub step_results: String,
    pub task_requests: String,
    pub task_finalizations: String,
}

impl Default for OrchestrationOwnedQueues {
    fn default() -> Self {
        Self {
            step_results: "orchestration_step_results".to_string(),
            task_requests: "orchestration_task_requests".to_string(),
            task_finalizations: "orchestration_task_finalizations".to_string(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct QueueSettings {
    pub visibility_timeout_seconds: u64,
    pub message_retention_seconds: u64,
    pub dead_letter_queue_enabled: bool,
    pub max_receive_count: u32,
}

impl Default for QueueSettings {
    fn default() -> Self {
        Self {
            visibility_timeout_seconds: 30,
            message_retention_seconds: 1209600,
            dead_letter_queue_enabled: true,
            max_receive_count: 3,
        }
    }
}

impl Default for QueueConfig {
    fn default() -> Self {
        Self {
            orchestration_owned: OrchestrationOwnedQueues::default(),
            worker_queues: HashMap::new(),
            settings: QueueSettings::default(),
            orchestration_namespace: "orchestration".to_string(),
            worker_namespace: "worker".to_string(),
        }
    }
}
