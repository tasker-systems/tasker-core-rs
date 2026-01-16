//! # Message Structures for pgmq Queues
//!
//! Defines message formats for queue-based workflow orchestration.
//! These replace the Command structures from the TCP system.
//!
//! ## TAS-133 Consolidation
//!
//! As of TAS-133, `StepMessage` uses UUID-based references only.
//! Workers query the database using these UUIDs to get full context.
//! The old embedded-context `StepMessage` has been removed.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

// Import canonical types from their unified locations
use super::execution_types::StepExecutionError;
use super::orchestration_messages::StepExecutionStatus;

/// Step message with UUID references (workers query DB for context)
///
/// This dramatically reduces message size and complexity by using UUIDs to reference
/// database records instead of embedding full execution context. Workers use
/// the UUIDs to fetch models directly from the shared database.
///
/// ## Benefits
///
/// - 80%+ message size reduction (3 UUIDs vs complex nested JSON)
/// - Eliminates type conversion issues (workers get real database models)
/// - Prevents stale queue messages (UUIDs are globally unique)
/// - Database as single source of truth (no data duplication)
/// - TAS-29: Correlation ID available immediately without database query
///
/// ## TAS-133 Note
///
/// This is the UUID-based step message (previously `SimpleStepMessage`).
/// The old embedded-context format has been removed.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StepMessage {
    /// Task UUID from tasker.tasks.task_uuid column
    pub task_uuid: Uuid,
    /// Step UUID from tasker.workflow_steps.step_uuid column
    pub step_uuid: Uuid,
    /// TAS-29: Correlation ID for distributed tracing (from tasker.tasks.correlation_id)
    pub correlation_id: Uuid,
}

impl StepMessage {
    /// Create a new step message
    pub fn new(task_uuid: Uuid, step_uuid: Uuid, correlation_id: Uuid) -> Self {
        Self {
            task_uuid,
            step_uuid,
            correlation_id,
        }
    }
}

/// Result of step execution for completion tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepResult {
    /// Step that was executed
    pub step_uuid: Uuid,
    /// Task containing the step
    pub task_uuid: Uuid,
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

// StepExecutionStatus is now imported from orchestration_messages module

// StepExecutionError is now imported from execution_types module

impl Default for OrchestrationMetadata {
    fn default() -> Self {
        Self::new()
    }
}

