//! # Execution Types for pgmq Architecture
//!
//! Message types for step execution requests and results in the pgmq-based orchestration system.
//! These types are used for communication between Rust orchestration and language workers.
//!
//! ## Key Design Principle
//!
//! These types are aligned with Ruby StepHandlerCallResult structure to ensure consistent
//! data persistence to `tasker_workflow_steps.results`. All results follow the pattern:
//! `{ success: bool, result: Any, metadata: OrchestrationMetadata }`
//!
//! Even failures include result and metadata attributes for API-level backoff evaluation.

use crate::messaging::message::OrchestrationMetadata;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use uuid::Uuid;

/// Request message sent from Rust orchestrator to language handlers
#[derive(Serialize, Debug, Clone)]
pub struct StepBatchRequest {
    pub batch_id: String,
    pub protocol_version: String,
    pub steps: Vec<StepExecutionRequest>,
}

/// Individual step execution request within a batch
#[derive(Serialize, Debug, Clone)]
pub struct StepExecutionRequest {
    pub step_uuid: Uuid,
    pub task_uuid: Uuid,
    pub step_name: String,
    pub handler_class: String,
    pub handler_config: HashMap<String, serde_json::Value>,
    pub task_context: serde_json::Value,
    pub previous_results: HashMap<String, StepExecutionResult>,
    pub metadata: StepRequestMetadata,
}

/// Metadata for step execution request
#[derive(Serialize, Debug, Clone)]
pub struct StepRequestMetadata {
    pub attempt: i32,
    pub retry_limit: i32,
    pub timeout_ms: i64,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Legacy response message for backward compatibility
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct StepBatchResponse {
    pub batch_id: String,
    pub protocol_version: String,
    pub results: Vec<StepExecutionResult>,
}

/// Individual step execution result within a batch response
///
/// This structure aligns with Ruby StepHandlerCallResult to ensure consistent
/// data serialization to `tasker_workflow_steps.results`.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct StepExecutionResult {
    pub step_uuid: Uuid,
    /// Always present - indicates overall success or failure of step execution
    pub success: bool,
    /// Always present - the actual result data (may be empty object for failures)
    /// This matches Ruby's expectation that even failures have a result field
    pub result: Value,
    /// Always present - comprehensive metadata for observability and backoff evaluation
    pub metadata: StepExecutionMetadata,
    /// Legacy status field for backward compatibility - derived from success field
    pub status: String, // "completed", "failed", "error"
    /// Error details when success=false - provides detailed error information
    pub error: Option<StepExecutionError>,
    /// Orchestration metadata for workflow coordination
    pub orchestration_metadata: Option<OrchestrationMetadata>,
}

/// Error information for failed step execution
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct StepExecutionError {
    pub message: String,
    pub error_type: Option<String>,
    pub backtrace: Option<Vec<String>>,
    pub retryable: bool,
}

/// Comprehensive metadata for step execution results
///
/// This structure provides rich metadata for observability, backoff evaluation,
/// and operational insights. Aligns with Ruby StepHandlerCallResult metadata expectations.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct StepExecutionMetadata {
    /// Execution time in milliseconds for performance monitoring
    pub execution_time_ms: i64,
    /// Handler version for compatibility tracking
    pub handler_version: Option<String>,
    /// Whether this operation should be retried on failure
    pub retryable: bool,
    /// When the step execution completed
    pub completed_at: chrono::DateTime<chrono::Utc>,
    /// Worker identification for distributed system observability
    pub worker_id: Option<String>,
    /// Worker hostname for operational debugging
    pub worker_hostname: Option<String>,
    /// When step execution began
    pub started_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Additional custom metadata for specific step types
    pub custom: HashMap<String, Value>,
    /// Error code for categorization (when success=false)
    pub error_code: Option<String>,
    /// Error type classification (when success=false)
    pub error_type: Option<String>,
    /// Context data for API-level backoff evaluation
    pub context: HashMap<String, Value>,
}

impl StepBatchRequest {
    pub fn new(batch_id: String, steps: Vec<StepExecutionRequest>) -> Self {
        Self {
            batch_id,
            protocol_version: "1.0".to_string(),
            steps,
        }
    }
}

impl StepBatchResponse {
    pub fn new(batch_id: String, results: Vec<StepExecutionResult>) -> Self {
        Self {
            batch_id,
            protocol_version: "1.0".to_string(),
            results,
        }
    }
}

