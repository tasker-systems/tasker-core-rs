//! # Message Structures for pgmq Queues
//!
//! Defines message formats for queue-based workflow orchestration.
//! These replace the Command structures from the TCP system.

use serde::{Deserialize, Serialize};
use serde_json;
use std::collections::HashMap;
use uuid::Uuid;

/// Message for step execution via queues
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepMessage {
    /// Database step ID (big integer from database)
    pub step_id: i64,
    /// Database task ID (big integer from database)
    pub task_id: i64,
    /// Task namespace (e.g., "fulfillment", "inventory", "notifications")
    pub namespace: String,
    /// Task name (e.g., "process_order")
    pub task_name: String,
    /// Task version for compatibility tracking
    pub task_version: String,
    /// Step name within the task
    pub step_name: String,
    /// Step execution payload/data
    pub step_payload: serde_json::Value,
    /// Message metadata
    pub metadata: StepMessageMetadata,
}

/// Metadata for step messages
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepMessageMetadata {
    /// When the message was created
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Current retry count
    pub retry_count: u32,
    /// Maximum retry attempts
    pub max_retries: u32,
    /// Message timeout in milliseconds
    pub timeout_ms: u64,
    /// Message correlation ID for tracking
    pub correlation_id: Option<String>,
    /// Priority level (higher number = higher priority)
    pub priority: u8,
    /// Additional context data
    pub context: HashMap<String, serde_json::Value>,
}

impl Default for StepMessageMetadata {
    fn default() -> Self {
        Self {
            created_at: chrono::Utc::now(),
            retry_count: 0,
            max_retries: 3,
            timeout_ms: 30000, // 30 seconds
            correlation_id: Some(Uuid::new_v4().to_string()),
            priority: 5, // Normal priority
            context: HashMap::new(),
        }
    }
}

impl StepMessage {
    /// Create a new step message
    pub fn new(
        step_id: i64,
        task_id: i64,
        namespace: String,
        task_name: String,
        task_version: String,
        step_name: String,
        step_payload: serde_json::Value,
    ) -> Self {
        Self {
            step_id,
            task_id,
            namespace,
            task_name,
            task_version,
            step_name,
            step_payload,
            metadata: StepMessageMetadata::default(),
        }
    }

    /// Create step message with custom metadata
    pub fn with_metadata(
        step_id: i64,
        task_id: i64,
        namespace: String,
        task_name: String,
        task_version: String,
        step_name: String,
        step_payload: serde_json::Value,
        metadata: StepMessageMetadata,
    ) -> Self {
        Self {
            step_id,
            task_id,
            namespace,
            task_name,
            task_version,
            step_name,
            step_payload,
            metadata,
        }
    }

    /// Get the queue name for this message based on namespace
    pub fn queue_name(&self) -> String {
        format!("{}_queue", self.namespace)
    }

    /// Convert to JSON for queue storage
    pub fn to_json(&self) -> Result<serde_json::Value, serde_json::Error> {
        serde_json::to_value(self)
    }

    /// Create from JSON from queue
    pub fn from_json(json: serde_json::Value) -> Result<Self, serde_json::Error> {
        serde_json::from_value(json)
    }

    /// Increment retry count
    pub fn increment_retry(&mut self) {
        self.metadata.retry_count += 1;
    }

    /// Check if message has exceeded max retries
    pub fn is_max_retries_exceeded(&self) -> bool {
        self.metadata.retry_count >= self.metadata.max_retries
    }

    /// Check if message has timed out
    pub fn is_expired(&self) -> bool {
        let elapsed = chrono::Utc::now()
            .signed_duration_since(self.metadata.created_at)
            .num_milliseconds() as u64;
        elapsed > self.metadata.timeout_ms
    }

    /// Get message age in milliseconds
    pub fn age_ms(&self) -> u64 {
        chrono::Utc::now()
            .signed_duration_since(self.metadata.created_at)
            .num_milliseconds() as u64
    }
}

/// Result of step execution for completion tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepResult {
    /// Step that was executed
    pub step_id: i64,
    /// Task containing the step
    pub task_id: i64,
    /// Execution status
    pub status: StepExecutionStatus,
    /// Result data from step execution
    pub result_data: Option<serde_json::Value>,
    /// Error information if step failed
    pub error: Option<StepExecutionError>,
    /// Execution timing
    pub execution_time_ms: u64,
    /// When the step completed
    pub completed_at: chrono::DateTime<chrono::Utc>,
}

