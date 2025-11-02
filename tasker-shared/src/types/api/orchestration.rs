use crate::database::sql_functions::{StepReadinessStatus, SystemHealthCounts};
use crate::models::core::task::PaginationInfo;
use crate::models::orchestration::execution_status::ExecutionStatus;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use uuid::Uuid;

#[cfg(feature = "web-api")]
use utoipa::ToSchema;

/// Response for successful task creation
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "web-api", derive(ToSchema))]
pub struct TaskCreationResponse {
    pub task_uuid: String,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub estimated_completion: Option<DateTime<Utc>>,
    /// Number of workflow steps created for this task
    pub step_count: usize,
    /// Mapping of step names to their workflow step UUIDs
    pub step_mapping: HashMap<String, String>,
    /// Handler configuration name used (if any)
    pub handler_config_name: Option<String>,
}

/// Task details response with execution context and step readiness information
#[derive(Debug, Serialize, Deserialize, Clone)]
#[cfg_attr(feature = "web-api", derive(ToSchema))]
pub struct TaskResponse {
    pub task_uuid: String,
    pub name: String,
    pub namespace: String,
    pub version: String,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub context: serde_json::Value,
    pub initiator: String,
    pub source_system: String,
    pub reason: String,
    pub priority: Option<i32>,
    pub tags: Option<Vec<String>>,
    pub correlation_id: Uuid,
    pub parent_correlation_id: Option<Uuid>,

    // Execution context fields from TaskExecutionContext
    pub total_steps: i64,
    pub pending_steps: i64,
    pub in_progress_steps: i64,
    pub completed_steps: i64,
    pub failed_steps: i64,
    pub ready_steps: i64,
    pub execution_status: String,
    pub recommended_action: String,
    pub completion_percentage: f64,
    pub health_status: String,

    // Step readiness information
    pub steps: Vec<StepReadinessStatus>,
}

/// Task list response with pagination
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "web-api", derive(ToSchema))]
pub struct TaskListResponse {
    pub tasks: Vec<TaskResponse>,
    pub pagination: PaginationInfo,
}

/// Manual completion data for providing step execution results
///
/// This structure contains only the operator-provided business data.
/// The system will automatically set:
/// - `success: true` (manual completion implies success)
/// - `status: "completed"` (always completed for manual resolution)
/// - `error: None` (no errors for successful manual completion)
#[derive(Debug, Serialize, Deserialize, Clone)]
#[cfg_attr(feature = "web-api", derive(ToSchema))]
pub struct ManualCompletionData {
    /// The actual result data for this step
    ///
    /// This should contain the business data that dependent steps expect.
    /// The shape must match what downstream step handlers require.
    pub result: serde_json::Value,

    /// Optional metadata for observability and tracking
    ///
    /// Can include execution timing, resource usage, or other operational metadata.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

/// Manual step action request for PATCH /tasks/{uuid}/workflow_steps/{step_uuid}
///
/// Supports three distinct operator actions via tagged enum:
///
/// ## 1. Reset for Retry
/// Reset attempt counter and return step to `pending` for automatic retry.
/// Use when the issue was transient and natural retry should succeed.
///
/// ```json
/// {
///   "action_type": "reset_for_retry",
///   "reason": "Database connection restored, retry should succeed",
///   "reset_by": "operator@example.com"
/// }
/// ```
///
/// ## 2. Resolve Manually (Simple)
/// Mark step as `resolved_manually` without providing results.
/// Use when acknowledging failure and moving workflow forward without correction.
///
/// ```json
/// {
///   "action_type": "resolve_manually",
///   "reason": "Non-critical validation step, bypassing",
///   "resolved_by": "operator@example.com"
/// }
/// ```
///
/// ## 3. Complete Manually (with Results)
/// Transition step to `complete` with operator-provided execution results.
/// Use when providing corrected data for dependent steps to consume.
///
/// ```json
/// {
///   "action_type": "complete_manually",
///   "completion_data": {
///     "result": {"validated": true, "score": 95},
///     "metadata": {"manually_verified": true}
///   },
///   "reason": "Manual verification completed",
///   "completed_by": "operator@example.com"
/// }
/// ```
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "web-api", derive(ToSchema))]
#[serde(tag = "action_type", rename_all = "snake_case")]
pub enum StepManualAction {
    /// Reset attempt counter and return step to pending for automatic retry
    ///
    /// State transition: `error` → `pending`
    /// Attempt counter: Reset to 0
    /// Use case: Transient failures that should be retried naturally
    ResetForRetry {
        /// Human-readable reason for reset
        reason: String,
        /// Operator performing the reset
        reset_by: String,
    },