impl StepExecutionRequest {
    /// Constructor with all required fields - clippy allows many args for data constructors
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        step_uuid: Uuid,
        task_uuid: Uuid,
        step_name: String,
        handler_class: String,
        handler_config: HashMap<String, serde_json::Value>,
        task_context: serde_json::Value,
        previous_results: HashMap<String, StepExecutionResult>,
        retry_limit: i32,
        timeout_ms: i64,
    ) -> Self {
        Self {
            step_uuid,
            task_uuid,
            step_name,
            handler_class,
            handler_config,
            task_context,
            previous_results,
            metadata: StepRequestMetadata {
                attempt: 0,
                retry_limit,
                timeout_ms,
                created_at: chrono::Utc::now(),
            },
        }
    }
}

impl StepExecutionResult {
    /// Create a successful step execution result
    ///
    /// Follows Ruby StepHandlerCallResult.success pattern with comprehensive metadata
    pub fn success(
        step_uuid: Uuid,
        result: Value,
        execution_time_ms: i64,
        custom_metadata: Option<HashMap<String, Value>>,
    ) -> Self {
        Self {
            step_uuid,
            success: true,
            result,
            status: "completed".to_string(),
            error: None,
            metadata: StepExecutionMetadata {
                execution_time_ms,
                handler_version: None,
                retryable: false, // Success doesn't need retry
                completed_at: chrono::Utc::now(),
                started_at: Some(
                    chrono::Utc::now() - chrono::Duration::milliseconds(execution_time_ms),
                ),
                worker_id: None,
                worker_hostname: None,
                custom: custom_metadata.unwrap_or_default(),
                error_code: None,
                error_type: None,
                context: HashMap::new(),
            },
            orchestration_metadata: Some(OrchestrationMetadata::default()),
        }
    }

    /// Create a failed step execution result
    ///
    /// Follows Ruby StepHandlerCallResult.error pattern - includes result field for consistency
    pub fn failure(
        step_uuid: Uuid,
        error_message: String,
        error_code: Option<String>,
        error_type: Option<String>,
        retryable: bool,
        execution_time_ms: i64,
        context: Option<HashMap<String, Value>>,
    ) -> Self {
        Self {
            step_uuid,
            success: false,
            result: serde_json::json!({}), // Empty object for failures, as per Ruby pattern
            status: "failed".to_string(),
            error: Some(StepExecutionError {
                message: error_message.clone(),
                error_type: error_type.clone(),
                backtrace: None,
                retryable,
            }),
            metadata: StepExecutionMetadata {
                execution_time_ms,
                handler_version: None,
                retryable,
                completed_at: chrono::Utc::now(),
                started_at: Some(
                    chrono::Utc::now() - chrono::Duration::milliseconds(execution_time_ms),
                ),
                worker_id: None,
                worker_hostname: None,
                custom: HashMap::new(),
                error_code,
                error_type,
                context: context.unwrap_or_default(),
            },
            orchestration_metadata: Some(OrchestrationMetadata::default()),
        }
    }

    /// Create an error step execution result (system-level errors)
    ///
    /// Distinguishes between business logic failures and system errors
    pub fn error(
        step_uuid: Uuid,
        error_message: String,
        error_type: Option<String>,
        backtrace: Option<Vec<String>>,
        execution_time_ms: i64,
        context: Option<HashMap<String, Value>>,
    ) -> Self {
        Self {
            step_uuid,
            success: false,
            result: serde_json::json!({}), // Empty object for errors
            status: "error".to_string(),
            error: Some(StepExecutionError {
                message: error_message.clone(),
                error_type: error_type.clone(),
                backtrace,
                retryable: false, // System errors typically aren't retryable
            }),
            metadata: StepExecutionMetadata {
                execution_time_ms,
                handler_version: None,
                retryable: false,
                completed_at: chrono::Utc::now(),
                started_at: Some(
                    chrono::Utc::now() - chrono::Duration::milliseconds(execution_time_ms),
                ),
                worker_id: None,
                worker_hostname: None,
                custom: HashMap::new(),
                error_code: None,
                error_type,
                context: context.unwrap_or_default(),
            },
            orchestration_metadata: Some(OrchestrationMetadata::default()),
        }
    }

    /// Check if this result represents a successful execution
    pub fn is_success(&self) -> bool {
        self.success
    }

    /// Check if this result should be retried
    pub fn is_retryable(&self) -> bool {
        self.metadata.retryable
    }

