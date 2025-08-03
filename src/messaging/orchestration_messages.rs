//! # Orchestration Message Types
//!
//! Message structures for the complete pgmq-based orchestration workflow.
//! These messages flow through the orchestration queues to coordinate
//! task processing from request to completion.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::messaging::message::OrchestrationMetadata;

/// Message for task requests sent to orchestration_task_requests queue
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRequestMessage {
    /// Unique identifier for this request
    pub request_id: String,
    /// Namespace for the task (e.g., "fulfillment", "inventory")
    pub namespace: String,
    /// Name of the task to execute
    pub task_name: String,
    /// Version of the task handler
    pub task_version: String,
    /// Input data for the task
    pub input_data: serde_json::Value,
    /// Request metadata
    pub metadata: TaskRequestMetadata,
}

/// Metadata for task requests
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRequestMetadata {
    /// When the request was created
    pub requested_at: DateTime<Utc>,
    /// Who/what made the request
    pub requester: String,
    /// Request priority
    pub priority: TaskPriority,
    /// Additional custom metadata
    pub custom: HashMap<String, serde_json::Value>,
}

/// Task priority levels
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum TaskPriority {
    Low,
    Normal,
    High,
    Urgent,
}

impl Default for TaskPriority {
    fn default() -> Self {
        TaskPriority::Normal
    }
}

impl From<TaskPriority> for i32 {
    fn from(priority: TaskPriority) -> i32 {
        match priority {
            TaskPriority::Low => 1,
            TaskPriority::Normal => 2,
            TaskPriority::High => 3,
            TaskPriority::Urgent => 4,
        }
    }
}

/// Message for step batches sent to namespace-specific queues
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchMessage {
    /// Database batch ID from step_execution_batch
    pub batch_id: i64,
    /// Task ID this batch belongs to
    pub task_id: i64,
    /// Namespace for worker routing
    pub namespace: String,
    /// Task name for context
    pub task_name: String,
    /// Task version for compatibility
    pub task_version: String,
    /// Steps in this batch
    pub steps: Vec<BatchStep>,
    /// Batch metadata
    pub metadata: BatchMetadata,
}

/// Individual step within a batch
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchStep {
    /// Database step ID
    pub step_id: i64,
    /// Step sequence number within task
    pub sequence: i32,
    /// Name of the step to execute
    pub step_name: String,
    /// Step execution payload
    pub step_payload: serde_json::Value,
    /// Step-specific metadata
    pub step_metadata: HashMap<String, serde_json::Value>,
}

/// Metadata for batches
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchMetadata {
    /// When the batch was created
    pub batch_created_at: DateTime<Utc>,
    /// Timeout for batch execution (seconds)
    pub timeout_seconds: i32,
    /// Retry policy for the batch
    pub retry_policy: BatchRetryPolicy,
    /// Maximum number of retry attempts
    pub max_retries: i32,
    /// Current retry attempt
    pub retry_count: i32,
}

/// Retry policies for batch execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BatchRetryPolicy {
    None,
    Linear {
        delay_seconds: i32,
    },
    ExponentialBackoff {
        base_delay_seconds: i32,
        max_delay_seconds: i32,
    },
    Custom {
        delays: Vec<i32>,
    },
}

impl Default for BatchRetryPolicy {
    fn default() -> Self {
        BatchRetryPolicy::ExponentialBackoff {
            base_delay_seconds: 30,
            max_delay_seconds: 300,
        }
    }
}

/// Message for individual step results sent to orchestration_step_results queue
/// Replaces BatchResultMessage for individual step processing architecture
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepResultMessage {
    /// Step ID that was processed
    pub step_id: i64,
    /// Task ID for context
    pub task_id: i64,
    /// Namespace for routing
    pub namespace: String,
    /// Step execution status
    pub status: StepExecutionStatus,
    /// Step execution results/output
    pub results: Option<serde_json::Value>,
    /// Error information if step failed
    pub error: Option<StepExecutionError>,
    /// Execution time in milliseconds
    pub execution_time_ms: u64,
    /// Orchestration metadata from worker
    pub orchestration_metadata: Option<OrchestrationMetadata>,
    /// Result metadata  
    pub metadata: StepResultMetadata,
}

/// DEPRECATED: Message for batch results - kept for backward compatibility
/// Use StepResultMessage for new individual step processing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchResultMessage {
    /// Batch ID that was processed
    pub batch_id: i64,
    /// Task ID for aggregation
    pub task_id: i64,
    /// Namespace for routing
    pub namespace: String,
    /// Overall batch execution status
    pub batch_status: BatchExecutionStatus,
    /// Results for individual steps
    pub step_results: Vec<StepResult>,
    /// Result metadata
    pub metadata: BatchResultMetadata,
}