    /// Mark step as manually resolved without providing results
    ///
    /// State transition: Any state → `resolved_manually`
    /// Terminal state: Yes
    /// Use case: Acknowledge failure, move workflow forward without correction
    ResolveManually {
        /// Human-readable reason for manual resolution
        reason: String,
        /// Operator performing the resolution
        resolved_by: String,
    },

    /// Complete step manually with execution results
    ///
    /// State transition: Any state → `complete`
    /// Terminal state: Yes
    /// Use case: Provide corrected data for dependent steps to consume
    ///
    /// System enforces `success: true` and `status: "completed"`.
    /// Operator only provides `result` and optional `metadata`.
    CompleteManually {
        /// Execution results to persist
        ///
        /// Contains business data that dependent steps expect.
        /// System automatically wraps this in `StepExecutionResult` with:
        /// - `success: true`
        /// - `status: "completed"`
        /// - `error: null`
        completion_data: ManualCompletionData,
        /// Human-readable reason for manual completion
        reason: String,
        /// Operator performing the completion
        completed_by: String,
    },
}

/// Step details response with readiness information
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "web-api", derive(ToSchema))]
pub struct StepResponse {
    pub step_uuid: String,
    pub task_uuid: String,
    pub name: String,
    pub created_at: String,
    pub updated_at: String,
    pub completed_at: Option<String>,
    pub results: Option<serde_json::Value>,

    // StepReadinessStatus fields
    pub current_state: String,
    pub dependencies_satisfied: bool,
    pub retry_eligible: bool,
    pub ready_for_execution: bool,
    pub total_parents: i32,
    pub completed_parents: i32,
    pub attempts: i32,
    pub max_attempts: i32,
    pub last_failure_at: Option<String>,
    pub next_retry_at: Option<String>,
    pub last_attempted_at: Option<String>,
}

/// Namespace information
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "web-api", derive(ToSchema))]
pub struct NamespaceInfo {
    pub name: String,
    pub description: Option<String>,
    pub handler_count: u32,
}

/// Handler information
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "web-api", derive(ToSchema))]
pub struct HandlerInfo {
    pub name: String,
    pub namespace: String,
    pub version: String,
    pub description: Option<String>,
    pub step_templates: Vec<String>,
}

/// Basic health check response
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "web-api", derive(ToSchema))]
pub struct HealthResponse {
    pub status: String,
    pub timestamp: String,
}

/// Detailed health check response
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "web-api", derive(ToSchema))]
pub struct DetailedHealthResponse {
    pub status: String,
    pub timestamp: String,
    pub checks: HashMap<String, HealthCheck>,
    pub info: HealthInfo,
}

/// Individual health check result
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "web-api", derive(ToSchema))]
pub struct HealthCheck {
    pub status: String,
    pub message: Option<String>,
    pub duration_ms: u64,
}

/// System information for detailed health
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "web-api", derive(ToSchema))]
pub struct HealthInfo {
    pub version: String,
    pub environment: String,
    pub operational_state: String,
    pub web_database_pool_size: u32,
    pub orchestration_database_pool_size: u32,
    pub circuit_breaker_state: String,
}

/// Query parameters for performance metrics
#[derive(Debug, Deserialize)]
pub struct MetricsQuery {
    /// Number of hours to look back (default: 24)
    pub hours: Option<u32>,
}

/// Query parameters for bottleneck analysis
#[derive(Debug, Deserialize)]
pub struct BottleneckQuery {
    /// Maximum number of slow steps to return (default: 10)
    pub limit: Option<i32>,
    /// Minimum number of executions for inclusion (default: 5)
    pub min_executions: Option<i32>,
}

/// Performance metrics response
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "web-api", derive(ToSchema))]
pub struct PerformanceMetrics {
    pub total_tasks: i64,
    pub active_tasks: i64,
    pub completed_tasks: i64,
    pub failed_tasks: i64,
    pub completion_rate: f64,
    pub error_rate: f64,
    pub average_task_duration_seconds: f64,
    pub average_step_duration_seconds: f64,
    pub tasks_per_hour: i64,
    pub steps_per_hour: i64,
    pub system_health_score: f64,
    pub analysis_period_start: String,
    pub calculated_at: String,
}