    /// Get the result as the expected structure for persistence
    ///
    /// Returns the structure that will be persisted to tasker_workflow_steps.results:
    /// { success: bool, result: Any, metadata: OrchestrationMetadata }
    pub fn to_persistence_format(&self) -> Value {
        serde_json::json!({
            "success": self.success,
            "result": self.result,
            "metadata": self.metadata
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_step_execution_request_creation() {
        let task_uuid = Uuid::now_v7();
        let step_uuid = Uuid::now_v7();
        let request = StepExecutionRequest::new(
            step_uuid,
            task_uuid,
            "validate_order".to_string(),
            "OrderValidator".to_string(),
            HashMap::new(),
            serde_json::json!({}),
            HashMap::new(),
            3,
            30000,
        );

        assert_eq!(request.step_uuid, step_uuid);
        assert_eq!(request.task_uuid, task_uuid);
        assert_eq!(request.step_name, "validate_order");
        assert_eq!(request.metadata.retry_limit, 3);
        assert_eq!(request.metadata.timeout_ms, 30000);
    }

    #[test]
    fn test_step_execution_result_success() {
        let step_uuid = Uuid::now_v7();
        let result = StepExecutionResult::success(
            step_uuid,
            serde_json::json!({"status": "valid"}),
            1500,
            None,
        );

        assert_eq!(result.step_uuid, step_uuid);
        assert_eq!(result.status, "completed");
        assert!(result.success);
        assert_eq!(result.result, serde_json::json!({"status": "valid"}));
        assert!(result.error.is_none());
        assert_eq!(result.metadata.execution_time_ms, 1500);
        assert!(!result.metadata.retryable); // Success shouldn't need retry
    }

    #[test]
    fn test_step_execution_result_failure() {
        let step_uuid = Uuid::now_v7();
        let result = StepExecutionResult::failure(
            step_uuid,
            "Validation failed".to_string(),
            Some("VALIDATION_ERROR".to_string()),
            Some("ValidationError".to_string()),
            true,
            800,
            None,
        );

        assert_eq!(result.step_uuid, step_uuid);
        assert_eq!(result.status, "failed");
        assert!(!result.success);
        assert_eq!(result.result, serde_json::json!({})); // Empty object for failures
        assert!(result.error.is_some());
        assert!(result.is_retryable());

        let error = result.error.unwrap();
        assert_eq!(error.message, "Validation failed");
        assert!(error.retryable);
    }

    #[test]
    fn test_step_execution_result_persistence_format() {
        let step_uuid = Uuid::now_v7();
        let result = StepExecutionResult::success(
            step_uuid,
            serde_json::json!({"order_id": 12345, "total": 99.99}),
            750,
            Some(HashMap::from([(
                "custom_field".to_string(),
                serde_json::json!("custom_value"),
            )])),
        );

        let persistence_format = result.to_persistence_format();

        // Verify the expected structure for tasker_workflow_steps.results
        assert_eq!(persistence_format["success"], true);
        assert_eq!(persistence_format["result"]["order_id"], 12345);
        assert_eq!(persistence_format["result"]["total"], 99.99);
        assert!(persistence_format["metadata"].is_object());
        assert_eq!(persistence_format["metadata"]["execution_time_ms"], 750);
        assert_eq!(
            persistence_format["metadata"]["custom"]["custom_field"],
            "custom_value"
        );
    }

    #[test]
    fn test_step_execution_result_failure_persistence_format() {
        let step_uuid = Uuid::now_v7();
        let context = HashMap::from([("retry_count".to_string(), serde_json::json!(2))]);
        let result = StepExecutionResult::failure(
            step_uuid,
            "Database connection failed".to_string(),
            Some("DB_CONNECTION_ERROR".to_string()),
            Some("RetryableError".to_string()),
            true,
            500,
            Some(context),
        );

        let persistence_format = result.to_persistence_format();

        // Verify failure structure still includes result field (empty object)
        assert_eq!(persistence_format["success"], false);
        assert_eq!(persistence_format["result"], serde_json::json!({}));
        assert!(persistence_format["metadata"].is_object());
        assert_eq!(persistence_format["metadata"]["retryable"], true);
        assert_eq!(
            persistence_format["metadata"]["error_code"],
            "DB_CONNECTION_ERROR"
        );
        assert_eq!(persistence_format["metadata"]["context"]["retry_count"], 2);
    }
}