/// Overall status of batch execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BatchExecutionStatus {
    Success,
    PartialSuccess,
    Failed,
    Timeout,
    Cancelled,
}

/// Result of individual step execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepResult {
    /// Step ID that was executed
    pub step_id: i64,
    /// Step execution status
    pub status: StepExecutionStatus,
    /// Step output data
    pub output: Option<serde_json::Value>,
    /// Error message if failed
    pub error: Option<String>,
    /// Error code for categorization
    pub error_code: Option<String>,
    /// When the step was executed
    pub executed_at: DateTime<Utc>,
    /// Execution duration in milliseconds
    pub execution_duration_ms: i64,
}

/// Status of individual step execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StepExecutionStatus {
    Success,
    Failed,
    Skipped,
    Timeout,
    Cancelled,
}

/// Error information for failed step execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepExecutionError {
    /// Error message
    pub message: String,
    /// Error type/category
    pub error_type: Option<String>,
    /// HTTP status code if applicable
    pub status_code: Option<u16>,
    /// Additional error context
    pub context: HashMap<String, serde_json::Value>,
    /// Whether this error is retryable
    pub retryable: bool,
}

/// Metadata for individual step results
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepResultMetadata {
    /// Worker that processed the step
    pub worker_id: String,
    /// Worker hostname
    pub worker_hostname: Option<String>,
    /// When step processing started
    pub started_at: DateTime<Utc>,
    /// When step processing completed
    pub completed_at: DateTime<Utc>,
    /// Additional custom metadata
    pub custom: HashMap<String, serde_json::Value>,
}

/// Metadata for batch results
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchResultMetadata {
    /// Worker that processed the batch
    pub worker_id: String,
    /// Worker hostname
    pub worker_hostname: Option<String>,
    /// Total batch execution time in milliseconds
    pub total_execution_time_ms: i64,
    /// When batch processing started
    pub started_at: DateTime<Utc>,
    /// When batch processing completed
    pub completed_at: DateTime<Utc>,
    /// Number of steps that succeeded
    pub successful_steps: i32,
    /// Number of steps that failed
    pub failed_steps: i32,
    /// Additional custom metadata
    pub custom: HashMap<String, serde_json::Value>,
}

impl TaskRequestMessage {
    /// Create a new task request message
    pub fn new(
        namespace: String,
        task_name: String,
        task_version: String,
        input_data: serde_json::Value,
        requester: String,
    ) -> Self {
        Self {
            request_id: uuid::Uuid::new_v4().to_string(),
            namespace,
            task_name,
            task_version,
            input_data,
            metadata: TaskRequestMetadata {
                requested_at: Utc::now(),
                requester,
                priority: TaskPriority::default(),
                custom: HashMap::new(),
            },
        }
    }

    /// Set request priority
    pub fn with_priority(mut self, priority: TaskPriority) -> Self {
        self.metadata.priority = priority;
        self
    }

    /// Add custom metadata
    pub fn with_custom_metadata(mut self, key: String, value: serde_json::Value) -> Self {
        self.metadata.custom.insert(key, value);
        self
    }
}

impl BatchMessage {
    /// Create a new batch message
    pub fn new(
        batch_id: i64,
        task_id: i64,
        namespace: String,
        task_name: String,
        task_version: String,
        steps: Vec<BatchStep>,
    ) -> Self {
        Self {
            batch_id,
            task_id,
            namespace,
            task_name,
            task_version,
            steps,
            metadata: BatchMetadata {
                batch_created_at: Utc::now(),
                timeout_seconds: 300, // 5 minutes default
                retry_policy: BatchRetryPolicy::default(),
                max_retries: 3,
                retry_count: 0,
            },
        }
    }

    /// Set batch timeout
    pub fn with_timeout(mut self, timeout_seconds: i32) -> Self {
        self.metadata.timeout_seconds = timeout_seconds;
        self
    }

    /// Set retry policy
    pub fn with_retry_policy(mut self, policy: BatchRetryPolicy) -> Self {
        self.metadata.retry_policy = policy;
        self
    }
}

impl BatchStep {
    /// Create a new batch step
    pub fn new(
        step_id: i64,
        sequence: i32,
        step_name: String,
        step_payload: serde_json::Value,
    ) -> Self {
        Self {
            step_id,
            sequence,
            step_name,
            step_payload,
            step_metadata: HashMap::new(),
        }
    }

    /// Add step metadata
    pub fn with_metadata(mut self, key: String, value: serde_json::Value) -> Self {
        self.step_metadata.insert(key, value);
        self
    }
}