/// Bottleneck analysis response
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "web-api", derive(ToSchema))]
pub struct BottleneckAnalysis {
    pub slow_steps: Vec<SlowStepInfo>,
    pub slow_tasks: Vec<SlowTaskInfo>,
    pub resource_utilization: ResourceUtilization,
    pub recommendations: Vec<String>,
}

/// Information about slow-performing steps
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "web-api", derive(ToSchema))]
pub struct SlowStepInfo {
    pub step_name: String,
    pub average_duration_seconds: f64,
    pub max_duration_seconds: f64,
    pub execution_count: i32,
    pub error_count: i32,
    pub error_rate: f64,
    pub last_executed_at: Option<String>,
}

/// Information about slow-performing tasks
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "web-api", derive(ToSchema))]
pub struct SlowTaskInfo {
    pub task_name: String,
    pub average_duration_seconds: f64,
    pub max_duration_seconds: f64,
    pub execution_count: i32,
    pub average_step_count: f64,
    pub error_count: i32,
    pub error_rate: f64,
    pub last_executed_at: Option<String>,
}

/// Resource utilization metrics
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "web-api", derive(ToSchema))]
pub struct ResourceUtilization {
    pub database_pool_utilization: f64,
    pub system_health: SystemHealthCounts,
}

impl TaskResponse {
    /// Convert the execution_status string to ExecutionStatus enum for type-safe handling
    pub fn execution_status_typed(&self) -> ExecutionStatus {
        ExecutionStatus::from(self.execution_status.clone())
    }

    /// Check if the task has completed all steps (execution status indicates all complete)
    pub fn is_execution_complete(&self) -> bool {
        matches!(self.execution_status_typed(), ExecutionStatus::AllComplete)
    }

    /// Check if the task is blocked by failures that cannot be retried
    pub fn is_execution_blocked(&self) -> bool {
        matches!(
            self.execution_status_typed(),
            ExecutionStatus::BlockedByFailures
        )
    }

    /// Check if the task is currently processing (has steps in progress)
    pub fn is_execution_processing(&self) -> bool {
        matches!(self.execution_status_typed(), ExecutionStatus::Processing)
    }