impl OrchestrationMetadata {
    /// Try to create OrchestrationMetadata from JSON value
    pub fn from_json(value: &serde_json::Value) -> Option<Self> {
        match value {
            serde_json::Value::Null => None,
            serde_json::Value::Object(obj) => {
                let headers = obj
                    .get("headers")
                    .and_then(|v| v.as_object())
                    .map(|h| {
                        h.iter()
                            .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                            .collect()
                    })
                    .unwrap_or_default();

                let error_context = obj
                    .get("error_context")
                    .and_then(|v| v.as_str())
                    .map(String::from);

                let backoff_hint = obj
                    .get("backoff_hint")
                    .and_then(|v| serde_json::from_value(v.clone()).ok());

                let custom = obj
                    .get("custom")
                    .and_then(|v| v.as_object())
                    .map(|c| c.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
                    .unwrap_or_default();

                Some(OrchestrationMetadata {
                    headers,
                    error_context,
                    backoff_hint,
                    custom,
                })
            }
            _ => None,
        }
    }

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
        step_uuid: Uuid,
        task_uuid: Uuid,
        result_data: Option<serde_json::Value>,
        execution_time_ms: u64,
    ) -> Self {
        Self {
            step_uuid,
            task_uuid,
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
        step_uuid: Uuid,
        task_uuid: Uuid,
        result_data: Option<serde_json::Value>,
        execution_time_ms: u64,
        orchestration_metadata: OrchestrationMetadata,
    ) -> Self {
        Self {
            step_uuid,
            task_uuid,
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
        step_uuid: Uuid,
        task_uuid: Uuid,
        error: StepExecutionError,
        execution_time_ms: u64,
    ) -> Self {
        Self {
            step_uuid,
            task_uuid,
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
        step_uuid: Uuid,
        task_uuid: Uuid,
        error: StepExecutionError,
        execution_time_ms: u64,
        orchestration_metadata: OrchestrationMetadata,
    ) -> Self {
        Self {
            step_uuid,
            task_uuid,
            status: StepExecutionStatus::Failed,
            result_data: None,
            error: Some(error),
            execution_time_ms,
            completed_at: chrono::Utc::now(),
            orchestration_metadata: Some(orchestration_metadata),
        }
    }

    /// Create timed out step result
    pub fn timed_out(step_uuid: Uuid, task_uuid: Uuid, execution_time_ms: u64) -> Self {
        Self {
            step_uuid,
            task_uuid,
            status: StepExecutionStatus::Timeout,
            result_data: None,
            error: Some(
                StepExecutionError::new("Step execution timed out".to_string(), true)
                    .with_error_type("TimeoutError".to_string()),
            ),
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
        let task_uuid = Uuid::now_v7();
        let step_uuid = Uuid::now_v7();
        let correlation_id = Uuid::now_v7();

        let message = StepMessage::new(task_uuid, step_uuid, correlation_id);

        assert_eq!(message.task_uuid, task_uuid);
        assert_eq!(message.step_uuid, step_uuid);
        assert_eq!(message.correlation_id, correlation_id);
    }

    #[test]
    fn test_step_message_json_serialization() {
        let task_uuid = Uuid::now_v7();
        let step_uuid = Uuid::now_v7();
        let correlation_id = Uuid::now_v7();

        let message = StepMessage::new(task_uuid, step_uuid, correlation_id);

        // Serialize to JSON
        let json = serde_json::to_value(&message).unwrap();
        assert!(json.get("task_uuid").is_some());
        assert!(json.get("step_uuid").is_some());
        assert!(json.get("correlation_id").is_some());

        // Deserialize back
        let deserialized: StepMessage = serde_json::from_value(json).unwrap();
        assert_eq!(message, deserialized);
    }

    #[test]
    fn test_step_message_equality() {
        let task_uuid = Uuid::now_v7();
        let step_uuid = Uuid::now_v7();
        let correlation_id = Uuid::now_v7();

        let message1 = StepMessage::new(task_uuid, step_uuid, correlation_id);
        let message2 = StepMessage::new(task_uuid, step_uuid, correlation_id);

        assert_eq!(message1, message2);
    }

    #[test]
    fn test_step_result_creation() {
        let step_uuid = Uuid::now_v7();
        let task_uuid = Uuid::now_v7();
        let result = StepResult::success(
            step_uuid,
            task_uuid,
            Some(serde_json::json!({"status": "validated"})),
            1500,
        );

        assert_eq!(result.step_uuid, step_uuid);
        assert_eq!(result.task_uuid, task_uuid);
        assert_eq!(result.status, StepExecutionStatus::Success);
        assert_eq!(result.execution_time_ms, 1500);
        assert!(result.error.is_none());
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
        let step_uuid = Uuid::now_v7();
        let task_uuid = Uuid::now_v7();
        let metadata =
            OrchestrationMetadata::new().with_header("retry-after".to_string(), "30".to_string());

        let result = StepResult::success_with_metadata(
            step_uuid,
            task_uuid,
            Some(serde_json::json!({"status": "validated"})),
            1500,
            metadata,
        );

        assert_eq!(result.step_uuid, step_uuid);
        assert_eq!(result.status, StepExecutionStatus::Success);
        assert!(result.orchestration_metadata.is_some());

        let meta = result.orchestration_metadata.unwrap();
        assert_eq!(meta.headers.get("retry-after"), Some(&"30".to_string()));
    }

    #[test]
    fn test_step_result_failed() {
        let step_uuid = Uuid::now_v7();
        let task_uuid = Uuid::now_v7();
        let error = StepExecutionError::new("Something went wrong".to_string(), true);

        let result = StepResult::failed(step_uuid, task_uuid, error, 500);

        assert_eq!(result.step_uuid, step_uuid);
        assert_eq!(result.task_uuid, task_uuid);
        assert_eq!(result.status, StepExecutionStatus::Failed);
        assert!(result.error.is_some());
        assert_eq!(result.execution_time_ms, 500);
    }

    #[test]
    fn test_step_result_timed_out() {
        let step_uuid = Uuid::now_v7();
        let task_uuid = Uuid::now_v7();

        let result = StepResult::timed_out(step_uuid, task_uuid, 30000);

        assert_eq!(result.step_uuid, step_uuid);
        assert_eq!(result.task_uuid, task_uuid);
        assert_eq!(result.status, StepExecutionStatus::Timeout);
        assert!(result.error.is_some());
        assert_eq!(result.execution_time_ms, 30000);
    }
}
