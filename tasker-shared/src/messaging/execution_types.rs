//! # Execution Types for pgmq Architecture
//!
//! Message types for step execution requests and results in the pgmq-based orchestration system.
//! These types are used for communication between Rust orchestration and language workers.

use serde::{Deserialize, Serialize};
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
    pub previous_results: HashMap<String, serde_json::Value>,
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
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct StepExecutionResult {
    pub step_uuid: Uuid,
    pub status: String, // "completed", "failed", "error"
    pub output: Option<serde_json::Value>,
    pub error: Option<StepExecutionError>,
    pub metadata: StepResultMetadata,
}

/// Error information for failed step execution
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct StepExecutionError {
    pub message: String,
    pub error_type: Option<String>,
    pub backtrace: Option<Vec<String>>,
    pub retryable: bool,
}

/// Metadata for step execution result
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct StepResultMetadata {
    pub execution_time_ms: i64,
    pub handler_version: Option<String>,
    pub retryable: bool,
    pub completed_at: chrono::DateTime<chrono::Utc>,
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
        previous_results: HashMap<String, serde_json::Value>,
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
    pub fn success(step_uuid: Uuid, output: serde_json::Value, execution_time_ms: i64) -> Self {
        Self {
            step_uuid,
            status: "completed".to_string(),
            output: Some(output),
            error: None,
            metadata: StepResultMetadata {
                execution_time_ms,
                handler_version: None,
                retryable: false, // Success doesn't need retry
                completed_at: chrono::Utc::now(),
            },
        }
    }

    pub fn failure(
        step_uuid: Uuid,
        error_message: String,
        retryable: bool,
        execution_time_ms: i64,
    ) -> Self {
        Self {
            step_uuid,
            status: "failed".to_string(),
            output: None,
            error: Some(StepExecutionError {
                message: error_message,
                error_type: None,
                backtrace: None,
                retryable,
            }),
            metadata: StepResultMetadata {
                execution_time_ms,
                handler_version: None,
                retryable,
                completed_at: chrono::Utc::now(),
            },
        }
    }

    pub fn error(
        step_uuid: Uuid,
        error_message: String,
        error_type: Option<String>,
        backtrace: Option<Vec<String>>,
        execution_time_ms: i64,
    ) -> Self {
        Self {
            step_uuid,
            status: "error".to_string(),
            output: None,
            error: Some(StepExecutionError {
                message: error_message,
                error_type,
                backtrace,
                retryable: false, // Errors typically aren't retryable
            }),
            metadata: StepResultMetadata {
                execution_time_ms,
                handler_version: None,
                retryable: false,
                completed_at: chrono::Utc::now(),
            },
        }
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
        let result =
            StepExecutionResult::success(step_uuid, serde_json::json!({"status": "valid"}), 1500);

        assert_eq!(result.step_uuid, step_uuid);
        assert_eq!(result.status, "completed");
        assert!(result.output.is_some());
        assert!(result.error.is_none());
        assert_eq!(result.metadata.execution_time_ms, 1500);
    }

    #[test]
    fn test_step_execution_result_failure() {
        let step_uuid = Uuid::now_v7();
        let result =
            StepExecutionResult::failure(step_uuid, "Validation failed".to_string(), true, 800);

        assert_eq!(result.step_uuid, step_uuid);
        assert_eq!(result.status, "failed");
        assert!(result.output.is_none());
        assert!(result.error.is_some());

        let error = result.error.unwrap();
        assert_eq!(error.message, "Validation failed");
        assert!(error.retryable);
    }
}