/// Step execution status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum StepExecutionStatus {
    /// Step completed successfully
    Success,
    /// Step failed with error
    Failed,
    /// Step was cancelled
    Cancelled,
    /// Step timed out
    TimedOut,
    /// Step was retried
    Retried,
}

/// Error information for failed steps
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepExecutionError {
    /// Error type/category
    pub error_type: String,
    /// Human-readable error message
    pub message: String,
    /// Whether the error is retryable
    pub retryable: bool,
    /// Additional error details
    pub details: Option<HashMap<String, serde_json::Value>>,
    /// Stack trace if available
    pub stack_trace: Option<String>,
}

impl StepResult {
    /// Create successful step result
    pub fn success(
        step_id: i64,
        task_id: i64,
        result_data: Option<serde_json::Value>,
        execution_time_ms: u64,
    ) -> Self {
        Self {
            step_id,
            task_id,
            status: StepExecutionStatus::Success,
            result_data,
            error: None,
            execution_time_ms,
            completed_at: chrono::Utc::now(),
        }
    }

    /// Create failed step result
    pub fn failed(
        step_id: i64,
        task_id: i64,
        error: StepExecutionError,
        execution_time_ms: u64,
    ) -> Self {
        Self {
            step_id,
            task_id,
            status: StepExecutionStatus::Failed,
            result_data: None,
            error: Some(error),
            execution_time_ms,
            completed_at: chrono::Utc::now(),
        }
    }

    /// Create timed out step result
    pub fn timed_out(step_id: i64, task_id: i64, execution_time_ms: u64) -> Self {
        Self {
            step_id,
            task_id,
            status: StepExecutionStatus::TimedOut,
            result_data: None,
            error: Some(StepExecutionError {
                error_type: "TimeoutError".to_string(),
                message: "Step execution timed out".to_string(),
                retryable: true,
                details: None,
                stack_trace: None,
            }),
            execution_time_ms,
            completed_at: chrono::Utc::now(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_step_message_creation() {
        let message = StepMessage::new(
            12345,
            67890,
            "fulfillment".to_string(),
            "process_order".to_string(),
            "1.0.0".to_string(),
            "validate_order".to_string(),
            serde_json::json!({"order_id": 1001}),
        );

        assert_eq!(message.step_id, 12345);
        assert_eq!(message.task_id, 67890);
        assert_eq!(message.namespace, "fulfillment");
        assert_eq!(message.queue_name(), "fulfillment_queue");
        assert_eq!(message.metadata.retry_count, 0);
        assert!(!message.is_max_retries_exceeded());
        assert!(!message.is_expired());
    }

    #[test]
    fn test_step_message_json_serialization() {
        let message = StepMessage::new(
            12345,
            67890,
            "fulfillment".to_string(),
            "process_order".to_string(),
            "1.0.0".to_string(),
            "validate_order".to_string(),
            serde_json::json!({"order_id": 1001}),
        );

        let json = message.to_json().unwrap();
        let deserialized = StepMessage::from_json(json).unwrap();

        assert_eq!(message.step_id, deserialized.step_id);
        assert_eq!(message.task_id, deserialized.task_id);
        assert_eq!(message.namespace, deserialized.namespace);
    }

    #[test]
    fn test_step_result_creation() {
        let result = StepResult::success(
            12345,
            67890,
            Some(serde_json::json!({"status": "validated"})),
            1500,
        );

        assert_eq!(result.step_id, 12345);
        assert_eq!(result.task_id, 67890);
        assert_eq!(result.status, StepExecutionStatus::Success);
        assert_eq!(result.execution_time_ms, 1500);
        assert!(result.error.is_none());
    }

    #[test]
    fn test_retry_logic() {
        let mut message = StepMessage::new(
            12345,
            67890,
            "fulfillment".to_string(),
            "process_order".to_string(),
            "1.0.0".to_string(),
            "validate_order".to_string(),
            serde_json::json!({"order_id": 1001}),
        );

        assert!(!message.is_max_retries_exceeded());

        // Increment retries beyond max
        for _ in 0..4 {
            message.increment_retry();
        }

        assert!(message.is_max_retries_exceeded());
    }
}