    /// Check if the task has ready steps to execute
    pub fn has_execution_ready_steps(&self) -> bool {
        matches!(
            self.execution_status_typed(),
            ExecutionStatus::HasReadySteps
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_response_execution_status_conversion() {
        let task_response = TaskResponse {
            task_uuid: "test-uuid".to_string(),
            name: "test_task".to_string(),
            namespace: "test".to_string(),
            version: "1.0.0".to_string(),
            status: "pending".to_string(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            completed_at: None,
            context: serde_json::json!({}),
            initiator: "test".to_string(),
            source_system: "test".to_string(),
            reason: "test".to_string(),
            priority: Some(1),
            tags: Some(vec!["test".to_string()]),
            total_steps: 3,
            pending_steps: 0,
            in_progress_steps: 0,
            completed_steps: 3,
            failed_steps: 0,
            ready_steps: 0,
            execution_status: "all_complete".to_string(),
            recommended_action: "finalize_task".to_string(),
            completion_percentage: 100.0,
            health_status: "healthy".to_string(),
            steps: vec![],
            correlation_id: Uuid::now_v7(),
            parent_correlation_id: None,
        };

        // Test conversion to ExecutionStatus enum
        assert_eq!(
            task_response.execution_status_typed(),
            ExecutionStatus::AllComplete
        );
        assert!(task_response.is_execution_complete());
        assert!(!task_response.is_execution_blocked());
        assert!(!task_response.is_execution_processing());
        assert!(!task_response.has_execution_ready_steps());
    }

    #[test]
    fn test_task_response_execution_status_variants() {
        let base_response = TaskResponse {
            task_uuid: "test-uuid".to_string(),
            name: "test_task".to_string(),
            namespace: "test".to_string(),
            version: "1.0.0".to_string(),
            status: "pending".to_string(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            completed_at: None,
            context: serde_json::json!({}),
            initiator: "test".to_string(),
            source_system: "test".to_string(),
            reason: "test".to_string(),
            priority: Some(1),
            tags: Some(vec!["test".to_string()]),
            total_steps: 3,
            pending_steps: 2,
            in_progress_steps: 1,
            completed_steps: 0,
            failed_steps: 0,
            ready_steps: 0,
            execution_status: "processing".to_string(),
            recommended_action: "wait_for_completion".to_string(),
            completion_percentage: 33.3,
            health_status: "healthy".to_string(),
            steps: vec![],
            correlation_id: Uuid::now_v7(),
            parent_correlation_id: None,
        };

        // Test processing status
        assert_eq!(
            base_response.execution_status_typed(),
            ExecutionStatus::Processing
        );
        assert!(base_response.is_execution_processing());
        assert!(!base_response.is_execution_complete());

        // Test blocked by failures status
        let blocked_response = TaskResponse {
            execution_status: "blocked_by_failures".to_string(),
            ..base_response.clone()
        };
        assert_eq!(
            blocked_response.execution_status_typed(),
            ExecutionStatus::BlockedByFailures
        );
        assert!(blocked_response.is_execution_blocked());

        // Test has ready steps status
        let ready_response = TaskResponse {
            execution_status: "has_ready_steps".to_string(),
            ..base_response
        };
        assert_eq!(
            ready_response.execution_status_typed(),
            ExecutionStatus::HasReadySteps
        );
        assert!(ready_response.has_execution_ready_steps());
    }
}

/// Unified orchestration configuration response with both common and orchestration-specific config
///
/// This provides a complete view of the orchestration system's configuration in a single response,
/// making it easier for operators to understand the full deployment without multiple API calls.
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "web-api", derive(ToSchema))]
pub struct OrchestrationConfigResponse {
    pub environment: String,
    /// Common (shared) configuration components
    pub common: JsonValue,
    /// Orchestration-specific configuration
    pub orchestration: JsonValue,
    pub metadata: ConfigMetadata,
}

/// Unified worker configuration response with both common and worker-specific config
///
/// This provides a complete view of the worker system's configuration in a single response,
/// making it easier for operators to understand the full deployment without multiple API calls.
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "web-api", derive(ToSchema))]
pub struct WorkerConfigResponse {
    pub environment: String,
    /// Common (shared) configuration components
    pub common: JsonValue,
    /// Worker-specific configuration
    pub worker: JsonValue,
    pub metadata: ConfigMetadata,
}

/// Metadata about the configuration response
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "web-api", derive(ToSchema))]
pub struct ConfigMetadata {
    pub timestamp: DateTime<Utc>,
    pub source: String,
    pub redacted_fields: Vec<String>,
}

/// Redact sensitive fields from a JSON configuration value
///
/// This function recursively walks through a JSON structure and redacts
/// values for keys that commonly contain secrets or sensitive information.
pub fn redact_secrets(value: JsonValue) -> (JsonValue, Vec<String>) {
    let mut redacted_fields = Vec::new();
    let redacted = redact_recursive(value, &mut redacted_fields, "");
    (redacted, redacted_fields)
}

fn redact_recursive(value: JsonValue, redacted: &mut Vec<String>, path: &str) -> JsonValue {
    match value {
        JsonValue::Object(mut map) => {
            let sensitive_keys = [
                "password",
                "secret",
                "token",
                "key",
                "api_key",
                "private_key",
                "jwt_private_key",
                "jwt_public_key",
                "auth_token",
                "credentials",
                "database_url",
                "url", // Database URLs often contain passwords
            ];

            for (key, val) in map.iter_mut() {
                let key_lower = key.to_lowercase();
                let field_path = if path.is_empty() {
                    key.clone()
                } else {
                    format!("{}.{}", path, key)
                };

                if sensitive_keys.iter().any(|&s| key_lower.contains(s)) {
                    // Check if the value is empty - don't redact empty values
                    let should_redact = match &val {
                        JsonValue::String(s) => !s.is_empty(),
                        JsonValue::Number(_) => true,
                        JsonValue::Bool(_) => false, // Don't redact booleans
                        _ => false,
                    };

                    if should_redact {
                        *val = JsonValue::String("***REDACTED***".to_string());
                        redacted.push(field_path);
                    }
                } else {
                    *val = redact_recursive(val.clone(), redacted, &field_path);
                }
            }
            JsonValue::Object(map)
        }
        JsonValue::Array(arr) => JsonValue::Array(
            arr.into_iter()
                .map(|v| redact_recursive(v, redacted, path))
                .collect(),
        ),
        _ => value,
    }
}
