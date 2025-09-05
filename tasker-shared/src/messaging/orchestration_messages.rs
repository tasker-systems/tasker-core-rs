//! # Orchestration Message Types
//!
//! Message structures for the complete pgmq-based orchestration workflow.
//! These messages flow through the orchestration queues to coordinate
//! task processing from request to completion.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use uuid::Uuid;

use crate::messaging::message::OrchestrationMetadata;
use crate::models::core::task_request::TaskRequest;

/// Message for task requests sent to orchestration_task_requests queue
///
/// This message embeds the core TaskRequest domain model to ensure consistency
/// and eliminate lossy conversions between messaging and domain representations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRequestMessage {
    /// Unique identifier for this request
    pub request_id: String,
    /// The core task request with all domain-specific fields
    pub task_request: TaskRequest,
    /// Messaging-specific metadata (creation time, routing info, etc.)
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
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
pub enum TaskPriority {
    Low,
    #[default]
    Normal,
    High,
    Urgent,
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
    pub task_uuid: Uuid,
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
    pub step_uuid: Uuid,
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
    pub step_uuid: Uuid,
    /// Task ID for context
    pub task_uuid: Uuid,
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

impl fmt::Display for StepResultMessage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "StepResultMessage {{ step_uuid: {}, task_uuid: {}, namespace: {}, status: {}, results: {:?}, error: {:?}, execution_time_ms: {}, orchestration_metadata: {:?}, metadata: {:?} }}",
            self.step_uuid,
            self.task_uuid,
            self.namespace,
            self.status,
            self.results,
            self.error,
            self.execution_time_ms,
            self.orchestration_metadata,
            self.metadata
        )
    }
}

/// DEPRECATED: Message for batch results - kept for backward compatibility
/// Use StepResultMessage for new individual step processing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchResultMessage {
    /// Batch ID that was processed
    pub batch_id: i64,
    /// Task ID for aggregation
    pub task_uuid: Uuid,
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
    pub step_uuid: Uuid,
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

impl fmt::Display for StepExecutionStatus {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            StepExecutionStatus::Success => write!(f, "Success"),
            StepExecutionStatus::Failed => write!(f, "Failed"),
            StepExecutionStatus::Skipped => write!(f, "Skipped"),
            StepExecutionStatus::Timeout => write!(f, "Timeout"),
            StepExecutionStatus::Cancelled => write!(f, "Cancelled"),
        }
    }
}

impl From<StepExecutionStatus> for String {
    fn from(status: StepExecutionStatus) -> Self {
        status.to_string().to_lowercase()
    }
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
    /// Create a new task request message from a TaskRequest
    pub fn new(task_request: TaskRequest, requester: String) -> Self {
        Self {
            request_id: uuid::Uuid::new_v4().to_string(),
            task_request,
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

    /// Get the embedded task request
    pub fn task_request(&self) -> &TaskRequest {
        &self.task_request
    }

    /// Get the embedded task request (mutable)
    pub fn task_request_mut(&mut self) -> &mut TaskRequest {
        &mut self.task_request
    }

    /// Get the request ID
    pub fn request_id(&self) -> &str {
        &self.request_id
    }
}

impl BatchMessage {
    /// Create a new batch message
    pub fn new(
        batch_id: i64,
        task_uuid: Uuid,
        namespace: String,
        task_name: String,
        task_version: String,
        steps: Vec<BatchStep>,
    ) -> Self {
        Self {
            batch_id,
            task_uuid,
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
        step_uuid: Uuid,
        sequence: i32,
        step_name: String,
        step_payload: serde_json::Value,
    ) -> Self {
        Self {
            step_uuid,
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
        task_uuid: Uuid,
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
            task_uuid,
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
    pub fn success(step_uuid: Uuid, output: serde_json::Value, execution_duration_ms: i64) -> Self {
        Self {
            step_uuid,
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
        step_uuid: Uuid,
        error: String,
        error_code: Option<String>,
        execution_duration_ms: i64,
    ) -> Self {
        Self {
            step_uuid,
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
        use crate::models::core::task_request::TaskRequest;

        let task_request = TaskRequest::new("process_order".to_string(), "fulfillment".to_string())
            .with_version("1.0.0".to_string())
            .with_context(json!({"order_id": 12345}))
            .with_initiator("api_gateway".to_string())
            .with_source_system("test".to_string())
            .with_reason("Test creation".to_string());

        let msg = TaskRequestMessage::new(task_request, "api_gateway".to_string())
            .with_priority(TaskPriority::High);

        assert_eq!(msg.task_request.namespace, "fulfillment");
        assert_eq!(msg.task_request.name, "process_order");
        assert_eq!(msg.task_request.version, "1.0.0");
        assert!(matches!(msg.metadata.priority, TaskPriority::High));
        assert_eq!(msg.metadata.requester, "api_gateway");
    }

    #[test]
    fn test_batch_message_creation() {
        let step_uuid = Uuid::now_v7();
        let step_two_uuid = Uuid::now_v7();
        let task_uuid = Uuid::now_v7();
        let steps = vec![
            BatchStep::new(
                step_uuid,
                1,
                "validate_order".to_string(),
                json!({"order_id": 12345}),
            ),
            BatchStep::new(
                step_two_uuid,
                2,
                "check_inventory".to_string(),
                json!({"product_id": 67890}),
            ),
        ];

        let batch = BatchMessage::new(
            100,
            task_uuid,
            "fulfillment".to_string(),
            "process_order".to_string(),
            "1.0.0".to_string(),
            steps,
        )
        .with_timeout(600);

        assert_eq!(batch.batch_id, 100);
        assert_eq!(batch.task_uuid, task_uuid);
        assert_eq!(batch.namespace, "fulfillment");
        assert_eq!(batch.steps.len(), 2);
        assert_eq!(batch.metadata.timeout_seconds, 600);
    }

    #[test]
    fn test_batch_result_message_creation() {
        let step_uuid = Uuid::now_v7();
        let step_two_uuid = Uuid::now_v7();
        let task_uuid = Uuid::now_v7();
        let step_results = vec![
            StepResult::success(step_uuid, json!({"status": "validated"}), 100),
            StepResult::failed(
                step_two_uuid,
                "Insufficient inventory".to_string(),
                Some("INVENTORY_ERROR".to_string()),
                50,
            ),
        ];

        let result = BatchResultMessage::new(
            100,
            task_uuid,
            "fulfillment".to_string(),
            BatchExecutionStatus::PartialSuccess,
            step_results,
            "worker-01".to_string(),
            150,
        );

        assert_eq!(result.batch_id, 100);
        assert_eq!(result.task_uuid, task_uuid);
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
        use crate::models::core::task_request::TaskRequest;

        let task_request = TaskRequest::new("test_task".to_string(), "test".to_string())
            .with_version("1.0.0".to_string())
            .with_context(json!({"data": "test"}))
            .with_initiator("test_requester".to_string())
            .with_source_system("test".to_string())
            .with_reason("Test serialization".to_string());

        let msg = TaskRequestMessage::new(task_request, "test_requester".to_string());

        let serialized = serde_json::to_string(&msg).expect("Should serialize");
        let deserialized: TaskRequestMessage =
            serde_json::from_str(&serialized).expect("Should deserialize");

        assert_eq!(
            msg.task_request.namespace,
            deserialized.task_request.namespace
        );
        assert_eq!(msg.task_request.name, deserialized.task_request.name);
        assert_eq!(msg.request_id, deserialized.request_id);
    }
}
