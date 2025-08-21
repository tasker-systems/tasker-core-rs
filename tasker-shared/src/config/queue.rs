use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Queue configuration for orchestration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct QueueConfig {
    pub task_requests: String,
    pub task_processing: String,
    pub batch_results: String,
    pub step_results: String,
    pub worker_queues: HashMap<String, String>,
    pub settings: QueueSettings,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct QueueSettings {
    pub visibility_timeout_seconds: u64,
    pub message_retention_seconds: u64,
    pub dead_letter_queue_enabled: bool,
    pub max_receive_count: u32,
}
