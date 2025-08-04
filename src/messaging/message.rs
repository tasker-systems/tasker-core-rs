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
    /// NEW: Execution context for (task, sequence, step) handler pattern
    pub execution_context: StepExecutionContext,
    /// Message metadata
    pub metadata: StepMessageMetadata,
}

/// Parameters for creating a StepMessage to avoid too many arguments
#[derive(Debug, Clone)]
pub struct StepMessageParams {
    pub step_id: i64,
    pub task_id: i64,
    pub namespace: String,
    pub task_name: String,
    pub task_version: String,
    pub step_name: String,
    pub step_payload: serde_json::Value,
    pub execution_context: StepExecutionContext,
}

/// Execution context that provides (task, sequence, step) to handlers
/// This enables the immediate delete pattern by ensuring all necessary data is in the message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepExecutionContext {
    /// Full task object for handler execution
    pub task: serde_json::Value,
    /// Dependency chain results: results from all steps that this step depends on
    /// Structure: [{"step_name": "validate_order", "results": {...}}, ...]
    pub sequence: Vec<StepDependencyResult>,
    /// Full step object for handler execution  
    pub step: serde_json::Value,
    /// Additional context for step execution
    pub additional_context: HashMap<String, serde_json::Value>,
}

/// Result data from a dependency step that this step depends on
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepDependencyResult {
    /// Name of the dependency step
    pub step_name: String,
    /// Step ID of the dependency
    pub step_id: i64,
    /// Named step ID for the dependency
    pub named_step_id: i32,
    /// Results data from the dependency step execution
    pub results: Option<serde_json::Value>,
    /// When the dependency step was processed
    pub processed_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Additional metadata about the dependency
    pub metadata: HashMap<String, serde_json::Value>,
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
    /// Create a new step message with execution context
    pub fn new(
        step_id: i64,
        task_id: i64,
        namespace: String,
        task_name: String,
        task_version: String,
        step_name: String,
        step_payload: serde_json::Value,
        execution_context: StepExecutionContext,
    ) -> Self {
        Self {
            step_id,
            task_id,
            namespace,
            task_name,
            task_version,
            step_name,
            step_payload,
            execution_context,
            metadata: StepMessageMetadata::default(),
        }
    }

    /// Create step message from parameters struct
    pub fn from_params(params: StepMessageParams) -> Self {
        Self {
            step_id: params.step_id,
            task_id: params.task_id,
            namespace: params.namespace,
            task_name: params.task_name,
            task_version: params.task_version,
            step_name: params.step_name,
            step_payload: params.step_payload,
            execution_context: params.execution_context,
            metadata: StepMessageMetadata::default(),
        }
    }

    /// Create step message from parameters with custom metadata
    pub fn from_params_with_metadata(
        params: StepMessageParams,
        metadata: StepMessageMetadata,
    ) -> Self {
        Self {
            step_id: params.step_id,
            task_id: params.task_id,
            namespace: params.namespace,
            task_name: params.task_name,
            task_version: params.task_version,
            step_name: params.step_name,
            step_payload: params.step_payload,
            execution_context: params.execution_context,
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
    /// NEW: Rich metadata for orchestration decisions (backoff, retry strategies)
    pub orchestration_metadata: Option<OrchestrationMetadata>,
}

/// Metadata from step execution that informs orchestration decisions
/// This enables intelligent backoff, retry strategies, and workflow coordination
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestrationMetadata {
    /// HTTP response headers from external API calls (e.g., Retry-After, Rate-Limit headers)
    pub headers: HashMap<String, String>,
    /// Error context that triggered this result (e.g., "Rate limited by payment gateway")
    pub error_context: Option<String>,
    /// Explicit backoff hint from handler execution
    pub backoff_hint: Option<BackoffHint>,
    /// Custom domain-specific metadata for orchestration decisions
    pub custom: HashMap<String, serde_json::Value>,
}

/// Backoff hint from handler to orchestration system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackoffHint {
    /// Type of backoff requested
    pub backoff_type: BackoffHintType,
    /// Suggested delay in seconds
    pub delay_seconds: u32,
    /// Additional context for the backoff decision
    pub context: Option<String>,
}

/// Types of backoff hints handlers can provide
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BackoffHintType {
    /// Server explicitly requested backoff (e.g., Retry-After header)
    ServerRequested,
    /// Rate limit detected, suggest exponential backoff
    RateLimit,
    /// Temporary service unavailable
    ServiceUnavailable,
    /// Custom backoff strategy
    Custom,
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

impl StepExecutionContext {
    /// Create a new execution context with dependency results
    pub fn new(
        task: serde_json::Value,
        sequence: Vec<StepDependencyResult>,
        step: serde_json::Value,
    ) -> Self {
        Self {
            task,
            sequence,
            step,
            additional_context: HashMap::new(),
        }
    }

    /// Create execution context with empty dependencies (for root steps)
    pub fn new_root_step(task: serde_json::Value, step: serde_json::Value) -> Self {
        Self {
            task,
            sequence: Vec::new(),
            step,
            additional_context: HashMap::new(),
        }
    }

    /// Add additional context
    pub fn with_context(mut self, key: String, value: serde_json::Value) -> Self {
        self.additional_context.insert(key, value);
        self
    }
}

impl StepDependencyResult {
    /// Create a new dependency result
    pub fn new(
        step_name: String,
        step_id: i64,
        named_step_id: i32,
        results: Option<serde_json::Value>,
        processed_at: Option<chrono::DateTime<chrono::Utc>>,
    ) -> Self {
        Self {
            step_name,
            step_id,
            named_step_id,
            results,
            processed_at,
            metadata: HashMap::new(),
        }
    }

    /// Add metadata to the dependency result
    pub fn with_metadata(mut self, key: String, value: serde_json::Value) -> Self {
        self.metadata.insert(key, value);
        self
    }
}

impl Default for OrchestrationMetadata {
    fn default() -> Self {
        Self::new()
    }
}

impl OrchestrationMetadata {
    /// Create new orchestration metadata
    pub fn new() -> Self {
        Self {
            headers: HashMap::new(),
            error_context: None,
            backoff_hint: None,
            custom: HashMap::new(),
        }
    }

    /// Add HTTP header
    pub fn with_header(mut self, key: String, value: String) -> Self {
        self.headers.insert(key, value);
        self
    }

    /// Add error context
    pub fn with_error_context(mut self, context: String) -> Self {
        self.error_context = Some(context);
        self
    }

    /// Add backoff hint
    pub fn with_backoff_hint(mut self, hint: BackoffHint) -> Self {
        self.backoff_hint = Some(hint);
        self
    }

    /// Add custom metadata
    pub fn with_custom(mut self, key: String, value: serde_json::Value) -> Self {
        self.custom.insert(key, value);
        self
    }
}

impl BackoffHint {
    /// Create server-requested backoff hint
    pub fn server_requested(delay_seconds: u32, context: Option<String>) -> Self {
        Self {
            backoff_type: BackoffHintType::ServerRequested,
            delay_seconds,
            context,
        }
    }

    /// Create rate limit backoff hint
    pub fn rate_limit(delay_seconds: u32, context: Option<String>) -> Self {
        Self {
            backoff_type: BackoffHintType::RateLimit,
            delay_seconds,
            context,
        }
    }
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
            orchestration_metadata: None,
        }
    }

    /// Create successful step result with orchestration metadata
    pub fn success_with_metadata(
        step_id: i64,
        task_id: i64,
        result_data: Option<serde_json::Value>,
        execution_time_ms: u64,
        orchestration_metadata: OrchestrationMetadata,
    ) -> Self {
        Self {
            step_id,
            task_id,
            status: StepExecutionStatus::Success,
            result_data,
            error: None,
            execution_time_ms,
            completed_at: chrono::Utc::now(),
            orchestration_metadata: Some(orchestration_metadata),
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
            orchestration_metadata: None,
        }
    }

    /// Create failed step result with orchestration metadata
    pub fn failed_with_metadata(
        step_id: i64,
        task_id: i64,
        error: StepExecutionError,
        execution_time_ms: u64,
        orchestration_metadata: OrchestrationMetadata,
    ) -> Self {
        Self {
            step_id,
            task_id,
            status: StepExecutionStatus::Failed,
            result_data: None,
            error: Some(error),
            execution_time_ms,
            completed_at: chrono::Utc::now(),
            orchestration_metadata: Some(orchestration_metadata),
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
            orchestration_metadata: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_step_message_creation() {
        let execution_context = StepExecutionContext::new_root_step(
            serde_json::json!({"task_id": 67890, "status": "pending"}),
            serde_json::json!({"step_id": 12345, "name": "validate_order"}),
        );

        let message = StepMessage::new(
            12345,
            67890,
            "fulfillment".to_string(),
            "process_order".to_string(),
            "1.0.0".to_string(),
            "validate_order".to_string(),
            serde_json::json!({"order_id": 1001}),
            execution_context,
        );

        assert_eq!(message.step_id, 12345);
        assert_eq!(message.task_id, 67890);
        assert_eq!(message.namespace, "fulfillment");
        assert_eq!(message.queue_name(), "fulfillment_queue");
        assert_eq!(message.metadata.retry_count, 0);
        assert_eq!(message.execution_context.sequence.len(), 0); // Root step has no dependencies
        assert!(!message.is_max_retries_exceeded());
        assert!(!message.is_expired());
    }

    #[test]
    fn test_step_message_json_serialization() {
        let execution_context = StepExecutionContext::new_root_step(
            serde_json::json!({"task_id": 67890, "status": "pending"}),
            serde_json::json!({"step_id": 12345, "name": "validate_order"}),
        );

        let message = StepMessage::new(
            12345,
            67890,
            "fulfillment".to_string(),
            "process_order".to_string(),
            "1.0.0".to_string(),
            "validate_order".to_string(),
            serde_json::json!({"order_id": 1001}),
            execution_context,
        );

        let json = message.to_json().unwrap();
        let deserialized = StepMessage::from_json(json).unwrap();

        assert_eq!(message.step_id, deserialized.step_id);
        assert_eq!(message.task_id, deserialized.task_id);
        assert_eq!(message.namespace, deserialized.namespace);
        assert_eq!(
            message.execution_context.sequence.len(),
            deserialized.execution_context.sequence.len()
        );
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
        let execution_context = StepExecutionContext::new_root_step(
            serde_json::json!({"task_id": 67890, "status": "pending"}),
            serde_json::json!({"step_id": 12345, "name": "validate_order"}),
        );

        let mut message = StepMessage::new(
            12345,
            67890,
            "fulfillment".to_string(),
            "process_order".to_string(),
            "1.0.0".to_string(),
            "validate_order".to_string(),
            serde_json::json!({"order_id": 1001}),
            execution_context,
        );

        assert!(!message.is_max_retries_exceeded());

        // Increment retries beyond max
        for _ in 0..4 {
            message.increment_retry();
        }

        assert!(message.is_max_retries_exceeded());
    }

    #[test]
    fn test_orchestration_metadata() {
        let metadata = OrchestrationMetadata::new()
            .with_header("retry-after".to_string(), "60".to_string())
            .with_error_context("Rate limited by payment gateway".to_string())
            .with_backoff_hint(BackoffHint::rate_limit(
                60,
                Some("API rate limit exceeded".to_string()),
            ))
            .with_custom("api_response_code".to_string(), serde_json::json!(429));

        assert_eq!(metadata.headers.get("retry-after"), Some(&"60".to_string()));
        assert_eq!(
            metadata.error_context,
            Some("Rate limited by payment gateway".to_string())
        );
        assert!(metadata.backoff_hint.is_some());
        assert_eq!(
            metadata.custom.get("api_response_code"),
            Some(&serde_json::json!(429))
        );
    }

    #[test]
    fn test_step_result_with_metadata() {
        let metadata =
            OrchestrationMetadata::new().with_header("retry-after".to_string(), "30".to_string());

        let result = StepResult::success_with_metadata(
            12345,
            67890,
            Some(serde_json::json!({"status": "validated"})),
            1500,
            metadata,
        );

        assert_eq!(result.step_id, 12345);
        assert_eq!(result.status, StepExecutionStatus::Success);
        assert!(result.orchestration_metadata.is_some());

        let meta = result.orchestration_metadata.unwrap();
        assert_eq!(meta.headers.get("retry-after"), Some(&"30".to_string()));
    }

    #[test]
    fn test_step_execution_context_with_dependencies() {
        // Create dependency results
        let dep1 = StepDependencyResult::new(
            "validate_order".to_string(),
            123,
            1,
            Some(serde_json::json!({"status": "validated", "order_id": 1001})),
            Some(chrono::Utc::now()),
        );

        let dep2 = StepDependencyResult::new(
            "check_inventory".to_string(),
            124,
            2,
            Some(serde_json::json!({"status": "available", "quantity": 5})),
            Some(chrono::Utc::now()),
        );

        let dependencies = vec![dep1, dep2];

        // Create execution context with dependencies
        let execution_context = StepExecutionContext::new(
            serde_json::json!({"task_id": 67890, "status": "pending"}),
            dependencies,
            serde_json::json!({"step_id": 12345, "name": "process_payment"}),
        );

        assert_eq!(execution_context.sequence.len(), 2);
        assert_eq!(execution_context.sequence[0].step_name, "validate_order");
        assert_eq!(execution_context.sequence[1].step_name, "check_inventory");
        assert_eq!(execution_context.sequence[0].step_id, 123);
        assert_eq!(execution_context.sequence[1].step_id, 124);
    }

    #[test]
    fn test_step_dependency_result_creation() {
        let dep_result = StepDependencyResult::new(
            "validate_order".to_string(),
            123,
            1,
            Some(serde_json::json!({"status": "validated"})),
            Some(chrono::Utc::now()),
        )
        .with_metadata("attempts".to_string(), serde_json::json!(2))
        .with_metadata("retryable".to_string(), serde_json::json!(true));

        assert_eq!(dep_result.step_name, "validate_order");
        assert_eq!(dep_result.step_id, 123);
        assert_eq!(dep_result.named_step_id, 1);
        assert!(dep_result.results.is_some());
        assert_eq!(
            dep_result.metadata.get("attempts"),
            Some(&serde_json::json!(2))
        );
        assert_eq!(
            dep_result.metadata.get("retryable"),
            Some(&serde_json::json!(true))
        );
    }
}