impl BatchResultMessage {
    /// Create a new batch result message
    pub fn new(
        batch_id: i64,
        task_id: i64,
        namespace: String,
        batch_status: BatchExecutionStatus,
        step_results: Vec<StepResult>,
        worker_id: String,
        execution_time_ms: i64,
    ) -> Self {
        let successful_steps = step_results
            .iter()
            .filter(|r| matches!(r.status, StepExecutionStatus::Success))
            .count() as i32;

        let failed_steps = step_results
            .iter()
            .filter(|r| matches!(r.status, StepExecutionStatus::Failed))
            .count() as i32;

        let now = Utc::now();

        Self {
            batch_id,
            task_id,
            namespace,
            batch_status,
            step_results,
            metadata: BatchResultMetadata {
                worker_id,
                worker_hostname: None,
                total_execution_time_ms: execution_time_ms,
                started_at: now - chrono::Duration::milliseconds(execution_time_ms),
                completed_at: now,
                successful_steps,
                failed_steps,
                custom: HashMap::new(),
            },
        }
    }

    /// Set worker hostname
    pub fn with_hostname(mut self, hostname: String) -> Self {
        self.metadata.worker_hostname = Some(hostname);
        self
    }

    /// Add custom metadata
    pub fn with_custom_metadata(mut self, key: String, value: serde_json::Value) -> Self {
        self.metadata.custom.insert(key, value);
        self
    }
}

impl StepResult {
    /// Create a successful step result
    pub fn success(step_id: i64, output: serde_json::Value, execution_duration_ms: i64) -> Self {
        Self {
            step_id,
            status: StepExecutionStatus::Success,
            output: Some(output),
            error: None,
            error_code: None,
            executed_at: Utc::now(),
            execution_duration_ms,
        }
    }

    /// Create a failed step result
    pub fn failed(
        step_id: i64,
        error: String,
        error_code: Option<String>,
        execution_duration_ms: i64,
    ) -> Self {
        Self {
            step_id,
            status: StepExecutionStatus::Failed,
            output: None,
            error: Some(error),
            error_code,
            executed_at: Utc::now(),
            execution_duration_ms,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_task_request_message_creation() {
        let msg = TaskRequestMessage::new(
            "fulfillment".to_string(),
            "process_order".to_string(),
            "1.0.0".to_string(),
            json!({"order_id": 12345}),
            "api_gateway".to_string(),
        )
        .with_priority(TaskPriority::High);

        assert_eq!(msg.namespace, "fulfillment");
        assert_eq!(msg.task_name, "process_order");
        assert_eq!(msg.task_version, "1.0.0");
        assert!(matches!(msg.metadata.priority, TaskPriority::High));
        assert_eq!(msg.metadata.requester, "api_gateway");
    }

    #[test]
    fn test_batch_message_creation() {
        let steps = vec![
            BatchStep::new(
                1,
                1,
                "validate_order".to_string(),
                json!({"order_id": 12345}),
            ),
            BatchStep::new(
                2,
                2,
                "check_inventory".to_string(),
                json!({"product_id": 67890}),
            ),
        ];

        let batch = BatchMessage::new(
            100,
            12345,
            "fulfillment".to_string(),
            "process_order".to_string(),
            "1.0.0".to_string(),
            steps,
        )
        .with_timeout(600);

        assert_eq!(batch.batch_id, 100);
        assert_eq!(batch.task_id, 12345);
        assert_eq!(batch.namespace, "fulfillment");
        assert_eq!(batch.steps.len(), 2);
        assert_eq!(batch.metadata.timeout_seconds, 600);
    }

    #[test]
    fn test_batch_result_message_creation() {
        let step_results = vec![
            StepResult::success(1, json!({"status": "validated"}), 100),
            StepResult::failed(
                2,
                "Insufficient inventory".to_string(),
                Some("INVENTORY_ERROR".to_string()),
                50,
            ),
        ];

        let result = BatchResultMessage::new(
            100,
            12345,
            "fulfillment".to_string(),
            BatchExecutionStatus::PartialSuccess,
            step_results,
            "worker-01".to_string(),
            150,
        );

        assert_eq!(result.batch_id, 100);
        assert_eq!(result.task_id, 12345);
        assert!(matches!(
            result.batch_status,
            BatchExecutionStatus::PartialSuccess
        ));
        assert_eq!(result.metadata.successful_steps, 1);
        assert_eq!(result.metadata.failed_steps, 1);
        assert_eq!(result.metadata.worker_id, "worker-01");
    }

    #[test]
    fn test_message_serialization() {
        let msg = TaskRequestMessage::new(
            "test".to_string(),
            "test_task".to_string(),
            "1.0.0".to_string(),
            json!({"data": "test"}),
            "test_requester".to_string(),
        );

        let serialized = serde_json::to_string(&msg).expect("Should serialize");
        let deserialized: TaskRequestMessage =
            serde_json::from_str(&serialized).expect("Should deserialize");

        assert_eq!(msg.namespace, deserialized.namespace);
        assert_eq!(msg.task_name, deserialized.task_name);
        assert_eq!(msg.request_id, deserialized.request_id);
    }
}
