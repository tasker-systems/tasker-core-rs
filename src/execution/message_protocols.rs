use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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
    pub step_id: i64,
    pub task_id: i64,
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

/// Unified result message supporting both partial and batch completion patterns
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "message_type")]
pub enum ResultMessage {
    /// Partial result sent by individual worker threads as steps complete
    PartialResult {
        batch_id: String,
        step_id: i64,
        status: String, // "completed", "failed", "in_progress"
        output: Option<serde_json::Value>,
        error: Option<StepExecutionError>,
        worker_id: String,
        sequence: u32,
        timestamp: chrono::DateTime<chrono::Utc>,
        execution_time_ms: i64,
    },
    /// Batch completion sent by orchestrator when all workers finish
    BatchCompletion {
        batch_id: String,
        protocol_version: String,
        total_steps: u32,
        completed_steps: u32,
        failed_steps: u32,
        in_progress_steps: u32,
        step_summaries: Vec<StepSummary>,
        completed_at: chrono::DateTime<chrono::Utc>,
    },
}

/// Summary of step status in batch completion message
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct StepSummary {
    pub step_id: i64,
    pub final_status: String,
    pub execution_time_ms: Option<i64>,
    pub worker_id: Option<String>,
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
    pub step_id: i64,
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

impl ResultMessage {
    /// Create a partial result message for immediate step completion
    pub fn partial_result(
        batch_id: String,
        step_id: i64,
        status: String,
        output: Option<serde_json::Value>,
        error: Option<StepExecutionError>,
        worker_id: String,
        sequence: u32,
        execution_time_ms: i64,
    ) -> Self {
        Self::PartialResult {
            batch_id,
            step_id,
            status,
            output,
            error,
            worker_id,
            sequence,
            timestamp: chrono::Utc::now(),
            execution_time_ms,
        }
    }

    /// Create a batch completion message when all workers finish
    pub fn batch_completion(
        batch_id: String,
        total_steps: u32,
        completed_steps: u32,
        failed_steps: u32,
        in_progress_steps: u32,
        step_summaries: Vec<StepSummary>,
    ) -> Self {
        Self::BatchCompletion {
            batch_id,
            protocol_version: "2.0".to_string(), // New protocol version for dual pattern
            total_steps,
            completed_steps,
            failed_steps,
            in_progress_steps,
            step_summaries,
            completed_at: chrono::Utc::now(),
        }
    }
}

impl StepSummary {
    pub fn new(
        step_id: i64,
        final_status: String,
        execution_time_ms: Option<i64>,
        worker_id: Option<String>,
    ) -> Self {
        Self {
            step_id,
            final_status,
            execution_time_ms,
            worker_id,
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
    pub fn new(
        step_id: i64,
        task_id: i64,
        step_name: String,
        handler_class: String,
        handler_config: HashMap<String, serde_json::Value>,
        task_context: serde_json::Value,
        previous_results: HashMap<String, serde_json::Value>,
        retry_limit: i32,
        timeout_ms: i64,
    ) -> Self {
        Self {
            step_id,
            task_id,
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
    pub fn success(
        step_id: i64,
        output: serde_json::Value,
        execution_time_ms: i64,
    ) -> Self {
        Self {
            step_id,
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
        step_id: i64,
        error_message: String,
        retryable: bool,
        execution_time_ms: i64,
    ) -> Self {
        Self {
            step_id,
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
        step_id: i64,
        error_message: String,
        error_type: Option<String>,
        backtrace: Option<Vec<String>>,
        execution_time_ms: i64,
    ) -> Self {
        Self {
            step_id,
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
    use serde_json;

    #[test]
    fn test_partial_result_serialization() {
        let partial_result = ResultMessage::partial_result(
            "batch_123".to_string(),
            456,
            "completed".to_string(),
            Some(serde_json::json!({"result": "success"})),
            None,
            "worker_1".to_string(),
            1,
            1500,
        );

        let json = serde_json::to_string(&partial_result).expect("Should serialize");
        assert!(json.contains("PartialResult"));
        assert!(json.contains("batch_123"));
        assert!(json.contains("worker_1"));

        let deserialized: ResultMessage = serde_json::from_str(&json).expect("Should deserialize");
        match deserialized {
            ResultMessage::PartialResult { batch_id, step_id, worker_id, .. } => {
                assert_eq!(batch_id, "batch_123");
                assert_eq!(step_id, 456);
                assert_eq!(worker_id, "worker_1");
            }
            _ => panic!("Should deserialize as PartialResult"),
        }
    }

    #[test]
    fn test_batch_completion_serialization() {
        let step_summaries = vec![
            StepSummary::new(456, "completed".to_string(), Some(1500), Some("worker_1".to_string())),
            StepSummary::new(457, "failed".to_string(), Some(800), Some("worker_2".to_string())),
        ];

        let batch_completion = ResultMessage::batch_completion(
            "batch_123".to_string(),
            2,
            1,
            1,
            0,
            step_summaries,
        );

        let json = serde_json::to_string(&batch_completion).expect("Should serialize");
        assert!(json.contains("BatchCompletion"));
        assert!(json.contains("batch_123"));
        assert!(json.contains("2.0")); // New protocol version

        let deserialized: ResultMessage = serde_json::from_str(&json).expect("Should deserialize");
        match deserialized {
            ResultMessage::BatchCompletion { batch_id, total_steps, completed_steps, failed_steps, .. } => {
                assert_eq!(batch_id, "batch_123");
                assert_eq!(total_steps, 2);
                assert_eq!(completed_steps, 1);
                assert_eq!(failed_steps, 1);
            }
            _ => panic!("Should deserialize as BatchCompletion"),
        }
    }

    #[test]
    fn test_partial_result_with_error() {
        let error = StepExecutionError {
            message: "Connection timeout".to_string(),
            error_type: Some("NetworkError".to_string()),
            backtrace: Some(vec!["line1".to_string(), "line2".to_string()]),
            retryable: true,
        };

        let partial_result = ResultMessage::partial_result(
            "batch_456".to_string(),
            789,
            "failed".to_string(),
            None,
            Some(error),
            "worker_3".to_string(),
            2,
            500,
        );

        let json = serde_json::to_string(&partial_result).expect("Should serialize");
        let deserialized: ResultMessage = serde_json::from_str(&json).expect("Should deserialize");

        match deserialized {
            ResultMessage::PartialResult { status, error, .. } => {
                assert_eq!(status, "failed");
                assert!(error.is_some());
                let err = error.unwrap();
                assert_eq!(err.message, "Connection timeout");
                assert_eq!(err.error_type, Some("NetworkError".to_string()));
                assert_eq!(err.retryable, true);
            }
            _ => panic!("Should deserialize as PartialResult"),
        }
    }
}