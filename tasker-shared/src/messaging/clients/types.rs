use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use uuid::Uuid;

/// Queue message for step execution (pgmq-rs version)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PgmqStepMessage {
    pub step_uuid: Uuid,
    pub task_uuid: Uuid,
    pub namespace: String,
    pub step_name: String,
    pub step_payload: serde_json::Value,
    pub metadata: PgmqStepMessageMetadata,
}

/// Metadata for step messages (pgmq-rs version)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PgmqStepMessageMetadata {
    pub enqueued_at: DateTime<Utc>,
    pub retry_count: i32,
    pub max_retries: i32,
    pub timeout_seconds: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct QueueMetrics {
    pub queue_name: String,
    pub message_count: i64,
    pub consumer_count: Option<i32>,
    pub oldest_message_age_seconds: Option<i64>,
}

/// Client status for health checks and monitoring
#[derive(Debug, Clone)]
pub struct ClientStatus {
    pub client_type: String,
    pub connected: bool,
    pub connection_info: HashMap<String, JsonValue>,
    pub last_activity: Option<chrono::DateTime<chrono::Utc>>,
}
